package analyze

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// FRD: specs/frds/FRD-20260302-build-commits-by-tick.md.

// testTickData is a minimal tick data type for testing BuildCommitsByTick.
type testTickData struct {
	Commits map[string]int
}

// commitTickHashA is a deterministic hash hex string for testing.
const commitTickHashA = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

// commitTickHashB is a second deterministic hash hex string for testing.
const commitTickHashB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

func extractTestTickData(data any) (map[string]int, bool) {
	td, ok := data.(*testTickData)
	if !ok || td == nil {
		return nil, false
	}

	return td.Commits, td.Commits != nil
}

func TestBuildCommitsByTick_EmptyTicks(t *testing.T) {
	t.Parallel()

	result := BuildCommitsByTick(nil, extractTestTickData)

	require.NotNil(t, result)
	assert.Empty(t, result)
}

func TestBuildCommitsByTick_NilData(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 0, Data: nil},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)
	assert.Empty(t, result)
}

func TestBuildCommitsByTick_ExtractReturnsFalse(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 0, Data: "wrong type"},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)
	assert.Empty(t, result)
}

func TestBuildCommitsByTick_SingleTick(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 1, Data: &testTickData{Commits: map[string]int{commitTickHashA: 1}}},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)

	require.Len(t, result, 1)
	require.Len(t, result[1], 1)
	assert.Equal(t, gitlib.NewHash(commitTickHashA), result[1][0])
}

func TestBuildCommitsByTick_MultipleTicks(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 0, Data: &testTickData{Commits: map[string]int{commitTickHashA: 1}}},
		{Tick: 1, Data: &testTickData{Commits: map[string]int{commitTickHashB: 2}}},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)

	require.Len(t, result, 2)
	require.Len(t, result[0], 1)
	assert.Equal(t, gitlib.NewHash(commitTickHashA), result[0][0])
	require.Len(t, result[1], 1)
	assert.Equal(t, gitlib.NewHash(commitTickHashB), result[1][0])
}

func TestBuildCommitsByTick_SameTickMerged(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 0, Data: &testTickData{Commits: map[string]int{commitTickHashA: 1}}},
		{Tick: 0, Data: &testTickData{Commits: map[string]int{commitTickHashB: 2}}},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)

	require.Len(t, result, 1)
	require.Len(t, result[0], 2)

	hashes := make(map[gitlib.Hash]bool)
	for _, h := range result[0] {
		hashes[h] = true
	}

	assert.True(t, hashes[gitlib.NewHash(commitTickHashA)])
	assert.True(t, hashes[gitlib.NewHash(commitTickHashB)])
}

func TestBuildCommitsByTick_EmptyMapSkipped(t *testing.T) {
	t.Parallel()

	ticks := []TICK{
		{Tick: 0, Data: &testTickData{Commits: map[string]int{}}},
		{Tick: 1, Data: &testTickData{Commits: map[string]int{commitTickHashA: 1}}},
	}

	result := BuildCommitsByTick(ticks, extractTestTickData)

	require.Len(t, result, 1)
	assert.Contains(t, result, 1)
}
