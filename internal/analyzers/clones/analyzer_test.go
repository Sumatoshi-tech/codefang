package clones

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test helper constants.
const (
	testFuncNameA    = "funcA"
	testFuncNameB    = "funcB"
	testFuncNameC    = "funcC"
	testMinSimilType = 0.5
	testFloatDelta   = 0.001
)

// buildFunctionNode creates a function node with the given name and child types.
func buildFunctionNode(name string, childTypes []node.Type) *node.Node {
	fn := node.NewBuilder().
		WithType(node.UASTFunction).
		WithProps(map[string]string{"name": name}).
		WithRoles([]node.Role{node.RoleFunction, node.RoleDeclaration}).
		Build()

	children := make([]*node.Node, 0, len(childTypes))

	for _, ct := range childTypes {
		child := node.NewBuilder().WithType(ct).Build()
		children = append(children, child)
	}

	fn.Children = children

	return fn
}

// buildRootWithFunctions creates a root File node containing the given functions.
func buildRootWithFunctions(functions ...*node.Node) *node.Node {
	root := node.NewBuilder().WithType(node.UASTFile).Build()
	root.Children = functions

	return root
}

// identicalChildTypes returns a slice of node types representing a typical function body.
func identicalChildTypes() []node.Type {
	return []node.Type{
		node.UASTBlock, node.UASTAssignment, node.UASTIdentifier,
		node.UASTCall, node.UASTIdentifier, node.UASTReturn,
		node.UASTBinaryOp, node.UASTLiteral,
	}
}

// differentChildTypes returns a different set of node types.
func differentChildTypes() []node.Type {
	return []node.Type{
		node.UASTLoop, node.UASTIf, node.UASTSwitch,
		node.UASTCatch, node.UASTThrow, node.UASTTry,
		node.UASTBreak, node.UASTContinue,
	}
}

// TestAnalyzer_Name verifies the analyzer name.
func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Equal(t, analyzerName, a.Name())
}

// TestAnalyzer_Flag verifies the analyzer flag.
func TestAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Equal(t, analyzerFlag, a.Flag())
}

// TestAnalyzer_Descriptor verifies the analyzer descriptor.
func TestAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	desc := a.Descriptor()
	assert.Equal(t, analyzerID, desc.ID)
	assert.Equal(t, analyze.ModeStatic, desc.Mode)
}

// TestAnalyzer_Thresholds verifies threshold keys exist.
func TestAnalyzer_Thresholds(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	thresholds := a.Thresholds()
	assert.Contains(t, thresholds, "clone_ratio")
	assert.Contains(t, thresholds, "total_clone_pairs")
}

// TestAnalyzer_Configure verifies configure returns nil.
func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	err := a.Configure(nil)
	require.NoError(t, err)
}

// TestAnalyzer_ListConfigurationOptions verifies empty options.
func TestAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	opts := a.ListConfigurationOptions()
	assert.Empty(t, opts)
}

// TestAnalyzer_Analyze_NilRoot verifies nil root produces empty report.
func TestAnalyzer_Analyze_NilRoot(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report, err := a.Analyze(nil)
	require.NoError(t, err)
	assert.Equal(t, analyzerName, report[keyAnalyzerName])
	assert.Equal(t, 0, report[keyTotalFunctions])
}

// TestAnalyzer_Analyze_NoFunctions verifies empty root produces empty report.
func TestAnalyzer_Analyze_NoFunctions(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	root := node.NewBuilder().WithType(node.UASTFile).Build()
	report, err := a.Analyze(root)
	require.NoError(t, err)
	assert.Equal(t, 0, report[keyTotalFunctions])
}

// TestAnalyzer_Analyze_SingleFunction verifies single function produces no clones.
func TestAnalyzer_Analyze_SingleFunction(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fn := buildFunctionNode(testFuncNameA, identicalChildTypes())
	root := buildRootWithFunctions(fn)

	report, err := a.Analyze(root)
	require.NoError(t, err)
	assert.Equal(t, 1, report[keyTotalFunctions])
	assert.Equal(t, 0, report[keyTotalClonePairs])
}

