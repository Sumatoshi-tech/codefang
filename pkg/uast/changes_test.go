package uast //nolint:testpackage // Tests need access to internal DetectChanges function.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestDetectChanges_NodeAdded(t *testing.T) {
	after := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			EndLine:   3,
		},
	}

	changes := DetectChanges(nil, after)

	if len(changes) != 1 {
		t.Fatalf("Expected 1 change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeAdded {
		t.Errorf("Expected ChangeAdded, got %s", change.Type)
	}

	if change.Before != nil {
		t.Error("Expected Before to be nil for added node")
	}

	if change.After != after {
		t.Error("Expected After to be the added node")
	}
}

func TestDetectChanges_NodeRemoved(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			EndLine:   3,
		},
	}

	changes := DetectChanges(before, nil)

	if len(changes) != 1 {
		t.Fatalf("Expected 1 change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeRemoved {
		t.Errorf("Expected ChangeRemoved, got %s", change.Type)
	}

	if change.Before != before {
		t.Error("Expected Before to be the removed node")
	}

	if change.After != nil {
		t.Error("Expected After to be nil for removed node")
	}
}

func TestDetectChanges_NoChanges(t *testing.T) {
	changes := DetectChanges(nil, nil)

	if len(changes) != 0 {
		t.Fatalf("Expected 0 changes, got %d", len(changes))
	}
}

func TestDetectChanges_NodeModified(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			EndLine:   3,
		},
	}

	after := &node.Node{
		Type:  "go:function",
		Token: "func subtract",
		Pos: &node.Positions{
			StartLine: 1,
			EndLine:   3,
		},
	}

	changes := DetectChanges(before, after)

	if len(changes) != 1 {
		t.Fatalf("Expected 1 change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeModified {
		t.Errorf("Expected ChangeModified, got %s", change.Type)
	}

	if change.Before != before {
		t.Error("Expected Before to be the original node")
	}

	if change.After != after {
		t.Error("Expected After to be the modified node")
	}
}

func TestDetectChanges_TypeChanged(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
	}

	after := &node.Node{
		Type:  "go:method",
		Token: "func add",
	}

	changes := DetectChanges(before, after)

	if len(changes) != 1 {
		t.Fatalf("Expected 1 change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeModified {
		t.Errorf("Expected ChangeModified, got %s", change.Type)
	}
}

func TestDetectChanges_PositionChanged(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			StartCol:  1,
			EndLine:   3,
			EndCol:    10,
		},
	}

	after := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 5,
			StartCol:  1,
			EndLine:   7,
			EndCol:    10,
		},
	}

	changes := DetectChanges(before, after)

	if len(changes) != 1 {
		t.Fatalf("Expected 1 change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeModified {
		t.Errorf("Expected ChangeModified, got %s", change.Type)
	}
}

func TestDetectChanges_PositionChangedMinor(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			StartCol:  1,
			EndLine:   3,
			EndCol:    10,
		},
	}

	after := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Pos: &node.Positions{
			StartLine: 1,
			StartCol:  2,
			EndLine:   3,
			EndCol:    11,
		},
	}

	changes := DetectChanges(before, after)

	// The current implementation considers any position change as a modification.
	// Let's check what type of change we get.
	if len(changes) != 1 {
		t.Fatalf("Expected 1 change for position change, got %d", len(changes))
	}

	change := changes[0]

	if change.Type != ChangeModified {
		t.Errorf("Expected ChangeModified, got %s", change.Type)
	}
}

func TestDetectChanges_ChildrenAdded(t *testing.T) {
	before := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
		},
	}

	after := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
			{Type: "go:function", Token: "func subtract"},
		},
	}

	changes := DetectChanges(before, after)

	if len(changes) != 2 {
		t.Fatalf("Expected 2 changes (parent modified and child added), got %d", len(changes))
	}

	var parentModified, childAdded bool

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "go:file" {
			parentModified = true
		}

		if change.Type == ChangeAdded && change.After != nil && change.After.Token == "func subtract" {
			childAdded = true
		}
	}

	if !parentModified {
		t.Error("Expected parent node to be marked as modified")
	}

	if !childAdded {
		t.Error("Expected to find added child 'func subtract'")
	}
}

func TestDetectChanges_ChildrenRemoved(t *testing.T) {
	before := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
			{Type: "go:function", Token: "func subtract"},
		},
	}

	after := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
		},
	}

	changes := DetectChanges(before, after)

	if len(changes) != 2 {
		t.Fatalf("Expected 2 changes (parent modified and child removed), got %d", len(changes))
	}

	var parentModified, childRemoved bool

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "go:file" {
			parentModified = true
		}

		if change.Type == ChangeRemoved && change.Before != nil && change.Before.Token == "func subtract" {
			childRemoved = true
		}
	}

	if !parentModified {
		t.Error("Expected parent node to be marked as modified")
	}

	if !childRemoved {
		t.Error("Expected to find removed child 'func subtract'")
	}
}

func TestDetectChanges_ChildrenModified(t *testing.T) {
	before := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
		},
	}

	after := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func subtract"},
		},
	}

	changes := DetectChanges(before, after)

	if len(changes) != 3 {
		t.Fatalf("Expected 3 changes (parent modified, child removed, child added), got %d", len(changes))
	}

	var parentModified, childRemoved, childAdded bool

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "go:file" {
			parentModified = true
		}

		if change.Type == ChangeRemoved && change.Before != nil && change.Before.Token == "func add" {
			childRemoved = true
		}

		if change.Type == ChangeAdded && change.After != nil && change.After.Token == "func subtract" {
			childAdded = true
		}
	}

	if !parentModified {
		t.Error("Expected parent node to be marked as modified")
	}

	if !childRemoved {
		t.Error("Expected to find removed child 'func add'")
	}

	if !childAdded {
		t.Error("Expected to find added child 'func subtract'")
	}
}

