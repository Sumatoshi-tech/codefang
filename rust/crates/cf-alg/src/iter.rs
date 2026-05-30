//! Pull-based iterator collection. Port of `pkg/alg/iter.go`.

use std::error::Error;
use std::fmt;

/// Error returned by a pull-based [`PullIterator`].
///
/// The Go original signals end-of-stream with the sentinel `io.EOF` and uses
/// `errors.Is(err, io.EOF)` to distinguish exhaustion from a genuine failure.
/// Rust has no equivalent sentinel-as-value idiom, so the distinction is modeled
/// explicitly: [`IteratorError::Eof`] means "exhausted" and
/// [`IteratorError::Other`] carries a real error.
#[derive(Debug)]
pub enum IteratorError {
    /// The iterator is exhausted (the equivalent of Go's `io.EOF`).
    Eof,
    /// A genuine, non-EOF error produced while pulling the next value.
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for IteratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IteratorError::Eof => write!(f, "EOF"),
            IteratorError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for IteratorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IteratorError::Eof => None,
            IteratorError::Other(e) => Some(&**e),
        }
    }
}

/// A pull-based sequence of `T` values.
///
/// Port of the Go `alg.Iterator[T]` interface. [`next`](PullIterator::next)
/// returns `Ok(value)` for each item and `Err(IteratorError::Eof)` when
/// exhausted; [`close`](PullIterator::close) releases any resources held by the
/// iterator. Named `PullIterator` to avoid colliding with the std
/// [`Iterator`](core::iter::Iterator) trait.
pub trait PullIterator<T> {
    /// Returns the next value, or [`IteratorError::Eof`] when exhausted.
    ///
    /// # Errors
    ///
    /// Returns [`IteratorError::Eof`] at end-of-stream, or
    /// [`IteratorError::Other`] for a genuine failure.
    fn next(&mut self) -> Result<T, IteratorError>;

    /// Releases any resources held by the iterator.
    fn close(&mut self);
}

/// Drains up to `limit` items from `iter` into a vector.
///
/// A `limit` of `0` means unlimited — all items are collected. Returns an empty
/// vector when the iterator is already exhausted. Non-EOF errors are returned
/// immediately (the partially built vector is discarded, matching the Go
/// original which returns a nil slice alongside the error).
///
/// This reproduces `alg.CollectN` exactly, including the "limit == 0 means
/// unlimited" convention and the discard-on-error behavior.
///
/// # Errors
///
/// Propagates any [`IteratorError::Other`] produced by `iter.next()`. EOF is not
/// an error here — it terminates collection normally.
pub fn collect_n<T, I: PullIterator<T> + ?Sized>(
    iter: &mut I,
    limit: usize,
) -> Result<Vec<T>, IteratorError> {
    let mut result = Vec::new();

    let mut i = 0usize;
    while limit == 0 || i < limit {
        match iter.next() {
            Ok(item) => result.push(item),
            Err(IteratorError::Eof) => break,
            Err(other) => return Err(other),
        }
        i += 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test stub implementing [`PullIterator`] over a slice, mirroring the Go
    /// `sliceIter[T]` test helper.
    struct SliceIter<T> {
        items: Vec<T>,
        pos: usize,
        /// Injected error at exhaustion (instead of EOF), if any.
        err: Option<&'static str>,
    }

    impl<T> SliceIter<T> {
        fn new(items: Vec<T>) -> Self {
            Self {
                items,
                pos: 0,
                err: None,
            }
        }

        fn with_error(items: Vec<T>, err: &'static str) -> Self {
            Self {
                items,
                pos: 0,
                err: Some(err),
            }
        }
    }

    impl<T: Clone> PullIterator<T> for SliceIter<T> {
        fn next(&mut self) -> Result<T, IteratorError> {
            if self.pos >= self.items.len() {
                if let Some(msg) = self.err {
                    return Err(IteratorError::Other(msg.into()));
                }
                return Err(IteratorError::Eof);
            }
            let item = self.items[self.pos].clone();
            self.pos += 1;
            Ok(item)
        }

        fn close(&mut self) {
            self.pos = self.items.len();
        }
    }

    fn is_failed(err: &IteratorError) -> bool {
        matches!(err, IteratorError::Other(e) if e.to_string() == "iterator failed")
    }

    #[test]
    fn empty_iterator() {
        // Go: TestCollectN_EmptyIterator.
        let mut iter = SliceIter::<i32>::new(vec![]);
        let got = collect_n(&mut iter, 0).expect("no error");
        assert!(got.is_empty());
    }

    #[test]
    fn collect_all() {
        // Go: TestCollectN_CollectAll.
        let mut iter = SliceIter::new(vec![1, 2, 3, 4, 5]);
        let got = collect_n(&mut iter, 0).expect("no error");
        assert_eq!(got, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn with_limit() {
        // Go: TestCollectN_WithLimit.
        let mut iter = SliceIter::new(vec![10, 20, 30, 40, 50]);
        let got = collect_n(&mut iter, 3).expect("no error");
        assert_eq!(got, vec![10, 20, 30]);
    }

    #[test]
    fn limit_exceeds_items() {
        // Go: TestCollectN_LimitExceedsItems.
        let mut iter = SliceIter::new(vec!["a", "b"]);
        let got = collect_n(&mut iter, 10).expect("no error");
        assert_eq!(got, vec!["a", "b"]);
    }

    #[test]
    fn error_propagation() {
        // Go: TestCollectN_ErrorPropagation.
        let mut iter = SliceIter::with_error(vec![1, 2], "iterator failed");
        let err = collect_n(&mut iter, 0).expect_err("expected error");
        assert!(is_failed(&err));
    }

    #[test]
    fn error_after_partial_read() {
        // Go: TestCollectN_ErrorAfterPartialRead.
        let mut iter = SliceIter::with_error(vec![1], "iterator failed");
        let err = collect_n(&mut iter, 0).expect_err("expected error");
        assert!(is_failed(&err));
    }

    #[test]
    fn exhausted_iterator() {
        // Go: TestCollectN_ExhaustedIterator.
        let mut iter = SliceIter::<i32>::new(vec![]);

        let got1 = collect_n(&mut iter, 0).expect("no error");
        assert!(got1.is_empty());

        let got2 = collect_n(&mut iter, 0).expect("no error");
        assert!(got2.is_empty());
    }

    #[test]
    fn limit_one() {
        // Go: TestCollectN_LimitOne.
        let mut iter = SliceIter::new(vec![42, 99]);
        let got = collect_n(&mut iter, 1).expect("no error");
        assert_eq!(got, vec![42]);
    }

    #[test]
    fn close_marks_exhausted() {
        // Close drains the iterator; subsequent collect yields nothing.
        let mut iter = SliceIter::new(vec![1, 2, 3]);
        iter.close();
        let got = collect_n(&mut iter, 0).expect("no error");
        assert!(got.is_empty());
    }
}