// TestAnalyzer_Analyze_IdenticalFunctions verifies two identical functions are detected as clones.
func TestAnalyzer_Analyze_IdenticalFunctions(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fnB := buildFunctionNode(testFuncNameB, identicalChildTypes())
	root := buildRootWithFunctions(fnA, fnB)

	report, err := a.Analyze(root)
	require.NoError(t, err)
	assert.Equal(t, 2, report[keyTotalFunctions])

	pairsRaw, ok := report[keyClonePairs]
	require.True(t, ok)

	pairs, pairsOK := pairsRaw.([]map[string]any)
	require.True(t, pairsOK)
	require.NotEmpty(t, pairs)

	// Both functions have identical structure, so similarity should be very high.
	similarity, simOK := pairs[0]["similarity"].(float64)
	require.True(t, simOK)
	assert.GreaterOrEqual(t, similarity, testMinSimilType)
}

// TestAnalyzer_Analyze_DifferentFunctions verifies different functions are not clones.
func TestAnalyzer_Analyze_DifferentFunctions(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fnB := buildFunctionNode(testFuncNameB, differentChildTypes())
	root := buildRootWithFunctions(fnA, fnB)

	report, err := a.Analyze(root)
	require.NoError(t, err)
	assert.Equal(t, 2, report[keyTotalFunctions])
	assert.Equal(t, 0, report[keyTotalClonePairs])
}

// TestAnalyzer_Analyze_SmallFunction verifies functions with too few nodes are excluded.
func TestAnalyzer_Analyze_SmallFunction(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	// Function with only 2 child nodes (+ function node itself = 3, less than shingle size).
	fn := buildFunctionNode(testFuncNameA, []node.Type{node.UASTBlock, node.UASTReturn})
	root := buildRootWithFunctions(fn)

	report, err := a.Analyze(root)
	require.NoError(t, err)

	// Function found but no clones (too small for shingles or just one func).
	assert.Equal(t, 1, report[keyTotalFunctions])
	assert.Equal(t, 0, report[keyTotalClonePairs])
}

// TestAnalyzer_Analyze_ThreeFunctions verifies clone detection among three functions.
func TestAnalyzer_Analyze_ThreeFunctions(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fnB := buildFunctionNode(testFuncNameB, identicalChildTypes())
	fnC := buildFunctionNode(testFuncNameC, differentChildTypes())
	root := buildRootWithFunctions(fnA, fnB, fnC)

	report, err := a.Analyze(root)
	require.NoError(t, err)
	assert.Equal(t, 3, report[keyTotalFunctions])

	// Only fnA and fnB should be clones; fnC is different.
	totalPairs, ok := report[keyTotalClonePairs].(int)
	require.True(t, ok)
	assert.GreaterOrEqual(t, totalPairs, 1)
}

// TestAnalyzer_Analyze_CloneRatio verifies clone ratio computation.
func TestAnalyzer_Analyze_CloneRatio(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fnB := buildFunctionNode(testFuncNameB, identicalChildTypes())
	root := buildRootWithFunctions(fnA, fnB)

	report, err := a.Analyze(root)
	require.NoError(t, err)

	ratio, ok := report[keyCloneRatio].(float64)
	require.True(t, ok)
	assert.GreaterOrEqual(t, ratio, 0.0)
}

// TestAnalyzer_Analyze_Message verifies message is present.
func TestAnalyzer_Analyze_Message(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	root := buildRootWithFunctions(fnA)

	report, err := a.Analyze(root)
	require.NoError(t, err)

	msg, ok := report[keyMessage].(string)
	require.True(t, ok)
	assert.NotEmpty(t, msg)
}

// TestAnalyzer_FormatReportJSON verifies JSON formatting.
func TestAnalyzer_FormatReportJSON(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)

	var buf bytes.Buffer

	err := a.FormatReportJSON(report, &buf)
	require.NoError(t, err)

	var metrics ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &metrics)
	require.NoError(t, err)
	assert.Equal(t, msgNoClones, metrics.Message)
}

