package halstead

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/cms"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test constants for CMS Halstead integration tests.
const (
	// cmsTestSmallTokens is the number of tokens for a small function (below threshold).
	cmsTestSmallTokens = 50

	// cmsTestLargeTokens is the number of tokens for a large function (above threshold).
	cmsTestLargeTokens = 2000

	// cmsTestOperatorPrefix is the prefix for generated operator names.
	cmsTestOperatorPrefix = "op"

	// cmsTestOperandPrefix is the prefix for generated operand names.
	cmsTestOperandPrefix = "var"

	// cmsTestFuncName is the function name used in CMS tests.
	cmsTestFuncName = "cmsTestFunction"

	// cmsTestBenchTokens is the number of tokens for benchmark tests.
	cmsTestBenchTokens = 10000

	// cmsTestDistinctOps is the number of distinct operators in generated ASTs.
	cmsTestDistinctOps = 10

	// cmsTestDistinctOpnds is the number of distinct operands in generated ASTs.
	cmsTestDistinctOpnds = 20
)

// --- CMS Constants Validation Tests ---.

func TestCMSConstants_Valid(t *testing.T) {
	t.Parallel()

	// Verify that CMS constants produce a valid sketch.
	sketch, err := cms.New(cmsEpsilon, cmsDelta)

	require.NoError(t, err, "CMS constants must produce valid sketch")
	require.NotNil(t, sketch)
	assert.Positive(t, sketch.Width())
	assert.Positive(t, sketch.Depth())
}

func TestCMSConstants_ThresholdPositive(t *testing.T) {
	t.Parallel()

	assert.Positive(t, cmsTokenThreshold, "CMS token threshold must be positive")
}

// --- Visitor CMS Integration Tests ---.

func TestVisitor_CMSSketchPopulated_LargeFunction(t *testing.T) {
	t.Parallel()

	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestLargeTokens)

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	// Retrieve function metrics.
	funcMetrics, ok := visitor.functionMetrics[cmsTestFuncName]

	require.True(t, ok, "function metrics must exist")
	require.NotNil(t, funcMetrics.OperatorSketch, "OperatorSketch should be populated for large function")
	require.NotNil(t, funcMetrics.OperandSketch, "OperandSketch should be populated for large function")

	// CMS TotalCount should match the total operators/operands.
	assert.Positive(t, funcMetrics.OperatorSketch.TotalCount())
	assert.Positive(t, funcMetrics.OperandSketch.TotalCount())
}

func TestVisitor_CMSNotUsed_SmallFunction(t *testing.T) {
	t.Parallel()

	root := buildSmallFunctionAST(cmsTestFuncName)

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	funcMetrics, ok := visitor.functionMetrics[cmsTestFuncName]

	require.True(t, ok, "function metrics must exist")

	// Small function should not use CMS (sketches nil).
	assert.Nil(t, funcMetrics.OperatorSketch, "OperatorSketch should be nil for small function")
	assert.Nil(t, funcMetrics.OperandSketch, "OperandSketch should be nil for small function")
}

func TestVisitor_CMSTotalMatchesExact(t *testing.T) {
	t.Parallel()

	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestLargeTokens)

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	funcMetrics, ok := visitor.functionMetrics[cmsTestFuncName]

	require.True(t, ok, "function metrics must exist")

	// CMS TotalCount() is exact (simple int64 counter), so it must match SumMap.
	exactTotalOps := sumMapHelper(funcMetrics.Operators)
	exactTotalOpnds := sumMapHelper(funcMetrics.Operands)

	assert.Equal(t, int64(exactTotalOps), funcMetrics.OperatorSketch.TotalCount(),
		"CMS operator total must match exact SumMap")
	assert.Equal(t, int64(exactTotalOpnds), funcMetrics.OperandSketch.TotalCount(),
		"CMS operand total must match exact SumMap")
}

func TestVisitor_EstimatedFields_Populated(t *testing.T) {
	t.Parallel()

	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestLargeTokens)

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	funcMetrics, ok := visitor.functionMetrics[cmsTestFuncName]

	require.True(t, ok, "function metrics must exist")
	assert.Positive(t, funcMetrics.EstimatedTotalOperators,
		"EstimatedTotalOperators must be populated for large function")
	assert.Positive(t, funcMetrics.EstimatedTotalOperands,
		"EstimatedTotalOperands must be populated for large function")
}

func TestVisitor_DerivedMetrics_CMSPath(t *testing.T) {
	t.Parallel()

	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestLargeTokens)

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	funcMetrics, ok := visitor.functionMetrics[cmsTestFuncName]

	require.True(t, ok, "function metrics must exist")

	// Derived metrics should be positive for a non-empty function.
	assert.Greater(t, funcMetrics.Volume, 0.0, "Volume must be positive")
	assert.Greater(t, funcMetrics.Difficulty, 0.0, "Difficulty must be positive")
	assert.Greater(t, funcMetrics.Effort, 0.0, "Effort must be positive")
	assert.Greater(t, funcMetrics.TimeToProgram, 0.0, "TimeToProgram must be positive")
	assert.Greater(t, funcMetrics.DeliveredBugs, 0.0, "DeliveredBugs must be positive")

	// Distinct counts must come from maps (exact).
	assert.Equal(t, len(funcMetrics.Operators), funcMetrics.DistinctOperators)
	assert.Equal(t, len(funcMetrics.Operands), funcMetrics.DistinctOperands)
}

