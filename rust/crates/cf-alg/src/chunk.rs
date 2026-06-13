//! Range chunking.

/// A half-open interval `[start, end)`: `start` is the inclusive lower bound
/// and `end` is the exclusive upper bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// Inclusive start index.
    pub start: usize,
    /// Exclusive end index.
    pub end: usize,
}

/// Splits the range `[0, total)` into chunks of the given `size`.
///
/// The last chunk may be smaller than `size`. A zero `total` or `size` yields
/// an empty vector; otherwise the chunks tile `[0, total)` contiguously with
/// the final chunk clamped to `total`.
///
/// # Examples
///
/// ```
/// use cf_alg::{chunk, Range};
/// assert_eq!(
///     chunk(7, 3),
///     vec![
///         Range { start: 0, end: 3 },
///         Range { start: 3, end: 6 },
///         Range { start: 6, end: 7 },
///     ],
/// );
/// ```
#[must_use]
pub fn chunk(total: usize, size: usize) -> Vec<Range> {
    if total == 0 || size == 0 {
        return Vec::new();
    }

    // Number of chunks = ceil(total / size).
    let n = total.div_ceil(size);
    let mut chunks = Vec::with_capacity(n);

    let mut start = 0usize;
    while start < total {
        let end = (start + size).min(total);
        chunks.push(Range { start, end });
        start += size;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_total() {
        // alg.Chunk(0, 5) is nil.
        assert!(chunk(0, 5).is_empty());
    }

    #[test]
    fn zero_size() {
        // alg.Chunk(10, 0) is nil.
        assert!(chunk(10, 0).is_empty());
    }

    #[test]
    fn size_greater_than_total() {
        assert_eq!(chunk(5, 10), vec![Range { start: 0, end: 5 }]);
    }

    #[test]
    fn exact_division() {
        assert_eq!(
            chunk(10, 5),
            vec![Range { start: 0, end: 5 }, Range { start: 5, end: 10 }],
        );
    }

    #[test]
    fn remainder() {
        assert_eq!(
            chunk(7, 3),
            vec![
                Range { start: 0, end: 3 },
                Range { start: 3, end: 6 },
                Range { start: 6, end: 7 },
            ],
        );
    }

    #[test]
    fn single_element() {
        assert_eq!(chunk(1, 1), vec![Range { start: 0, end: 1 }]);
    }

    #[test]
    fn size_equals_total() {
        assert_eq!(chunk(5, 5), vec![Range { start: 0, end: 5 }]);
    }

    #[test]
    fn contiguous() {
        const TOTAL: usize = 100;
        const SIZE: usize = 7;

        let chunks = chunk(TOTAL, SIZE);

        // First chunk starts at 0.
        assert_eq!(chunks[0].start, 0);
        // Last chunk ends at total.
        assert_eq!(chunks[chunks.len() - 1].end, TOTAL);
        // Adjacent chunks are contiguous.
        for i in 1..chunks.len() {
            assert_eq!(chunks[i - 1].end, chunks[i].start);
        }
    }
}
