package comments

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/assert/yaml"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const (
	testGoodComment  = "// This is a good comment"
	testFunctionName = "testFunction"
)

func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	assert.Equal(t, "comments", analyzer.Name())
}

func TestAnalyzer_Thresholds(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	thresholds := analyzer.Thresholds()

	assert.NotNil(t, thresholds)
	assert.Contains(t, thresholds, "overall_score")
	assert.Contains(t, thresholds, "good_comments_ratio")
	assert.Contains(t, thresholds, "documentation_coverage")

	overallScore := thresholds["overall_score"]
	assert.InDelta(t, 0.8, overallScore["green"], 0.001)
	assert.InDelta(t, 0.6, overallScore["yellow"], 0.001)
	assert.InDelta(t, 0.4, overallScore["red"], 0.001)
}

func TestAnalyzer_CreateAggregator(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	aggregator := analyzer.CreateAggregator()

	assert.NotNil(t, aggregator)
	assert.IsType(t, &Aggregator{}, aggregator)
}

func TestAnalyzer_DefaultConfig(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	config := analyzer.DefaultConfig()

	assert.InDelta(t, 1.0, config.RewardScore, 0.001)
	assert.Equal(t, 500, config.MaxCommentLength)
	assert.NotNil(t, config.PenaltyScores)

	// Check penalty scores for different node types.
	assert.InDelta(t, -0.5, config.PenaltyScores[node.UASTFunction], 0.001)
	assert.InDelta(t, -0.5, config.PenaltyScores[node.UASTMethod], 0.001)
	assert.InDelta(t, -0.3, config.PenaltyScores[node.UASTClass], 0.001)
	assert.InDelta(t, -0.1, config.PenaltyScores[node.UASTVariable], 0.001)
}

func TestAnalyzer_Analyze_EmptyTree(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	root := &node.Node{Type: node.UASTFile}

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 0, result["total_comments"])
	assert.Equal(t, 0, result["good_comments"])
	assert.Equal(t, 0, result["bad_comments"])
	assert.InDelta(t, 0.0, result["overall_score"], 0.001)
	assert.Equal(t, 0, result["total_functions"])
	assert.Equal(t, 0, result["documented_functions"])
}

func TestAnalyzer_Analyze_GoodCommentPlacement(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with a good comment above a function.
	root := &node.Node{Type: node.UASTFile}

	// Add a comment.
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = testGoodComment
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(comment)

	// Add a function.
	function := &node.Node{Type: node.UASTFunction}
	function.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	// Add function name.
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = testFunctionName
	name.Roles = []node.Role{node.RoleName}
	function.AddChild(name)

	root.AddChild(function)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 1, result["total_comments"])
	assert.Equal(t, 1, result["good_comments"])
	assert.Equal(t, 0, result["bad_comments"])
	assert.InDelta(t, 1.0, result["overall_score"], 0.001)
	assert.Equal(t, 1, result["total_functions"])
	assert.Equal(t, 1, result["documented_functions"])

	// Check if line numbers are included in comment details.
	commentDetails, ok := result["comment_details"].([]map[string]any)
	assert.True(t, ok)
	assert.Len(t, commentDetails, 1)

	detail := commentDetails[0]
	lineNumber, exists := detail["line_number"]
	assert.True(t, exists, "line_number field should exist")
	assert.Equal(t, 1, lineNumber, "line_number should be 1")
}

func TestAnalyzer_Analyze_BadCommentPlacement(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with a bad comment (inside function body).
	root := &node.Node{Type: node.UASTFile}

	// Add a function.
	function := &node.Node{Type: node.UASTFunction}
	function.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   5,
	}

	// Add function name.
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = testFunctionName
	name.Roles = []node.Role{node.RoleName}
	function.AddChild(name)

	// Add function body.
	body := &node.Node{Type: node.UASTBlock}
	body.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	// Add a comment inside the function body (bad placement).
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = "// This is a bad comment"
	comment.Pos = &node.Positions{
		StartLine: 3,
		EndLine:   3,
	}
	body.AddChild(comment)

	function.AddChild(body)
	root.AddChild(function)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 1, result["total_comments"])
	assert.Equal(t, 0, result["good_comments"])
	assert.Equal(t, 1, result["bad_comments"])
	assert.InDelta(t, 0.0, result["overall_score"], 0.001)
	assert.Equal(t, 1, result["total_functions"])
	assert.Equal(t, 0, result["documented_functions"])
}

