//! Cursor/word utilities for the mapping-DSL LSP server.
//!
//! Direct port of the unexported helpers in Go `pkg/uast/lsp/server.go`:
//! `extractWordAtPosition`, `isWordChar`, and `splitLines`. The byte-oriented
//! behaviour of the Go code is reproduced exactly:
//!
//! * `splitLines` is `strings.Split(s, "\n")` — it never collapses a trailing
//!   newline, so `"hello\n"` yields `["hello", ""]`.
//! * `extractWordAtPosition` indexes into a line **by byte offset** (Go indexes
//!   `string` by byte), clamps an out-of-range `character` to the line length,
//!   and grows a word in both directions while [`is_word_char`] holds.
//! * `is_word_char` accepts ASCII letters, `_`, and the DSL operator bytes
//!   `< > - =` (so `<-` and `=>` are treated as single words), and nothing else.

/// Splits `input` on `'\n'`, exactly like Go `strings.Split(input, "\n")`.
///
/// Notably this does NOT treat a trailing newline specially: an empty input
/// yields `[""]`, and a trailing `'\n'` produces a trailing empty element.
#[must_use]
pub fn split_lines(input: &str) -> Vec<&str> {
    input.split('\n').collect()
}

/// Reports whether `ch` is part of a mapping-DSL "word".
///
/// Matches Go `isWordChar`: ASCII `a-z`, `A-Z`, `_`, and the operator bytes
/// `<`, `>`, `-`, `=`. Operates on a raw byte so multi-byte UTF-8 sequences are
/// never word characters (identical to Go indexing a `string` by byte).
#[must_use]
pub fn is_word_char(ch: u8) -> bool {
    ch.is_ascii_lowercase()
        || ch.is_ascii_uppercase()
        || ch == b'_'
        || ch == b'<'
        || ch == b'>'
        || ch == b'-'
        || ch == b'='
}

/// Returns the word at the given zero-based `line` / `character` position.
///
/// Port of Go `extractWordAtPosition`. `character` is a **byte** offset into the
/// line (LSP positions are UTF-16 offsets, but the Go server treats them as byte
/// offsets, and we reproduce that behaviour verbatim for parity). Returns `""`
/// when `line` is out of range or the cursor sits between non-word bytes.
#[must_use]
pub fn extract_word_at_position(text: &str, line: usize, character: usize) -> String {
    let lines = split_lines(text);
    if line >= lines.len() {
        return String::new();
    }

    let line_bytes = lines[line].as_bytes();

    // Clamp the cursor to the end of the line (Go: `if character > len(lineText)`).
    let mut character = character;
    if character > line_bytes.len() {
        character = line_bytes.len();
    }

    // Expand left while the preceding byte is a word byte.
    let mut start = character;
    while start > 0 && is_word_char(line_bytes[start - 1]) {
        start -= 1;
    }

    // Expand right while the current byte is a word byte.
    let mut end = character;
    while end < line_bytes.len() && is_word_char(line_bytes[end]) {
        end += 1;
    }

    // The slice [start, end) is guaranteed to lie on ASCII word bytes (word
    // bytes are all single-byte ASCII), so it is always valid UTF-8.
    String::from_utf8_lossy(&line_bytes[start..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from Go `TestExtractWordAtPosition` (table-driven).
    #[test]
    fn test_extract_word_at_position() {
        struct Case {
            name: &'static str,
            text: &'static str,
            line: usize,
            character: usize,
            expected: &'static str,
        }

        let cases = [
            Case { name: "simple word", text: "hello world", line: 0, character: 2, expected: "hello" },
            Case { name: "second word", text: "hello world", line: 0, character: 8, expected: "world" },
            Case { name: "keyword arrow", text: "rule <- pattern", line: 0, character: 6, expected: "<-" },
            Case { name: "keyword mapping", text: "pattern => uast", line: 0, character: 9, expected: "=>" },
            Case { name: "multiline first line", text: "first\nsecond\nthird", line: 0, character: 2, expected: "first" },
            Case { name: "multiline second line", text: "first\nsecond\nthird", line: 1, character: 3, expected: "second" },
            Case { name: "multiline third line", text: "first\nsecond\nthird", line: 2, character: 2, expected: "third" },
            Case { name: "line out of bounds", text: "single line", line: 5, character: 0, expected: "" },
            // Clamps to line length and returns last word.
            Case { name: "character past end of line", text: "short", line: 0, character: 100, expected: "short" },
            Case { name: "underscore in word", text: "my_variable = 1", line: 0, character: 5, expected: "my_variable" },
            Case { name: "empty text", text: "", line: 0, character: 0, expected: "" },
        ];

        for c in cases {
            let got = extract_word_at_position(c.text, c.line, c.character);
            assert_eq!(
                got, c.expected,
                "[{}] extract_word_at_position({:?}, {}, {}) = {:?}, expected {:?}",
                c.name, c.text, c.line, c.character, got, c.expected
            );
        }
    }

    /// Ported from Go `TestIsWordChar`.
    #[test]
    fn test_is_word_char() {
        let cases: &[(u8, bool)] = &[
            (b'a', true),
            (b'z', true),
            (b'A', true),
            (b'Z', true),
            (b'_', true),
            (b'<', true),
            (b'>', true),
            (b'-', true),
            (b'=', true),
            (b'0', false),
            (b'9', false),
            (b' ', false),
            (b'\t', false),
            (b'\n', false),
            (b'(', false),
            (b')', false),
            (b'{', false),
            (b'}', false),
            (b':', false),
        ];

        for &(ch, expected) in cases {
            assert_eq!(
                is_word_char(ch),
                expected,
                "is_word_char({:?}) = {}, expected {}",
                ch as char,
                is_word_char(ch),
                expected
            );
        }
    }

    /// Ported from Go `TestSplitLines` (table-driven).
    #[test]
    fn test_split_lines() {
        let cases: &[(&str, &str, &[&str])] = &[
            ("single line", "hello", &["hello"]),
            ("two lines", "hello\nworld", &["hello", "world"]),
            ("three lines", "one\ntwo\nthree", &["one", "two", "three"]),
            ("empty string", "", &[""]),
            ("trailing newline", "hello\n", &["hello", ""]),
        ];

        for &(name, input, expected) in cases {
            let got = split_lines(input);
            assert_eq!(
                got.len(),
                expected.len(),
                "[{name}] split_lines({input:?}) returned {} lines, expected {}",
                got.len(),
                expected.len()
            );
            for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
                assert_eq!(g, e, "[{name}] split_lines({input:?})[{i}] = {g:?}, expected {e:?}");
            }
        }
    }
}
