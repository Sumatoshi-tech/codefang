package node //nolint:testpackage // Tests need access to internal types.

import (
	"fmt"
	"reflect"
	"sort"
	"strings"
	"testing"
)

// StringifyAST is a test helper that converts a DSLNode to a string representation.
func stringifyAST(n DSLNode) string {
	if n == nil {
		return "<nil>"
	}

	switch v := n.(type) {
	case *PipelineNode:
		parts := make([]string, 0, len(v.Stages))
		for _, s := range v.Stages {
			parts = append(parts, stringifyAST(s))
		}

		return fmt.Sprintf("Pipeline(%s)", strings.Join(parts, " | "))
	case *MapNode:
		return fmt.Sprintf("Map(%s)", stringifyAST(v.Expr))
	case *FilterNode:
		return fmt.Sprintf("Filter(%s)", stringifyAST(v.Expr))
	case *RMapNode:
		return fmt.Sprintf("RMap(%s)", stringifyAST(v.Expr))
	case *RFilterNode:
		return fmt.Sprintf("RFilter(%s)", stringifyAST(v.Expr))
	case *ReduceNode:
		return fmt.Sprintf("Reduce(%s)", stringifyAST(v.Expr))
	case *FieldNode:
		return fmt.Sprintf("Field(%s)", strings.Join(v.Fields, "."))
	case *LiteralNode:
		return fmt.Sprintf("Literal(%v)", v.Value)
	case *CallNode:
		if len(v.Args) == 0 {
			return fmt.Sprintf("Call(%s)", v.Name)
		}

		args := make([]string, 0, len(v.Args))
		for _, a := range v.Args {
			args = append(args, stringifyAST(a))
		}

		return fmt.Sprintf("Call(%s, %s)", v.Name, strings.Join(args, ", "))
	default:
		return fmt.Sprintf("<%T>", n)
	}
}

func TestDSLParser_Parse_Valid(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		{"map(.children)", "Map(Field(children))"},
		{
			"map(.children) |> filter(.type == \"FunctionDecl\")",
			"Pipeline(Map(Field(children)) | Filter(Call(==, Field(type), Literal(FunctionDecl))))",
		},
		{".foo", "Field(foo)"},
		{"42", "Literal(42)"},
		{
			"map(.children) |> filter(.type == \"FunctionDecl\") |> reduce(count)",
			"Pipeline(Map(Field(children)) | Filter(Call(==, Field(type), Literal(FunctionDecl))) | Reduce(Call(count)))",
		},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_Parse_Invalid(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantErr string
	}{
		// {"bad", "parse error at 1:1: unexpected token 'bad'"}.
		{"@#$", "parse error at 1:1: unknown input"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			_, err := ParseDSL(tc.input)

			if err == nil || err.Error() != tc.wantErr {
				t.Errorf("got error %v, want %q", err, tc.wantErr)
			}
		})
	}
}

func TestDSLParser_LoweringAndExecution(t *testing.T) {
	t.Parallel()

	testUAST := &Node{
		Type:  "Root",
		Props: map[string]string{"foo": "bar", "type": "FunctionDecl"},
		Roles: []Role{"Root"},
		Children: []*Node{
			{Type: "Child", Props: map[string]string{"foo": "baz", "type": "Other"}, Roles: []Role{"Other"}},
			{Type: "Child", Props: map[string]string{"foo": "qux", "type": "FunctionDecl"}, Roles: []Role{"FunctionDecl"}},
		},
	}

	cases := []struct {
		dsl     string
		input   *Node
		want    []string // Expected output tokens.
		wantErr bool
	}{
		{".foo", testUAST, []string{"bar"}, false},
		{"42", testUAST, []string{"42"}, false},
		{"map(.foo)", testUAST, []string{"bar"}, false},
		{"reduce(count)", testUAST, []string{"1"}, false},
		{"map(.foo) |> reduce(count)", testUAST, []string{"1"}, false},
		{"bad syntax", testUAST, nil, true},
		{"map(.notfound)", testUAST, []string{}, false},
	}

	for _, tc := range cases {
		t.Run(tc.dsl, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.dsl)
			if tc.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}

				return
			}

			if err != nil {
				t.Fatalf("parse error: %v", err)
			}

			qf, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("lowering error: %v", err)
			}

			out := qf([]*Node{tc.input})

			var got []string

			for _, n := range out {
				got = append(got, n.Token)
			}

			if len(got) != len(tc.want) {
				t.Errorf("%s got %v, want %v", tc.dsl, got, tc.want)

				return
			}

			for i := range got {
				if got[i] != tc.want[i] {
					t.Errorf("%s got %v, want %v", tc.dsl, got, tc.want)
				}
			}
		})
	}
}

func TestDSLParser_Parse_MembershipAndLogical(t *testing.T) {
	t.Parallel()

	query := "filter(.type == \"Function\" && .roles has \"Exported\")"

	_, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("ParseDSL failed: %v", err)
	}
}

