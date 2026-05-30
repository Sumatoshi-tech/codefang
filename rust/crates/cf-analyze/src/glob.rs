//! A port of Go's `path.Match` for analyzer-ID glob expansion.
//!
//! `internal/analyzers/analyze/registry.go` uses `path.Match(pattern, id)` to
//! expand patterns like `history/*` and `static/co*`. Rust's standard library
//! has no equivalent, so this module reimplements `path.Match` semantics
//! exactly (slash is a path separator that `*`/`?`/`[…]` do not cross), which
//! is byte-identity relevant because it decides which analyzers appear in
//! output.

/// Error returned for a malformed pattern (mirrors Go `path.ErrBadPattern`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadPattern;

impl std::fmt::Display for BadPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("syntax error in pattern")
    }
}

impl std::error::Error for BadPattern {}

/// Reports whether `name` matches the shell pattern `pattern`, using Go
/// `path.Match` semantics. Port of Go `path.Match`.
///
/// Pattern syntax:
/// * `*` matches any sequence of non-`/` characters.
/// * `?` matches any single non-`/` character.
/// * `[…]` is a character class (with `^`/`!` negation and `a-z` ranges);
///   it matches a single non-`/` character.
/// * all other characters match themselves.
pub fn path_match(pattern: &str, name: &str) -> Result<bool, BadPattern> {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_segment(&p, &n)
}

fn match_segment(mut pattern: &[char], mut name: &[char]) -> Result<bool, BadPattern> {
    loop {
        if pattern.is_empty() {
            return Ok(name.is_empty());
        }
        match pattern[0] {
            '*' => {
                // Consume consecutive '*' (a single '*' suffices in path.Match,
                // but collapsing keeps behavior identical).
                let rest = &pattern[1..];
                // '*' matches the rest of the segment up to a '/'. Try the
                // shortest-to-longest expansions.
                // Fast path: trailing '*' matches everything with no '/'.
                if rest.is_empty() {
                    return Ok(!name.iter().any(|&c| c == '/'));
                }
                let mut i = 0;
                loop {
                    if match_segment(rest, &name[i..])? {
                        return Ok(true);
                    }
                    if i >= name.len() || name[i] == '/' {
                        return Ok(false);
                    }
                    i += 1;
                }
            }
            '?' => {
                if name.is_empty() || name[0] == '/' {
                    return Ok(false);
                }
                pattern = &pattern[1..];
                name = &name[1..];
            }
            '[' => {
                if name.is_empty() || name[0] == '/' {
                    return Ok(false);
                }
                let (matched, consumed) = match_class(&pattern[1..], name[0])?;
                if !matched {
                    return Ok(false);
                }
                pattern = &pattern[1 + consumed..];
                name = &name[1..];
            }
            c => {
                if name.is_empty() || name[0] != c {
                    return Ok(false);
                }
                pattern = &pattern[1..];
                name = &name[1..];
            }
        }
    }
}

/// Matches a single character against a `[...]` class. `class` starts just
/// after the `[`. Returns `(matched, chars_consumed_including_trailing_bracket)`.
fn match_class(class: &[char], ch: char) -> Result<(bool, usize), BadPattern> {
    let mut i = 0;
    let mut negate = false;
    if i < class.len() && (class[i] == '^' || class[i] == '!') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    loop {
        if i >= class.len() {
            return Err(BadPattern); // unterminated class
        }
        if class[i] == ']' && !first {
            i += 1; // consume ']'
            break;
        }
        first = false;
        let lo = class[i];
        i += 1;
        // Range a-z (the '-' must be followed by a class char, not ']').
        if i + 1 < class.len() && class[i] == '-' && class[i + 1] != ']' {
            let hi = class[i + 1];
            i += 2;
            if lo <= ch && ch <= hi {
                matched = true;
            }
        } else if ch == lo {
            matched = true;
        }
    }
    Ok((matched ^ negate, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_segment() {
        assert!(path_match("history/*", "history/burndown").unwrap());
        assert!(path_match("static/co*", "static/complexity").unwrap());
    }

    #[test]
    fn star_does_not_cross_slash() {
        assert!(!path_match("history/*", "history/sub/x").unwrap());
        assert!(!path_match("*", "a/b").unwrap());
    }

    #[test]
    fn question_matches_single() {
        assert!(path_match("a?c", "abc").unwrap());
        assert!(!path_match("a?c", "ac").unwrap());
        assert!(!path_match("a?c", "a/c").unwrap());
    }

    #[test]
    fn literal_match() {
        assert!(path_match("static/complexity", "static/complexity").unwrap());
        assert!(!path_match("static/complexity", "static/cohesion").unwrap());
    }

    #[test]
    fn char_class() {
        assert!(path_match("[abc]", "b").unwrap());
        assert!(!path_match("[abc]", "d").unwrap());
        assert!(path_match("[a-c]x", "bx").unwrap());
        assert!(path_match("[!a-c]x", "dx").unwrap());
        assert!(!path_match("[!a-c]x", "bx").unwrap());
    }

    #[test]
    fn unterminated_class_errors() {
        assert!(path_match("[abc", "a").is_err());
    }

    #[test]
    fn trailing_star_matches_rest() {
        assert!(path_match("history/*", "history/").unwrap());
    }
}
