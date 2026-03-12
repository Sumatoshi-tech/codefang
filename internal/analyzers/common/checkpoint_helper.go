package common

import "github.com/Sumatoshi-tech/codefang/pkg/persist"

// CheckpointHelper provides SaveCheckpoint and LoadCheckpoint methods backed
// by a [persist.Persister] with pre-bound build and restore callbacks.
// Embed *CheckpointHelper[T] in an analyzer struct to promote these methods
// and partially satisfy the checkpoint.Checkpointable interface.
type CheckpointHelper[T any] struct {
	persister *persist.Persister[T]
	build     func() *T
	restore   func(*T)
}

// NewCheckpointHelper creates a helper that saves/loads state of type T using
// the given basename, codec, and callbacks.
func NewCheckpointHelper[T any](
	basename string,
	codec persist.Codec,
	build func() *T,
	restore func(*T),
) *CheckpointHelper[T] {
	return &CheckpointHelper[T]{
		persister: persist.NewPersister[T](basename, codec),
		build:     build,
		restore:   restore,
	}
}

// SaveCheckpoint writes the state to the given directory.
func (h *CheckpointHelper[T]) SaveCheckpoint(dir string) error {
	return h.persister.Save(dir, h.build)
}

// LoadCheckpoint restores the state from the given directory.
func (h *CheckpointHelper[T]) LoadCheckpoint(dir string) error {
	return h.persister.Load(dir, h.restore)
}
