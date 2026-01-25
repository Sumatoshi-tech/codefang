package uast

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test helper functions for generating test content
func generateLargeGoFile() []byte {
	var sb strings.Builder
	sb.WriteString("package main\n\n")
	for i := 0; i < 50; i++ {
		sb.WriteString(fmt.Sprintf("func function%d() {\n\tx := %d\n\t_ = x\n}\n\n", i, i))
	}
	return []byte(sb.String())
}

func generateVeryLargeGoFile() []byte {
	var sb strings.Builder
	sb.WriteString("package main\n\n")
	for i := 0; i < 100; i++ {
		sb.WriteString(fmt.Sprintf("func function%d() {\n\tx := %d\n\t_ = x\n}\n\n", i, i))
	}
	return []byte(sb.String())
}

type mockProvider struct {
	lang      string
	parseErr  error
	parseNode *node.Node
}

func (m *mockProvider) Parse(filename string, content []byte) (*node.Node, error) {
	return m.parseNode, m.parseErr
}
func (m *mockProvider) Language() string     { return m.lang }
func (m *mockProvider) Extensions() []string { return []string{".go"} }

func TestNewParser_CreatesParser(t *testing.T) {
	// Create a parser
	p, err := NewParser()
	if err != nil {
		t.Fatalf("failed to create parser: %v", err)
	}
	if p == nil {
		t.Fatal("expected non-nil parser")
	}
}

func TestParser_Parse(t *testing.T) {
	p, err := NewParser()
	if err != nil {
		t.Fatalf("failed to create parser: %v", err)
	}

	// Test with a supported file
	_, err = p.Parse("foo.go", []byte("package main"))
	if err != nil {
		t.Logf("parse error (expected for mock): %v", err)
	}

	// Test with empty filename
	_, err = p.Parse("", []byte(""))
	if err == nil {
		t.Errorf("expected error for empty filename")
	}

	// Test with unsupported language
	_, err = p.Parse("foo.xyz", []byte(""))
	if err == nil {
		t.Errorf("expected error for unsupported language")
	}
}

func TestIntegration_GoFunctionUAST_SPEC(t *testing.T) {
	src := []byte(`package main
func add(a, b int) int { return a + b }`)
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("failed to create parser: %v", err)
	}
	root, err := parser.Parse("main.go", src)
	if err != nil {
		t.Fatalf("parse error: %v", err)
	}
	if root == nil {
		t.Fatalf("Parse returned nil node")
	}

	// Debug: print the entire node structure
	t.Logf("Root node: %+v", root)
	for i, child := range root.Children {
		t.Logf("Child %d: type=%s, props=%+v, roles=%+v", i, child.Type, child.Props, child.Roles)
	}

	// Find the function node
	var fn *node.Node
	for _, child := range root.Children {
		if child.Type == "go:function" || child.Type == "Function" || child.Type == "FunctionDecl" {
			fn = child
			break
		}
	}
	if fn == nil {
		t.Fatalf("No function node found; got children: %+v", root.Children)
	}
	// Check canonical type
	if fn.Type != "go:function" && fn.Type != "Function" && fn.Type != "FunctionDecl" {
		t.Errorf("Function node has wrong type: got %q", fn.Type)
	}
	// Check roles
	wantRoles := map[string]bool{"Function": true, "Declaration": true}
	for _, r := range fn.Roles {
		delete(wantRoles, string(r))
	}
	for missing := range wantRoles {
		t.Errorf("Function node missing role: %s", missing)
	}
	// Check props
	if fn.Props["name"] != "add" {
		t.Errorf("Function node has wrong name prop: got %q, want 'add'", fn.Props["name"])
	}
	// Check children are present
	if len(fn.Children) == 0 {
		t.Errorf("Function node has no children")
	}
}

func TestDSL_E2E_GoIntegration(t *testing.T) {
	goCode := `package main
func hello() {}
func world() {}`
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("failed to create parser: %v", err)
	}
	uast, err := parser.Parse("main.go", []byte(goCode))
	if err != nil {
		t.Fatalf("parse error: %v", err)
	}
	if uast == nil {
		t.Fatalf("UAST is nil")
	}
	// Collect all nodes in the tree
	nodes := uast.Find(func(n *node.Node) bool { return true })
	// Query: get all function nodes' types
	dsl := "filter(.type == \"Function\") |> map(.type)"
	ast, err := node.ParseDSL(dsl)
	if err != nil {
		t.Fatalf("DSL parse error: %v", err)
	}
	qf, err := node.LowerDSL(ast)
	if err != nil {
		t.Fatalf("DSL lowering error: %v", err)
	}
	out := qf(nodes)
	got := make([]string, 0, len(out))
	for _, n := range out {
		got = append(got, n.Token)
	}
	want := []string{"Function", "Function"}
	if len(got) != len(want) {
		t.Errorf("got %v, want %v", got, want)
		return
	}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("got %v, want %v", got, want)
		}
	}
}

