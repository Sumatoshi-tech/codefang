package common

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestNewUASTTraverser(t *testing.T) {
	t.Parallel()

	config := TraversalConfig{
		MaxDepth:    10,
		IncludeRoot: true,
	}

	traverser := NewUASTTraverser(config)
	if traverser == nil {
		t.Fatal("NewUASTTraverser returned nil")
	}

	if traverser.config.MaxDepth != 10 {
		t.Errorf("expected MaxDepth 10, got %d", traverser.config.MaxDepth)
	}
}

func TestUASTTraverser_FindNodesByType(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Create test tree.
	root := &node.Node{
		Type: "Program",
		Children: []*node.Node{
			{Type: "FunctionDeclaration"},
			{Type: "VariableDeclaration"},
			{
				Type: "ClassDeclaration",
				Children: []*node.Node{
					{Type: "FunctionDeclaration"},
				},
			},
		},
	}

	// Find FunctionDeclaration nodes.
	nodes := traverser.FindNodesByType(root, []string{"FunctionDeclaration"})
	if len(nodes) != 2 {
		t.Errorf("expected 2 FunctionDeclaration nodes, got %d", len(nodes))
	}

	// Find multiple types.
	nodes = traverser.FindNodesByType(root, []string{"FunctionDeclaration", "VariableDeclaration"})
	if len(nodes) != 3 {
		t.Errorf("expected 3 nodes, got %d", len(nodes))
	}

	// Test empty type filter (matches all).
	nodes = traverser.FindNodesByType(root, []string{})
	if len(nodes) != 5 {
		t.Errorf("expected 5 nodes (all), got %d", len(nodes))
	}

	// Test nil root.
	nodes = traverser.FindNodesByType(nil, []string{"FunctionDeclaration"})
	if nodes != nil {
		t.Error("expected nil for nil root")
	}
}

func TestUASTTraverser_FindNodesByRoles(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Create test tree.
	root := &node.Node{
		Type:  "Program",
		Roles: []node.Role{"File"},
		Children: []*node.Node{
			{Type: "FunctionDeclaration", Roles: []node.Role{"Function", "Declaration"}},
			{Type: "VariableDeclaration", Roles: []node.Role{"Variable", "Declaration"}},
		},
	}

	// Find Function role nodes.
	nodes := traverser.FindNodesByRoles(root, []string{"Function"})
	if len(nodes) != 1 {
		t.Errorf("expected 1 Function node, got %d", len(nodes))
	}

	// Find Declaration role nodes.
	nodes = traverser.FindNodesByRoles(root, []string{"Declaration"})
	if len(nodes) != 2 {
		t.Errorf("expected 2 Declaration nodes, got %d", len(nodes))
	}

	// Test empty role filter (matches all).
	nodes = traverser.FindNodesByRoles(root, []string{})
	if len(nodes) != 3 {
		t.Errorf("expected 3 nodes (all), got %d", len(nodes))
	}

	// Test nil root.
	nodes = traverser.FindNodesByRoles(nil, []string{"Function"})
	if nodes != nil {
		t.Error("expected nil for nil root")
	}
}

func TestUASTTraverser_FindNodesByFilter(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Create test tree with positions.
	root := &node.Node{
		Type: "Program",
		Pos:  &node.Positions{StartLine: 1, EndLine: 100},
		Children: []*node.Node{
			{
				Type:  "FunctionDeclaration",
				Roles: []node.Role{"Function"},
				Pos:   &node.Positions{StartLine: 1, EndLine: 10},
			},
			{
				Type:  "FunctionDeclaration",
				Roles: []node.Role{"Function"},
				Pos:   &node.Positions{StartLine: 20, EndLine: 50},
			},
		},
	}

	// Filter by type and role.
	filter := NodeFilter{
		Types: []string{"FunctionDeclaration"},
		Roles: []string{"Function"},
	}

	nodes := traverser.FindNodesByFilter(root, filter)
	if len(nodes) != 2 {
		t.Errorf("expected 2 nodes, got %d", len(nodes))
	}

	// Filter by min lines.
	filter = NodeFilter{
		MinLines: 20,
	}

	nodes = traverser.FindNodesByFilter(root, filter)
	if len(nodes) != 2 { // Program (100 lines) and second function (31 lines).
		t.Errorf("expected 2 nodes with >= 20 lines, got %d", len(nodes))
	}

	// Filter by max lines.
	filter = NodeFilter{
		MaxLines: 15,
	}

	nodes = traverser.FindNodesByFilter(root, filter)
	if len(nodes) != 1 { // First function (10 lines).
		t.Errorf("expected 1 node with <= 15 lines, got %d", len(nodes))
	}

	// Test nil root.
	nodes = traverser.FindNodesByFilter(nil, filter)
	if nodes != nil {
		t.Error("expected nil for nil root")
	}
}

