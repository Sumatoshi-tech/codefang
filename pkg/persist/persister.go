package persist

// Persister handles I/O for a specific state type using a Codec.
type Persister[T any] struct {
	basename string
	codec    Codec
}

// NewPersister creates a persister with the given basename and codec.
func NewPersister[T any](basename string, codec Codec) *Persister[T] {
	return &Persister[T]{
		basename: basename,
		codec:    codec,
	}
}

// Save writes state to the given directory using the provided build function.
func (p *Persister[T]) Save(dir string, buildState func() *T) error {
	state := buildState()

	return SaveState(dir, p.basename, p.codec, state)
}

// Load restores state from the given directory using the provided restore function.
func (p *Persister[T]) Load(dir string, restoreState func(*T)) error {
	var state T

	err := LoadState(dir, p.basename, p.codec, &state)
	if err != nil {
		return err
	}

	restoreState(&state)

	return nil
}