func TestDSLParser_RecursiveFunctions(t *testing.T) {
	t.Parallel()

	testUAST := &Node{
		Type:  "Root",
		Props: map[string]string{"foo": "bar", "type": "FunctionDecl"},
		Children: []*Node{
			{Type: "Child", Props: map[string]string{"foo": "baz", "type": "Other"}},
			{Type: "Child", Props: map[string]string{"foo": "qux", "type": "FunctionDecl"}},
		},
	}

	cases := []struct {
		dsl     string
		input   *Node
		want    []string // Expected output tokens.
		wantErr bool
	}{
		// Recursive filter: should find all FunctionDecl nodes in the entire tree.
		{"rfilter(.props.type == \"FunctionDecl\")", testUAST, []string{"Root", "Child"}, false},
		// Recursive map: should map all foo values in the entire tree.
		{"rmap(.foo)", testUAST, []string{"bar", "baz", "qux"}, false},
		// Recursive filter + recursive map: should find FunctionDecl nodes and map their foo values recursively.
		{"rfilter(.props.type == \"FunctionDecl\") |> rmap(.foo)", testUAST, []string{"bar", "baz", "qux", "qux"}, false},
		// Recursive filter + non-recursive map: should find FunctionDecl nodes but only map those specific nodes.
		{"rfilter(.props.type == \"FunctionDecl\") |> map(.foo)", testUAST, []string{"bar", "qux"}, false},
	}

	for _, tc := range cases {
		t.Run(tc.dsl, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.dsl)
			if tc.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}

				return
			}

			if err != nil {
				t.Fatalf("parse error: %v", err)
			}

			qf, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("lowering error: %v", err)
			}

			out := qf([]*Node{tc.input})

			var got []string

			for _, n := range out {
				// For filter results, use the node type; for map results, use the token.
				if strings.Contains(tc.dsl, "rfilter") && !strings.Contains(tc.dsl, "map") {
					got = append(got, string(n.Type))
				} else {
					got = append(got, n.Token)
				}
			}

			if len(got) != len(tc.want) {
				t.Errorf("%s got %v, want %v", tc.dsl, got, tc.want)

				return
			}

			for i := range got {
				if got[i] != tc.want[i] {
					t.Errorf("got %v, want %v", got, tc.want)
				}
			}
		})
	}
}

func TestNestedFieldAccess(t *testing.T) {
	t.Parallel()

	// Test parsing nested field access.
	ast, err := ParseDSL(".props.name")
	if err != nil {
		t.Fatalf("Failed to parse nested field access: %v", err)
	}

	fieldNode, ok := ast.(*FieldNode)
	if !ok {
		t.Fatalf("Expected FieldNode, got %T", ast)
	}

	if len(fieldNode.Fields) != 2 {
		t.Fatalf("Expected 2 fields, got %d", len(fieldNode.Fields))
	}

	if fieldNode.Fields[0] != "props" {
		t.Errorf("Expected first field to be 'props', got '%s'", fieldNode.Fields[0])
	}

	if fieldNode.Fields[1] != "name" {
		t.Errorf("Expected second field to be 'name', got '%s'", fieldNode.Fields[1])
	}

	// Test deeper nesting.
	ast, err = ParseDSL(".props.deep.nested.field")
	if err != nil {
		t.Fatalf("Failed to parse deep nested field access: %v", err)
	}

	fieldNode, ok = ast.(*FieldNode)
	if !ok {
		t.Fatalf("Expected FieldNode, got %T", ast)
	}

	expected := []string{"props", "deep", "nested", "field"}

	if len(fieldNode.Fields) != len(expected) {
		t.Fatalf("Expected %d fields, got %d", len(expected), len(fieldNode.Fields))
	}

	for i, expectedField := range expected {
		if fieldNode.Fields[i] != expectedField {
			t.Errorf("Expected field[%d] to be '%s', got '%s'", i, expectedField, fieldNode.Fields[i])
		}
	}
}

func TestNestedFieldAccessExecution(t *testing.T) {
	t.Parallel()

	// Create a test node with nested properties.
	testNode := &Node{
		Type: "Function",
		Props: map[string]string{
			"name": "testFunction",
			"deep": "nestedValue",
		},
	}

	// Test single field access (backward compatibility).
	ast, err := ParseDSL(".type")
	if err != nil {
		t.Fatalf("Failed to parse single field access: %v", err)
	}

	queryFunc, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	results := queryFunc([]*Node{testNode})

	if len(results) != 1 {
		t.Fatalf("Expected 1 result, got %d", len(results))
	}

	if results[0].Token != "Function" {
		t.Errorf("Expected 'Function', got '%s'", results[0].Token)
	}

	// Test nested field access.
	ast, err = ParseDSL(".props.name")
	if err != nil {
		t.Fatalf("Failed to parse nested field access: %v", err)
	}

	// Debug: print the AST.
	t.Logf("AST: %+v", ast)

	queryFunc, err = LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	results = queryFunc([]*Node{testNode})
	t.Logf("Results: %+v", results)

	if len(results) != 1 {
		t.Fatalf("Expected 1 result, got %d", len(results))
	}

	if results[0].Token != "testFunction" {
		t.Errorf("Expected 'testFunction', got '%s'", results[0].Token)
	}
}

