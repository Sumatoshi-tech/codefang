package halstead

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

func TestMetricsCalculator_DeliveredBugsUsesVolume(t *testing.T) {
	t.Parallel()

	calculator := NewMetricsCalculator()
	m := &FunctionHalsteadMetrics{
		DistinctOperators: 2,
		DistinctOperands:  3,
		TotalOperators:    4,
		TotalOperands:     6,
	}

	calculator.CalculateHalsteadMetrics(m)

	expected := m.Volume / BugConstant
	assert.InDelta(t, expected, m.DeliveredBugs, 1e-12)
}

func TestAnalyzer_DoesNotCountParameterNodeAsOperand(t *testing.T) {
	t.Parallel()

	source := `def foo(x, y):
    z = x + y
    return z
`

	parser, err := uast.NewParser()
	require.NoError(t, err)

	root, err := parser.Parse(context.Background(), "sample.py", []byte(source))
	require.NoError(t, err)

	analyzer := NewAnalyzer()
	report, err := analyzer.Analyze(root)
	require.NoError(t, err)

	functions, ok := report["functions"].([]map[string]any)
	require.True(t, ok)
	require.Len(t, functions, 1)

	operands, ok := functions[0]["operands"].(map[string]int)
	require.True(t, ok)

	assert.NotContains(t, operands, "Parameter")
}

func TestAnalyzer_ReportIncludesPerFunctionTokenCounts(t *testing.T) {
	t.Parallel()

	source := `func sample(x int) int {
	y := x + 1
	return y
}`

	parser, err := uast.NewParser()
	require.NoError(t, err)

	root, err := parser.Parse(context.Background(), "sample.go", []byte(source))
	require.NoError(t, err)

	analyzer := NewAnalyzer()
	report, err := analyzer.Analyze(root)
	require.NoError(t, err)

	functions, ok := report["functions"].([]map[string]any)
	require.True(t, ok)
	require.Len(t, functions, 1)

	function := functions[0]
	assert.Contains(t, function, "distinct_operators")
	assert.Contains(t, function, "distinct_operands")
	assert.Contains(t, function, "total_operators")
	assert.Contains(t, function, "total_operands")
	assert.Contains(t, function, "vocabulary")
	assert.Contains(t, function, "length")
	assert.Contains(t, function, "estimated_length")
}
