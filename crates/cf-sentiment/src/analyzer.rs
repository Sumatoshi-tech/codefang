//! Comment extraction, merging, and filtering pipeline.
//!
//! The pure (UAST-independent) part of the analyzer: grouping comment nodes by
//! line, merging adjacent comments, stripping comment delimiters, filtering
//! comments down to those suitable for sentiment analysis, and the
//! before/after multiset difference ([`new_or_changed`]) that keeps only the
//! comments a commit introduced or edited.
//!
//! The UAST-bound pieces (the history walk, the per-commit tree diff) live in
//! the command layer; this module provides the deterministic text pipeline
//! plus the analyzer's identity constant and the default minimum length, all
//! directly testable.
//!
//! # Pattern semantics
//!
//! The filters are expressed as RE2-style patterns, reproduced here with
//! hand-written Unicode-class scans (no `regex` crate dependency); length and
//! letter-ratio checks use **byte** lengths:
//! * `[^\p{L}\p{N}]` — first char must be a Unicode letter or number;
//! * `[^\p{L}\p{N}\-_:;,./?!#&%+*=\n \t()]+` — strip runs of disallowed
//!   characters;
//! * `\p{L}+` — count letter bytes for the 60% letters ratio;
//! * `\s*[a-zA-Z_][a-zA-Z_0-9]*\(\)` — strip `name()` tokens;
//! * `\s+` — collapse whitespace to a single space (RE2 `\s` is the ASCII set
//!   `[\t\n\f\r ]`);
//! * `(?i)(licen[cs]e|copyright|©)` — drop license/copyright text.

/// Analyzer ID as it appears in reports and on the CLI.
pub const ANALYZER_ID: &str = "history/sentiment";

/// Default minimum comment length in bytes (`--min-comment-len`).
pub const DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH: i64 = 20;

/// Minimum letters ratio for a comment.
pub const COMMENT_LETTERS_RATIO: f32 = 0.6;

/// Comment prefixes stripped before analysis, longest first so `///` matches
/// before `//`.
pub const COMMENT_PREFIXES: &[&str] = &[
    "///", "//!", "//", "/**", "/*", "#!", "##", "#", "--", ";;", ";",
];

/// Comment suffixes stripped from lines.
pub const COMMENT_SUFFIXES: &[&str] = &["*/"];

/// The per-line continuation marker of Javadoc / Doxygen / C block comments
/// (` * text`), stripped after the prefixes.
const BLOCK_CONTINUATION_MARKER: char = '*';

/// Returns true if `c` is a Unicode letter (`\p{L}`).
fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

/// Returns true if `c` is a Unicode number (`\p{N}` = Nd | Nl | No, which is
/// exactly what `char::is_numeric` matches).
fn is_number(c: char) -> bool {
    c.is_numeric()
}

/// Returns true if `c` is whitespace per RE2's default `\s`, the ASCII set
/// `[\t\n\f\r ]` plus vertical tab.
fn is_ascii_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ')
}

/// Whether `c` is an allowed character for the filtered-chars negated class:
/// `\p{L}` | `\p{N}` | one of `-_:;,./?!#&%+*=` | `\n` | ` ` | `\t` | `(` | `)`.
fn is_allowed_filtered_char(c: char) -> bool {
    if is_letter(c) || is_number(c) {
        return true;
    }
    matches!(
        c,
        '-' | '_'
            | ':'
            | ';'
            | ','
            | '.'
            | '/'
            | '?'
            | '!'
            | '#'
            | '&'
            | '%'
            | '+'
            | '*'
            | '='
            | '\n'
            | ' '
            | '\t'
            | '('
            | ')'
    )
}

/// Trims leading/trailing Unicode whitespace.
fn trim_space(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace())
}