func TestDSLParser_HasSyntax(t *testing.T) {
	t.Parallel()

	// Test cases for "has" syntax.
	testCases := []struct {
		name        string
		query       string
		shouldParse bool
	}{
		{
			name:        "has with roles",
			query:       `.roles has "Function"`,
			shouldParse: true,
		},
		{
			name:        "has with type",
			query:       `.type has "Function"`,
			shouldParse: true,
		},
		{
			name:        "has with props",
			query:       `.props has "name"`,
			shouldParse: true,
		},
		{
			name:        "has with nested field",
			query:       `.props.name has "value"`,
			shouldParse: true,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			// Parse the query.
			ast, err := ParseDSL(tc.query)

			if tc.shouldParse {
				if err != nil {
					t.Fatalf("Failed to parse query '%s': %v", tc.query, err)
				}

				if ast == nil {
					t.Fatalf("AST is nil for query '%s'", tc.query)
				}

				t.Logf("Successfully parsed: %s", tc.query)
			} else {
				if err == nil {
					t.Fatalf("Expected parse error for '%s' but got none", tc.query)
				}

				t.Logf("Expected parse error for: %s", tc.query)
			}
		})
	}
}

func TestDSLParser_HasSyntaxExecution(t *testing.T) {
	t.Parallel()

	// Create a test node with roles.
	testNode := &Node{
		Type:  "Function",
		Token: "testFunction",
		Roles: []Role{"Function", "Declaration"},
		Props: map[string]string{
			"name": "testFunction",
		},
	}

	testCases := []struct {
		name     string
		query    string
		expected bool
	}{
		{
			name:     "has with roles - should be true",
			query:    `filter(.roles has "Function")`,
			expected: true,
		},
		{
			name:     "has with roles - should be false",
			query:    `filter(.roles has "NonExistent")`,
			expected: false,
		},
		{
			name:     "has with type - should be true (type matches)",
			query:    `filter(.type has "Function")`,
			expected: true,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			// Parse the query.
			ast, err := ParseDSL(tc.query)
			if err != nil {
				t.Fatalf("Failed to parse query '%s': %v", tc.query, err)
			}

			// Lower the DSL.
			queryFunc, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("Failed to lower DSL for '%s': %v", tc.query, err)
			}

			// Execute the query.
			results := queryFunc([]*Node{testNode})

			t.Logf("Query: %s", tc.query)
			t.Logf("Results count: %d", len(results))

			if tc.expected {
				if len(results) == 0 {
					t.Errorf("Expected results but got none for query: %s", tc.query)
				}
			} else {
				if len(results) > 0 {
					t.Errorf("Expected no results but got %d for query: %s", len(results), tc.query)
				}
			}
		})
	}
}

func TestPropertyAccess(t *testing.T) {
	t.Parallel()

	// Create a test node with a name property.
	nd := &Node{
		Type: "Function",
		Props: map[string]string{
			"name": "my_function",
		},
	}

	// Test the query: map(.name).
	query := "map(.name)"

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("Failed to parse query: %v", err)
	}

	fn, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	result := fn([]*Node{nd})

	if len(result) == 0 {
		t.Fatalf("Expected result, got empty")
	}

	if result[0].Type != "Literal" {
		t.Errorf("Expected Literal node, got %s", result[0].Type)
	}

	if result[0].Token != "my_function" {
		t.Errorf("Expected token 'my_function', got '%s'", result[0].Token)
	}
}

func TestDebugFunctionNameQuery(t *testing.T) {
	t.Parallel()

	// Create a test node that matches the actual UAST structure from the Perl test.
	nd := &Node{
		Type:  "Function",
		Roles: []Role{"Function", "Declaration"},
		Props: map[string]string{
			"name": "my_function",
		},
		Token: "my_function", // Also set as token.
	}

	// Test the full query: filter(.type == "Function") |> map(.name).
	query := `filter(.type == "Function") |> map(.name)`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("Failed to parse query: %v", err)
	}

	fn, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	result := fn([]*Node{nd})
	t.Logf("Query result: %+v", result)

	if len(result) == 0 {
		t.Fatalf("Expected result, got empty")
	}

	if result[0].Type != "Literal" {
		t.Errorf("Expected Literal node, got %s", result[0].Type)
	}

	if result[0].Token != "my_function" {
		t.Errorf("Expected token 'my_function', got '%s'", result[0].Token)
	}
}

func TestDebugMapFunction(t *testing.T) {
	t.Parallel()

	// Create a test node that matches the actual UAST structure from the Perl test.
	nd := &Node{
		Type:  "Function",
		Roles: []Role{"Function", "Declaration"},
		Props: map[string]string{
			"name": "my_function",
		},
		Token: "my_function",
		Children: []*Node{
			{
				Type:  "Identifier",
				Token: "my_function",
				Roles: []Role{"Name"},
			},
		},
	}

	// Test the exact query from the Perl test.
	query := `filter(.type == "Function") |> map(.name)`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("Failed to parse query: %v", err)
	}

	fn, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	result := fn([]*Node{nd})
	t.Logf("Input node: %+v", nd)
	t.Logf("Query result: %+v", result)

	if len(result) == 0 {
		t.Fatalf("Expected result, got empty")
	}

	if result[0].Type != "Literal" {
		t.Errorf("Expected Literal node, got %s", result[0].Type)
	}

	if result[0].Token != "my_function" {
		t.Errorf("Expected token 'my_function', got '%s'", result[0].Token)
	}
}

