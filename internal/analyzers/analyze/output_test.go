package analyze

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	testHashA = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	testHashB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
)

func TestOutputHistoryResults_NDJSONReturnsNil(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	err := OutputHistoryResults(nil, nil, FormatNDJSON, &buf)
	require.NoError(t, err)
	assert.Empty(t, buf.String(), "NDJSON format should produce no output from OutputHistoryResults")
}

func TestBuildOrderedCommitMetaFromReports_WithMetadata(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashB := testHashB

	reports := map[string]Report{
		"quality": {
			"commits_by_tick": map[int][]gitlib.Hash{
				0: {gitlib.NewHash(hashA)},
				1: {gitlib.NewHash(hashB)},
			},
			ReportKeyCommitMeta: map[string]CommitMeta{
				hashA: {Hash: hashA, Tick: 0, Timestamp: "2024-01-01T00:00:00Z", Author: "alice"},
				hashB: {Hash: hashB, Tick: 1, Timestamp: "2024-01-02T00:00:00Z", Author: "bob"},
			},
		},
	}

	meta := buildOrderedCommitMetaFromReports(reports)

	require.Len(t, meta, 2)
	assert.Equal(t, hashA, meta[0].Hash)
	assert.Equal(t, 0, meta[0].Tick)
	assert.Equal(t, "2024-01-01T00:00:00Z", meta[0].Timestamp)
	assert.Equal(t, "alice", meta[0].Author)
	assert.Equal(t, hashB, meta[1].Hash)
	assert.Equal(t, 1, meta[1].Tick)
	assert.Equal(t, "2024-01-02T00:00:00Z", meta[1].Timestamp)
	assert.Equal(t, "bob", meta[1].Author)
}

func TestBuildOrderedCommitMetaFromReports_WithoutMetadata(t *testing.T) {
	t.Parallel()

	hashA := testHashA

	reports := map[string]Report{
		"quality": {
			"commits_by_tick": map[int][]gitlib.Hash{
				0: {gitlib.NewHash(hashA)},
			},
		},
	}

	meta := buildOrderedCommitMetaFromReports(reports)

	require.Len(t, meta, 1)
	assert.Equal(t, hashA, meta[0].Hash)
	assert.Equal(t, 0, meta[0].Tick)
	// Without commit_meta, Timestamp and Author remain empty.
	assert.Empty(t, meta[0].Timestamp)
	assert.Empty(t, meta[0].Author)
}

func TestOutputHistoryResults_TimeSeriesNDJSON(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	// FormatTimeSeriesNDJSON with no leaves/results produces empty output.
	err := OutputHistoryResults(nil, nil, FormatTimeSeriesNDJSON, &buf)
	require.NoError(t, err)
	assert.Empty(t, buf.String())
}

func TestBuildOrderedCommitMetaFromReports_PartialMetadata(t *testing.T) {
	t.Parallel()

	hashA := testHashA
	hashB := testHashB

	reports := map[string]Report{
		"quality": {
			"commits_by_tick": map[int][]gitlib.Hash{
				0: {gitlib.NewHash(hashA), gitlib.NewHash(hashB)},
			},
			// Only hashA has metadata.
			ReportKeyCommitMeta: map[string]CommitMeta{
				hashA: {Hash: hashA, Tick: 0, Timestamp: "2024-01-01T00:00:00Z", Author: "alice"},
			},
		},
	}

	meta := buildOrderedCommitMetaFromReports(reports)

	require.Len(t, meta, 2)
	// hashA has metadata.
	assert.Equal(t, "2024-01-01T00:00:00Z", meta[0].Timestamp)
	assert.Equal(t, "alice", meta[0].Author)
	// hashB has no metadata — empty fields.
	assert.Empty(t, meta[1].Timestamp)
	assert.Empty(t, meta[1].Author)
}
