package gitlib

// Default batch processing configuration values.
const (
	// defaultBlobBatchSize is the default number of blobs to load per batch.
	defaultBlobBatchSize = 100
	// defaultDiffBatchSize is the default number of diffs to compute per batch.
	defaultDiffBatchSize = 50
)

// BatchConfig configures batch processing parameters.
type BatchConfig struct {
	// BlobBatchSize is the number of blobs to load per batch.
	// Default: 100.
	BlobBatchSize int

	// DiffBatchSize is the number of diffs to compute per batch.
	// Default: 50.
	DiffBatchSize int

	// Workers is the number of parallel workers for processing.
	// Default: 1 (sequential processing within gitlib).
	Workers int
}

// DefaultBatchConfig returns the default batch configuration.
func DefaultBatchConfig() BatchConfig {
	return BatchConfig{
		BlobBatchSize: defaultBlobBatchSize,
		DiffBatchSize: defaultDiffBatchSize,
		Workers:       1,
	}
}