func TestDebugFilterPart(t *testing.T) {
	t.Parallel()

	nd := &Node{
		Type:  "Function",
		Roles: []Role{"Function", "Declaration"},
		Props: map[string]string{
			"name": "my_function",
		},
		Token: "my_function",
	}

	// Test just the filter part.
	query := `filter(.type == "Function")`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("Failed to parse query: %v", err)
	}

	fn, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	result := fn([]*Node{nd})
	t.Logf("Filter result: %+v", result)

	if len(result) == 0 {
		t.Fatalf("Expected Function node, got empty")
	}
}

func TestDebugMapPart(t *testing.T) {
	t.Parallel()

	nd := &Node{
		Type:  "Function",
		Roles: []Role{"Function", "Declaration"},
		Props: map[string]string{
			"name": "my_function",
		},
		Token: "my_function",
	}

	// Test just the map part.
	query := `map(.name)`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("Failed to parse query: %v", err)
	}

	fn, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("Failed to lower DSL: %v", err)
	}

	result := fn([]*Node{nd})
	t.Logf("Map result: %+v", result)

	if len(result) == 0 {
		t.Fatalf("Expected result, got empty")
	}
}

func TestHasSyntax(t *testing.T) {
	t.Parallel()

	// Test cases for "has" syntax.
	testCases := []struct {
		name     string
		query    string
		expected bool
	}{
		{
			name:     "has with roles",
			query:    `.roles has "Function"`,
			expected: true,
		},
		{
			name:     "has with type",
			query:    `.type has "Function"`,
			expected: false, // Type is a single value, not a collection.
		},
		{
			name:     "has with props",
			query:    `.props has "name"`,
			expected: false, // Props is a map, not a collection.
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			// Create a test node with roles.
			testNode := &Node{
				Type:  "Function",
				Token: "testFunction",
				Roles: []Role{"Function", "Declaration"},
				Props: map[string]string{
					"name": "testFunction",
				},
			}

			// Parse the query.
			ast, err := ParseDSL(tc.query)
			if err != nil {
				t.Fatalf("Failed to parse query '%s': %v", tc.query, err)
			}

			// Lower the DSL.
			queryFunc, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("Failed to lower DSL for '%s': %v", tc.query, err)
			}

			// Execute the query.
			results := queryFunc([]*Node{testNode})

			t.Logf("Query: %s", tc.query)
			t.Logf("Results: %+v", results)

			if len(results) > 0 {
				t.Logf("First result: %+v", results[0])
			}
		})
	}
}

