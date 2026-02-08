package imports //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		BlobCache: &plumbing.BlobCacheAnalyzer{},
	}
	err := h.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}

	opts := h.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		Goroutines:  4,
		MaxFileSize: 1024,
	}
	require.NoError(t, h.Initialize(nil))

	// Add some import data
	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {
			"fmt": {0: 5},
		},
	}

	forks := h.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Forks should have empty independent maps
	require.Empty(t, fork1.imports, "fork should have empty imports map")
	require.Empty(t, fork2.imports, "fork should have empty imports map")

	// Modifying one fork should not affect the other
	fork1.imports[1] = map[string]map[string]map[int]int64{
		"python": {"os": {0: 1}},
	}

	require.Len(t, fork1.imports, 1)
	require.Empty(t, fork2.imports, "fork2 should not see fork1's changes")
}

func TestFork_SharesConfig(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		Goroutines:         8,
		MaxFileSize:        2048,
		reversedPeopleDict: []string{"alice", "bob"},
	}
	require.NoError(t, h.Initialize(nil))

	forks := h.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	// Config should be shared
	require.Equal(t, h.Goroutines, fork1.Goroutines)
	require.Equal(t, h.MaxFileSize, fork1.MaxFileSize)
	require.Equal(t, h.reversedPeopleDict, fork1.reversedPeopleDict)
}

func TestMerge_CombinesImports(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Original has imports for author 0
	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 5}},
	}

	// Branch has imports for author 1
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.imports[1] = map[string]map[string]map[int]int64{
		"python": {"os": {0: 3}},
	}

	h.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have both authors
	require.Len(t, h.imports, 2)
	require.Equal(t, int64(5), h.imports[0]["go"]["fmt"][0])
	require.Equal(t, int64(3), h.imports[1]["python"]["os"][0])
}

func TestMerge_SumsImportCounts(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Original has imports
	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 5, 1: 2}},
	}

	// Branch has same import with different counts
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 3, 2: 1}},
	}

	h.Merge([]analyze.HistoryAnalyzer{branch})

	// Counts should be summed
	require.Equal(t, int64(8), h.imports[0]["go"]["fmt"][0]) // 5 + 3
	require.Equal(t, int64(2), h.imports[0]["go"]["fmt"][1]) // original only
	require.Equal(t, int64(1), h.imports[0]["go"]["fmt"][2]) // branch only
}

func TestForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		Goroutines:  4,
		MaxFileSize: 1024,
	}
	require.NoError(t, h.Initialize(nil))

	// Fork
	forks := h.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Each fork adds different imports
	fork1.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 5}},
	}
	fork2.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 3}, "os": {0: 2}},
	}

	// Merge
	h.Merge(forks)

	// Verify imports are merged
	require.Equal(t, int64(8), h.imports[0]["go"]["fmt"][0]) // 5 + 3
	require.Equal(t, int64(2), h.imports[0]["go"]["os"][0])
}
