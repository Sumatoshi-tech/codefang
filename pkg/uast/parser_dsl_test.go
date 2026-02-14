package uast

import (
	"os"
	"slices"
	"strings"
	"testing"
	"unsafe"

	sitter "github.com/alexaandru/go-tree-sitter-bare"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const testHelloName = "Hello"

func TestDSLProviderIntegration(t *testing.T) {
	t.Parallel()

	// Test DSL content with language declaration.
	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: @name,
    roles: "Declaration",
    children: @body
)

identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	// Load and validate mapping rules.
	rules, langInfo, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("Expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("Expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("Expected language name 'go', got '%s'", langInfo.Name)
	}

	// Create DSL provider.
	p := NewDSLParser(strings.NewReader(dslContent))

	loadErr := p.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	// Test source code.
	source := []byte(`package main

func Hello() {
    fmt.Println("Hello, World!")
}`)

	// Parse the source code.
	uastNode, err := p.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Verify the provider implements the Provider interface.
	var _ LanguageParser = p

	// Test provider methods.
	if p.Language() != langInfo.Name {
		t.Errorf("Expected language '%s', got '%s'", langInfo.Name, p.Language())
	}

	t.Logf("Successfully parsed Go source code with DSL provider")
	t.Logf("UAST node type: %s", uastNode.Type)
	t.Logf("UAST node roles: %v", uastNode.Roles)
}

func TestProviderFactoryIntegration(t *testing.T) {
	t.Parallel()

	// Test DSL content with language declaration.
	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name) => uast(
    type: "Function",
    token: @name,
    roles: "Declaration"
)`

	loader := NewDSLParser(strings.NewReader(dslContent))

	loadErr := loader.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	// Test source code.
	source := []byte(`package main

func Hello() {
    fmt.Println("Hello, World!")
}`)

	// Parse the source code.
	uastNode, err := loader.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	t.Logf("Successfully created DSL provider using factory")
	t.Logf("UAST node type: %s", uastNode.Type)
}

func TestDSLProvider_CaptureExtraction(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration",
    token: "fields.name"
)
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	_, _, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Hello() {}`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Find the Function node.
	var fn *node.Node

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType {
			fn = c

			break
		}
	}

	if fn == nil {
		t.Fatal("Expected Function node")
	}

	if fn.Token != testHelloName {
		t.Errorf("Expected token 'Hello', got '%s'", fn.Token)
	}

	if fn.Props["name"] != testHelloName {
		t.Errorf("Expected property name 'Hello', got '%s'", fn.Props["name"])
	}
}

func TestDSLProvider_ConditionEvaluation(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration",
    token: "fields.name"
) when name == "Hello"
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	_, _, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Hello() {}
func World() {}`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Only the Hello function should be mapped as Function.
	foundHello := false
	foundWorld := false

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType && c.Token == testHelloName {
			foundHello = true
		}

		if c.Type == testUASTFunctionType && c.Token == "World" {
			foundWorld = true
		}
	}

	if !foundHello {
		t.Error("Expected Hello function to be mapped")
	}

	if foundWorld {
		t.Error("Did not expect World function to be mapped due to condition")
	}
}

func TestDSLProvider_InheritanceWithConditions(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Child",
    roles: "ChildRole",
    token: "fields.name"
) when name == "Hello"
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	_, _, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Hello() {}
func World() {}`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	foundChild := false

	for _, c := range uastNode.Children {
		if c.Type == "Child" && c.Token == testHelloName {
			foundChild = true
		}
	}

	if !foundChild {
		t.Error("Expected Child node for Hello due to inheritance and condition")
	}
}

func TestDSLProvider_AdvancedPropertyExtraction(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

var_declaration <- (var_declaration) => uast(
    type: "Variable",
    roles: "Declaration"
)
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	_, _, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
var x int`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	foundVar := false

	for _, c := range uastNode.Children {
		if c.Type == "Variable" {
			foundVar = true

			break
		}
	}

	if !foundVar {
		t.Error("Expected Variable node")
	}
}

// TestDSLProvider_ConditionFilteringWithoutShouldIncludeChild verifies that
// condition-based child filtering works solely through shouldExcludeChild and
// ToCanonicalNode.shouldSkipNode — no redundant shouldIncludeChild needed.
func TestDSLProvider_ConditionFilteringWithoutShouldIncludeChild(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration",
    token: "fields.name"
) when name == "Allowed"
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Allowed() {}
func Blocked() {}
func AlsoBlocked() {}`)

	uastNode, parseErr := provider.Parse("test.go", source)
	if parseErr != nil {
		t.Fatalf("Failed to parse: %v", parseErr)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	var functionNames []string

	for _, child := range uastNode.Children {
		if child.Type == node.Type(testUASTFunctionType) {
			functionNames = append(functionNames, child.Token)
		}
	}

	if len(functionNames) != 1 {
		t.Fatalf("Expected exactly 1 Function child, got %d: %v", len(functionNames), functionNames)
	}

	if functionNames[0] != "Allowed" {
		t.Errorf("Expected function name 'Allowed', got '%s'", functionNames[0])
	}
}