func TestDSLParser_Operators(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Comparison operators.
		{".type == \"Function\"", "Call(==, Field(type), Literal(Function))"},
		{".type != \"Function\"", "Call(!=, Field(type), Literal(Function))"},
		{".id > 10", "Call(>, Field(id), Literal(10))"},
		{".id >= 10", "Call(>=, Field(id), Literal(10))"},
		{".id < 10", "Call(<, Field(id), Literal(10))"},
		{".id <= 10", "Call(<=, Field(id), Literal(10))"},

		// Logical operators.
		{
			".type == \"Function\" && .roles has \"Exported\"",
			"Call(&&, Call(==, Field(type), Literal(Function)), Call(has, Field(roles), Literal(Exported)))",
		},
		{
			".type == \"Function\" || .type == \"Method\"",
			"Call(||, Call(==, Field(type), Literal(Function)), Call(==, Field(type), Literal(Method)))",
		},
		{"!.type == \"Function\"", "Call(!, Call(==, Field(type), Literal(Function)))"},

		// Parentheses.
		{"(.type == \"Function\")", "Call(==, Field(type), Literal(Function))"},
		{
			"(.type == \"Function\" && .roles has \"Exported\")",
			"Call(&&, Call(==, Field(type), Literal(Function)), Call(has, Field(roles), Literal(Exported)))",
		},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_Literals(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Numbers.
		{"42", "Literal(42)"},
		{"123", "Literal(123)"},
		{"3.14", "Literal(3.14)"},
		{"0", "Literal(0)"},

		// Strings.
		{"\"hello\"", "Literal(hello)"},
		{"'world'", "Literal(world)"},
		{"\"function name\"", "Literal(function name)"},
		{"\"\"", "Literal()"},
		{"''", "Literal()"},

		// Booleans.
		{"true", "Literal(true)"},
		{"false", "Literal(false)"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_FieldAccess(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Simple field access.
		{".type", "Field(type)"},
		{".token", "Field(token)"},
		{".id", "Field(id)"},
		{".children", "Field(children)"},
		{".roles", "Field(roles)"},

		// Nested field access.
		{".props.name", "Field(props.name)"},
		{".props.deep.nested.field", "Field(props.deep.nested.field)"},
		{".props.foo.bar.baz", "Field(props.foo.bar.baz)"},

		// Properties.
		{".name", "Field(name)"},
		{".value", "Field(value)"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_Membership(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		{".roles has \"Function\"", "Call(has, Field(roles), Literal(Function))"},
		{".type has \"Function\"", "Call(has, Field(type), Literal(Function))"},
		{".props has \"name\"", "Call(has, Field(props), Literal(name))"},
		{".props.name has \"value\"", "Call(has, Field(props.name), Literal(value))"},
		{".children has \"Identifier\"", "Call(has, Field(children), Literal(Identifier))"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_Functions(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Map function.
		{"map(.children)", "Map(Field(children))"},
		{"map(.type)", "Map(Field(type))"},
		{"map(.name)", "Map(Field(name))"},
		{"map(.props.name)", "Map(Field(props.name))"},

		// Filter function.
		{"filter(.type == \"Function\")", "Filter(Call(==, Field(type), Literal(Function)))"},
		{"filter(.roles has \"Exported\")", "Filter(Call(has, Field(roles), Literal(Exported)))"},
		{
			"filter(.type == \"Function\" && .roles has \"Exported\")",
			"Filter(Call(&&, Call(==, Field(type), Literal(Function)), Call(has, Field(roles), Literal(Exported))))",
		},

		// Recursive functions.
		{"rmap(.children)", "RMap(Field(children))"},
		{"rfilter(.type == \"Function\")", "RFilter(Call(==, Field(type), Literal(Function)))"},

		// Reduce function.
		{"reduce(count)", "Reduce(Call(count))"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_Pipelines(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Simple pipelines.
		{
			"map(.children) |> filter(.type == \"Function\")",
			"Pipeline(Map(Field(children)) | Filter(Call(==, Field(type), Literal(Function))))",
		},
		{
			"filter(.type == \"Function\") |> map(.name)",
			"Pipeline(Filter(Call(==, Field(type), Literal(Function))) | Map(Field(name)))",
		},
		{
			"map(.children) |> filter(.type == \"Function\") |> reduce(count)",
			"Pipeline(Map(Field(children)) | Filter(Call(==, Field(type), Literal(Function))) | Reduce(Call(count)))",
		},

		// Complex pipelines.
		{
			"rfilter(.type == \"Function\") |> map(.name) |> filter(.name != \"\")",
			"Pipeline(RFilter(Call(==, Field(type), Literal(Function))) | Map(Field(name)) | Filter(Call(!=, Field(name), Literal())))",
		},
		{
			"map(.children) |> filter(.type == \"Function\" || .type == \"Method\") |> map(.name)",
			"Pipeline(Map(Field(children)) | Filter(Call(||, Call(==, Field(type), Literal(Function)), " +
				"Call(==, Field(type), Literal(Method)))) | Map(Field(name)))",
		},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_ComplexQueries(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Complex logical expressions.
		{
			`.type == "Function" && .roles has "Exported" && .name != ""`,
			//nolint:dupword // Repeated Call(&&,) is correct AST representation.
			"Call(&&, Call(&&, Call(==, Field(type), Literal(Function)), " +
				"Call(has, Field(roles), Literal(Exported))), Call(!=, Field(name), Literal()))",
		},
		{
			`.type == "Function" || .type == "Method" || .type == "Constructor"`,
			//nolint:dupword // Repeated Call(||,) is correct AST representation.
			"Call(||, Call(||, Call(==, Field(type), Literal(Function)), " +
				"Call(==, Field(type), Literal(Method))), Call(==, Field(type), Literal(Constructor)))",
		},

		// Nested parentheses.
		{
			`(.type == "Function" && .roles has "Exported") || (.type == "Method" && .roles has "Private")`,
			"Call(||, Call(&&, Call(==, Field(type), Literal(Function)), Call(has, Field(roles), Literal(Exported))), " +
				"Call(&&, Call(==, Field(type), Literal(Method)), Call(has, Field(roles), Literal(Private))))",
		},

		// Complex field access.
		{".props.function.name == \"main\"", "Call(==, Field(props.function.name), Literal(main))"},
		{
			".props.deeply.nested.property.has.value == true",
			"Call(==, Field(props.deeply.nested.property.has.value), Literal(true))",
		},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_InvalidSyntax(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input string
		want  string
	}{
		{"@#$", "parse error at 1:1: unknown input"},
		{"123abc", "parse error: \nparse error near Number (line 1 symbol 1 - line 1 symbol 4):\n\"123\"\n"},
		{"function", "parse error at 1:1: unknown input"},
		{".type = \"Function\"", "parse error: \nparse error near Space (line 1 symbol 6 - line 1 symbol 7):\n\" \"\n"},
		{".type === \"Function\"", "parse error: \nparse error near CompOp (line 1 symbol 7 - line 1 symbol 9):\n\"==\"\n"},
		{".type !==", "parse error: \nparse error near CompOp (line 1 symbol 7 - line 1 symbol 9):\n\"!=\"\n"},
		{".", "parse error at 1:1: unknown input"},
		{"..type", "parse error at 1:1: unknown input"},
		{".123", "parse error at 1:1: unknown input"},
		{"\"unclosed", "parse error at 1:1: unknown input"},
		{"'unclosed", "parse error at 1:1: unknown input"},
		{"1.2.3", "parse error: \nparse error near Number (line 1 symbol 1 - line 1 symbol 4):\n\"1.2\"\n"},
		{"map()", "parse error at 1:1: unknown input"},
		{"filter()", "parse error at 1:1: unknown input"},
		{"reduce()", "parse error at 1:1: unknown input"},
		{"|>", "parse error at 1:1: unknown input"},
		{"map(.children) |>", "parse error: \nparse error near Space (line 1 symbol 15 - line 1 symbol 16):\n\" \"\n"},
		{"|> map(.children)", "parse error at 1:1: unknown input"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			_, err := ParseDSL(tc.input)
			if err == nil {
				t.Fatalf("expected error, got nil")
			}

			got := err.Error()

			if got != tc.want {
				t.Errorf("got error %q, want %q", got, tc.want)
			}
		})
	}
}

func TestDSLParser_Spacing(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input   string
		wantAST string
	}{
		// Basic spacing.
		{".type == \"Function\"", "Call(==, Field(type), Literal(Function))"},
		{"map(.children)", "Map(Field(children))"},
		{"filter(.type == \"Function\")", "Filter(Call(==, Field(type), Literal(Function)))"},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.input)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			got := stringifyAST(ast)

			if got != tc.wantAST {
				t.Errorf("got %q, want %q", got, tc.wantAST)
			}
		})
	}
}

func TestDSLParser_ExecutionWithComplexQueries(t *testing.T) {
	t.Parallel()

	testUAST := &Node{
		Type:  "Root",
		Token: "root",
		Roles: []Role{"Root", "Declaration"},
		Props: map[string]string{
			"name": "root_function",
			"type": "Function",
		},
		Children: []*Node{
			{
				Type:  "Function",
				Token: "function1",
				Roles: []Role{"Function", "Declaration", "Exported"},
				Props: map[string]string{
					"name": "function1",
					"type": "Function",
				},
			},
			{
				Type:  "Method",
				Token: "method1",
				Roles: []Role{"Function", "Declaration", "Private"},
				Props: map[string]string{
					"name": "method1",
					"type": "Method",
				},
			},
			{
				Type:  "Function",
				Token: "function2",
				Roles: []Role{"Function", "Declaration"},
				Props: map[string]string{
					"name": "function2",
					"type": "Function",
				},
			},
		},
	}

	cases := []struct {
		dsl     string
		input   *Node
		want    []string
		wantErr bool
	}{
		// Complex filter with logical operators.
		{"rfilter(.type == \"Function\" && .roles has \"Exported\")", testUAST, []string{"Function"}, false},
		{
			"rfilter(.type == \"Function\" || .type == \"Method\")",
			testUAST, []string{"Function", "Method", "Function"}, false,
		},
		{"rfilter(.type == \"Function\" && .name != \"\")", testUAST, []string{"Function", "Function"}, false},
		// Complex pipeline.
		{"rfilter(.type == \"Function\") |> map(.name)", testUAST, []string{"function1", "function2"}, false},
		{"rfilter(.roles has \"Exported\") |> map(.name)", testUAST, []string{"function1"}, false},
		// Recursive queries.
		{"rfilter(.props.type == \"Function\")", testUAST, []string{"Root", "Function", "Function"}, false},
		{"rfilter(.roles has \"Exported\")", testUAST, []string{"Function"}, false},
		// Complex nested queries.
		{
			"rfilter(.type == \"Function\" && .roles has \"Exported\") |> map(.name)",
			testUAST, []string{"function1"}, false,
		},
	}

	for _, tc := range cases {
		t.Run(tc.dsl, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.dsl)
			if tc.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}

				return
			}

			if err != nil {
				t.Fatalf("parse error: %v", err)
			}

			qf, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("lowering error: %v", err)
			}

			out := qf([]*Node{tc.input})

			var got []string

			for _, n := range out {
				if strings.Contains(tc.dsl, "map") {
					got = append(got, n.Token)
				} else {
					got = append(got, string(n.Type))
				}
			}

			if len(got) != len(tc.want) {
				t.Errorf("got %v, want %v", got, tc.want)

				return
			}

			for i := range got {
				if got[i] != tc.want[i] {
					t.Errorf("got %v, want %v", got, tc.want)
				}
			}
		})
	}
}

