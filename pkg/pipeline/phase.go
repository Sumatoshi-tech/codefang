package pipeline

import "context"

// Phase represents a single processing stage that transforms state S.
type Phase[S any] interface {
	Run(ctx context.Context, s S) (S, error)
}

// PhaseFunc adapts a plain function to the Phase interface.
type PhaseFunc[S any] func(ctx context.Context, s S) (S, error)

// Run executes the phase function.
func (f PhaseFunc[S]) Run(ctx context.Context, s S) (S, error) {
	return f(ctx, s)
}

// RunPhases executes phases sequentially, threading state through each one.
// Returns immediately on the first error, preserving the partial state.
// Returns the input state unchanged when no phases are provided.
func RunPhases[S any](ctx context.Context, s S, phases ...Phase[S]) (S, error) {
	var err error

	for _, p := range phases {
		s, err = p.Run(ctx, s)
		if err != nil {
			return s, err
		}
	}

	return s, nil
}