func TestUASTTraverser_FindNodesByFilters(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Create test tree.
	root := &node.Node{
		Type: "Program",
		Children: []*node.Node{
			{Type: "FunctionDeclaration", Roles: []node.Role{"Function"}},
			{Type: "VariableDeclaration", Roles: []node.Role{"Variable"}},
			{Type: "ClassDeclaration", Roles: []node.Role{"Class"}},
		},
	}

	// Multiple filters.
	filters := []NodeFilter{
		{Types: []string{"FunctionDeclaration"}},
		{Types: []string{"ClassDeclaration"}},
	}

	nodes := traverser.FindNodesByFilters(root, filters)
	if len(nodes) != 2 {
		t.Errorf("expected 2 nodes matching filters, got %d", len(nodes))
	}

	// Test nil root.
	nodes = traverser.FindNodesByFilters(nil, filters)
	if nodes != nil {
		t.Error("expected nil for nil root")
	}
}

func TestUASTTraverser_CountLines(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Test node with position.
	testNode := &node.Node{
		Pos: &node.Positions{StartLine: 10, EndLine: 20},
	}

	lines := traverser.CountLines(testNode)
	if lines != 11 {
		t.Errorf("expected 11 lines, got %d", lines)
	}

	// Test node with children.
	testNode2 := &node.Node{
		Pos: &node.Positions{StartLine: 1, EndLine: 5},
		Children: []*node.Node{
			{Pos: &node.Positions{StartLine: 2, EndLine: 4}},
		},
	}
	lines = traverser.CountLines(testNode2)
	// Parent: 5 lines, Child: 3 lines = 8 total.
	if lines != 8 {
		t.Errorf("expected 8 lines, got %d", lines)
	}

	// Test nil node.
	lines = traverser.CountLines(nil)
	if lines != 0 {
		t.Errorf("expected 0 lines for nil, got %d", lines)
	}

	// Test node without position.
	testNode3 := &node.Node{}

	lines = traverser.CountLines(testNode3)
	if lines != 0 {
		t.Errorf("expected 0 lines without position, got %d", lines)
	}
}

func TestUASTTraverser_GetNodePosition(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	// Test node with position.
	testNode := &node.Node{
		Pos: &node.Positions{StartLine: 10, EndLine: 20},
	}

	startLine, endLine := traverser.GetNodePosition(testNode)
	if startLine != 10 || endLine != 20 {
		t.Errorf("expected (10, 20), got (%d, %d)", startLine, endLine)
	}

	// Test nil node.
	startLine, endLine = traverser.GetNodePosition(nil)
	if startLine != 0 || endLine != 0 {
		t.Errorf("expected (0, 0) for nil, got (%d, %d)", startLine, endLine)
	}

	// Test node without position.
	testNode2 := &node.Node{}

	startLine, endLine = traverser.GetNodePosition(testNode2)
	if startLine != 0 || endLine != 0 {
		t.Errorf("expected (0, 0) without position, got (%d, %d)", startLine, endLine)
	}
}

