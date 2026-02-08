package mapping //nolint:testpackage // Tests use internal type switches.

import (
	"context"
	"slices"
	"strings"
	"testing"

	gositter "github.com/alexaandru/go-sitter-forest/go"
	sitter "github.com/alexaandru/go-tree-sitter-bare"
)

func containsStringSlice(slice []string, s string) bool {
	return slices.Contains(slice, s)
}

func TestMatchPattern(t *testing.T) {
	pm := NewPatternMatcher(nil) // Legacy stub.

	_, err := pm.MatchPattern(nil, nil, nil)
	if err == nil || err.Error() != "query or node is nil" {
		t.Errorf("expected query or node is nil error, got %v", err)
	}
}

func TestParseMappingRule(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: @name,
    roles: "Declaration",
    children: @body
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	if len(langInfo.Extensions) != 1 || langInfo.Extensions[0] != ".go" {
		t.Errorf("expected extensions ['.go'], got %v", langInfo.Extensions)
	}

	rule := rules[0]
	if rule.Name != "function_declaration" {
		t.Errorf("expected rule name 'function_declaration', got '%s'", rule.Name)
	}

	if rule.UASTSpec.Type != "Function" {
		t.Errorf("expected UAST type 'Function', got '%s'", rule.UASTSpec.Type)
	}

	if rule.UASTSpec.Token != "@name" {
		t.Errorf("expected token '@name', got '%s'", rule.UASTSpec.Token)
	}

	if len(rule.UASTSpec.Roles) == 0 || rule.UASTSpec.Roles[0] != "Declaration" {
		t.Errorf("expected roles to contain 'Declaration', got %v", rule.UASTSpec.Roles)
	}

	if len(rule.UASTSpec.Children) == 0 || rule.UASTSpec.Children[0] != "@body" {
		t.Errorf("expected children to contain '@body', got %v", rule.UASTSpec.Children)
	}
}