/// Removes common comment syntax from each line: a leading prefix
/// ([`COMMENT_PREFIXES`]), a trailing suffix ([`COMMENT_SUFFIXES`]), then a
/// leading block-continuation `*`; the stripped lines are joined with one
/// space.
///
/// ```
/// use cf_sentiment::analyzer::strip_comment_delimiters;
///
/// assert_eq!(strip_comment_delimiters("// a line\n// another"), "a line another");
/// assert_eq!(
///     strip_comment_delimiters("/**\n * Javadoc text\n * continues\n */"),
///     "Javadoc text continues"
/// );
/// ```
#[must_use]
pub fn strip_comment_delimiters(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let mut trimmed = trim_space(line).to_string();
        for prefix in COMMENT_PREFIXES {
            if trimmed.starts_with(prefix) {
                trimmed = trim_space(&trimmed[prefix.len()..]).to_string();
                break;
            }
        }
        for suffix in COMMENT_SUFFIXES {
            if trimmed.ends_with(suffix) {
                let end = trimmed.len() - suffix.len();
                trimmed = trim_space(&trimmed[..end]).to_string();
            }
        }
        if let Some(rest) = trimmed.strip_prefix(BLOCK_CONTINUATION_MARKER) {
            trimmed = trim_space(rest).to_string();
        }
        lines.push(trimmed);
    }
    trim_space(&lines.join(" ")).to_string()
}

/// Strips `name()` tokens (pattern `\s*[a-zA-Z_][a-zA-Z_0-9]*\(\)`).
///
/// An optional run of ASCII whitespace followed by an identifier immediately
/// followed by `()`. All non-overlapping matches are removed.
fn strip_function_names(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // Try to match at position i: \s* then ident then "()".
        // RE2's default \s is ASCII-only: [\t\n\f\r ].
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
            j += 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == b'(' && bytes[j + 1] == b')' {
                // Matched \s*ident(): skip the whole match.
                i = j + 2;
                continue;
            }
        }
        // No match at i: emit the char at i and advance by one UTF-8 char.
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Removes runs of disallowed characters (the filtered-chars class).
fn strip_filtered_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_allowed_filtered_char(c) {
            out.push(c);
        }
        // disallowed chars are dropped (replaced with "").
    }
    out
}

/// Collapses runs of ASCII whitespace (`[\t\n\f\r ]+`) to a single space.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_ascii_space(c) {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// Counts the total byte length of `\p{L}+` matches (letter bytes).
fn count_letter_bytes(s: &str) -> usize {
    s.chars()
        .filter(|&c| is_letter(c))
        .map(char::len_utf8)
        .sum()
}

/// Returns true if `s` contains a license/copyright marker
/// (`(?i)(licen[cs]e|copyright|©)`).
#[must_use]
pub fn is_license(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("license")
        || lower.contains("licence")
        || lower.contains("copyright")
        || lower.contains('\u{00A9}')
}

/// Filters merged comments down to those suitable for sentiment analysis.
///
/// `min_length` is compared against the comment's length in **characters**
/// (Unicode scalar values), so the filter treats every script alike.
#[must_use]
pub fn filter_comments(comments: &[String], min_length: i64) -> Vec<String> {
    let mut filtered = Vec::with_capacity(comments.len());

    for comment in comments {
        let mut comment = strip_comment_delimiters(comment);
        comment = trim_space(&comment).to_string();

        if comment.is_empty() {
            continue;
        }

        // First rune must be a letter/number.
        let Some(first_rune) = comment.chars().next() else {
            continue;
        };
        if !(is_letter(first_rune) || is_number(first_rune)) {
            continue;
        }

        comment = strip_function_names(&comment);
        comment = strip_filtered_chars(&comment);

        if (comment.chars().count() as i64) < min_length {
            continue;
        }

        comment = collapse_whitespace(&comment);

        let chars_count = count_letter_bytes(&comment);
        // Letter bytes must be at least 60% of the byte length (the threshold
        // is computed in f32 and truncated, per the report contract).
        let threshold = (comment.len() as f32 * COMMENT_LETTERS_RATIO) as i64;
        if (chars_count as i64) < threshold {
            continue;
        }

        if is_license(&comment) {
            continue;
        }

        filtered.push(comment);
    }

    filtered
}

/// A minimal comment node for line-grouping/merging.
///
/// Stand-in for a UAST node with start/end line positions and a token. The
/// full UAST node lives in `cf-uast-node`; the merge logic is independent of it.
#[derive(Debug, Clone)]
pub struct CommentNode {
    /// 1-based start line.
    pub start_line: i64,
    /// 1-based end line.
    pub end_line: i64,
    /// The comment token text.
    pub token: String,
}