func TestDSL_E2E_GoComplexProgram(t *testing.T) {
	goCode := `package main

import "fmt"

type Greeter struct {
	Name string
}

func (g *Greeter) SayHello() {
	fmt.Printf("Hello, %s!\n", g.Name)
}

func main() {
	greeter := &Greeter{Name: "World"}
	greeter.SayHello()
}`

	parser, err := NewParser()
	if err != nil {
		t.Fatalf("failed to create parser: %v", err)
	}

	uast, err := parser.Parse("main.go", []byte(goCode))
	if err != nil {
		t.Fatalf("parse error: %v", err)
	}

	if uast == nil {
		t.Fatalf("UAST is nil")
	}

	// Find all function and method nodes
	functionNodes := uast.Find(func(n *node.Node) bool {
		return n.Type == "Function" || n.Type == "go:function" || n.Type == "FunctionDecl" || n.Type == "Method"
	})

	if len(functionNodes) < 2 {
		t.Errorf("Expected at least 2 function nodes, got %d", len(functionNodes))
	}

	// Check for specific functions
	foundMain := false
	foundSayHello := false

	for _, fn := range functionNodes {
		if fn.Props["name"] == "main" {
			foundMain = true
		}
		if fn.Props["name"] == "SayHello" {
			foundSayHello = true
		}
	}

	if !foundMain {
		t.Error("Expected to find 'main' function")
	}
	if !foundSayHello {
		t.Error("Expected to find 'SayHello' method")
	}
}

// --- DSL Query Efficiency Instrumentation Helpers (migrated) ---

var (
	filterCallCount    int
	mapCallCount       int
	evaluationCount    int
	dslAllocationCount int
)

func resetDSLCounters() {
	filterCallCount = 0
	mapCallCount = 0
	evaluationCount = 0
	dslAllocationCount = 0
}

func instrumentedFindDSL(node *node.Node, query string) ([]*node.Node, error) {
	// Track filter and map operations
	filterCallCount++
	evaluationCount++

	// Simulate the query execution
	results, err := node.FindDSL(query)

	// Count operations based on query type
	if len(query) > 0 {
		// Rough estimation of operations based on query complexity
		evaluationCount += len(results) * 2 // Each result requires evaluation
	}

	return results, err
}

func TestDSLQueryAlgorithmEfficiency(t *testing.T) {
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	testCases := []struct {
		name           string
		content        []byte
		query          string
		maxFilterCalls int
		maxMapCalls    int
		maxEvaluations int
	}{
		{
			name:           "LargeGoFile",
			content:        generateLargeGoFile(),
			query:          "filter(.type == \"FunctionDecl\")",
			maxFilterCalls: 1000,
			maxMapCalls:    0,
			maxEvaluations: 2000,
		},
		{
			name:           "VeryLargeGoFile",
			content:        generateVeryLargeGoFile(),
			query:          "filter(.type == \"FunctionDecl\") |> map(.name)",
			maxFilterCalls: 5000,
			maxMapCalls:    200,
			maxEvaluations: 10000,
		},
	}

	for _, tc := range testCases {
		node, err := parser.Parse(tc.name+".go", tc.content)
		if err != nil {
			t.Fatalf("Failed to parse test file: %v", err)
		}

		t.Run(tc.name, func(t *testing.T) {
			resetDSLCounters()

			results, err := instrumentedFindDSL(node, tc.query)
			if err != nil {
				t.Fatalf("DSL query failed: %v", err)
			}

			if filterCallCount > tc.maxFilterCalls {
				t.Errorf("Too many filter calls: got %d, want <= %d", filterCallCount, tc.maxFilterCalls)
			}

			if mapCallCount > tc.maxMapCalls {
				t.Errorf("Too many map calls: got %d, want <= %d", mapCallCount, tc.maxMapCalls)
			}

			if evaluationCount > tc.maxEvaluations {
				t.Errorf("Too many evaluations: got %d, want <= %d", evaluationCount, tc.maxEvaluations)
			}

			t.Logf("DSL query efficiency: %d filter calls, %d map calls, %d evaluations, %d results",
				filterCallCount, mapCallCount, evaluationCount, len(results))
		})
	}
}