func TestParseMappingRule_MultiRoles(t *testing.T) {
	input := `[language "go", extensions: ".go"]

base_rule <- (base_node) => uast(
    type: "Base",
    roles: "ChildRole", "ExtraRole"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	base := rules[0]
	if base.Name != "base_rule" {
		t.Errorf("unexpected rule names: %v", base.Name)
	}

	if len(base.UASTSpec.Roles) != 2 || base.UASTSpec.Roles[0] != "ChildRole" || base.UASTSpec.Roles[1] != "ExtraRole" {
		t.Errorf("expected roles [ChildRole, ExtraRole], got %v", base.UASTSpec.Roles)
	}
}

func TestParseMappingRule_InheritanceAndConditions(t *testing.T) {
	input := `[language "go", extensions: ".go"]

base_rule <- (base_node) => uast(
    type: "Base",
    roles: "BaseRole"
)

child_rule <- (child_node) => uast(
    type: "Child",
    roles: "ChildRole", "ExtraRole"
) when some_field == "value" and other_field != "bad"

inherited_rule <- (inherited_node) => uast(
    type: "Inherited"
) # Extends base_rule
`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	base := rules[0]
	child := rules[1]

	inherited := rules[2]
	if base.Name != "base_rule" || child.Name != "child_rule" || inherited.Name != "inherited_rule" {
		t.Errorf("unexpected rule names: %v, %v, %v", base.Name, child.Name, inherited.Name)
	}

	if len(child.UASTSpec.Roles) != 2 || child.UASTSpec.Roles[0] != "ChildRole" || child.UASTSpec.Roles[1] != "ExtraRole" {
		t.Errorf("expected roles [ChildRole, ExtraRole], got %v", child.UASTSpec.Roles)
	}

	if len(child.Conditions) != 2 {
		t.Errorf("expected 2 conditions, got %d", len(child.Conditions))
	}

	if !strings.Contains(child.Conditions[0].Expr, "some_field") {
		t.Errorf("expected first condition to mention 'some_field', got %v", child.Conditions[0].Expr)
	}
}

func TestPatternMatcher_CompileAndCache(t *testing.T) {
	pm := NewPatternMatcher(nil) // Legacy stub.

	_, err := pm.CompileAndCache("(identifier) @id")
	if err == nil {
		t.Errorf("expected error for nil language, got nil")
	}
	// Should cache the error result.
	_, err2 := pm.CompileAndCache("(identifier) @id")
	if err2 == nil {
		t.Errorf("expected cached error for nil language, got nil")
	}
}

func TestPatternMatcher_RealGoFunctionMatch(t *testing.T) {
	langPtr := gositter.GetLanguage()
	if langPtr == nil {
		t.Skip("Go language parser not available")
	}

	lang := sitter.NewLanguage(langPtr)
	pm := NewPatternMatcher(lang)

	source := []byte(`package main
func Hello() {}`)
	parser := sitter.NewParser()
	parser.SetLanguage(lang)

	tree, err := parser.ParseString(context.Background(), nil, source)
	if err != nil {
		t.Fatalf("ParseString failed: %v", err)
	}

	root := tree.RootNode()

	pattern := `(
	  (function_declaration
	    name: (identifier) @funcname
	  )
	)`

	query, err := pm.CompileAndCache(pattern)
	if err != nil {
		t.Fatalf("CompileAndCache failed: %v", err)
	}

	captures, err := pm.MatchPattern(query, &root, source)
	if err != nil {
		t.Fatalf("MatchPattern failed: %v", err)
	}

	name, ok := captures["funcname"]
	if !ok {
		t.Fatalf("Expected capture 'funcname' not found in %v", captures)
	}

	if name != "Hello" {
		t.Errorf("Expected function name 'Hello', got '%s'", name)
	}
}

func TestLoadMappings_Valid(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: @name,
    roles: "Declaration",
    children: @body
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if rules[0].Name != "function_declaration" {
		t.Errorf("expected rule name 'function_declaration', got '%s'", rules[0].Name)
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}
}

func TestLoadMappings_Invalid(t *testing.T) {
	input := `function_declaration <- (function_declaration) => uast()` // Missing fields.
	parser := &Parser{}

	_, _, err := parser.ParseMapping(strings.NewReader(input))
	if err == nil {
		t.Error("expected error for invalid mapping DSL, got nil")
	}
}

func TestParseLanguageDeclaration_WithFiles(t *testing.T) {
	input := `[language "cmake", extensions: ".cmake", files: "CMakeLists.txt"]

cmake_list_file <- (cmake_list_file) => uast(type: "CMakeListFile")`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if rules[0].Name != "cmake_list_file" {
		t.Errorf("expected rule name 'cmake_list_file', got '%s'", rules[0].Name)
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "cmake" {
		t.Errorf("expected language name 'cmake', got '%s'", langInfo.Name)
	}

	if len(langInfo.Extensions) != 1 || langInfo.Extensions[0] != ".cmake" {
		t.Errorf("expected extensions ['.cmake'], got %v", langInfo.Extensions)
	}

	if len(langInfo.Files) != 1 || langInfo.Files[0] != "CMakeLists.txt" {
		t.Errorf("expected files ['CMakeLists.txt'], got %v", langInfo.Files)
	}
}

func TestParseLanguageDeclaration_WithMultipleFiles(t *testing.T) {
	input := `[language "dockerfile", extensions: ".dockerfile", files: "Dockerfile", "dockerfile"]

dockerfile_instruction <- (dockerfile_instruction) => uast(type: "Instruction")`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "dockerfile" {
		t.Errorf("expected language name 'dockerfile', got '%s'", langInfo.Name)
	}

	if len(langInfo.Extensions) != 1 || langInfo.Extensions[0] != ".dockerfile" {
		t.Errorf("expected extensions ['.dockerfile'], got %v", langInfo.Extensions)
	}

	expectedFiles := []string{"Dockerfile", "dockerfile"}
	if len(langInfo.Files) != len(expectedFiles) {
		t.Errorf("expected %d files, got %d", len(expectedFiles), len(langInfo.Files))
	} else {
		for i, file := range expectedFiles {
			if langInfo.Files[i] != file {
				t.Errorf("expected file '%s' at position %d, got '%s'", file, i, langInfo.Files[i])
			}
		}
	}
}

func TestParseLanguageDeclaration_ExtensionsOnly(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration) => uast(type: "Function")`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	if len(langInfo.Extensions) != 1 || langInfo.Extensions[0] != ".go" {
		t.Errorf("expected extensions ['.go'], got %v", langInfo.Extensions)
	}

	if len(langInfo.Files) != 0 {
		t.Errorf("expected no files, got %v", langInfo.Files)
	}
}

