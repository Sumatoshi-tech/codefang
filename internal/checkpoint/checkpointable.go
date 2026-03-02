package checkpoint

// Checkpointable is an optional interface for analyzers that support checkpointing.
type Checkpointable interface {
	// SaveCheckpoint writes analyzer state to the given directory.
	SaveCheckpoint(dir string) error

	// LoadCheckpoint restores analyzer state from the given directory.
	LoadCheckpoint(dir string) error

	// CheckpointSize returns the estimated size of the checkpoint in bytes.
	CheckpointSize() int64
}
