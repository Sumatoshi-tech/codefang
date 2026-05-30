//! Byte-level text utilities: binary detection and line counting.
//!
//! Direct port of the byte helpers in Go `pkg/textutil/textutil.go`. These
//! operate on raw byte slices (`&[u8]`), mirroring Go's `[]byte` semantics
//! exactly — no UTF-8 decoding is performed.

/// Maximum number of bytes scanned for null-byte detection.
///
/// Matches the heuristic used by Git and most editors. Port of Go
/// `BinarySniffLength`.
pub const BINARY_SNIFF_LENGTH: usize = 8000;

/// Returns `true` if `data` contains a null byte within the first
/// [`BINARY_SNIFF_LENGTH`] bytes. Empty data is not binary.
///
/// Port of Go `IsBinary`.
///
/// # Examples
///
/// ```
/// use cf_textutil::is_binary;
/// assert!(!is_binary(b""));
/// assert!(!is_binary(b"hello world\n"));
/// assert!(is_binary(b"hello\x00world"));
/// ```
pub fn is_binary(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let sniff = if data.len() > BINARY_SNIFF_LENGTH {
        &data[..BINARY_SNIFF_LENGTH]
    } else {
        data
    };
    sniff.contains(&0)
}

/// Returns the number of newline-delimited lines in `data`.
///
/// A non-empty buffer without a trailing newline counts the last partial line.
/// Returns `0` for empty data. Port of Go `CountLines`.
///
/// # Examples
///
/// ```
/// use cf_textutil::count_lines;
/// assert_eq!(count_lines(b""), 0);
/// assert_eq!(count_lines(b"hello"), 1);
/// assert_eq!(count_lines(b"hello\n"), 1);
/// assert_eq!(count_lines(b"a\nb\nc"), 3);
/// assert_eq!(count_lines(b"\n\n\n"), 3);
/// ```
pub fn count_lines(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }
    let mut lines = bytecount_newlines(data);
    if data[data.len() - 1] != b'\n' {
        lines += 1;
    }
    lines
}

/// Counts `'\n'` bytes in `data`. Equivalent to Go's `bytes.Count(data, "\n")`.
#[inline]
fn bytecount_newlines(data: &[u8]) -> usize {
    data.iter().filter(|&&b| b == b'\n').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- IsBinary (ported from Go) ---

    #[test]
    fn test_is_binary_empty_data() {
        assert!(!is_binary(b""));
    }

    #[test]
    fn test_is_binary_pure_text() {
        assert!(!is_binary(b"hello world\n"));
    }

    #[test]
    fn test_is_binary_null_byte() {
        assert!(is_binary(b"hello\x00world"));
    }

    #[test]
    fn test_is_binary_null_at_start() {
        assert!(is_binary(b"\x00start"));
    }

    #[test]
    fn test_is_binary_null_at_sniff_boundary() {
        // Null byte at exactly position BINARY_SNIFF_LENGTH-1 should be detected.
        let mut data = vec![b'a'; BINARY_SNIFF_LENGTH];
        data[BINARY_SNIFF_LENGTH - 1] = 0x00;
        assert!(is_binary(&data));
    }

    #[test]
    fn test_is_binary_null_beyond_sniff_boundary() {
        // Null byte beyond the sniff window should NOT be detected.
        let mut data = vec![b'a'; BINARY_SNIFF_LENGTH + 100];
        data[BINARY_SNIFF_LENGTH + 50] = 0x00;
        assert!(!is_binary(&data));
    }

    #[test]
    fn test_is_binary_short_data_no_null() {
        assert!(!is_binary(b"short"));
    }

    // --- CountLines (ported from Go) ---

    #[test]
    fn test_count_lines_empty_data() {
        assert_eq!(count_lines(b""), 0);
    }

    #[test]
    fn test_count_lines_single_line_no_newline() {
        assert_eq!(count_lines(b"hello"), 1);
    }

    #[test]
    fn test_count_lines_single_line_with_newline() {
        assert_eq!(count_lines(b"hello\n"), 1);
    }

    #[test]
    fn test_count_lines_multiple_lines() {
        assert_eq!(count_lines(b"a\nb\nc\n"), 3);
    }

    #[test]
    fn test_count_lines_multiple_lines_no_trailing_newline() {
        assert_eq!(count_lines(b"a\nb\nc"), 3);
    }

    #[test]
    fn test_count_lines_empty_lines() {
        // "\n\n\n" = 3 empty lines.
        assert_eq!(count_lines(b"\n\n\n"), 3);
    }

    #[test]
    fn test_count_lines_single_newline() {
        assert_eq!(count_lines(b"\n"), 1);
    }

    #[test]
    fn test_count_lines_large_file() {
        let lines = b"line\n".repeat(10000);
        assert_eq!(count_lines(&lines), 10000);
    }

    // --- BinarySniffLength constant ---

    #[test]
    fn test_binary_sniff_length_value() {
        // BINARY_SNIFF_LENGTH matches the well-known 8000-byte heuristic.
        assert_eq!(BINARY_SNIFF_LENGTH, 8000);
    }
}