func TestDSLParser_Lowering_ComplexOrQuery(t *testing.T) {
	t.Parallel()

	testUAST := &Node{
		Type: "Root",
		Children: []*Node{
			{Type: "If", Token: "if1"},
			{Type: "Loop", Token: "loop1"},
			{Type: "Switch", Token: "switch1"},
			{Type: "Case", Token: "case1"},
			{Type: "Try", Token: "try1"},
			{Type: "Catch", Token: "catch1"},
			{Type: "Throw", Token: "throw1"},
			{Type: "Other", Token: "other1"},
		},
	}

	query := `rfilter(.type == "If" || .type == "Loop" || .type == "Switch"` +
		` || .type == "Case" || .type == "Try" || .type == "Catch" || .type == "Throw")`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("ParseDSL failed: %v", err)
	}

	qf, err := LowerDSL(ast)
	if err != nil {
		t.Fatalf("LowerDSL failed: %v", err)
	}

	out := qf([]*Node{testUAST})
	got := make([]string, 0, len(out))

	for _, n := range out {
		got = append(got, string(n.Type))
	}

	want := []string{"If", "Loop", "Switch", "Case", "Try", "Catch", "Throw"}

	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}

	for i := range got {
		if got[i] != want[i] {
			t.Errorf("at %d: got %v, want %v", i, got[i], want[i])
		}
	}
}

