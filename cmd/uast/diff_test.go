package main

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TestDetectChangesUsesUASTPackage verifies that the diff command
// uses uast.DetectChanges instead of a local stub implementation.
func TestDetectChangesUsesUASTPackage(t *testing.T) {
	t.Parallel()

	// Create two different nodes to detect changes between.
	beforeNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "foo",
	}
	afterNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "bar", // Different token -- should be detected as modified.
	}

	// Use the package-level DetectChanges function.
	changes := uast.DetectChanges(beforeNode, afterNode)

	// Verify changes were detected.
	if len(changes) == 0 {
		t.Error("expected at least one change to be detected, got none")
	}
}

// TestLocalDetectChangesWired verifies that the local detectChanges function
// in diff.go actually calls uast.DetectChanges and returns real changes.
func TestLocalDetectChangesWired(t *testing.T) {
	t.Parallel()

	// Create two different nodes.
	beforeNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "oldFunction",
	}
	afterNode := &node.Node{
		Type:  node.UASTFunction,
		Token: "newFunction", // Different token.
	}

	// Call the local detectChanges function from diff.go.
	changes := detectChanges(beforeNode, afterNode, "test1.go")

	// This test will FAIL until we wire detectChanges to uast.DetectChanges.
	if len(changes) == 0 {
		t.Error("detectChanges should return at least one change for different nodes, got 0")
	}
}

// TestChangeTypeStringValues verifies that ChangeType.String() returns expected values.
func TestChangeTypeStringValues(t *testing.T) {
	t.Parallel()

	tests := []struct {
		expected   string
		changeType uast.ChangeType
	}{
		{"added", uast.ChangeAdded},
		{"removed", uast.ChangeRemoved},
		{"modified", uast.ChangeModified},
	}

	for _, testCase := range tests {
		t.Run(testCase.expected, func(t *testing.T) {
			t.Parallel()

			result := testCase.changeType.String()
			if result != testCase.expected {
				t.Errorf("ChangeType.String() = %q, want %q", result, testCase.expected)
			}
		})
	}
}
