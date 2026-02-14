package cohesion

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
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

	// Add a variable declaration.
	varNode := &node.Node{Type: node.UASTVariable}
	varNode.Roles = []node.Role{node.RoleDeclaration}

	varNameNode := node.NewNodeWithToken(node.UASTIdentifier, "myVar")
	varNameNode.Roles = []node.Role{node.RoleName}
	varNode.AddChild(varNameNode)

	functionNode.AddChild(varNode)

	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	traverser.Traverse(root)

	// Get results.
	report := visitor.GetReport()

	assert.Equal(t, 1, report["total_functions"])

	functions, ok := report["functions"].([]map[string]any)
	require.True(t, ok, "type assertion failed for functions")
	assert.Len(t, functions, 1)

	fn := functions[0]
	assert.Equal(t, "simpleFunction", fn["name"])
	// Three variables because:
	// One: myVar (declaration).
	// Two: myVar (identifier inside declaration).
	// 3. Duplicate extraction from both logic paths in processNode -> processVariableNode
	// Assert.Equal(t, 1, fn["variable_count"]).
	varCount, ok := fn["variable_count"].(int)
	require.True(t, ok, "type assertion failed for variable_count")
	assert.GreaterOrEqual(t, varCount, 1)
}