/// Merges adjacent comment nodes into comment strings.
///
/// Groups nodes by start line, walks lines in ascending order, and emits a
/// merged comment whenever the next line is not within `maxEnd + 1` of the
/// current group.
#[must_use]
pub fn merge_adjacent_comments(nodes: &[CommentNode]) -> Vec<String> {
    use std::collections::BTreeMap;

    // Group by start line (BTreeMap keeps lines ascending).
    let mut lines: BTreeMap<i64, Vec<&CommentNode>> = BTreeMap::new();
    for n in nodes {
        lines.entry(n.start_line).or_default().push(n);
    }

    let line_nums: Vec<i64> = lines.keys().copied().collect();
    let mut merged: Vec<String> = Vec::new();
    let mut buffer: Vec<String> = Vec::new();

    for (idx, &line) in line_nums.iter().enumerate() {
        let line_nodes = &lines[&line];

        let mut max_end = line;
        for n in line_nodes {
            if max_end < n.end_line {
                max_end = n.end_line;
            }
            let token = trim_space(&n.token);
            if !token.is_empty() {
                buffer.push(token.to_string());
            }
        }

        if idx < line_nums.len() - 1 && line_nums[idx + 1] <= max_end + 1 {
            continue;
        }

        merged.push(buffer.join("\n"));
        buffer.clear();
    }

    merged
}

/// Full comment pipeline: merge adjacent nodes then filter.
#[must_use]
pub fn merge_comments(nodes: &[CommentNode], min_length: i64) -> Vec<String> {
    let merged = merge_adjacent_comments(nodes);
    filter_comments(&merged, min_length)
}