// --- Direct Analyzer Path CMS Tests ---.

func TestAnalyzer_CMSIntegration_LargeAST(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestLargeTokens)

	result, err := analyzer.Analyze(root)

	require.NoError(t, err)
	require.NotNil(t, result)

	// File-level totals should be positive.
	totalOps, ok := result["total_operators"].(int)
	require.True(t, ok, "total_operators should be int")
	assert.Positive(t, totalOps)

	totalOpnds, ok := result["total_operands"].(int)
	require.True(t, ok, "total_operands should be int")
	assert.Positive(t, totalOpnds)

	// Estimated fields should be present at file level.
	estOps, ok := result["estimated_total_operators"].(int64)
	require.True(t, ok, "estimated_total_operators should be int64")
	assert.Positive(t, estOps)

	estOpnds, ok := result["estimated_total_operands"].(int64)
	require.True(t, ok, "estimated_total_operands should be int64")
	assert.Positive(t, estOpnds)
}

func TestFileLevelMetrics_WithCMS(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	root := &node.Node{Type: node.UASTFile}

	// Add two large functions.
	fn1 := buildLargeFunctionAST("func1", cmsTestLargeTokens)
	fn2 := buildLargeFunctionAST("func2", cmsTestLargeTokens)

	// Extract function nodes from the roots.
	for _, child := range fn1.Children {
		root.AddChild(child)
	}

	for _, child := range fn2.Children {
		root.AddChild(child)
	}

	result, err := analyzer.Analyze(root)

	require.NoError(t, err)
	require.NotNil(t, result)

	// File-level should aggregate both functions.
	functions, ok := result["functions"].([]map[string]any)
	require.True(t, ok, "functions should be []map[string]any")
	assert.Len(t, functions, 2)

	// Volume should be positive (aggregated from both functions).
	volume, ok := result["volume"].(float64)
	require.True(t, ok, "volume should be float64")
	assert.Greater(t, volume, 0.0)
}

// --- Benchmarks ---.

func BenchmarkHalstead_CMS_LargeFunction(b *testing.B) {
	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestBenchTokens)

	b.ResetTimer()

	for range b.N {
		visitor := NewVisitor()
		traverser := analyze.NewMultiAnalyzerTraverser()
		traverser.RegisterVisitor(visitor)
		traverser.Traverse(root)
	}
}

func BenchmarkHalstead_Exact_LargeFunction(b *testing.B) {
	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestSmallTokens)

	b.ResetTimer()

	for range b.N {
		visitor := NewVisitor()
		traverser := analyze.NewMultiAnalyzerTraverser()
		traverser.RegisterVisitor(visitor)
		traverser.Traverse(root)
	}
}

func BenchmarkHalstead_Memory_CMS(b *testing.B) {
	root := buildLargeFunctionAST(cmsTestFuncName, cmsTestBenchTokens)

	b.ResetTimer()
	b.ReportAllocs()

	for range b.N {
		visitor := NewVisitor()
		traverser := analyze.NewMultiAnalyzerTraverser()
		traverser.RegisterVisitor(visitor)
		traverser.Traverse(root)
	}
}

// --- Helpers ---.

// buildLargeFunctionAST creates a function AST node with the specified number of
// operator+operand tokens, using a rotating set of distinct names.
func buildLargeFunctionAST(funcName string, totalTokens int) *node.Node {
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	nameNode := node.NewNodeWithToken(node.UASTIdentifier, funcName)
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Generate alternating operator/operand pairs.
	for i := range totalTokens {
		if i%2 == 0 {
			// Operator: rotate through distinct operators.
			opIdx := i % cmsTestDistinctOps
			opName := fmt.Sprintf("%s%d", cmsTestOperatorPrefix, opIdx)

			opNode := &node.Node{Type: node.UASTBinaryOp}
			opNode.Props = map[string]string{"operator": opName}
			opNode.Roles = []node.Role{node.RoleOperator}
			functionNode.AddChild(opNode)
		} else {
			// Operand: rotate through distinct operands.
			opndIdx := i % cmsTestDistinctOpnds
			opndName := fmt.Sprintf("%s%d", cmsTestOperandPrefix, opndIdx)

			opndNode := &node.Node{Type: node.UASTIdentifier, Token: opndName}
			opndNode.Roles = []node.Role{node.RoleVariable}
			functionNode.AddChild(opndNode)
		}
	}

	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	return root
}

// buildSmallFunctionAST creates a function AST with a small number of tokens (below CMS threshold).
func buildSmallFunctionAST(funcName string) *node.Node {
	return buildLargeFunctionAST(funcName, cmsTestSmallTokens)
}

// sumMapHelper sums all values in an integer map (test helper).
func sumMapHelper(m map[string]int) int {
	sum := 0
	for _, count := range m {
		sum += count
	}

	return sum
}