func TestAnalyzer_Analyze_MixedCommentPlacement(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with both good and bad comments.
	root := &node.Node{Type: node.UASTFile}

	// Add a good comment above first function.
	goodComment := &node.Node{Type: node.UASTComment}
	goodComment.Token = "// Good comment above function"
	goodComment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(goodComment)

	// Add first function.
	func1 := &node.Node{Type: node.UASTFunction}
	func1.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	name1 := &node.Node{Type: node.UASTIdentifier}
	name1.Token = "function1"
	name1.Roles = []node.Role{node.RoleName}
	func1.AddChild(name1)
	root.AddChild(func1)

	// Add second function without comment.
	func2 := &node.Node{Type: node.UASTFunction}
	func2.Pos = &node.Positions{
		StartLine: 6,
		EndLine:   8,
	}

	name2 := &node.Node{Type: node.UASTIdentifier}
	name2.Token = "function2"
	name2.Roles = []node.Role{node.RoleName}
	func2.AddChild(name2)
	root.AddChild(func2)

	// Add a bad comment after second function.
	badComment := &node.Node{Type: node.UASTComment}
	badComment.Token = "// Bad comment after function"
	badComment.Pos = &node.Positions{
		StartLine: 9,
		EndLine:   9,
	}
	root.AddChild(badComment)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 2, result["total_comments"])
	assert.Equal(t, 1, result["good_comments"])
	assert.Equal(t, 1, result["bad_comments"])
	assert.InDelta(t, 0.5, result["overall_score"], 0.001)
	assert.Equal(t, 2, result["total_functions"])
	assert.Equal(t, 1, result["documented_functions"])
}

func TestAnalyzer_Analyze_ClassWithMethod(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with a class and method.
	root := &node.Node{Type: node.UASTFile}

	// Add a good comment above class.
	classComment := &node.Node{Type: node.UASTComment}
	classComment.Token = "// This is a class"
	classComment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(classComment)

	// Add a class.
	class := &node.Node{Type: node.UASTClass}
	class.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   8,
	}

	className := &node.Node{Type: node.UASTIdentifier}
	className.Token = "TestClass"
	className.Roles = []node.Role{node.RoleName}
	class.AddChild(className)

	// Add a good comment above method.
	methodComment := &node.Node{Type: node.UASTComment}
	methodComment.Token = "// This is a method"
	methodComment.Pos = &node.Positions{
		StartLine: 4,
		EndLine:   4,
	}
	class.AddChild(methodComment)

	// Add a method.
	method := &node.Node{Type: node.UASTMethod}
	method.Pos = &node.Positions{
		StartLine: 5,
		EndLine:   7,
	}

	methodName := &node.Node{Type: node.UASTIdentifier}
	methodName.Token = "testMethod"
	methodName.Roles = []node.Role{node.RoleName}
	method.AddChild(methodName)

	class.AddChild(method)
	root.AddChild(class)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 2, result["total_comments"])
	assert.Equal(t, 2, result["good_comments"])
	assert.Equal(t, 0, result["bad_comments"])
	assert.InDelta(t, 1.0, result["overall_score"], 0.001)
	assert.Equal(t, 2, result["total_functions"]) // Class + method.
	assert.Equal(t, 2, result["documented_functions"])
}

func TestAnalyzer_Analyze_UnassociatedComment(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with an unassociated comment.
	root := &node.Node{Type: node.UASTFile}

	// Add a comment without any function/class.
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = "// This comment is not associated with anything"
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(comment)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	assert.Equal(t, 1, result["total_comments"])
	assert.Equal(t, 0, result["good_comments"])
	assert.Equal(t, 1, result["bad_comments"])
	assert.InDelta(t, 0.0, result["overall_score"], 0.001)
	assert.Equal(t, 0, result["total_functions"])
	assert.Equal(t, 0, result["documented_functions"])
}

func TestAnalyzer_FindComments(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with comments at different levels.
	root := &node.Node{Type: node.UASTFile}

	// Add comment at root level.
	comment1 := &node.Node{Type: node.UASTComment}
	comment1.Token = "// Root comment"
	root.AddChild(comment1)

	// Add a function with comment.
	function := &node.Node{Type: node.UASTFunction}
	comment2 := &node.Node{Type: node.UASTComment}
	comment2.Token = "// Function comment"
	function.AddChild(comment2)
	root.AddChild(function)

	comments := analyzer.findComments(root)
	assert.Len(t, comments, 2)
	assert.Equal(t, "// Root comment", comments[0].Token)
	assert.Equal(t, "// Function comment", comments[1].Token)
}

