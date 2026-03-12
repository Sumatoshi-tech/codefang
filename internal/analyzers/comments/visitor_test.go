package comments

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestVisitor_Basic(t *testing.T) {
	t.Parallel()

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)

	// Create a simple function.
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	// Add function name.
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "simpleFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Add position for function.
	functionNode.Pos = &node.Positions{
		StartLine: 10,
		EndLine:   15,
	}

	// Add a comment above the function.
	commentNode := &node.Node{Type: node.UASTComment, Token: "// simple function"}
	commentNode.Pos = &node.Positions{
		StartLine: 9,
		EndLine:   9,
	}

	root := &node.Node{Type: node.UASTFile}
	root.AddChild(commentNode)
	root.AddChild(functionNode)

	traverser.Traverse(root)

	// Get results.
	report := visitor.GetReport()

	assert.Equal(t, 1, report["total_comments"])
	assert.Equal(t, 1, report["total_functions"])
	assert.Equal(t, 1, report["documented_functions"])
}
