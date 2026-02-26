package typos

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/assert/yaml"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	assert.NotEmpty(t, h.Name())
}

func TestAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	assert.NotEmpty(t, h.Flag())
}

func TestAnalyzer_Description(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	assert.NotEmpty(t, h.Description())
}

func TestAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	opts := h.ListConfigurationOptions()
	assert.NotEmpty(t, opts)
}

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	err := h.Configure(map[string]any{
		ConfigTyposDatasetMaximumAllowedDistance: 3,
	})
	require.NoError(t, err)
	assert.Equal(t, 3, h.MaximumAllowedDistance)
}

func TestAnalyzer_Configure_Default(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	err := h.Initialize(nil)
	require.NoError(t, err)
	assert.NotNil(t, h.lcontext)
	assert.Equal(t, DefaultMaximumAllowedTypoDistance, h.MaximumAllowedDistance)
}

func TestAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	h.MaximumAllowedDistance = 5
	require.NoError(t, h.Initialize(nil))

	forks := h.Fork(3)
	require.Len(t, forks, 3)

	for i, fork := range forks {
		analyzer, ok := fork.(*Analyzer)
		require.True(t, ok, "fork %d should be *Analyzer", i)
		assert.NotSame(t, h, analyzer)
		assert.Equal(t, 5, analyzer.MaximumAllowedDistance)
		assert.NotNil(t, analyzer.lcontext)
		assert.NotNil(t, analyzer.UAST)
		assert.NotNil(t, analyzer.BlobCache)
		assert.NotNil(t, analyzer.FileDiff)
	}
}

func TestAnalyzer_Merge_IsNoOp(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	// Should not panic with nil or empty slices.
	h.Merge(nil)
	h.Merge([]analyze.HistoryAnalyzer{})
}

func TestAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	assert.False(t, h.SequentialOnly())
}

func TestAnalyzer_CPUHeavy(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	assert.True(t, h.CPUHeavy())
}

func TestAnalyzer_NewAggregator(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	agg := h.NewAggregator(analyze.AggregatorOptions{SpillBudget: 1 << 20})
	require.NotNil(t, agg)
}

func TestAnalyzer_SerializeTICKs_JSON(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Typos: []Typo{
					{Wrong: "tets", Correct: "test", File: "main.go", Line: 10},
					{Wrong: "functon", Correct: "function", File: "util.go", Line: 20},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := h.SerializeTICKs(ticks, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestAnalyzer_SerializeTICKs_YAML(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Typos: []Typo{
					{Wrong: "tets", Correct: "test", File: "main.go", Line: 10},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := h.SerializeTICKs(ticks, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "typo_list:")
	assert.Contains(t, output, "aggregate:")
}

// --- Serialize Tests (legacy path) ---.

func TestAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
		{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer

	err := h.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestAnalyzer_Serialize_JSON_Empty(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	var buf bytes.Buffer

	err := h.Serialize(analyze.Report{}, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)
	assert.Empty(t, result.TypoList)
}

func TestAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer

	err := h.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	var result ComputedMetrics

	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)
	assert.Len(t, result.TypoList, 1)
}

func TestAnalyzer_Serialize_YAML_ContainsExpectedFields(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
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

func TestAnalyzer_Serialize_Unsupported(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()

	err := h.Serialize(analyze.Report{}, "", &bytes.Buffer{})
	assert.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	desc := h.Descriptor()
	assert.Equal(t, "history/typos", desc.ID)
	assert.Equal(t, analyze.ModeHistory, desc.Mode)
}