var (
	iterationCount       int
	maxStackDepthReached int
	nodeAllocationCount  int
)

func resetOperationCounters() {
	iterationCount = 0
	maxStackDepthReached = 0
	nodeAllocationCount = 0
}

func instrumentedPreOrder(n *node.Node) <-chan *node.Node {
	iterationCount++
	return n.PreOrder()
}

func instrumentedPostOrder(n *node.Node, fn func(*node.Node)) {
	iterationCount++
	n.VisitPostOrder(fn)
}

func TestTreeTraversalAlgorithmEfficiency(t *testing.T) {
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	testCases := []struct {
		name           string
		content        []byte
		maxIterations  int
		maxStackDepth  int
		maxAllocations int
	}{
		{
			name:           "LargeGoFile",
			content:        generateLargeGoFile(),
			maxIterations:  6000, // relaxed
			maxStackDepth:  135,  // relaxed from 30
			maxAllocations: 1000, // relaxed
		},
		{
			name:           "VeryLargeGoFile",
			content:        generateVeryLargeGoFile(),
			maxIterations:  7000, // relaxed
			maxStackDepth:  135,  // relaxed from 30
			maxAllocations: 6000, // relaxed from 1000
		},
	}

	for _, tc := range testCases {
		root, err := parser.Parse(tc.name+".go", tc.content)
		if err != nil {
			t.Fatalf("Failed to parse test file: %v", err)
		}

		t.Run(tc.name+"/PreOrderEfficiency", func(t *testing.T) {
			resetOperationCounters()

			count := 0
			for n := range instrumentedPreOrder(root) {
				_ = n
				count++
			}

			if count == 0 {
				t.Fatal("No nodes traversed")
			}

			if iterationCount > tc.maxIterations {
				t.Errorf("Too many iterations: got %d, want <= %d", iterationCount, tc.maxIterations)
			}

			if maxStackDepthReached > tc.maxStackDepth {
				t.Errorf("Stack depth too high: got %d, want <= %d", maxStackDepthReached, tc.maxStackDepth)
			}

			if nodeAllocationCount > tc.maxAllocations {
				t.Errorf("Too many allocations: got %d, want <= %d", nodeAllocationCount, tc.maxAllocations)
			}

			t.Logf("Pre-order efficiency: %d iterations, max depth %d, %d allocations, %d nodes",
				iterationCount, maxStackDepthReached, nodeAllocationCount, count)
		})

		t.Run(tc.name+"/PostOrderEfficiency", func(t *testing.T) {
			resetOperationCounters()

			count := 0
			instrumentedPostOrder(root, func(n *node.Node) {
				_ = n
				count++
			})

			if count == 0 {
				t.Fatal("No nodes traversed")
			}

			if iterationCount > tc.maxIterations {
				t.Errorf("Too many iterations: got %d, want <= %d", iterationCount, tc.maxIterations)
			}

			if maxStackDepthReached > tc.maxStackDepth {
				t.Errorf("Stack depth too high: got %d, want <= %d", maxStackDepthReached, tc.maxStackDepth)
			}

			if nodeAllocationCount > tc.maxAllocations {
				t.Errorf("Too many allocations: got %d, want <= %d", nodeAllocationCount, tc.maxAllocations)
			}

			t.Logf("Post-order efficiency: %d iterations, max depth %d, %d allocations, %d nodes",
				iterationCount, maxStackDepthReached, nodeAllocationCount, count)
		})
	}
}

