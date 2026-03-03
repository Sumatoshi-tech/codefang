package pipeline

import (
	"context"
	"sync"
)

// SharedResponse evaluates a computation exactly once and caches the result
// for concurrent access by multiple goroutines. The computation receives a
// [context.Context] for cancellation support.
type SharedResponse[T any] struct {
	once    sync.Once
	result  T
	err     error
	compute func(context.Context) (T, error)
}

// NewSharedResponse creates a [SharedResponse] that will evaluate compute
// on the first call to [SharedResponse.Get]. The compute function must not
// be nil.
func NewSharedResponse[T any](compute func(context.Context) (T, error)) *SharedResponse[T] {
	return &SharedResponse[T]{compute: compute}
}

// Get evaluates the compute function exactly once (via [sync.Once]) and
// returns the cached (result, error) pair. Subsequent calls return the same
// values without re-evaluation, regardless of the context passed.
func (s *SharedResponse[T]) Get(ctx context.Context) (T, error) {
	s.once.Do(func() {
		s.result, s.err = s.compute(ctx)
	})

	return s.result, s.err
}