func TestParseLanguageDeclaration_FilesOnly(t *testing.T) {
	input := `[language "gosum", files: "go.sum"]

checksum <- (checksum) => uast(type: "Checksum")`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "gosum" {
		t.Errorf("expected language name 'gosum', got '%s'", langInfo.Name)
	}

	if len(langInfo.Extensions) != 0 {
		t.Errorf("expected no extensions, got %v", langInfo.Extensions)
	}

	if len(langInfo.Files) != 1 || langInfo.Files[0] != "go.sum" {
		t.Errorf("expected files ['go.sum'], got %v", langInfo.Files)
	}
}

func TestParseLanguageDeclaration_Gotmpl(t *testing.T) {
	input := `[language "gotmpl", extensions: ".gotmpl", ".go.tmpl"]

function_call <- (function_call) => uast(type: "Function")`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "gotmpl" {
		t.Errorf("expected language name 'gotmpl', got '%s'", langInfo.Name)
	}

	expectedExtensions := []string{".gotmpl", ".go.tmpl"}
	if len(langInfo.Extensions) != len(expectedExtensions) {
		t.Errorf("expected %d extensions, got %d", len(expectedExtensions), len(langInfo.Extensions))
	} else {
		for i, ext := range expectedExtensions {
			if langInfo.Extensions[i] != ext {
				t.Errorf("expected extension '%s' at position %d, got '%s'", ext, i, langInfo.Extensions[i])
			}
		}
	}

	if len(langInfo.Files) != 0 {
		t.Errorf("expected no files, got %v", langInfo.Files)
	}
}

func TestParseLanguageDeclaration_WithRules(t *testing.T) {
	input := `[language "javascript", extensions: ".js", ".jsx", ".mjs"]

function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: @name,
    roles: "Declaration",
    children: @body
)

variable_declaration <- (variable_declaration name: (identifier) @name) => uast(
    type: "Variable",
    token: @name,
    roles: "Declaration"
)`

	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "javascript" {
		t.Errorf("expected language name 'javascript', got '%s'", langInfo.Name)
	}

	expectedExtensions := []string{".js", ".jsx", ".mjs"}
	if len(langInfo.Extensions) != len(expectedExtensions) {
		t.Errorf("expected %d extensions, got %d", len(expectedExtensions), len(langInfo.Extensions))
	} else {
		for i, ext := range expectedExtensions {
			if langInfo.Extensions[i] != ext {
				t.Errorf("expected extension '%s' at position %d, got '%s'", ext, i, langInfo.Extensions[i])
			}
		}
	}

	// Test rules.
	if len(rules) != 2 {
		t.Fatalf("expected 2 rules, got %d", len(rules))
	}

	// Check first rule.
	if rules[0].Name != "function_declaration" {
		t.Errorf("expected first rule name 'function_declaration', got '%s'", rules[0].Name)
	}

	if rules[0].UASTSpec.Type != "Function" {
		t.Errorf("expected first rule type 'Function', got '%s'", rules[0].UASTSpec.Type)
	}

	// Check second rule.
	if rules[1].Name != "variable_declaration" {
		t.Errorf("expected second rule name 'variable_declaration', got '%s'", rules[1].Name)
	}

	if rules[1].UASTSpec.Type != "Variable" {
		t.Errorf("expected second rule type 'Variable', got '%s'", rules[1].UASTSpec.Type)
	}
}

func TestParseLanguageDeclaration_MissingDeclaration(t *testing.T) {
	input := `function_declaration <- (function_declaration) => uast(type: "Function")`
	parser := &Parser{}

	_, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err == nil {
		t.Error("expected error for missing language declaration, got nil")
	}

	if langInfo != nil {
		t.Errorf("expected nil language info for missing declaration, got %v", langInfo)
	}
}

