package main

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TestDetectChangesUsesUASTPackage verifies that the diff command
// uses uast.DetectChanges instead of a local stub implementation.
func TestDetectChangesUsesUASTPackage(t *testing.T) {
	// Create two different nodes to detect changes between
	beforeNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "foo",
	}
	afterNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "bar", // Different token - should be detected as modified
	}

	// Use the package-level DetectChanges function
	changes := uast.DetectChanges(beforeNode, afterNode)

	// Verify changes were detected
	if len(changes) == 0 {
		t.Error("expected at least one change to be detected, got none")
	}
}

// TestLocalDetectChangesWired verifies that the local detectChanges function
// in diff.go actually calls uast.DetectChanges and returns real changes.
func TestLocalDetectChangesWired(t *testing.T) {
	// Create two different nodes
	beforeNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "oldFunction",
	}
	afterNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "newFunction", // Different token
	}

	// Call the local detectChanges function from diff.go
	changes := detectChanges(beforeNode, afterNode, "test1.go", "test2.go")

	// This test will FAIL until we wire detectChanges to uast.DetectChanges
	if len(changes) == 0 {
		t.Error("detectChanges should return at least one change for different nodes, got 0")
	}
}

// TestChangeTypeStringValues verifies that ChangeType.String() returns expected values.
func TestChangeTypeStringValues(t *testing.T) {
	tests := []struct {
		changeType uast.ChangeType
		expected   string
	}{
		{uast.ChangeAdded, "added"},
		{uast.ChangeRemoved, "removed"},
		{uast.ChangeModified, "modified"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			result := tt.changeType.String()
			if result != tt.expected {
				t.Errorf("ChangeType.String() = %q, want %q", result, tt.expected)
			}
		})
	}
}