/// The comments of `after` that are NEW or CHANGED relative to `before`: the
/// multiset difference `after − before`, in `after` order. A comment text
/// present `n` times in `before` cancels its first `n` occurrences in
/// `after`, so a commit that only adds code to a commented file contributes
/// nothing and an edited comment counts once (as new text).
///
/// ```
/// use cf_sentiment::analyzer::new_or_changed;
///
/// let after = vec!["kept".to_string(), "edited text".to_string()];
/// let before = vec!["kept".to_string(), "old text".to_string()];
/// assert_eq!(new_or_changed(&after, &before), vec!["edited text".to_string()]);
/// ```
#[must_use]
pub fn new_or_changed(after: &[String], before: &[String]) -> Vec<String> {
    use std::collections::HashMap;

    let mut unchanged: HashMap<&str, usize> = HashMap::new();
    for text in before {
        *unchanged.entry(text.as_str()).or_insert(0) += 1;
    }
    after
        .iter()
        .filter(|text| match unchanged.get_mut(text.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

/// Returns the UTF-8 byte length of the char starting with lead byte `b`.
fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The minimum length counts characters, so a short comment in a
    /// non-ASCII script is filtered like its ASCII counterpart.
    #[test]
    fn min_length_counts_characters_not_bytes() {
        // 14 characters, 26 bytes in UTF-8.
        let short_cyrillic = "// Хороший код".to_string();
        assert_eq!(filter_comments(&[short_cyrillic], 20), Vec::<String>::new());
        // 21 characters (and bytes).
        let ascii = "// a really great design".to_string();
        assert_eq!(
            filter_comments(&[ascii], 20),
            vec!["a really great design".to_string()]
        );
    }

    const MIN_LEN: i64 = 10;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().copied().map(String::from).collect()
    }

    #[test]
    fn filter_comments_basic() {
        // english_accepted
        assert_eq!(
            filter_comments(&s(&["This function handles validation correctly"]), MIN_LEN).len(),
            1
        );
        // short_filtered
        assert!(filter_comments(&s(&["bad"]), MIN_LEN).is_empty());
        // license_filtered
        assert!(filter_comments(
            &s(&["Copyright 2024 Acme Corp Licensed under MIT"]),
            MIN_LEN
        )
        .is_empty());
    }

    #[test]
    fn filter_comments_multilingual() {
        let cases = [
            ("chinese", "这个函数处理输入验证逻辑"),
            ("japanese", "この関数は入力を処理して正しい"),
            ("korean", "이 함수는 입력 유효성 검사를 처리"),
            ("cyrillic", "Эта функция"),
            ("arabic", "هذه الدالة تتعامل مع"),
        ];
        for (lang, comment) in cases {
            let r = filter_comments(&s(&[comment]), MIN_LEN);
            assert_eq!(r.len(), 1, "{lang} should be included");
        }
    }

    #[test]
    fn filter_comments_license_uk_spelling() {
        let r = filter_comments(
            &s(&["This code is under the License agreement terms"]),
            MIN_LEN,
        );
        assert!(r.is_empty(), "UK license should be filtered");
    }

    #[test]
    fn license_regex() {
        assert!(is_license("Licensed under MIT License"));
        assert!(is_license("Copyright 2024 Acme Corp"));
        assert!(is_license("\u{00A9} 2024 All rights reserved"));
        assert!(!is_license("This function processes data"));
        // UK spelling.
        assert!(is_license("This Licence covers all usage"));
    }

    #[test]
    fn strip_comment_delimiters_table() {
        let cases = [
            ("// This is a comment", "This is a comment"),
            ("/* Block comment */", "Block comment"),
            ("# Python comment", "Python comment"),
            ("/// Doc comment", "Doc comment"),
            ("//! Module doc", "Module doc"),
            ("-- SQL comment", "SQL comment"),
            ("; Lisp comment", "Lisp comment"),
            ("Normal text", "Normal text"),
            ("// Line 1\n// Line 2", "Line 1 Line 2"),
            ("", ""),
            ("//", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(strip_comment_delimiters(input), expected, "input={input:?}");
        }
    }

    #[test]
    fn merge_adjacent_lines() {
        let nodes = vec![
            CommentNode {
                start_line: 1,
                end_line: 1,
                token: "Line 1 is good".into(),
            },
            CommentNode {
                start_line: 2,
                end_line: 2,
                token: "Line 2 is nice".into(),
            },
        ];
        let merged = merge_adjacent_comments(&nodes);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].contains("Line 1 is good"));
        assert!(merged[0].contains("Line 2 is nice"));
    }

    #[test]
    fn merge_comments_filters_short() {
        let nodes = vec![CommentNode {
            start_line: 2,
            end_line: 2,
            token: "bad".into(),
        }];
        let out = merge_comments(&nodes, MIN_LEN);
        assert!(out.is_empty(), "short comment should be filtered");
    }

    #[test]
    fn merge_comments_keeps_good() {
        let nodes = vec![CommentNode {
            start_line: 1,
            end_line: 1,
            token: "This is a good comment".into(),
        }];
        let out = merge_comments(&nodes, MIN_LEN);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "This is a good comment");
    }

    #[test]
    fn function_name_stripping() {
        // `foo()` should be removed, leaving the descriptive text long enough.
        let r = filter_comments(
            &s(&["initialize foo() and configure bar() properly here"]),
            MIN_LEN,
        );
        assert_eq!(r.len(), 1);
        assert!(!r[0].contains("()"));
    }

    #[test]
    fn identity_constants() {
        assert_eq!(ANALYZER_ID, "history/sentiment");
        assert_eq!(DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH, 20);
    }

    /// A Javadoc / Doxygen block keeps its text once the ` * ` continuation
    /// markers are stripped line by line.
    #[test]
    fn strip_javadoc_continuation_asterisks() {
        let block = "/**\n * This function is wonderful\n * and works every time\n */";
        assert_eq!(
            strip_comment_delimiters(block),
            "This function is wonderful and works every time"
        );
        assert_eq!(strip_comment_delimiters("* bare bullet"), "bare bullet");
        assert_eq!(strip_comment_delimiters(" *"), "");
        // The Javadoc block survives the whole filter pipeline.
        let out = filter_comments(&s(&[block]), DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH);
        assert_eq!(out.len(), 1, "javadoc block must not be rejected: {out:?}");
    }

    /// Only comments NEW in `after` survive: an unchanged comment is removed
    /// once per occurrence in `before`, order of `after` is preserved.
    #[test]
    fn new_or_changed_is_a_multiset_difference() {
        let after = s(&["same text", "new text", "same text", "same text"]);
        let before = s(&["same text", "gone text", "same text"]);
        assert_eq!(
            new_or_changed(&after, &before),
            s(&["new text", "same text"])
        );
        assert!(new_or_changed(&[], &before).is_empty());
        assert_eq!(new_or_changed(&after, &[]), after);
    }

    /// Comment nodes separated by a non-comment line are two merged comments.
    #[test]
    fn merge_non_adjacent_lines_split() {
        let nodes = vec![
            CommentNode {
                start_line: 1,
                end_line: 1,
                token: "first block".into(),
            },
            CommentNode {
                start_line: 5,
                end_line: 6,
                token: "second block".into(),
            },
            CommentNode {
                start_line: 7,
                end_line: 7,
                token: "still second".into(),
            },
        ];
        assert_eq!(
            merge_adjacent_comments(&nodes),
            s(&["first block", "second block\nstill second"])
        );
    }
}