func TestDSLParser_RFilter_DeepNestedStructure(t *testing.T) {
	t.Parallel()

	// Create a deeply nested UAST structure with 3+ levels.
	deepUAST := &Node{
		Type:  "Root",
		Props: map[string]string{"type": "Root", "name": "root"},
		Roles: []Role{"Root"},
		Children: []*Node{
			{
				Type:  "Function",
				Props: map[string]string{"type": "Function", "name": "func1"},
				Roles: []Role{"Function", "Exported"},
				Children: []*Node{
					{
						Type:  "If",
						Props: map[string]string{"type": "If", "name": "if1"},
						Roles: []Role{"If", "Condition"},
						Children: []*Node{
							{
								Type:  "Loop",
								Props: map[string]string{"type": "Loop", "name": "loop1"},
								Roles: []Role{"Loop", "Iteration"},
								Children: []*Node{
									{
										Type:  "Switch",
										Props: map[string]string{"type": "Switch", "name": "switch1"},
										Roles: []Role{"Switch", "Decision"},
										Children: []*Node{
											{
												Type:  "Case",
												Props: map[string]string{"type": "Case", "name": "case1"},
												Roles: []Role{"Case", "Decision"},
											},
											{
												Type:  "Try",
												Props: map[string]string{"type": "Try", "name": "try1"},
												Roles: []Role{"Try", "Exception"},
												Children: []*Node{
													{
														Type:  "Catch",
														Props: map[string]string{"type": "Catch", "name": "catch1"},
														Roles: []Role{"Catch", "Exception"},
													},
													{
														Type:  "Throw",
														Props: map[string]string{"type": "Throw", "name": "throw1"},
														Roles: []Role{"Throw", "Exception"},
													},
												},
											},
										},
									},
								},
							},
						},
					},
					{
						Type:  "Method",
						Props: map[string]string{"type": "Method", "name": "method1"},
						Roles: []Role{"Method", "Private"},
						Children: []*Node{
							{
								Type:  "Constructor",
								Props: map[string]string{"type": "Constructor", "name": "ctor1"},
								Roles: []Role{"Constructor", "Initialization"},
							},
						},
					},
				},
			},
			{
				Type:  "Class",
				Props: map[string]string{"type": "Class", "name": "class1"},
				Roles: []Role{"Class", "Declaration"},
				Children: []*Node{
					{
						Type:  "Field",
						Props: map[string]string{"type": "Field", "name": "field1"},
						Roles: []Role{"Field", "Declaration"},
						Children: []*Node{
							{
								Type:  "Variable",
								Props: map[string]string{"type": "Variable", "name": "var1"},
								Roles: []Role{"Variable", "Declaration"},
							},
						},
					},
				},
			},
		},
	}

	testCases := []struct {
		name     string
		query    string
		expected []string // Expected node names/types to be found.
	}{
		{
			name:     "rfilter_one_node_type",
			query:    `rfilter(.type == "Function")`,
			expected: []string{"func1"},
		},
		{
			name: "rfilter_all_decision_nodes",
			query: `rfilter(.type == "If" || .type == "Loop" || .type == "Switch"` +
				` || .type == "Case" || .type == "Try" || .type == "Catch" || .type == "Throw")`,
			expected: []string{"if1", "loop1", "switch1", "case1", "try1", "catch1", "throw1"},
		},
		{
			name:     "rfilter_all_functions_and_methods",
			query:    `rfilter(.type == "Function" || .type == "Method" || .type == "Constructor")`,
			expected: []string{"func1", "method1", "ctor1"},
		},
		{
			name:     "rfilter_exception_handling",
			query:    `rfilter(.type == "Try" || .type == "Catch" || .type == "Throw")`,
			expected: []string{"try1", "catch1", "throw1"},
		},
		{
			name:     "rfilter_declaration_nodes",
			query:    `rfilter(.type == "Class" || .type == "Field" || .type == "Variable")`,
			expected: []string{"class1", "field1", "var1"},
		},
		{
			name:     "rfilter_with_roles",
			query:    `rfilter(.roles has "Exception")`,
			expected: []string{"try1", "catch1", "throw1"},
		},
		{
			name:  "rfilter_with_name_condition",
			query: `rfilter(.name != "" && .name != "root")`,
			expected: []string{
				"func1", "if1", "loop1", "switch1", "case1", "try1",
				"catch1", "throw1", "method1", "ctor1", "class1", "field1", "var1",
			},
		},
		{
			name: "rfilter_complex_condition",
			query: `rfilter((.type == "Function" && .roles has "Exported")` +
				` || (.type == "Method" && .roles has "Private"))`,
			expected: []string{"func1", "method1"},
		},
		{
			name:     "rfilter_nested_condition",
			query:    `rfilter(.type == "If" || .type == "Loop" || .type == "Switch")`,
			expected: []string{"if1", "loop1", "switch1"},
		},
		{
			name:  "rfilter_with_not_operator",
			query: `rfilter(!(.type == "Root" || .type == "Class"))`,
			expected: []string{
				"func1", "if1", "loop1", "switch1", "case1", "try1",
				"catch1", "throw1", "method1", "ctor1", "field1", "var1",
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.query)
			if err != nil {
				t.Fatalf("ParseDSL failed: %v", err)
			}

			qf, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("LowerDSL failed: %v", err)
			}

			result := qf([]*Node{deepUAST})

			// Extract names from result nodes.
			var foundNames []string

			for _, nd := range result {
				if name, ok := nd.Props["name"]; ok {
					foundNames = append(foundNames, name)
				}
			}

			// Sort both slices for comparison.
			sort.Strings(foundNames)
			sort.Strings(tc.expected)

			if !reflect.DeepEqual(foundNames, tc.expected) {
				t.Errorf("got %v, want %v", foundNames, tc.expected)
			}

			t.Logf("Query: %s", tc.query)
			t.Logf("Found %d nodes: %v", len(foundNames), foundNames)
		})
	}
}

