// Package checkpoint provides state persistence for streaming analysis.
package checkpoint

// AggregatorSpillEntry records on-disk spill state for a single aggregator.
type AggregatorSpillEntry struct {
	// Dir is the directory containing gob-encoded spill files.
	Dir string `json:"dir,omitempty"`

	// Count is the number of spill files in Dir.
	Count int `json:"count,omitempty"`
}

// StreamingState tracks chunk orchestrator progress.
type StreamingState struct {
	TotalCommits     int    `json:"total_commits"`
	ProcessedCommits int    `json:"processed_commits"`
	CurrentChunk     int    `json:"current_chunk"`
	TotalChunks      int    `json:"total_chunks"`
	LastCommitHash   string `json:"last_commit_hash"`
	LastTick         int    `json:"last_tick"`

	// AggregatorSpills records the spill state of each aggregator at checkpoint time.
	// Indexed by analyzer position in the Runner.Analyzers slice.
	// Nil entries mean the analyzer has no aggregator (plumbing, file_history).
	AggregatorSpills []AggregatorSpillEntry `json:"aggregator_spills,omitempty"`
}

// Metadata holds checkpoint metadata for validation and resume.
type Metadata struct {
	Version        int               `json:"version"`
	RepoPath       string            `json:"repo_path"`
	RepoHash       string            `json:"repo_hash"`
	CreatedAt      string            `json:"created_at"`
	Analyzers      []string          `json:"analyzers"`
	StreamingState StreamingState    `json:"streaming_state"`
	Checksums      map[string]string `json:"checksums"`
}