func TestAnalyzer_FindFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with different function types.
	root := &node.Node{Type: node.UASTFile}

	// Add a function.
	function := &node.Node{Type: node.UASTFunction}
	root.AddChild(function)

	// Add a method.
	method := &node.Node{Type: node.UASTMethod}
	root.AddChild(method)

	// Add a class.
	class := &node.Node{Type: node.UASTClass}
	root.AddChild(class)

	// Add a variable (should not be included).
	variable := &node.Node{Type: node.UASTVariable}
	root.AddChild(variable)

	functions := analyzer.findFunctions(root)
	assert.Len(t, functions, 3) // Function, method, class.
	assert.Equal(t, string(node.UASTFunction), string(functions[0].Type))
	assert.Equal(t, string(node.UASTMethod), string(functions[1].Type))
	assert.Equal(t, string(node.UASTClass), string(functions[2].Type))
}

func TestAnalyzer_ExtractTargetName(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test with Name role.
	function := &node.Node{Type: node.UASTFunction}
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = testFunctionName
	name.Roles = []node.Role{node.RoleName}
	function.AddChild(name)

	result := analyzer.extractTargetName(function)
	assert.Equal(t, testFunctionName, result)

	// Test with props.
	function2 := &node.Node{Type: node.UASTFunction}
	function2.Props = map[string]string{"name": "functionFromProps"}

	result2 := analyzer.extractTargetName(function2)
	assert.Equal(t, "functionFromProps", result2)

	// Test fallback to first identifier.
	function3 := &node.Node{Type: node.UASTFunction}
	identifier := &node.Node{Type: node.UASTIdentifier}
	identifier.Token = "fallbackName"
	function3.AddChild(identifier)

	result3 := analyzer.extractTargetName(function3)
	assert.Equal(t, "fallbackName", result3)

	// Test unknown case.
	function4 := &node.Node{Type: node.UASTFunction}
	result4 := analyzer.extractTargetName(function4)
	assert.Equal(t, "unknown", result4)
}

func TestAnalyzer_IsCommentProperlyPlaced(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Test properly placed comment (directly above).
	comment := &node.Node{Type: node.UASTComment}
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}

	target := &node.Node{Type: node.UASTFunction}
	target.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	assert.True(t, analyzer.isCommentProperlyPlaced(comment, target))

	// Test comment with gap.
	comment2 := &node.Node{Type: node.UASTComment}
	comment2.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}

	target2 := &node.Node{Type: node.UASTFunction}
	target2.Pos = &node.Positions{
		StartLine: 4,
		EndLine:   6,
	}

	assert.False(t, analyzer.isCommentProperlyPlaced(comment2, target2))

	// Test comment below target.
	comment3 := &node.Node{Type: node.UASTComment}
	comment3.Pos = &node.Positions{
		StartLine: 3,
		EndLine:   3,
	}

	target3 := &node.Node{Type: node.UASTFunction}
	target3.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   2,
	}

	assert.False(t, analyzer.isCommentProperlyPlaced(comment3, target3))

	// Test with missing position info.
	comment4 := &node.Node{Type: node.UASTComment}
	target4 := &node.Node{Type: node.UASTFunction}

	assert.False(t, analyzer.isCommentProperlyPlaced(comment4, target4))
}

func TestAggregator_Aggregate(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	// Test aggregation.
	results := map[string]map[string]any{
		"file1": {
			"total_comments":       2,
			"good_comments":        1,
			"bad_comments":         1,
			"total_functions":      3,
			"documented_functions": 1,
			"overall_score":        0.5, // 1 good out of 2 total.
		},
		"file2": {
			"total_comments":       1,
			"good_comments":        1,
			"bad_comments":         0,
			"total_functions":      2,
			"documented_functions": 1,
			"overall_score":        1.0, // 1 good out of 1 total.
		},
	}

	aggregator.Aggregate(results)

	result := aggregator.GetResult()
	assert.Equal(t, 3, result["total_comments"])
	assert.Equal(t, 2, result["good_comments"])
	assert.Equal(t, 1, result["bad_comments"])
	assert.Equal(t, 5, result["total_functions"])
	assert.Equal(t, 2, result["documented_functions"])
	assert.InDelta(t, 0.75, result["overall_score"], 0.001) // Average of 0.5 and 1.0.
}