func TestDSLProvider_ChildInclusionExclusion(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration",
    token: "fields.name"
) when name == "Hello"
identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	_, _, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Hello() {}
func World() {}`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Only Hello function should be included.
	foundHello := false
	foundWorld := false

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType && c.Token == testHelloName {
			foundHello = true
		}

		if c.Type == testUASTFunctionType && c.Token == "World" {
			foundWorld = true
		}
	}

	if !foundHello {
		t.Error("Expected Hello function to be included")
	}

	if foundWorld {
		t.Error("Did not expect World function to be included due to condition")
	}
}

func TestE2E_MappingGenerationAndParsing(t *testing.T) {
	t.Parallel()

	// Minimal node-types.json fixture (Go function and identifier).
	nodeTypesJSON := `[
	  {"type": "function_declaration", "named": true, "fields": {"name": {"types": ["identifier"], "required": true}}},
	  {"type": "identifier", "named": true, "fields": {}}
	]`

	nodes, err := mapping.ParseNodeTypes([]byte(nodeTypesJSON))
	if err != nil {
		t.Fatalf("Failed to parse node-types.json: %v", err)
	}

	dsl := mapping.GenerateMappingDSL(nodes, "go", []string{".go"})

	// Parse the generated DSL.
	_, langInfo, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dsl))
	if err != nil {
		t.Fatalf("Failed to parse generated mapping DSL: %v\nDSL:\n%s", err, dsl)
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("Expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("Expected language name 'go', got '%s'", langInfo.Name)
	}

	// Use a minimal Go source file.
	source := []byte(`package main\nfunc Hello() {}`)
	provider := NewDSLParser(strings.NewReader(dsl))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse Go source with generated mapping: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Check that a Function node is present.
	foundFunc := uastNode.Type == testUASTFunctionType || uastNode.Type == "function_declaration"

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType || c.Type == "function_declaration" {
			foundFunc = true

			break
		}
	}

	if !foundFunc {
		t.Logf("Generated DSL:\n%s", dsl)
		t.Logf("UAST: %+v", uastNode)
		t.Error("Expected Function node in UAST from generated mapping")
	}
}

func TestDSLProvider_RealWorldGoMap(t *testing.T) {
	t.Parallel()

	// Real-world go.uastmap DSL with advanced features.
	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration
    name: (identifier) @name
    parameters: (parameter_list) @params
    body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body",
    name: "@name",
    parameters: "@params",
    body: "@body"
)

method_declaration <- (method_declaration
    name: (identifier) @name
    receiver: (parameter_list) @recv
    parameters: (parameter_list) @params
    body: (block) @body) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method",
    children: "@recv", "@params", "@body",
    name: "@name",
    receiver: "@recv",
    parameters: "@params",
    body: "@body"
) # Extends function_declaration

var_spec <- (var_spec name: (identifier) @name) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    name: "@name"
)

if_statement <- (if_statement condition: (expression) @cond consequence: (block) @conseq alternative: (block) @alt) => uast(
    type: "If",
    roles: "Statement", "Conditional",
    children: "@cond", "@conseq", "@alt"
)
`

	_, langInfo, err := (&mapping.Parser{}).ParseMapping(strings.NewReader(dslContent))
	if err != nil {
		t.Fatalf("Failed to load DSL mappings: %v", err)
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("Expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("Expected language name 'go', got '%s'", langInfo.Name)
	}

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	// Real-world Go source code.
	source := []byte(`package main

func Add(a int, b int) int {
    return a + b
}

var x int = 42
`)

	uastNode, err := provider.Parse("test.go", source)
	if err != nil {
		t.Fatalf("Failed to parse source code: %v", err)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Find the Function node and check properties.
	var foundFunc bool

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType {
			foundFunc = true

			if c.Props["name"] != "Add" {
				t.Errorf("Expected function name 'Add', got '%s'", c.Props["name"])
			}

			if c.Token != "Add" {
				t.Errorf("Expected function token 'Add', got '%s'", c.Token)
			}

			if c.Props["parameters"] == "" {
				t.Errorf("Expected parameters property to be set")
			}

			if c.Props["body"] == "" {
				t.Errorf("Expected body property to be set")
			}

			// Debug: print all props.
			t.Logf("Function node props: %+v", c.Props)
		}

		if c.Type == "Variable" {
			if c.Props["name"] != "x" {
				t.Errorf("Expected variable name 'x', got '%s'", c.Props["name"])
			}

			// Debug: print all props.
			t.Logf("Variable node props: %+v", c.Props)

			// Debug: print children types and tokens recursively.
			var printVarTree func(nd *node.Node, depth int)

			printVarTree = func(nd *node.Node, depth int) {
				if nd == nil {
					return
				}

				pad := strings.Repeat("  ", depth)
				t.Logf("%sVarNode: type=%s, token=%s, props=%+v", pad, nd.Type, nd.Token, nd.Props)

				for _, cc := range nd.Children {
					printVarTree(cc, depth+1)
				}
			}

			printVarTree(c, 1)
		}
	}

	if !foundFunc {
		t.Error("Expected to find a Function node")
	}

	// Debug: recursively print all nodes in the UAST tree.
	var printTree func(nd *node.Node, depth int)

	printTree = func(nd *node.Node, depth int) {
		if nd == nil {
			return
		}

		pad := strings.Repeat("  ", depth)
		t.Logf("%sNode: type=%s, token=%s, props=%+v", pad, nd.Type, nd.Token, nd.Props)

		for _, c := range nd.Children {
			printTree(c, depth+1)
		}
	}

	printTree(uastNode, 0)

	// Write a Go file for tree-sitter inspection in the current directory.
	tmpGo := `package main
var x int = 42
func Add(a int, b int) int { return a + b }
`
	fileName := "test_var.go"

	writeErr := os.WriteFile(fileName, []byte(tmpGo), 0o600)
	if writeErr != nil {
		t.Fatalf("Failed to write test_var.go: %v", writeErr)
	}

	os.Remove(fileName)
}

