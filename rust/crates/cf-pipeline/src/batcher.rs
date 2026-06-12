//! Batching strategies.

/// Accumulates input items and produces batches.
pub trait Batcher<In, Batch> {
    /// Adds an item. Returns `true` if the batch is ready to flush.
    fn add(&mut self, item: In) -> bool;

    /// Returns the current batch and resets. Returns `None` if empty.
    fn flush(&mut self) -> Option<Batch>;
}

/// Accumulates items into a `Vec` until the count reaches the configured
/// threshold, at which point [`Batcher::add`] returns `true`.
#[derive(Debug, Clone)]
pub struct ThresholdBatcher<T> {
    threshold: usize,
    items: Vec<T>,
}

impl<T> ThresholdBatcher<T> {
    /// Creates a batcher that signals readiness after `threshold` items.
    /// Threshold values below 1 are clamped to 1.
    #[must_use]
    pub fn new(threshold: i64) -> Self {
        let threshold = threshold.max(1) as usize;
        ThresholdBatcher {
            threshold,
            items: Vec::new(),
        }
    }
}

impl<T> Batcher<T, Vec<T>> for ThresholdBatcher<T> {
    /// Appends an item. Returns `true` when the batch reaches the threshold.
    fn add(&mut self, item: T) -> bool {
        self.items.push(item);
        self.items.len() >= self.threshold
    }

    /// Returns the accumulated items and resets the internal buffer.
    /// Returns `None` if no items have been added since the last flush.
    fn flush(&mut self) -> Option<Vec<T>> {
        if self.items.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.items))
    }
}

/// Wraps each input item as a single-element batch. [`Batcher::add`] always
/// returns `true`, meaning every item is immediately ready.
#[derive(Debug, Clone, Default)]
pub struct PassthroughBatcher<T> {
    item: Option<T>,
}

impl<T> PassthroughBatcher<T> {
    /// Creates an empty passthrough batcher.
    #[must_use]
    pub const fn new() -> Self {
        Self { item: None }
    }
}

impl<T> Batcher<T, Vec<T>> for PassthroughBatcher<T> {
    /// Stores the item and returns `true` (always ready). A previously
    /// stored, un-flushed item is overwritten.
    fn add(&mut self, item: T) -> bool {
        self.item = Some(item);
        true
    }

    /// Returns the stored item as a single-element `Vec` and resets.
    /// Returns `None` if `add` was not called since the last flush.
    fn flush(&mut self) -> Option<Vec<T>> {
        self.item.take().map(|item| vec![item])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_batcher_signals_at_threshold() {
        let mut b = ThresholdBatcher::new(3);
        assert!(!b.add(1));
        assert!(!b.add(2));
        assert!(b.add(3), "third add reaches threshold");
        assert_eq!(b.flush(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn threshold_batcher_clamps_below_one() {
        let mut b = ThresholdBatcher::new(0);
        assert!(b.add(1), "threshold clamped to 1, first add is ready");
        let mut b = ThresholdBatcher::new(-5);
        assert!(b.add(1));
    }

    #[test]
    fn threshold_batcher_flush_empty_returns_none() {
        let mut b: ThresholdBatcher<i32> = ThresholdBatcher::new(2);
        assert_eq!(b.flush(), None);
    }

    #[test]
    fn threshold_batcher_resets_after_flush() {
        let mut b = ThresholdBatcher::new(2);
        b.add(1);
        b.add(2);
        assert_eq!(b.flush(), Some(vec![1, 2]));
        // Buffer reset; next flush with no adds is empty.
        assert_eq!(b.flush(), None);
        b.add(3);
        assert_eq!(b.flush(), Some(vec![3]));
    }

    #[test]
    fn passthrough_always_ready() {
        let mut b = PassthroughBatcher::new();
        assert!(b.add(42));
        assert_eq!(b.flush(), Some(vec![42]));
    }

    #[test]
    fn passthrough_flush_without_add_returns_none() {
        let mut b: PassthroughBatcher<i32> = PassthroughBatcher::new();
        assert_eq!(b.flush(), None);
    }

    #[test]
    fn passthrough_overwrites_pending_item() {
        let mut b = PassthroughBatcher::new();
        b.add(1);
        b.add(2);
        assert_eq!(b.flush(), Some(vec![2]));
    }

    // Compile-time check that both concrete types satisfy the trait.
    fn _assert_impls() {
        fn takes<B: Batcher<i32, Vec<i32>>>(_: &B) {}
        takes(&ThresholdBatcher::<i32>::new(1));
        takes(&PassthroughBatcher::<i32>::new());
    }
}