func TestAggregator_GetResult_Empty(t *testing.T) {
	t.Parallel()

	aggregator := NewAggregator()

	result := aggregator.GetResult()
	assert.Equal(t, 0, result["total_comments"])
	assert.Equal(t, 0, result["good_comments"])
	assert.Equal(t, 0, result["bad_comments"])
	assert.InDelta(t, 0.0, result["overall_score"], 0.001)
	assert.Equal(t, 0, result["total_functions"])
	assert.Equal(t, 0, result["documented_functions"])
}

func TestAnalyzer_DebugOutput(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with a good comment above a function.
	root := &node.Node{Type: node.UASTFile}

	// Add a comment.
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = testGoodComment
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(comment)

	// Add a function.
	function := &node.Node{Type: node.UASTFunction}
	function.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	// Add function name.
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = testFunctionName
	name.Roles = []node.Role{node.RoleName}
	function.AddChild(name)

	root.AddChild(function)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	// Print the result structure.
	t.Logf("Result keys: %v", getKeys(result))

	if commentDetails, ok := result["comment_details"]; ok {
		t.Logf("Comment details: %+v", commentDetails)
	}

	if functionSummary, ok := result["function_summary"]; ok {
		t.Logf("Function summary: %+v", functionSummary)
	}

	// Print full result for debugging.
	resultJSON, err := json.MarshalIndent(result, "", "  ")
	require.NoError(t, err)
	t.Logf("Full result: %s", string(resultJSON))
}

func TestAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with a good comment above a function.
	root := &node.Node{Type: node.UASTFile}

	// Add a comment.
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = testGoodComment
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(comment)

	// Add a function.
	function := &node.Node{Type: node.UASTFunction}
	function.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   4,
	}

	// Add function name.
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = testFunctionName
	name.Roles = []node.Role{node.RoleName}
	function.AddChild(name)

	root.AddChild(function)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	// Test the formatted output.
	var buf strings.Builder

	err = analyzer.FormatReport(result, &buf)
	require.NoError(t, err)

	formatted := buf.String()
	t.Logf("Formatted Report:\n%s", formatted)

	// Verify the output contains expected sections from SectionRenderer.
	assert.Contains(t, formatted, "COMMENTS")
	assert.Contains(t, formatted, "Score: 10/10")
	assert.Contains(t, formatted, "Excellent comment quality and placement")
	assert.Contains(t, formatted, "Total Comments")
	assert.Contains(t, formatted, "Doc Coverage")
	assert.Contains(t, formatted, "100.0%")
}

func TestAnalyzer_FormatReport_Complex(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Create a tree with multiple functions and comments.
	root := &node.Node{Type: node.UASTFile}

	// Add a good comment above first function.
	goodComment := &node.Node{Type: node.UASTComment}
	goodComment.Token = "// This is a well-documented function"
	goodComment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(goodComment)

	// Add first function (well documented).
	func1 := &node.Node{Type: node.UASTFunction}
	func1.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   5,
	}
	name1 := &node.Node{Type: node.UASTIdentifier}
	name1.Token = "wellDocumentedFunction"
	name1.Roles = []node.Role{node.RoleName}
	func1.AddChild(name1)
	root.AddChild(func1)

	// Add second function without comment (missing documentation).
	func2 := &node.Node{Type: node.UASTFunction}
	func2.Pos = &node.Positions{
		StartLine: 7,
		EndLine:   10,
	}
	name2 := &node.Node{Type: node.UASTIdentifier}
	name2.Token = "undocumentedFunction"
	name2.Roles = []node.Role{node.RoleName}
	func2.AddChild(name2)
	root.AddChild(func2)

	// Add a bad comment (inside function body).
	func3 := &node.Node{Type: node.UASTFunction}
	func3.Pos = &node.Positions{
		StartLine: 12,
		EndLine:   16,
	}
	name3 := &node.Node{Type: node.UASTIdentifier}
	name3.Token = "functionWithBadComment"
	name3.Roles = []node.Role{node.RoleName}
	func3.AddChild(name3)

	// Add function body.
	body := &node.Node{Type: node.UASTBlock}
	body.Pos = &node.Positions{
		StartLine: 13,
		EndLine:   15,
	}

	// Add a comment inside the function body (bad placement).
	badComment := &node.Node{Type: node.UASTComment}
	badComment.Token = "// This comment is in the wrong place"
	badComment.Pos = &node.Positions{
		StartLine: 14,
		EndLine:   14,
	}
	body.AddChild(badComment)
	func3.AddChild(body)
	root.AddChild(func3)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	// Test the formatted output.
	var buf strings.Builder

	err = analyzer.FormatReport(result, &buf)
	require.NoError(t, err)

	formatted := buf.String()
	t.Logf("Complex Formatted Report:\n%s", formatted)

	// Verify the output contains expected sections from SectionRenderer.
	assert.Contains(t, formatted, "COMMENTS")
	assert.Contains(t, formatted, "Score: 5/10")
	assert.Contains(t, formatted, "Fair comment quality")
	assert.Contains(t, formatted, "Total Comments")
	assert.Contains(t, formatted, "Doc Coverage")
	assert.Contains(t, formatted, "33.3%")
	assert.Contains(t, formatted, "Top Issues")
}