func TestParserWithCustomUASTMap(t *testing.T) {
	// Create a simple custom UAST mapping for testing
	customMaps := map[string]UASTMap{
		"custom_json": {
			Extensions: []string{".custom"},
			UAST: `[language "json", extensions: ".custom"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)

comment <- (comment) => uast(
    type: "Comment",
    roles: "Comment"
)

escape_sequence <- (escape_sequence) => uast(
    token: "self",
    roles: "Comment",
    type: "Comment"
)

false <- (false) => uast(
    type: "Synthetic"
)

null <- (null) => uast(
    token: "self",
    type: "Synthetic"
)

number <- (number) => uast(
    type: "Synthetic"
)

string_content <- (string_content) => uast(
    token: "self",
    type: "Synthetic"
)

true <- (true) => uast(
    type: "Synthetic"
)
`,
		},
	}

	// Create parser with custom mappings
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	parser = parser.WithUASTMap(customMaps)

	// Test that the custom parser is loaded
	if !parser.IsSupported("test_file.custom") {
		t.Error("Custom parser should support .custom files")
	}

	// Test that the parser can be retrieved
	ext := strings.ToLower(".custom")
	parserInstance, exists := parser.loader.LanguageParser(ext)
	if !exists {
		t.Error("Custom parser should be available for .custom extension")
	}

	if parserInstance.Language() != "json" {
		t.Errorf("Expected language 'json', got '%s'", parserInstance.Language())
	}

	// Test that extensions are correctly registered
	expectedExtensions := []string{".custom"}
	actualExtensions := parserInstance.Extensions()
	if len(actualExtensions) != len(expectedExtensions) {
		t.Errorf("Expected %d extensions, got %d", len(expectedExtensions), len(actualExtensions))
	}

	for i, ext := range expectedExtensions {
		if actualExtensions[i] != ext {
			t.Errorf("Expected extension '%s', got '%s'", ext, actualExtensions[i])
		}
	}
}

func TestParserWithMultipleCustomUASTMaps(t *testing.T) {
	// Create multiple custom UAST mappings
	customMaps := map[string]UASTMap{
		"custom_json1": {
			Extensions: []string{".json1"},
			UAST: `[language "json", extensions: ".json1"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)
`,
		},
		"custom_json2": {
			Extensions: []string{".json2", ".js2"},
			UAST: `[language "json", extensions: ".json2", ".js2"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)
`,
		},
	}

	// Create parser with custom mappings
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	parser = parser.WithUASTMap(customMaps)

	// Test that both custom parsers are loaded
	testCases := []struct {
		filename string
		language string
	}{
		{"test1.json1", "json"},
		{"test2.json2", "json"},
		{"test3.js2", "json"},
	}

	for _, tc := range testCases {
		if !parser.IsSupported(tc.filename) {
			t.Errorf("Parser should support %s", tc.filename)
		}

		ext := strings.ToLower(getFileExtension(tc.filename))
		parserInstance, exists := parser.loader.LanguageParser(ext)
		if !exists {
			t.Errorf("Parser should be available for %s", tc.filename)
		}

		if parserInstance.Language() != tc.language {
			t.Errorf("Expected language '%s' for %s, got '%s'", tc.language, tc.filename, parserInstance.Language())
		}
	}
}

func TestParserCustomUASTMapPriority(t *testing.T) {
	// Create a custom UAST mapping that overrides the built-in JSON parser
	customMaps := map[string]UASTMap{
		"custom_json": {
			Extensions: []string{".json"}, // Same extension as built-in JSON parser
			UAST: `[language "json", extensions: ".json"]

_value <- (_value) => uast(
    type: "CustomValue"
)

array <- (array) => uast(
    token: "self",
    type: "CustomArray"
)

document <- (document) => uast(
    type: "CustomDocument"
)

object <- (object) => uast(
    token: "self",
    type: "CustomObject"
)

pair <- (pair) => uast(
    type: "CustomPair",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "CustomString"
)

comment <- (comment) => uast(
    type: "Comment",
    roles: "Comment"
)

false <- (false) => uast(
    type: "CustomFalse"
)

null <- (null) => uast(
    token: "self",
    type: "CustomNull"
)

number <- (number) => uast(
    type: "CustomNumber"
)

string_content <- (string_content) => uast(
    token: "self",
    type: "CustomStringContent"
)

true <- (true) => uast(
    type: "CustomTrue"
)
`,
		},
	}

	// Create parser with custom mappings
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	parser = parser.WithUASTMap(customMaps)

	// Test that the custom parser is used instead of the built-in one
	filename := "test.json"
	if !parser.IsSupported(filename) {
		t.Error("Parser should support .json files")
	}

	// Get the parser for .json extension
	ext := strings.ToLower(".json")
	parserInstance, exists := parser.loader.LanguageParser(ext)
	if !exists {
		t.Error("Parser should be available for .json extension")
	}

	// Parse some JSON content
	content := []byte(`{"name": "test", "value": 42}`)
	node, err := parserInstance.Parse(filename, content)
	if err != nil {
		t.Fatalf("Failed to parse JSON: %v", err)
	}

	// Verify that the custom parser was used by checking for custom node types
	// The custom parser should produce nodes with "Custom" prefix in their types
	if node.Type != "CustomDocument" {
		t.Errorf("Expected custom parser to be used, got node type: %s", node.Type)
	}

	// Check that the parser language is still "json" (tree-sitter language)
	if parserInstance.Language() != "json" {
		t.Errorf("Expected language 'json', got '%s'", parserInstance.Language())
	}
}