func TestParseMappingRule_ChildrenDeduplication(t *testing.T) {
	input := `[language "go", extensions: ".go"]

complex_node <- (complex_node field1: (child1) @c1 (child2) @c2 (child1) @c1) => uast(type: "Complex", children: @c1, @c2)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	children := rules[0].UASTSpec.Children
	if len(children) != 2 || !containsStringSlice(children, "@c1") || !containsStringSlice(children, "@c2") {
		t.Errorf("expected children to contain '@c1' and '@c2', got %v", children)
	}
}

func TestParseMappingRule_InheritanceAndConditions_Advanced(t *testing.T) {
	input := `[language "go", extensions: ".go"]

base_rule <- (base_node) => uast(
    type: "Base",
    roles: "BaseRole"
)

child_rule <- (child_node) => uast(
    type: "Child",
    roles: "ChildRole"
) # Extends base when field == "val"`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 2 {
		t.Fatalf("expected 2 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	child := rules[1]
	if child.Extends != "base" {
		t.Errorf("expected Extends 'base', got '%s'", child.Extends)
	}

	if len(child.Conditions) != 1 || !strings.Contains(child.Conditions[0].Expr, "field == \"val\"") {
		t.Errorf("expected condition on 'field', got %v", child.Conditions)
	}
}

// Tests for advanced token extraction features.
func TestParseMappingRule_TokenExtractionStrategies(t *testing.T) {
	input := `[language "go", extensions: ".go"]

self_token <- (identifier) => uast(
    type: "Identifier",
    token: "self"
)

child_token <- (function_call function: (identifier) @func) => uast(
    type: "Call",
    token: "child:identifier"
)

descendant_token <- (complex_expression) => uast(
    type: "Expression",
    token: "descendant:identifier"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test self token extraction.
	selfRule := rules[0]
	if selfRule.UASTSpec.Token != "self" {
		t.Errorf("expected self token extraction, got '%s'", selfRule.UASTSpec.Token)
	}

	// Test child token extraction.
	childRule := rules[1]
	if childRule.UASTSpec.Token != "child:identifier" {
		t.Errorf("expected child token extraction, got '%s'", childRule.UASTSpec.Token)
	}

	// Test descendant token extraction.
	descRule := rules[2]
	if descRule.UASTSpec.Token != "descendant:identifier" {
		t.Errorf("expected descendant token extraction, got '%s'", descRule.UASTSpec.Token)
	}
}

// Tests for property mapping.
func TestParseMappingRule_PropertyMapping(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@params", "@body",
    name: "@name",
    params: "@params",
    body: "@body"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 1 {
		t.Fatalf("expected 1 rule, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	rule := rules[0]
	expectedProps := map[string]string{
		"name":   "@name",
		"params": "@params",
		"body":   "@body",
	}

	for key, expectedValue := range expectedProps {
		if actualValue, exists := rule.UASTSpec.Props[key]; !exists {
			t.Errorf("expected property '%s', not found", key)
		} else if actualValue != expectedValue {
			t.Errorf("expected property '%s' to be '%s', got '%s'", key, expectedValue, actualValue)
		}
	}
}

// Tests for real-world Go language mapping examples.
func TestParseMappingRule_GoLanguageMapping(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@params", "@body",
    name: "@name",
    params: "@params"
)

method_declaration <- (method_declaration name: (identifier) @name receiver: (parameter_list) @receiver params: (parameter_list) @params body: (block) @body) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method"
) # Extends function_declaration

var_declaration <- (var_declaration name: (identifier) @name type: (type_annotation) @type value: (expression) @value) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    children: "@type", "@value",
    name: "@name",
    type_info: @type
)

if_statement <- (if_statement condition: (expression) @cond consequence: (block) @conseq alternative: (block) @alt) => uast(
    type: "If",
    roles: "Statement", "Conditional",
    children: "@cond", "@conseq", "@alt"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 4 {
		t.Fatalf("expected 4 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test function declaration.
	funcRule := rules[0]
	if funcRule.Name != "function_declaration" {
		t.Errorf("expected function_declaration rule, got '%s'", funcRule.Name)
	}

	if funcRule.UASTSpec.Type != "Function" {
		t.Errorf("expected Function type, got '%s'", funcRule.UASTSpec.Type)
	}

	if len(funcRule.UASTSpec.Roles) != 1 || funcRule.UASTSpec.Roles[0] != "Declaration" {
		t.Errorf("expected Declaration role, got %v", funcRule.UASTSpec.Roles)
	}

	// Test method declaration inheritance.
	methodRule := rules[1]
	if methodRule.Name != "method_declaration" {
		t.Errorf("expected method_declaration rule, got '%s'", methodRule.Name)
	}

	if methodRule.Extends != "function_declaration" {
		t.Errorf("expected inheritance from function_declaration, got '%s'", methodRule.Extends)
	}

	hasDecl := containsStringSlice(methodRule.UASTSpec.Roles, "Declaration")
	hasMethod := containsStringSlice(methodRule.UASTSpec.Roles, "Method")

	if len(methodRule.UASTSpec.Roles) != 2 || !hasDecl || !hasMethod {
		t.Errorf("expected Declaration and Method roles, got %v", methodRule.UASTSpec.Roles)
	}

	// Test variable declaration with descendant property.
	varRule := rules[2]
	if varRule.Name != "var_declaration" {
		t.Errorf("expected var_declaration rule, got '%s'", varRule.Name)
	}

	if varRule.UASTSpec.Type != "Variable" {
		t.Errorf("expected Variable type, got '%s'", varRule.UASTSpec.Type)
	}

	hasVarDecl := containsStringSlice(varRule.UASTSpec.Roles, "Declaration")
	hasVariable := containsStringSlice(varRule.UASTSpec.Roles, "Variable")

	if len(varRule.UASTSpec.Roles) != 2 || !hasVarDecl || !hasVariable {
		t.Errorf("expected Declaration and Variable roles, got %v", varRule.UASTSpec.Roles)
	}

	// Test if statement.
	ifRule := rules[3]
	if ifRule.Name != "if_statement" {
		t.Errorf("expected if_statement rule, got '%s'", ifRule.Name)
	}

	if ifRule.UASTSpec.Type != "If" {
		t.Errorf("expected If type, got '%s'", ifRule.UASTSpec.Type)
	}

	hasStmt := containsStringSlice(ifRule.UASTSpec.Roles, "Statement")
	hasCond := containsStringSlice(ifRule.UASTSpec.Roles, "Conditional")

	if len(ifRule.UASTSpec.Roles) != 2 || !hasStmt || !hasCond {
		t.Errorf("expected Statement and Conditional roles, got %v", ifRule.UASTSpec.Roles)
	}
}

// Tests for Python mapping examples.
func TestParseMappingRule_PythonMapping(t *testing.T) {
	input := `[language "python", extensions: ".py", ".pyw", ".pyi"]

function_definition <- (function_definition name: (identifier) @name parameters: (parameters) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
)

class_definition <- (class_definition name: (identifier) @name body: (block) @body) => uast(
    type: "Class",
    token: "@name",
    roles: "Declaration", "Class",
    children: "@body"
)

base_expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

comparison_expression <- (comparison_expression left: (expression) @left operator: (comparison_operator) @op right: (expression) @right) => uast(
    type: "Comparison",
    token: "@op",
    roles: "Expression", "Comparison"
) # Extends base_expression`
	parser := &Parser{}

	rules, _, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 4 {
		t.Fatalf("expected 4 rules, got %d", len(rules))
	}

	// Test function definition.
	funcDefRule := rules[0]
	if funcDefRule.Name != "function_definition" {
		t.Errorf("expected function_definition rule, got '%s'", funcDefRule.Name)
	}

	if funcDefRule.UASTSpec.Type != "Function" {
		t.Errorf("expected Function type, got '%s'", funcDefRule.UASTSpec.Type)
	}

	hasFuncDecl := containsStringSlice(funcDefRule.UASTSpec.Roles, "Declaration")
	hasFuncRole := containsStringSlice(funcDefRule.UASTSpec.Roles, "Function")

	if len(funcDefRule.UASTSpec.Roles) != 2 || !hasFuncDecl || !hasFuncRole {
		t.Errorf("expected Declaration and Function roles, got %v", funcDefRule.UASTSpec.Roles)
	}

	// Test class definition.
	classDefRule := rules[1]
	if classDefRule.Name != "class_definition" {
		t.Errorf("expected class_definition rule, got '%s'", classDefRule.Name)
	}

	if classDefRule.UASTSpec.Type != "Class" {
		t.Errorf("expected Class type, got '%s'", classDefRule.UASTSpec.Type)
	}

	// Test base expression.
	baseRule := rules[2]
	if baseRule.Name != "base_expression" {
		t.Errorf("expected base_expression rule, got '%s'", baseRule.Name)
	}

	if baseRule.UASTSpec.Type != "Expression" {
		t.Errorf("expected Expression type, got '%s'", baseRule.UASTSpec.Type)
	}

	// Test comparison expression with inheritance.
	compRule := rules[3]
	if compRule.Name != "comparison_expression" {
		t.Errorf("expected comparison_expression rule, got '%s'", compRule.Name)
	}

	if compRule.Extends != "base_expression" {
		t.Errorf("expected inheritance from base_expression, got '%s'", compRule.Extends)
	}
}

// Tests for recipes and best practices.
func TestParseMappingRule_LanguageAgnosticFunctionMapping(t *testing.T) {
	input := `[language "multilang", extensions: ".go", ".js", ".ts"]

function_base <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@body"
)

go_function <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
) # Extends function_base

js_function <- (function_declaration name: (identifier) @name params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
) # Extends function_base`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "multilang" {
		t.Errorf("expected language name 'multilang', got '%s'", langInfo.Name)
	}

	// Test base function pattern.
	baseRule := rules[0]
	if baseRule.Name != "function_base" {
		t.Errorf("expected function_base rule, got '%s'", baseRule.Name)
	}

	hasBaseDecl := containsStringSlice(baseRule.UASTSpec.Roles, "Declaration")
	hasBaseFunc := containsStringSlice(baseRule.UASTSpec.Roles, "Function")

	if len(baseRule.UASTSpec.Roles) != 2 || !hasBaseDecl || !hasBaseFunc {
		t.Errorf("expected Declaration and Function roles, got %v", baseRule.UASTSpec.Roles)
	}

	// Test Go-specific function.
	goRule := rules[1]
	if goRule.Name != "go_function" {
		t.Errorf("expected go_function rule, got '%s'", goRule.Name)
	}

	if goRule.Extends != "function_base" {
		t.Errorf("expected inheritance from function_base, got '%s'", goRule.Extends)
	}

	// Test JavaScript function.
	jsRule := rules[2]
	if jsRule.Name != "js_function" {
		t.Errorf("expected js_function rule, got '%s'", jsRule.Name)
	}

	if jsRule.Extends != "function_base" {
		t.Errorf("expected inheritance from function_base, got '%s'", jsRule.Extends)
	}
}

// Tests for conditional type mapping.
func TestParseMappingRule_ConditionalTypeMapping(t *testing.T) {
	input := `[language "go", extensions: ".go"]

expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

arithmetic_expression <- (binary_expression left: (expression) @left operator: (arithmetic_operator) @op right: (expression) @right) => uast(
    type: "ArithmeticExpression",
    token: "@op",
    roles: "Expression", "Arithmetic"
)

logical_expression <- (binary_expression left: (expression) @left operator: (logical_operator) @op right: (expression) @right) => uast(
    type: "LogicalExpression",
    token: "@op",
    roles: "Expression", "Logical"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test base expression.
	baseRule := rules[0]
	if baseRule.Name != "expression" {
		t.Errorf("expected expression rule, got '%s'", baseRule.Name)
	}

	if baseRule.UASTSpec.Type != "Expression" {
		t.Errorf("expected Expression type, got '%s'", baseRule.UASTSpec.Type)
	}

	// Test arithmetic expression.
	arithRule := rules[1]
	if arithRule.Name != "arithmetic_expression" {
		t.Errorf("expected arithmetic_expression rule, got '%s'", arithRule.Name)
	}

	if arithRule.UASTSpec.Type != "ArithmeticExpression" {
		t.Errorf("expected ArithmeticExpression type, got '%s'", arithRule.UASTSpec.Type)
	}

	// Test logical expression.
	logicRule := rules[2]
	if logicRule.Name != "logical_expression" {
		t.Errorf("expected logical_expression rule, got '%s'", logicRule.Name)
	}

	if logicRule.UASTSpec.Type != "LogicalExpression" {
		t.Errorf("expected LogicalExpression type, got '%s'", logicRule.UASTSpec.Type)
	}
}

// Tests for advanced token extraction recipes.
func TestParseMappingRule_AdvancedTokenExtraction(t *testing.T) {
	input := `[language "go", extensions: ".go"]

function_call <- (call_expression function: (identifier) @func) => uast(
    type: "Call",
    token: "child:identifier",
    roles: "Expression", "Call",
    function: "child:identifier"
)

typed_variable <- (variable_declaration name: (identifier) @name type: (type_annotation) @type) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    name: "child:identifier",
    type_info: "descendant:type_annotation"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 2 {
		t.Fatalf("expected 2 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test function call with child token extraction.
	callRule := rules[0]
	if callRule.Name != "function_call" {
		t.Errorf("expected function_call rule, got '%s'", callRule.Name)
	}

	if callRule.UASTSpec.Token != "child:identifier" {
		t.Errorf("expected child:identifier token, got '%s'", callRule.UASTSpec.Token)
	}

	if callRule.UASTSpec.Props["function"] != "child:identifier" {
		t.Errorf("expected function property, got '%s'", callRule.UASTSpec.Props["function"])
	}

	// Test typed variable with descendant property.
	varRule := rules[1]
	if varRule.Name != "typed_variable" {
		t.Errorf("expected typed_variable rule, got '%s'", varRule.Name)
	}

	if varRule.UASTSpec.Props["type_info"] != "descendant:type_annotation" {
		t.Errorf("expected descendant type property, got '%s'", varRule.UASTSpec.Props["type_info"])
	}
}

// Tests for error handling and validation recipes.
func TestParseMappingRule_ErrorHandlingAndValidation(t *testing.T) {
	input := `[language "go", extensions: ".go"]

safe_property <- (object_property key: (property_identifier) @key value: (expression) @value) => uast(
    type: "Property",
    token: "@key",
    roles: "Property",
    key: "@key",
    value: "@value"
)

conditional_role <- (identifier) => uast(
    type: "Identifier",
    token: "self",
    roles: "Name"
)`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 2 {
		t.Fatalf("expected 2 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test safe property.
	safeRule := rules[0]
	if safeRule.Name != "safe_property" {
		t.Errorf("expected safe_property rule, got '%s'", safeRule.Name)
	}

	if safeRule.UASTSpec.Type != "Property" {
		t.Errorf("expected Property type, got '%s'", safeRule.UASTSpec.Type)
	}

	// Test conditional role assignment.
	condRule := rules[1]
	if condRule.Name != "conditional_role" {
		t.Errorf("expected conditional_role rule, got '%s'", condRule.Name)
	}

	if condRule.UASTSpec.Token != "self" {
		t.Errorf("expected self token, got '%s'", condRule.UASTSpec.Token)
	}
}

// Tests for complex inheritance chains.
func TestParseMappingRule_ComplexInheritanceChain(t *testing.T) {
	input := `[language "go", extensions: ".go"]

base_expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

binary_expression <- (binary_expression left: (expression) @left op: (operator) @op right: (expression) @right) => uast(
    type: "BinaryExpression",
    token: "@op",
    roles: "Expression", "Binary"
) # Extends base_expression

arithmetic_expression <- (arithmetic_expression left: (expression) @left op: (arithmetic_operator) @op right: (expression) @right) => uast(
    type: "ArithmeticExpression",
    token: "@op",
    roles: "Expression", "Arithmetic"
) # Extends binary_expression`
	parser := &Parser{}

	rules, langInfo, err := parser.ParseMapping(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseMapping failed: %v", err)
	}

	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(rules))
	}

	// Test language info.
	if langInfo == nil {
		t.Fatalf("expected language info, got nil")
	}

	if langInfo.Name != "go" {
		t.Errorf("expected language name 'go', got '%s'", langInfo.Name)
	}

	// Test base expression.
	baseRule := rules[0]
	if baseRule.Name != "base_expression" {
		t.Errorf("expected base_expression rule, got '%s'", baseRule.Name)
	}

	// Test binary expression inheritance.
	binaryRule := rules[1]
	if binaryRule.Name != "binary_expression" {
		t.Errorf("expected binary_expression rule, got '%s'", binaryRule.Name)
	}

	if binaryRule.Extends != "base_expression" {
		t.Errorf("expected inheritance from base_expression, got '%s'", binaryRule.Extends)
	}

	// Test arithmetic expression with inheritance.
	arithRule := rules[2]
	if arithRule.Name != "arithmetic_expression" {
		t.Errorf("expected arithmetic_expression rule, got '%s'", arithRule.Name)
	}

	if arithRule.Extends != "binary_expression" {
		t.Errorf("expected inheritance from binary_expression, got '%s'", arithRule.Extends)
	}
}