func TestAnalyzer_RealFile(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	// Parse a real Go file.
	root := &node.Node{Type: node.UASTFile}

	// Add a real comment.
	comment := &node.Node{Type: node.UASTComment}
	comment.Token = "// This is a real comment"
	comment.Pos = &node.Positions{
		StartLine: 1,
		EndLine:   1,
	}
	root.AddChild(comment)

	// Add a real function.
	funcNode := &node.Node{Type: node.UASTFunction}
	funcNode.Pos = &node.Positions{
		StartLine: 2,
		EndLine:   5,
	}
	name := &node.Node{Type: node.UASTIdentifier}
	name.Token = "realFunction"
	name.Roles = []node.Role{node.RoleName}
	funcNode.AddChild(name)
	root.AddChild(funcNode)

	result, err := analyzer.Analyze(root)
	require.NoError(t, err)

	t.Logf("Result keys: %v", getKeys(result))

	if commentDetails, ok := result["comment_details"]; ok {
		t.Logf("Comment details type: %T", commentDetails)
		t.Logf("Comment details: %+v", commentDetails)

		if details, detailsOK := commentDetails.([]map[string]any); detailsOK {
			t.Logf("Comment details length: %d", len(details))
		}
	}

	if functions, ok := result["functions"]; ok {
		t.Logf("Functions type: %T", functions)
		t.Logf("Functions: %+v", functions)

		if funcs, funcsOK := functions.([]map[string]any); funcsOK {
			t.Logf("Functions length: %d", len(funcs))
		}
	}

	resultJSON, err := json.MarshalIndent(result, "", "  ")
	require.NoError(t, err)
	t.Logf("Full result: %s", string(resultJSON))
}

func getKeys(m map[string]any) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}

	return keys
}

// --- FormatReportJSON Tests ---.

func TestAnalyzer_FormatReportJSON(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"total_comments":       2,
		"good_comments":        1,
		"bad_comments":         1,
		"overall_score":        0.5,
		"total_functions":      1,
		"documented_functions": 1,
		"comments": []map[string]any{
			{"line": 10, "quality": "good"},
			{"line": 20, "quality": "bad"},
		},
		"functions": []map[string]any{
			{"name": "testFunc", "has_comment": true, "comment_score": 0.8},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportJSON(report, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON.
	var result ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure.
	assert.Len(t, result.CommentQuality, 2)
	assert.Equal(t, 2, result.Aggregate.TotalComments)
}

func TestAnalyzer_FormatReportJSON_Empty(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{}

	var buf bytes.Buffer

	err := analyzer.FormatReportJSON(report, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON.
	var result ComputedMetrics

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.CommentQuality)
	assert.Equal(t, 0, result.Aggregate.TotalComments)
}

// --- FormatReportYAML Tests ---.

func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"total_comments":       2,
		"good_comments":        1,
		"bad_comments":         1,
		"overall_score":        0.5,
		"total_functions":      1,
		"documented_functions": 1,
		"comments": []map[string]any{
			{"line": 10, "quality": "good"},
			{"line": 20, "quality": "bad"},
		},
		"functions": []map[string]any{
			{"name": "testFunc", "has_comment": true, "comment_score": 0.8},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML.
	var result ComputedMetrics

	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure.
	assert.Len(t, result.CommentQuality, 2)
	assert.Equal(t, 2, result.Aggregate.TotalComments)
}

func TestAnalyzer_FormatReportYAML_Empty(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{}

	var buf bytes.Buffer

	err := analyzer.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML.
	var result ComputedMetrics

	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.CommentQuality)
	assert.Equal(t, 0, result.Aggregate.TotalComments)
}

func TestAnalyzer_FormatReportYAML_ContainsExpectedFields(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"total_comments": 1,
		"comments": []map[string]any{
			{"line": 10, "quality": "good"},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "comment_quality:")
	assert.Contains(t, output, "function_documentation:")
	assert.Contains(t, output, "undocumented_functions:")
	assert.Contains(t, output, "aggregate:")
}