// TestAnalyzer_FormatReportYAML verifies YAML formatting.
func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)

	var buf bytes.Buffer

	err := a.FormatReportYAML(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "message")
}

// TestAnalyzer_FormatReport verifies text formatting.
func TestAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)

	var buf bytes.Buffer

	err := a.FormatReport(report, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.String())
}

// TestAnalyzer_FormatReportBinary verifies binary formatting.
func TestAnalyzer_FormatReportBinary(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)

	var buf bytes.Buffer

	err := a.FormatReportBinary(report, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.Bytes())
}

// TestAnalyzer_FormatReportPlot verifies plot formatting.
func TestAnalyzer_FormatReportPlot(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)

	var buf bytes.Buffer

	err := a.FormatReportPlot(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Clone")
}

// TestAnalyzer_CreateAggregator verifies aggregator creation.
func TestAnalyzer_CreateAggregator(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	agg := a.CreateAggregator()
	assert.NotNil(t, agg)
}

// TestAnalyzer_CreateVisitor verifies visitor creation.
func TestAnalyzer_CreateVisitor(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	v := a.CreateVisitor()
	assert.NotNil(t, v)
}

// TestAnalyzer_CreateReportSection verifies report section creation.
func TestAnalyzer_CreateReportSection(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := buildEmptyReport(msgNoClones)
	section := a.CreateReportSection(report)
	assert.NotNil(t, section)
	assert.Equal(t, sectionTitle, section.SectionTitle())
}

// TestShingler_ExtractShingles_NilNode verifies nil input.
func TestShingler_ExtractShingles_NilNode(t *testing.T) {
	t.Parallel()

	s := NewShingler(defaultShingleSize)
	shingles := s.ExtractShingles(nil)
	assert.Nil(t, shingles)
}

// TestShingler_ExtractShingles_TooFewNodes verifies small trees.
func TestShingler_ExtractShingles_TooFewNodes(t *testing.T) {
	t.Parallel()

	s := NewShingler(defaultShingleSize)
	n := node.NewBuilder().WithType(node.UASTFunction).Build()
	n.Children = []*node.Node{
		node.NewBuilder().WithType(node.UASTBlock).Build(),
	}

	shingles := s.ExtractShingles(n)
	assert.Nil(t, shingles)
}

// TestShingler_ExtractShingles_Valid verifies shingle extraction from valid tree.
func TestShingler_ExtractShingles_Valid(t *testing.T) {
	t.Parallel()

	s := NewShingler(defaultShingleSize)
	fn := buildFunctionNode(testFuncNameA, identicalChildTypes())

	shingles := s.ExtractShingles(fn)
	require.NotNil(t, shingles)

	// Function node itself + 8 children = 9 nodes.
	// With k=5: 9 - 5 + 1 = 5 shingles.
	assert.Len(t, shingles, defaultShingleSize)
}

// TestShingler_ExtractShingles_Deterministic verifies same tree produces same shingles.
func TestShingler_ExtractShingles_Deterministic(t *testing.T) {
	t.Parallel()

	s := NewShingler(defaultShingleSize)
	fn1 := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fn2 := buildFunctionNode(testFuncNameB, identicalChildTypes())

	shingles1 := s.ExtractShingles(fn1)
	shingles2 := s.ExtractShingles(fn2)

	require.Len(t, shingles1, len(shingles2))

	for i := range shingles1 {
		assert.Equal(t, shingles1[i], shingles2[i])
	}
}

// TestClassifyCloneType verifies clone type classification.
func TestClassifyCloneType(t *testing.T) {
	t.Parallel()

	assert.Equal(t, CloneType1, classifyCloneType(1.0))
	assert.Equal(t, CloneType2, classifyCloneType(0.9))
	assert.Equal(t, CloneType2, classifyCloneType(0.8))
	assert.Equal(t, CloneType3, classifyCloneType(0.7))
	assert.Equal(t, CloneType3, classifyCloneType(0.5))
}