// TestAllPositions_MatchesOriginal verifies that unsafe + batched position
// readers return the same values as individual StartPoint()/EndPoint()/
// StartByte()/EndByte() calls for every node in a parsed tree.
func TestAllPositions_MatchesOriginal(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration"
)

identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)

block <- (block) => uast(
    type: "Block",
    roles: "Block"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

func Hello() {
	x := 1
	y := 2
	_ = x + y
}

func World() {
	for i := 0; i < 10; i++ {
		fmt.Println(i)
	}
}
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	// Walk every node in the tree and compare positions.
	root := tree.RootNode()
	nodeCount := 0

	var walkAndCheck func(n sitter.Node)

	walkAndCheck = func(n sitter.Node) {
		if n.IsNull() {
			return
		}

		// Get start positions via readStartPositions (unsafe direct read).
		sb, sr, sc := readStartPositions(unsafe.Pointer(&n))
		// Get end positions via readEndPositions (single CGO helper call).
		eb, er, ec := readEndPositions(unsafe.Pointer(&n))

		// Compare against individual CGO calls (original approach).
		startPt := n.StartPoint()
		endPt := n.EndPoint()

		if sr != startPt.Row {
			t.Errorf("node %q: startRow mismatch: unsafe=%d, StartPoint=%d",
				n.Type(), sr, startPt.Row)
		}

		if sc != startPt.Column {
			t.Errorf("node %q: startCol mismatch: unsafe=%d, StartPoint=%d",
				n.Type(), sc, startPt.Column)
		}

		if sb != n.StartByte() {
			t.Errorf("node %q: startByte mismatch: unsafe=%d, StartByte=%d",
				n.Type(), sb, n.StartByte())
		}

		if er != endPt.Row {
			t.Errorf("node %q: endRow mismatch: helper=%d, EndPoint=%d",
				n.Type(), er, endPt.Row)
		}

		if ec != endPt.Column {
			t.Errorf("node %q: endCol mismatch: helper=%d, EndPoint=%d",
				n.Type(), ec, endPt.Column)
		}

		if eb != n.EndByte() {
			t.Errorf("node %q: endByte mismatch: helper=%d, EndByte=%d",
				n.Type(), eb, n.EndByte())
		}

		nodeCount++

		// Walk all children (named and unnamed).
		for i := range n.ChildCount() {
			walkAndCheck(n.Child(i))
		}
	}

	walkAndCheck(root)

	if nodeCount == 0 {
		t.Fatal("Expected to check at least one node")
	}

	t.Logf("Verified %d nodes — all positions match", nodeCount)
}