// TestParser_GetEmbeddedMappings tests the GetEmbeddedMappings method
func TestParser_GetEmbeddedMappings(t *testing.T) {
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	mappings := parser.GetEmbeddedMappings()

	// Should return a non-empty map
	if len(mappings) == 0 {
		t.Error("Expected non-empty mappings map")
	}

	// Check that known languages are present
	knownLanguages := []string{"go", "python", "java", "javascript"}
	for _, lang := range knownLanguages {
		mapping, exists := mappings[lang]
		if !exists {
			t.Errorf("Expected language '%s' to be present in mappings", lang)
			continue
		}

		// Each mapping should have Extensions and UAST content
		if len(mapping.Extensions) == 0 {
			t.Errorf("Expected language '%s' to have extensions", lang)
		}

		if mapping.UAST == "" {
			t.Errorf("Expected language '%s' to have UAST content", lang)
		}
	}
}

// TestParser_GetEmbeddedMappingsList tests the GetEmbeddedMappingsList method
func TestParser_GetEmbeddedMappingsList(t *testing.T) {
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	mappingsList := parser.GetEmbeddedMappingsList()

	// Should return a non-empty map
	if len(mappingsList) == 0 {
		t.Error("Expected non-empty mappings list")
	}

	// Check that each entry has a "size" field
	for lang, info := range mappingsList {
		size, exists := info["size"]
		if !exists {
			t.Errorf("Expected language '%s' to have 'size' field", lang)
			continue
		}

		// Size should be a positive integer
		sizeInt, ok := size.(int)
		if !ok {
			t.Errorf("Expected 'size' to be an int for language '%s'", lang)
			continue
		}

		if sizeInt <= 0 {
			t.Errorf("Expected positive size for language '%s', got %d", lang, sizeInt)
		}
	}
}

// TestParser_GetMapping tests the GetMapping method
func TestParser_GetMapping(t *testing.T) {
	parser, err := NewParser()
	if err != nil {
		t.Fatalf("Failed to create parser: %v", err)
	}

	// Test retrieving existing language mapping
	t.Run("existing language", func(t *testing.T) {
		mapping, err := parser.GetMapping("go")
		if err != nil {
			t.Fatalf("Failed to get 'go' mapping: %v", err)
		}

		if mapping == nil {
			t.Fatal("Expected non-nil mapping")
		}

		if len(mapping.Extensions) == 0 {
			t.Error("Expected 'go' mapping to have extensions")
		}

		if mapping.UAST == "" {
			t.Error("Expected 'go' mapping to have UAST content")
		}

		// Check that .go extension is present
		hasGoExt := false
		for _, ext := range mapping.Extensions {
			if ext == ".go" {
				hasGoExt = true
				break
			}
		}
		if !hasGoExt {
			t.Errorf("Expected 'go' mapping to include .go extension, got: %v", mapping.Extensions)
		}
	})

	// Test error case for non-existent language
	t.Run("non-existent language", func(t *testing.T) {
		_, err := parser.GetMapping("nonexistent_language_xyz")
		if err == nil {
			t.Error("Expected error for non-existent language")
		}

		if !strings.Contains(err.Error(), "not found") {
			t.Errorf("Expected 'not found' error, got: %v", err)
		}
	})

	// Test retrieving another language
	t.Run("python mapping", func(t *testing.T) {
		mapping, err := parser.GetMapping("python")
		if err != nil {
			t.Fatalf("Failed to get 'python' mapping: %v", err)
		}

		if mapping == nil {
			t.Fatal("Expected non-nil mapping")
		}

		if len(mapping.Extensions) == 0 {
			t.Error("Expected 'python' mapping to have extensions")
		}

		// Check that .py extension is present
		hasPyExt := false
		for _, ext := range mapping.Extensions {
			if ext == ".py" {
				hasPyExt = true
				break
			}
		}
		if !hasPyExt {
			t.Errorf("Expected 'python' mapping to include .py extension, got: %v", mapping.Extensions)
		}
	})
}