// TestClonePairKey verifies canonical key generation.
func TestClonePairKey(t *testing.T) {
	t.Parallel()

	key1 := clonePairKey("a", "b")
	key2 := clonePairKey("b", "a")
	assert.Equal(t, key1, key2)
}

// TestComputeCloneRatio verifies ratio computation.
func TestComputeCloneRatio(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, 0.0, computeCloneRatio(0, 0), testFloatDelta)
	assert.InDelta(t, 0.0, computeCloneRatio(0, 10), testFloatDelta)
	assert.InDelta(t, 0.5, computeCloneRatio(5, 10), testFloatDelta)
}

// TestCloneMessage verifies message selection.
func TestCloneMessage(t *testing.T) {
	t.Parallel()

	assert.Equal(t, msgNoClones, cloneMessage(0))
	assert.Equal(t, msgLowClones, cloneMessage(3))
	assert.Equal(t, msgModClones, cloneMessage(10))
	assert.Equal(t, msgHighClones, cloneMessage(20))
}

// TestComputeMetricsFromReport verifies metrics extraction.
func TestComputeMetricsFromReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		keyTotalFunctions:  10,
		keyTotalClonePairs: 3,
		keyCloneRatio:      0.3,
		keyMessage:         msgLowClones,
	}

	metrics := computeMetricsFromReport(report)
	assert.Equal(t, 10, metrics.TotalFunctions)
	assert.Equal(t, 3, metrics.TotalClonePairs)
	assert.InDelta(t, 0.3, metrics.CloneRatio, testFloatDelta)
	assert.Equal(t, msgLowClones, metrics.Message)
}

// TestReportSection_KeyMetrics verifies key metrics display.
func TestReportSection_KeyMetrics(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		keyTotalFunctions:  10,
		keyTotalClonePairs: 2,
		keyCloneRatio:      0.2,
		keyMessage:         msgLowClones,
	}

	section := NewReportSection(report)
	metrics := section.KeyMetrics()
	assert.Len(t, metrics, 3)
}

// TestReportSection_Distribution verifies distribution data.
func TestReportSection_Distribution(t *testing.T) {
	t.Parallel()

	pairs := []ClonePair{
		{FuncA: "a", FuncB: "b", Similarity: 1.0, CloneType: CloneType1},
		{FuncA: "c", FuncB: "d", Similarity: 0.9, CloneType: CloneType2},
		{FuncA: "e", FuncB: "f", Similarity: 0.6, CloneType: CloneType3},
	}

	report := analyze.Report{
		keyClonePairs: pairs,
		keyMessage:    msgLowClones,
	}

	section := NewReportSection(report)
	dist := section.Distribution()
	require.Len(t, dist, 3)
	assert.Equal(t, distLabelType1, dist[0].Label)
	assert.Equal(t, 1, dist[0].Count)
}

// TestReportSection_TopIssues verifies top issues display.
func TestReportSection_TopIssues(t *testing.T) {
	t.Parallel()

	pairs := []ClonePair{
		{FuncA: "a", FuncB: "b", Similarity: 1.0, CloneType: CloneType1},
		{FuncA: "c", FuncB: "d", Similarity: 0.6, CloneType: CloneType3},
	}

	report := analyze.Report{
		keyClonePairs: pairs,
		keyMessage:    msgLowClones,
	}

	section := NewReportSection(report)
	issues := section.TopIssues(1)
	require.Len(t, issues, 1)
	assert.Equal(t, analyze.SeverityPoor, issues[0].Severity)
}

// TestReportSection_AllIssues verifies all issues.
func TestReportSection_AllIssues(t *testing.T) {
	t.Parallel()

	pairs := []ClonePair{
		{FuncA: "a", FuncB: "b", Similarity: 1.0, CloneType: CloneType1},
	}

	report := analyze.Report{
		keyClonePairs: pairs,
		keyMessage:    msgLowClones,
	}

	section := NewReportSection(report)
	issues := section.AllIssues()
	require.Len(t, issues, 1)
}

