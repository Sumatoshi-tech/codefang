package common

// ContextStack is a generic LIFO stack for tracking nested analysis contexts
// during UAST tree traversal. It replaces the repeated push/pop/peek pattern
// found in visitor implementations.
type ContextStack[T any] struct {
	items []T
}

// NewContextStack creates a new empty [ContextStack].
func NewContextStack[T any]() *ContextStack[T] {
	return &ContextStack[T]{}
}

// Push adds an element to the top of the stack.
func (s *ContextStack[T]) Push(ctx T) {
	s.items = append(s.items, ctx)
}

// Pop removes and returns the top element. Returns the zero value and false
// if the stack is empty.
func (s *ContextStack[T]) Pop() (T, bool) {
	if len(s.items) == 0 {
		var zero T

		return zero, false
	}

	top := s.items[len(s.items)-1]
	s.items = s.items[:len(s.items)-1]

	return top, true
}

// Current returns the top element without removing it. Returns the zero
// value and false if the stack is empty.
func (s *ContextStack[T]) Current() (T, bool) {
	if len(s.items) == 0 {
		var zero T

		return zero, false
	}

	return s.items[len(s.items)-1], true
}

// Depth returns the number of elements on the stack.
func (s *ContextStack[T]) Depth() int {
	return len(s.items)
}