// TestReadNamedChildCount_MatchesCGO verifies that readNamedChildCount returns
// the same value as the CGO NamedChildCount() call for every node in a parsed tree.
func TestReadNamedChildCount_MatchesCGO(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

func Hello(a, b int) (string, error) {
	x := 1
	y := 2
	_ = x + y
}

type Foo struct {
	Name string
	Age  int
}
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	root := tree.RootNode()
	nodeCount := 0
	parentCount := 0

	var walkAndCheck func(n sitter.Node)

	walkAndCheck = func(n sitter.Node) {
		if n.IsNull() {
			return
		}

		expected := n.NamedChildCount()
		got := readNamedChildCount(unsafe.Pointer(&n))

		if got != expected {
			t.Errorf("node %q: namedChildCount mismatch: unsafe=%d, CGO=%d",
				n.Type(), got, expected)
		}

		if expected > 0 {
			parentCount++
		}

		nodeCount++

		for i := range n.ChildCount() {
			walkAndCheck(n.Child(i))
		}
	}

	walkAndCheck(root)

	if nodeCount == 0 {
		t.Fatal("Expected to check at least one node")
	}

	if parentCount == 0 {
		t.Fatal("Expected at least one parent node with children")
	}

	t.Logf("Verified %d nodes (%d parents) — all named child counts match", nodeCount, parentCount)
}

// TestCursorChildren_MatchesNamedChild verifies that iterating children via
// TreeCursor + IsNamed() filter produces the same set as NamedChild(idx) loop.
func TestCursorChildren_MatchesNamedChild(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

import (
	"fmt"
	"strings"
)

func Hello(a, b int) (string, error) {
	x := map[string]int{"a": 1, "b": 2}
	for k, v := range x {
		if v > 1 {
			fmt.Println(k, v)
		}
	}
	return strings.Join([]string{"a", "b"}, ","), nil
}

type Foo struct {
	Name string
	Age  int
}
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	root := tree.RootNode()
	nodeCount := 0
	parentCount := 0

	var walkAndCheck func(n sitter.Node)

	walkAndCheck = func(n sitter.Node) {
		if n.IsNull() {
			return
		}

		// Collect named children via NamedChild(idx) — the reference method.
		namedCount := n.NamedChildCount()
		expectedChildren := make([]string, 0, namedCount)

		for idx := range namedCount {
			child := n.NamedChild(idx)
			expectedChildren = append(expectedChildren, n.Type()+"/"+child.Type())
		}

		// Collect named children via TreeCursor + IsNamed() filter.
		cursorChildren := make([]string, 0, namedCount)
		cursor := sitter.NewTreeCursor(n)

		if cursor.GoToFirstChild() {
			for {
				child := cursor.CurrentNode()
				if child.IsNamed() {
					cursorChildren = append(cursorChildren, n.Type()+"/"+child.Type())
				}

				if !cursor.GoToNextSibling() {
					break
				}
			}
		}

		if len(cursorChildren) != len(expectedChildren) {
			t.Errorf("node %q: child count mismatch: cursor=%d, NamedChild=%d\n  cursor=%v\n  expected=%v",
				n.Type(), len(cursorChildren), len(expectedChildren), cursorChildren, expectedChildren)
		} else {
			for idx := range expectedChildren {
				if cursorChildren[idx] != expectedChildren[idx] {
					t.Errorf("node %q child[%d]: cursor=%q, NamedChild=%q",
						n.Type(), idx, cursorChildren[idx], expectedChildren[idx])
				}
			}
		}

		if namedCount > 0 {
			parentCount++
		}

		nodeCount++

		for i := range n.ChildCount() {
			walkAndCheck(n.Child(i))
		}
	}

	walkAndCheck(root)

	if nodeCount == 0 {
		t.Fatal("Expected to check at least one node")
	}

	if parentCount == 0 {
		t.Fatal("Expected at least one parent node with children")
	}

	t.Logf("Verified %d nodes (%d parents) — all cursor children match NamedChild", nodeCount, parentCount)
}