func TestDetectChanges_GoFunctionBodyChanged(t *testing.T) {
	before := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Children: []*node.Node{
			{Type: "go:block", Token: "{ return a + b }"},
		},
	}

	after := &node.Node{
		Type:  "go:function",
		Token: "func add",
		Children: []*node.Node{
			{Type: "go:block", Token: "{ return a - b }"},
		},
	}

	changes := DetectChanges(before, after)

	// When function body changes, we get modifications for the function and its children.
	if len(changes) != 3 {
		t.Fatalf("Expected 3 changes (function + 2 children), got %d", len(changes))
	}

	// Check that we have function modification.
	hasFunctionModification := false

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "go:function" {
			hasFunctionModification = true

			break
		}
	}

	if !hasFunctionModification {
		t.Error("Expected to find function modification")
	}
}

func TestDetectChanges_JavaClassChanged(t *testing.T) {
	before := &node.Node{
		Type:  "java:class",
		Token: "class Test",
		Children: []*node.Node{
			{Type: "java:field", Token: "private int x;"},
		},
	}

	after := &node.Node{
		Type:  "java:class",
		Token: "class Test",
		Children: []*node.Node{
			{Type: "java:field", Token: "private String name;"},
		},
	}

	changes := DetectChanges(before, after)

	// When class content changes, we get modifications for the class and its children.
	if len(changes) != 3 {
		t.Fatalf("Expected 3 changes (class + 2 children), got %d", len(changes))
	}

	// Check that we have class modification.
	hasClassModification := false

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "java:class" {
			hasClassModification = true

			break
		}
	}

	if !hasClassModification {
		t.Error("Expected to find class modification")
	}
}

func TestDetectChanges_JavaMethodChanged(t *testing.T) {
	before := &node.Node{
		Type:  "java:method",
		Token: "public void test()",
		Children: []*node.Node{
			{Type: "java:block", Token: "{ System.out.println(\"test\"); }"},
		},
	}

	after := &node.Node{
		Type:  "java:method",
		Token: "public void test()",
		Children: []*node.Node{
			{Type: "java:block", Token: "{ System.out.println(\"updated\"); }"},
		},
	}

	changes := DetectChanges(before, after)

	// When method body changes, we get modifications for the method and its children.
	if len(changes) != 3 {
		t.Fatalf("Expected 3 changes (method + 2 children), got %d", len(changes))
	}

	// Check that we have method modification.
	hasMethodModification := false

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "java:method" {
			hasMethodModification = true

			break
		}
	}

	if !hasMethodModification {
		t.Error("Expected to find method modification")
	}
}

func TestDetectChanges_JavaConstructorChanged(t *testing.T) {
	before := &node.Node{
		Type:  "java:constructor",
		Token: "public Test()",
		Children: []*node.Node{
			{Type: "java:block", Token: "{ this.x = 0; }"},
		},
	}

	after := &node.Node{
		Type:  "java:constructor",
		Token: "public Test()",
		Children: []*node.Node{
			{Type: "java:block", Token: "{ this.x = 1; }"},
		},
	}

	changes := DetectChanges(before, after)

	// When constructor body changes, we get modifications for the constructor and its children.
	if len(changes) != 3 {
		t.Fatalf("Expected 3 changes (constructor + 2 children), got %d", len(changes))
	}

	// Check that we have constructor modification.
	hasConstructorModification := false

	for _, change := range changes {
		if change.Type == ChangeModified && change.Before != nil && change.Before.Type == "java:constructor" {
			hasConstructorModification = true

			break
		}
	}

	if !hasConstructorModification {
		t.Error("Expected to find constructor modification")
	}
}

//nolint:gocognit,gocyclo,cyclop // Complex test scenario with many assertions.
func TestDetectChanges_ComplexScenario(t *testing.T) {
	// Test a complex scenario with multiple types of changes.
	before := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},
			{Type: "go:function", Token: "func multiply"},
			{Type: "go:variable", Token: "var x"},
		},
	}

	after := &node.Node{
		Type: "go:file",
		Children: []*node.Node{
			{Type: "go:function", Token: "func add"},      // Unchanged.
			{Type: "go:function", Token: "func subtract"}, // Modified (reported as removed+added).
			{Type: "go:constant", Token: "const y"},       // Added.
			// Removed: func multiply, var x.
		},
	}

	changes := DetectChanges(before, after)

	// Should have 5 changes: 2 removed, 2 added, 1 modified.
	if len(changes) != 5 {
		t.Fatalf("Expected 5 changes, got %d", len(changes))
	}

	var addedConstY, addedFuncSubtract, removedFuncMultiply, removedVarX, modified bool

	for _, change := range changes {
		switch change.Type {
		case ChangeAdded:
			if change.After != nil && change.After.Token == "const y" {
				addedConstY = true
			}

			if change.After != nil && change.After.Token == "func subtract" {
				addedFuncSubtract = true
			}
		case ChangeRemoved:
			if change.Before != nil && change.Before.Token == "func multiply" {
				removedFuncMultiply = true
			}

			if change.Before != nil && change.Before.Token == "var x" {
				removedVarX = true
			}
		case ChangeModified:
			modified = true
		}
	}

	if !addedConstY {
		t.Error("Expected to find added child 'const y'")
	}

	if !addedFuncSubtract {
		t.Error("Expected to find added child 'func subtract'")
	}

	if !removedFuncMultiply {
		t.Error("Expected to find removed child 'func multiply'")
	}

	if !removedVarX {
		t.Error("Expected to find removed child 'var x'")
	}

	if !modified {
		t.Error("Expected to find at least one modification")
	}
}