// TestReportSection_EmptyDistribution verifies empty distribution.
func TestReportSection_EmptyDistribution(t *testing.T) {
	t.Parallel()

	report := buildEmptyReport(msgNoClones)
	section := NewReportSection(report)
	dist := section.Distribution()
	assert.Nil(t, dist)
}

// TestReportSection_Score verifies score computation.
func TestReportSection_Score(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		keyCloneRatio: 0.3,
		keyMessage:    msgModClones,
	}

	section := NewReportSection(report)
	assert.InDelta(t, 0.7, section.Score(), testFloatDelta)
}

// TestVisitor_GetReport_NoFunctions verifies empty visitor report.
func TestVisitor_GetReport_NoFunctions(t *testing.T) {
	t.Parallel()

	v := NewVisitor()
	report := v.GetReport()
	assert.Equal(t, 0, report[keyTotalFunctions])
}

// TestVisitor_GetReport_WithFunctions verifies visitor collects functions.
func TestVisitor_GetReport_WithFunctions(t *testing.T) {
	t.Parallel()

	v := NewVisitor()
	fnA := buildFunctionNode(testFuncNameA, identicalChildTypes())
	fnB := buildFunctionNode(testFuncNameB, identicalChildTypes())

	// Simulate traversal.
	v.OnEnter(fnA, 0)

	for _, child := range fnA.Children {
		v.OnEnter(child, 1)
	}

	v.OnEnter(fnB, 0)

	for _, child := range fnB.Children {
		v.OnEnter(child, 1)
	}

	report := v.GetReport()
	assert.Equal(t, 2, report[keyTotalFunctions])

	totalPairs, ok := report[keyTotalClonePairs].(int)
	require.True(t, ok)
	assert.GreaterOrEqual(t, totalPairs, 1)
}

// TestVisitor_OnExit_NoOp verifies OnExit does nothing.
func TestVisitor_OnExit_NoOp(t *testing.T) {
	t.Parallel()

	v := NewVisitor()
	n := node.NewBuilder().WithType(node.UASTFunction).Build()
	v.OnExit(n, 0)

	// No panic and no state change.
	assert.Empty(t, v.functions)
}

// TestAggregator_Aggregate verifies aggregation of multiple reports.
func TestAggregator_Aggregate(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()

	reports := map[string]analyze.Report{
		"file1": {
			keyTotalFunctions:  5,
			keyTotalClonePairs: 2,
			keyCloneRatio:      0.4,
			keyMessage:         msgLowClones,
		},
		"file2": {
			keyTotalFunctions:  3,
			keyTotalClonePairs: 1,
			keyCloneRatio:      0.33,
			keyMessage:         msgLowClones,
		},
	}

	agg.Aggregate(reports)
	result := agg.GetResult()
	assert.NotNil(t, result)
}

// TestAggregator_EmptyResult verifies empty aggregation.
func TestAggregator_EmptyResult(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()
	agg.Aggregate(map[string]analyze.Report{})
	result := agg.GetResult()
	assert.NotNil(t, result)
}

// TestExtractClonePairs_FromTyped verifies typed clone pair extraction.
func TestExtractClonePairs_FromTyped(t *testing.T) {
	t.Parallel()

	pairs := []ClonePair{
		{FuncA: "a", FuncB: "b", Similarity: 1.0, CloneType: CloneType1},
	}

	report := analyze.Report{keyClonePairs: pairs}

	extracted := extractClonePairs(report)
	require.Len(t, extracted, 1)
	assert.Equal(t, "a", extracted[0].FuncA)
}