func TestUASTTraverser_traverse_MaxDepth(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{
		MaxDepth: 1,
	})

	// Create tree with depth > 1.
	root := &node.Node{
		Type: "Level0",
		Children: []*node.Node{
			{
				Type: "Level1",
				Children: []*node.Node{
					{Type: "Level2"}, // Should not be visited due to MaxDepth.
				},
			},
		},
	}

	// Only Level0 and Level1 should be visited.
	nodes := traverser.FindNodesByType(root, []string{})
	if len(nodes) != 2 {
		t.Errorf("expected 2 nodes with MaxDepth=1, got %d", len(nodes))
	}
}

func TestUASTTraverser_traverse_StopVisiting(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	root := &node.Node{
		Type: "Root",
		Children: []*node.Node{
			{Type: "Child1"},
			{Type: "Child2"},
		},
	}

	// Test that visitor returning false stops traversal.
	count := 0

	traverser.traverse(root, 0, func(_ *node.Node, _ int) bool {
		count++

		return false // Stop after first node.
	})

	if count != 1 {
		t.Errorf("expected visitor to stop after 1 node, visited %d", count)
	}
}

func TestUASTTraverser_matchesTypes(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	testNode := &node.Node{Type: "FunctionDeclaration"}

	// Test matching type.
	if !traverser.matchesTypes(testNode, []string{"FunctionDeclaration"}) {
		t.Error("expected match for FunctionDeclaration")
	}

	// Test non-matching type.
	if traverser.matchesTypes(testNode, []string{"VariableDeclaration"}) {
		t.Error("expected no match for VariableDeclaration")
	}

	// Test empty types (matches all).
	if !traverser.matchesTypes(testNode, []string{}) {
		t.Error("expected match for empty types")
	}

	// Test multiple types with one matching.
	if !traverser.matchesTypes(testNode, []string{"VariableDeclaration", "FunctionDeclaration"}) {
		t.Error("expected match for multiple types")
	}
}

func TestUASTTraverser_matchesRoles(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	testNode := &node.Node{
		Roles: []node.Role{"Function", "Declaration"},
	}

	// Test matching role.
	if !traverser.matchesRoles(testNode, []string{"Function"}) {
		t.Error("expected match for Function role")
	}

	// Test non-matching role.
	if traverser.matchesRoles(testNode, []string{"Variable"}) {
		t.Error("expected no match for Variable role")
	}

	// Test empty roles (matches all).
	if !traverser.matchesRoles(testNode, []string{}) {
		t.Error("expected match for empty roles")
	}
}

func TestUASTTraverser_matchesFilter(t *testing.T) {
	t.Parallel()

	traverser := NewUASTTraverser(TraversalConfig{})

	testNode := &node.Node{
		Type:  "FunctionDeclaration",
		Roles: []node.Role{"Function"},
		Pos:   &node.Positions{StartLine: 1, EndLine: 10},
	}

	// Test filter with roles that don't match.
	filter := NodeFilter{
		Roles: []string{"Variable"},
	}
	if traverser.matchesFilter(testNode, filter) {
		t.Error("expected no match for wrong role")
	}

	// Test filter with types that don't match.
	filter = NodeFilter{
		Types: []string{"VariableDeclaration"},
	}
	if traverser.matchesFilter(testNode, filter) {
		t.Error("expected no match for wrong type")
	}

	// Test filter with MinLines larger than node.
	filter = NodeFilter{
		MinLines: 20,
	}
	if traverser.matchesFilter(testNode, filter) {
		t.Error("expected no match for MinLines > node lines")
	}

	// Test filter with MaxLines smaller than node.
	filter = NodeFilter{
		MaxLines: 5,
	}
	if traverser.matchesFilter(testNode, filter) {
		t.Error("expected no match for MaxLines < node lines")
	}

	// Test filter that matches everything.
	filter = NodeFilter{
		Types:    []string{"FunctionDeclaration"},
		Roles:    []string{"Function"},
		MinLines: 5,
		MaxLines: 15,
	}
	if !traverser.matchesFilter(testNode, filter) {
		t.Error("expected match for all criteria")
	}
}