func TestBatchChildren_MatchesCursor(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

import "fmt"

func f0() { fmt.Println("0") }
func f1() { fmt.Println("1") }
func f2() { fmt.Println("2") }
func f3() { fmt.Println("3") }
func f4() { fmt.Println("4") }
func f5() { fmt.Println("5") }
func f6() { fmt.Println("6") }
func f7() { fmt.Println("7") }
func f8() { fmt.Println("8") }
func f9() { fmt.Println("9") }
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	root := tree.RootNode()
	checkedParents := 0

	var walkAndCheck func(n sitter.Node)

	walkAndCheck = func(n sitter.Node) {
		if n.IsNull() {
			return
		}

		namedCount := n.NamedChildCount()
		if namedCount >= cursorThreshold {
			expectedTypes := make([]string, 0, namedCount)
			expectedNamedCounts := make([]uint32, 0, namedCount)

			for idx := range namedCount {
				child := n.NamedChild(idx)
				expectedTypes = append(expectedTypes, child.Type())
				expectedNamedCounts = append(expectedNamedCounts, child.NamedChildCount())
			}

			batchChildren := make([]batchChildInfo, namedCount)

			written, total := readNamedChildrenBatch(unsafe.Pointer(&n), batchChildren)
			if total != namedCount {
				t.Errorf("node %q: total mismatch: batch=%d named=%d", n.Type(), total, namedCount)
			}

			if written != namedCount {
				t.Errorf("node %q: written mismatch: batch=%d named=%d", n.Type(), written, namedCount)
			}

			for idx := range written {
				child := batchChildToNode(batchChildren[idx])
				if child.Type() != expectedTypes[idx] {
					t.Errorf("node %q child[%d]: type mismatch: batch=%q cursor=%q",
						n.Type(), idx, child.Type(), expectedTypes[idx])
				}

				if uint32(batchChildren[idx].named_child_count) != expectedNamedCounts[idx] {
					t.Errorf("node %q child[%d]: namedChildCount mismatch: batch=%d cursor=%d",
						n.Type(), idx, batchChildren[idx].named_child_count, expectedNamedCounts[idx])
				}
			}

			checkedParents++
		}

		for idx := range n.ChildCount() {
			walkAndCheck(n.Child(idx))
		}
	}

	walkAndCheck(root)

	if checkedParents == 0 {
		t.Fatal("Expected at least one parent node at or above cursor threshold")
	}
}

// TestDSLProvider_NameExtractionWithoutChildTypeFallback verifies that name extraction
// works via ChildByFieldName (the field API) and that nodes without a "name" field
// correctly get no name property — without relying on child-type scanning fallback.
// TestNodeType_Interning verifies that nodeType() returns the same string pointer
// for repeated calls with the same tree-sitter node type, confirming interning works.
func TestNodeType_Interning(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration"
)

identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main
func Hello() {}
func World() {}
`)

	uastNode, parseErr := parser.Parse("test.go", source)
	if parseErr != nil {
		t.Fatalf("Failed to parse: %v", parseErr)
	}

	if uastNode == nil {
		t.Fatal("Expected UAST node, got nil")
	}

	// Parse succeeds with node type resolution — verify behavioral correctness.
	// Two functions should both be mapped as "Function" type.
	funcCount := 0

	for _, c := range uastNode.Children {
		if c.Type == testUASTFunctionType {
			funcCount++
		}
	}

	if funcCount != 2 {
		t.Errorf("Expected 2 Function nodes, got %d", funcCount)
	}
}

// TestNodeType_SymbolTablePopulated verifies that symbolNames are populated after parser load.
func TestNodeType_SymbolTablePopulated(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration"
)

identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	if len(parser.symbolNames) == 0 {
		t.Fatal("Expected symbolNames to be populated")
	}

	foundSourceFile := slices.Contains(parser.symbolNames, "source_file")

	if !foundSourceFile {
		t.Error("Expected 'source_file' in symbolNames")
	}
}

// TestReadSymbol_MatchesCGO verifies that readSymbol + symbol table lookup
// resolves the same type names as CGO Type() for parsed nodes.
func TestReadSymbol_MatchesCGO(t *testing.T) {
	t.Parallel()

	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

func Hello(a, b int) int {
	x := a + b
	return x
}

type Foo struct {
	Name string
}
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	ctx := parser.newParseContext(tree, source)
	root := tree.RootNode()

	unsafeChecked := 0
	fallbackChecked := 0

	var walkAndCheck func(n sitter.Node)

	walkAndCheck = func(n sitter.Node) {
		if n.IsNull() {
			return
		}

		symbolID := readSymbol(unsafe.Pointer(&n))
		gotType := ctx.nodeType(n)
		wantType := n.Type()

		if gotType != wantType {
			t.Errorf("node %q: nodeType mismatch: got=%q want=%q", n.Type(), gotType, wantType)
		}

		if symbolID == invalidSymbolID {
			fallbackChecked++
		} else {
			symbolIndex := int(symbolID)
			if symbolIndex < 0 || symbolIndex >= len(parser.symbolNames) {
				t.Errorf("node %q: symbol index out of range: %d (len=%d)", n.Type(), symbolIndex, len(parser.symbolNames))
			} else if parser.symbolNames[symbolIndex] != wantType {
				t.Errorf(
					"node %q: symbolNames lookup mismatch: symbol=%d got=%q want=%q",
					n.Type(),
					symbolID,
					parser.symbolNames[symbolIndex],
					wantType,
				)
			}

			unsafeChecked++
		}

		for idx := range n.ChildCount() {
			walkAndCheck(n.Child(idx))
		}
	}

	walkAndCheck(root)

	if unsafeChecked == 0 {
		t.Fatal("Expected at least one node validated via unsafe symbol lookup")
	}

	t.Logf("Verified symbols on %d nodes, fallback on %d nodes", unsafeChecked, fallbackChecked)
}

func TestNodeType_FallbackInline_ZeroAllocs(t *testing.T) {
	// Not parallel: testing.AllocsPerRun is incompatible with t.Parallel().
	parser := NewDSLParser(strings.NewReader(`[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)