// TestExtractClonePairs_FromMaps verifies map-based clone pair extraction.
func TestExtractClonePairs_FromMaps(t *testing.T) {
	t.Parallel()

	raw := []any{
		map[string]any{
			"func_a":     "x",
			"func_b":     "y",
			"similarity": 0.9,
			"clone_type": CloneType2,
		},
	}

	report := analyze.Report{keyClonePairs: raw}

	extracted := extractClonePairs(report)
	require.Len(t, extracted, 1)
	assert.Equal(t, "x", extracted[0].FuncA)
	assert.InDelta(t, 0.9, extracted[0].Similarity, testFloatDelta)
}

// TestExtractClonePairs_Missing verifies missing key.
func TestExtractClonePairs_Missing(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	extracted := extractClonePairs(report)
	assert.Nil(t, extracted)
}

// TestIsFunctionNode verifies function node detection.
func TestIsFunctionNode(t *testing.T) {
	t.Parallel()

	assert.False(t, isFunctionNode(nil))

	fn := node.NewBuilder().WithType(node.UASTFunction).Build()
	assert.True(t, isFunctionNode(fn))

	method := node.NewBuilder().WithType(node.UASTMethod).Build()
	assert.True(t, isFunctionNode(method))

	block := node.NewBuilder().WithType(node.UASTBlock).Build()
	assert.False(t, isFunctionNode(block))

	roleFunc := node.NewBuilder().
		WithType("CustomType").
		WithRoles([]node.Role{node.RoleFunction, node.RoleDeclaration}).
		Build()
	assert.True(t, isFunctionNode(roleFunc))
}

// TestExtractFuncName verifies function name extraction.
func TestExtractFuncName(t *testing.T) {
	t.Parallel()

	fn := node.NewBuilder().
		WithType(node.UASTFunction).
		WithProps(map[string]string{"name": "myFunc"}).
		Build()
	assert.Equal(t, "myFunc", extractFuncName(fn))

	fn2 := node.NewBuilder().
		WithType(node.UASTFunction).
		WithToken("tokenFunc").
		Build()
	assert.Equal(t, "tokenFunc", extractFuncName(fn2))

	fn3 := node.NewBuilder().
		WithType(node.UASTFunction).
		Build()
	assert.Equal(t, string(node.UASTFunction), extractFuncName(fn3))
}

// TestComputeScore verifies score computation from clone ratio.
func TestComputeScore(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, 1.0, computeScore(0.0), testFloatDelta)
	assert.InDelta(t, 0.7, computeScore(0.3), testFloatDelta)
	assert.InDelta(t, 0.0, computeScore(1.5), testFloatDelta)
}

// TestRegisterPlotSections verifies plot section registration doesn't panic.
func TestRegisterPlotSections(t *testing.T) {
	t.Parallel()

	assert.NotPanics(t, func() {
		RegisterPlotSections()
	})
}

// TestCategorizeClonePairs verifies clone pair categorization.
func TestCategorizeClonePairs(t *testing.T) {
	t.Parallel()

	pairs := []ClonePair{
		{CloneType: CloneType1},
		{CloneType: CloneType1},
		{CloneType: CloneType2},
		{CloneType: CloneType3},
	}

	counts := categorizeClonePairs(pairs)
	assert.Equal(t, 2, counts.type1)
	assert.Equal(t, 1, counts.type2)
	assert.Equal(t, 1, counts.type3)
}

// TestJoinTypes verifies type joining.
func TestJoinTypes(t *testing.T) {
	t.Parallel()

	assert.Empty(t, joinTypes(nil))
	assert.Equal(t, "A", joinTypes([]string{"A"}))
	assert.Equal(t, "A|B|C", joinTypes([]string{"A", "B", "C"}))
}

// TestCollectNodeTypes verifies node type collection.
func TestCollectNodeTypes(t *testing.T) {
	t.Parallel()

	assert.Nil(t, collectNodeTypes(nil))

	root := node.NewBuilder().WithType(node.UASTFunction).Build()
	root.Children = []*node.Node{
		node.NewBuilder().WithType(node.UASTBlock).Build(),
		node.NewBuilder().WithType(node.UASTReturn).Build(),
	}

	types := collectNodeTypes(root)
	assert.Equal(t, []string{"Function", "Block", "Return"}, types)
}
