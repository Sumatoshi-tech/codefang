package pipeline

// Batcher accumulates input items and produces batches.
type Batcher[In, Batch any] interface {
	// Add adds an item. Returns true if the batch is ready to flush.
	Add(In) bool

	// Flush returns the current batch and resets. Returns false if empty.
	Flush() (Batch, bool)
}

// Compile-time interface satisfaction checks.
var (
	_ Batcher[int, []int] = (*ThresholdBatcher[int])(nil)
	_ Batcher[int, []int] = (*PassthroughBatcher[int])(nil)
)

// ThresholdBatcher accumulates items into a slice until the count reaches
// the configured threshold, at which point Add returns true.
type ThresholdBatcher[T any] struct {
	threshold int
	items     []T
}

// NewThresholdBatcher creates a batcher that signals readiness after threshold items.
// Threshold values below 1 are clamped to 1.
func NewThresholdBatcher[T any](threshold int) *ThresholdBatcher[T] {
	return &ThresholdBatcher[T]{threshold: max(threshold, 1)}
}

// Add appends an item. Returns true when the batch reaches the threshold.
func (b *ThresholdBatcher[T]) Add(item T) bool {
	b.items = append(b.items, item)

	return len(b.items) >= b.threshold
}

// Flush returns the accumulated items and resets the internal buffer.
// Returns false if no items have been added since the last flush.
func (b *ThresholdBatcher[T]) Flush() ([]T, bool) {
	if len(b.items) == 0 {
		return nil, false
	}

	batch := b.items
	b.items = nil

	return batch, true
}

// PassthroughBatcher wraps each input item as a single-element batch.
// Add always returns true, meaning every item is immediately ready.
type PassthroughBatcher[T any] struct {
	item *T
}

// Add stores the item and returns true (always ready).
func (b *PassthroughBatcher[T]) Add(item T) bool {
	b.item = &item

	return true
}

// Flush returns the stored item as a single-element slice and resets.
// Returns false if Add was not called since the last flush.
func (b *PassthroughBatcher[T]) Flush() ([]T, bool) {
	if b.item == nil {
		return nil, false
	}

	batch := []T{*b.item}
	b.item = nil

	return batch, true
}
