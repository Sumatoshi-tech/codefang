// Package checkpoint provides state persistence for streaming analysis.
package checkpoint

// StreamingState tracks chunk orchestrator progress.
type StreamingState struct {
	TotalCommits     int    `json:"total_commits"`
	ProcessedCommits int    `json:"processed_commits"`
	CurrentChunk     int    `json:"current_chunk"`
	TotalChunks      int    `json:"total_chunks"`
	LastCommitHash   string `json:"last_commit_hash"`
	LastTick         int    `json:"last_tick"`
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
