package typos //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/assert/yaml"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	opts := h.ListConfigurationOptions()
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	clones := h.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestHistoryAnalyzer_Fork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		MaximumAllowedDistance: 5,
	}
	require.NoError(t, h.Initialize(nil))

	h.typos = []Typo{{Wrong: "original", Correct: "test"}}

	const forkCount = 3

	forks := h.Fork(forkCount)
	require.Len(t, forks, forkCount)

	// Each fork should be independent.
	for i, fork := range forks {
		analyzer, ok := fork.(*HistoryAnalyzer)
		require.True(t, ok, "fork %d should be *HistoryAnalyzer", i)

		// Should be different pointer.
		require.NotSame(t, h, analyzer, "fork %d should not be same pointer as parent", i)

		// Config should be copied.
		require.Equal(t, h.MaximumAllowedDistance, analyzer.MaximumAllowedDistance)

		// Should have fresh typos slice (empty).
		require.Empty(t, analyzer.typos, "fork %d should have empty typos", i)

		// Should have its own lcontext.
		require.NotNil(t, analyzer.lcontext, "fork %d should have lcontext", i)
	}
}

func TestHistoryAnalyzer_Fork_IndependentModification(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	forks := h.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Modify fork1.
	fork1.typos = append(fork1.typos, Typo{Wrong: "fork1typo", Correct: "test"})

	// fork2 should be unaffected.
	require.Empty(t, fork2.typos, "fork2 should be independent of fork1")

	// Parent should be unaffected.
	require.Empty(t, h.typos, "parent should be independent of forks")
}

func TestHistoryAnalyzer_Merge_CombinesTypos(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	h.typos = []Typo{{Wrong: "parent", Correct: "test"}}

	// Create branches with different typos.
	branch1 := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "typo1", Correct: "correct1"},
			{Wrong: "typo2", Correct: "correct2"},
		},
	}

	branch2 := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "typo3", Correct: "correct3"},
		},
	}

	h.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	// Parent should now have all typos from branches.
	require.Len(t, h.typos, 4) // 1 original + 2 from branch1 + 1 from branch2
	require.Equal(t, "parent", h.typos[0].Wrong)
	require.Equal(t, "typo1", h.typos[1].Wrong)
	require.Equal(t, "typo2", h.typos[2].Wrong)
	require.Equal(t, "typo3", h.typos[3].Wrong)
}

func TestHistoryAnalyzer_Merge_HandlesEmptyBranches(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	h.typos = []Typo{{Wrong: "original", Correct: "test"}}

	emptyBranch := &HistoryAnalyzer{typos: nil}

	h.Merge([]analyze.HistoryAnalyzer{emptyBranch})

	require.Len(t, h.typos, 1)
	require.Equal(t, "original", h.typos[0].Wrong)
}

func TestHistoryAnalyzer_Merge_HandlesNilBranches(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	h.typos = []Typo{{Wrong: "original", Correct: "test"}}

	// Merge with nil slice should not panic.
	h.Merge(nil)

	require.Len(t, h.typos, 1)
}

func TestHistoryAnalyzer_ForkMerge_RoundTrip(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		MaximumAllowedDistance: DefaultMaximumAllowedTypoDistance,
	}
	require.NoError(t, h.Initialize(nil))

	// Fork.
	forks := h.Fork(2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Simulate parallel processing.
	fork1.typos = append(fork1.typos, Typo{Wrong: "branch1typo", Correct: "test1"})
	fork2.typos = append(fork2.typos, Typo{Wrong: "branch2typo", Correct: "test2"})

	// Merge back.
	h.Merge(forks)

	// Should have both typos.
	require.Len(t, h.typos, 2)

	wrongValues := []string{h.typos[0].Wrong, h.typos[1].Wrong}
	require.Contains(t, wrongValues, "branch1typo")
	require.Contains(t, wrongValues, "branch2typo")
}

// --- Serialize Tests ---

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
		{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatJSON, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_JSON_Empty(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatJSON, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.TypoList)
	assert.Equal(t, 0, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
		{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML_Empty(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.TypoList)
	assert.Equal(t, 0, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML_ContainsExpectedFields(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "typo_list:")
	assert.Contains(t, output, "patterns:")
	assert.Contains(t, output, "file_typos:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_DefaultFormat(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, "", &buf)
	require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}