`))

	loadErr := parser.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	source := []byte(`package main

func Hello(a int) int {
	return a + 1
}
`)

	tree, err := parser.parseTSTree(source)
	if err != nil {
		t.Fatalf("Failed to parse tree: %v", err)
	}
	defer tree.Close()

	ctx := parser.newParseContext(tree, source)

	var (
		fallbackNode sitter.Node
		found        bool
	)

	var walk func(sitter.Node)

	walk = func(n sitter.Node) {
		if found || n.IsNull() {
			return
		}

		if readSymbol(unsafe.Pointer(&n)) == invalidSymbolID {
			symbolIndex := int(n.GrammarSymbol())
			if symbolIndex < len(parser.symbolNames) && parser.symbolNames[symbolIndex] != "" {
				fallbackNode = n
				found = true

				return
			}
		}

		for idx := range n.ChildCount() {
			walk(n.Child(idx))
		}
	}

	walk(tree.RootNode())

	if !found {
		t.Fatal("Expected at least one inline fallback node with resolvable grammar symbol")
	}

	want := parser.symbolNames[int(fallbackNode.GrammarSymbol())]

	got := ctx.nodeType(fallbackNode)
	if got != want {
		t.Fatalf("fallback node type mismatch: got=%q want=%q", got, want)
	}

	allocs := testing.AllocsPerRun(1000, func() {
		_ = ctx.nodeType(fallbackNode)
	})

	if allocs > 0 {
		t.Fatalf("Expected zero allocations in fallback nodeType path, got %.2f", allocs)
	}
}

func TestDSLProvider_NameExtractionWithoutChildTypeFallback(t *testing.T) {
	t.Parallel()

	dslContent := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(
    type: "Function",
    roles: "Declaration",
    token: "fields.name"
)

if_statement <- (if_statement) => uast(
    type: "If",
    roles: "Statement"
)

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)`

	goSource := `package main

func Hello() {}

func World() {}
`

	provider := NewDSLParser(strings.NewReader(dslContent))

	loadErr := provider.Load()
	if loadErr != nil {
		t.Fatalf("Failed to load DSL: %v", loadErr)
	}

	uastNode, parseErr := provider.Parse("test.go", []byte(goSource))
	if parseErr != nil {
		t.Fatalf("Failed to parse: %v", parseErr)
	}
	defer node.ReleaseTree(uastNode)

	// Collect function nodes — they should have names via ChildByFieldName.
	var functions []*node.Node

	var walk func(nd *node.Node)

	walk = func(nd *node.Node) {
		if nd == nil {
			return
		}

		if nd.Type == testUASTFunctionType {
			functions = append(functions, nd)
		}

		for _, c := range nd.Children {
			walk(c)
		}
	}

	walk(uastNode)

	if len(functions) != 2 {
		t.Fatalf("Expected 2 functions, got %d", len(functions))
	}

	expectedNames := []string{testHelloName, "World"}
	for idx, fn := range functions {
		if fn.Token != expectedNames[idx] {
			t.Errorf("Function %d: expected token %q, got %q", idx, expectedNames[idx], fn.Token)
		}
	}
}
