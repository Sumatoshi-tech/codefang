//! Comment extraction, merging, and filtering pipeline.
//!
//! The pure (UAST-independent) part of the analyzer: grouping comment nodes by
//! line, merging adjacent comments, stripping comment delimiters, and filtering
//! comments down to those suitable for sentiment analysis.
//!
//! The UAST-bound pieces (`Consume`, `Fork`, snapshots, the history-analyzer
//! wiring) live in the framework crates; this module provides the deterministic
//! text pipeline plus the analyzer's identity constants and config defaults,
//! all directly testable.
//!
//! # Pattern semantics (report compatibility)
//!
//! The reference implementation expresses these filters as RE2 patterns. They
//! are reproduced here with hand-written Unicode-class scans (no `regex` crate
//! dependency) so that length/letter-ratio checks use **byte** lengths, exactly
//! as the reference does:
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

/// Analyzer description shown in CLI help.
pub const ANALYZER_DESCRIPTION: &str =
    "Classifies each new or changed comment per commit as containing positive or negative emotions.";

/// Minimum comment length below which the configured value is replaced by the
/// default.
pub const MIN_COMMENT_LENGTH_THRESHOLD_HIGH: i64 = 10;

/// Default minimum comment length.
pub const DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH: i64 = 20;

/// Default sentiment gap threshold.
pub const DEFAULT_COMMENT_SENTIMENT_GAP: f32 = 0.5;

/// Minimum letters ratio for a comment.
pub const COMMENT_LETTERS_RATIO: f32 = 0.6;

/// Estimated bytes of TC payload per commit.
pub const SENTIMENT_AVG_TC_SIZE: i64 = 500;

/// Config key for the minimum comment length.
pub const CONFIG_MIN_LENGTH: &str = "CommentSentiment.MinLength";
/// Config key for the sentiment gap.
pub const CONFIG_GAP: &str = "CommentSentiment.Gap";

/// Comment prefixes stripped before analysis, longest first so `///` matches
/// before `//`.
pub const COMMENT_PREFIXES: &[&str] =
    &["///", "//!", "//", "/**", "/*", "#!", "##", "#", "--", ";;", ";"];

/// Comment suffixes stripped from lines.
pub const COMMENT_SUFFIXES: &[&str] = &["*/"];

/// Configuration shared by the analyzer: the configurable fields that affect
/// the text pipeline.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    /// Minimum comment byte length.
    pub min_comment_length: i64,
    /// Sentiment gap threshold.
    pub gap: f32,
}

impl Config {
    /// Clamps invalid configuration to defaults.
    pub fn validate(&mut self) {
        if self.gap < 0.0 || self.gap >= 1.0 {
            self.gap = DEFAULT_COMMENT_SENTIMENT_GAP;
        }
        if self.min_comment_length < MIN_COMMENT_LENGTH_THRESHOLD_HIGH {
            self.min_comment_length = DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH;
        }
    }
}

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

/// Removes common comment syntax from each line.
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
    s.chars().filter(|&c| is_letter(c)).map(char::len_utf8).sum()
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
/// `min_length` is compared against the **byte** length (report compatibility:
/// the reference implementation compares byte lengths).
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

        if (comment.len() as i64) < min_length {
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

    const MIN_LEN: i64 = 10;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn validate_clamps() {
        let mut c = Config { min_comment_length: 5, gap: 2.0 };
        c.validate();
        assert_eq!(c.gap, DEFAULT_COMMENT_SENTIMENT_GAP);
        assert_eq!(c.min_comment_length, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH);

        let mut ok = Config { min_comment_length: 30, gap: 0.8 };
        ok.validate();
        assert_eq!(ok.min_comment_length, 30);
        assert!((ok.gap - 0.8).abs() < 1e-6);
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
        assert!(filter_comments(&s(&["Copyright 2024 Acme Corp Licensed under MIT"]), MIN_LEN).is_empty());
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
        let r = filter_comments(&s(&["This code is under the License agreement terms"]), MIN_LEN);
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
            CommentNode { start_line: 1, end_line: 1, token: "Line 1 is good".into() },
            CommentNode { start_line: 2, end_line: 2, token: "Line 2 is nice".into() },
        ];
        let merged = merge_adjacent_comments(&nodes);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].contains("Line 1 is good"));
        assert!(merged[0].contains("Line 2 is nice"));
    }

    #[test]
    fn merge_comments_filters_short() {
        let nodes = vec![CommentNode { start_line: 2, end_line: 2, token: "bad".into() }];
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
        let r = filter_comments(&s(&["initialize foo() and configure bar() properly here"]), MIN_LEN);
        assert_eq!(r.len(), 1);
        assert!(!r[0].contains("()"));
    }

    #[test]
    fn identity_constants() {
        assert_eq!(ANALYZER_ID, "history/sentiment");
        assert_eq!(DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH, 20);
        assert!((DEFAULT_COMMENT_SENTIMENT_GAP - 0.5).abs() < 1e-6);
    }
}