func TestDSLParser_RFilter_PipelineWithRFilter(t *testing.T) {
	t.Parallel()

	// Create a nested structure for pipeline testing.
	nestedUAST := &Node{
		Type:  "Root",
		Props: map[string]string{"type": "Root", "name": "root"},
		Children: []*Node{
			{
				Type:  "Function",
				Props: map[string]string{"type": "Function", "name": "func1", "complexity": "5"},
				Children: []*Node{
					{
						Type:  "If",
						Props: map[string]string{"type": "If", "name": "if1", "complexity": "2"},
					},
					{
						Type:  "Loop",
						Props: map[string]string{"type": "Loop", "name": "loop1", "complexity": "3"},
					},
				},
			},
			{
				Type:  "Function",
				Props: map[string]string{"type": "Function", "name": "func2", "complexity": "8"},
				Children: []*Node{
					{
						Type:  "Switch",
						Props: map[string]string{"type": "Switch", "name": "switch1", "complexity": "4"},
					},
					{
						Type:  "Try",
						Props: map[string]string{"type": "Try", "name": "try1", "complexity": "1"},
					},
				},
			},
		},
	}

	testCases := []struct {
		name     string
		query    string
		expected []string
	}{
		{
			name: "rfilter_then_map_names",
			query: `rfilter(.type == "Function" || .type == "If" || .type == "Loop"` +
				` || .type == "Switch" || .type == "Try") |> map(.name)`,
			expected: []string{"func1", "if1", "loop1", "func2", "switch1", "try1"},
		},
		{
			name:     "rfilter_then_filter_complexity",
			query:    `rfilter(.type == "Function") |> filter(.complexity > "3") |> map(.name)`,
			expected: []string{"func1", "func2"},
		},
		{
			name: "rfilter_then_map_complexity",
			query: `rfilter(.type == "Function" || .type == "If" || .type == "Loop"` +
				` || .type == "Switch" || .type == "Try") |> map(.complexity)`,
			expected: []string{"5", "2", "3", "8", "4", "1"},
		},
		{
			name:     "rfilter_with_condition_then_map",
			query:    `rfilter(.type == "Function" && .complexity > "5") |> map(.name)`,
			expected: []string{"func2"},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			ast, err := ParseDSL(tc.query)
			if err != nil {
				t.Fatalf("ParseDSL failed: %v", err)
			}

			qf, err := LowerDSL(ast)
			if err != nil {
				t.Fatalf("LowerDSL failed: %v", err)
			}

			result := qf([]*Node{nestedUAST})

			// Extract values from result nodes.
			var foundValues []string

			for _, nd := range result {
				if nd.Type == "Literal" {
					foundValues = append(foundValues, nd.Token)
				}
			}

			// Sort both slices for comparison.
			sort.Strings(foundValues)
			sort.Strings(tc.expected)

			if !reflect.DeepEqual(foundValues, tc.expected) {
				t.Errorf("got %v, want %v", foundValues, tc.expected)
			}

			t.Logf("Query: %s", tc.query)
			t.Logf("Found %d values: %v", len(foundValues), foundValues)
		})
	}
}

func TestDSLParser_ComplexityAnalyzerQuery(t *testing.T) {
	t.Parallel()

	// Test the specific query used in the complexity analyzer.
	query := `rfilter(.type == "If" || .type == "Loop" || .type == "Switch"` +
		` || .type == "Case" || .type == "Try" || .type == "Catch"` +
		` || .type == "Throw" || .type == "Conditional" || .type == "While"` +
		` || .type == "For" || .type == "DoWhile" || .type == "ForEach"` +
		` || .type == "Guard" || .type == "Assert" || .type == "Break"` +
		` || .type == "Continue" || .type == "Return" || .type == "Goto"` +
		` || .type == "Label" || .type == "BinaryOp" || .type == "UnaryOp")`

	ast, err := ParseDSL(query)
	if err != nil {
		t.Fatalf("ParseDSL failed: %v", err)
	}

	if ast == nil {
		t.Fatalf("ParseDSL returned nil")
	}

	// Verify it's a recursive filter.
	rfilterNode, ok := ast.(*RFilterNode)
	if !ok {
		t.Fatalf("ParseDSL returned wrong type")
	}

	if rfilterNode.Expr == nil {
		t.Fatalf("ParseDSL returned nil expr")
	}

	// Verify the condition is a complex OR expression.
	callNode, ok := rfilterNode.Expr.(*CallNode)
	if !ok {
		t.Fatalf("ParseDSL returned wrong type")
	}

	if callNode.Name != "||" {
		t.Fatalf("ParseDSL returned wrong name")
	}

	if callNode.Args == nil {
		t.Fatalf("ParseDSL returned nil args")
	}

	if len(callNode.Args) != 2 {
		t.Fatalf("ParseDSL returned wrong args length")
	}
}
