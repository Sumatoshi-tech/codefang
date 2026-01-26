package complexity //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestComplexityVisitor_Basic(t *testing.T) {
	t.Parallel()

	visitor := NewComplexityVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)

	// Create a simple function.
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	// Add function name.
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "simpleFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Add an if statement (complexity +1).
	ifNode := &node.Node{Type: node.UASTIf}
	ifNode.Roles = []node.Role{node.RoleCondition}
	functionNode.AddChild(ifNode)

	// Add nested if (complexity +1, nesting +2).
	nestedIf := &node.Node{Type: node.UASTIf}
	nestedIf.Roles = []node.Role{node.RoleCondition}
	ifNode.AddChild(nestedIf)

	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	traverser.Traverse(root)

	// Get results.
	report := visitor.GetReport()

	assert.Equal(t, 1, report["total_functions"])
	assert.Equal(t, 3, report["total_complexity"]) // 1 (base) + 1 (if) + 1 (nested if).
	assert.Equal(t, 3, report["nesting_depth"])    // Max nesting depth (Function -> If -> If).
}
