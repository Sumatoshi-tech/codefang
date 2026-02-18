package checkpoint

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestStreamingState_JSONRoundTrip(t *testing.T) {
	t.Parallel()

	state := StreamingState{
		TotalCommits:     100000,
		ProcessedCommits: 50000,
		CurrentChunk:     1,
		TotalChunks:      2,
		LastCommitHash:   "abc123def456",
		LastTick:         42,
	}

	data, err := json.Marshal(state)
	require.NoError(t, err)

	var restored StreamingState

	err = json.Unmarshal(data, &restored)
	require.NoError(t, err)

	assert.Equal(t, state, restored)
}

func TestMetadata_JSONRoundTrip(t *testing.T) {
	t.Parallel()

	meta := Metadata{
		Version:        1,
		RepoPath:       "/home/user/repo",
		RepoHash:       "abc123",
		Analyzers:      []string{"burndown", "devs"},
		StreamingState: StreamingState{TotalCommits: 100, ProcessedCommits: 50},
		Checksums:      map[string]string{"file1.bin": "sha256:abc"},
	}

	data, err := json.Marshal(meta)
	require.NoError(t, err)

	var restored Metadata

	err = json.Unmarshal(data, &restored)
	require.NoError(t, err)

	assert.Equal(t, meta.Version, restored.Version)
	assert.Equal(t, meta.RepoPath, restored.RepoPath)
	assert.Equal(t, meta.Analyzers, restored.Analyzers)
	assert.Equal(t, meta.Checksums, restored.Checksums)
}

func TestMetadata_CreatedAt(t *testing.T) {
	t.Parallel()

	meta := Metadata{
		Version:   1,
		CreatedAt: "2026-02-05T12:00:00Z",
	}

	data, err := json.Marshal(meta)
	require.NoError(t, err)

	var restored Metadata

	err = json.Unmarshal(data, &restored)
	require.NoError(t, err)

	assert.Equal(t, "2026-02-05T12:00:00Z", restored.CreatedAt)
}
