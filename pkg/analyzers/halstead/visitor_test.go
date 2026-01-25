package halstead

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/stretchr/testify/assert"
)

func TestHalsteadVisitor_Basic(t *testing.T) {
	visitor := NewHalsteadVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)

	// Create a simple function
	functionNode := &node.Node{Type: node.UASTFunction}
	functionNode.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}
	
	// Add function name
	nameNode := node.NewNodeWithToken(node.UASTIdentifier, "simpleFunction")
	nameNode.Roles = []node.Role{node.RoleName}
	functionNode.AddChild(nameNode)

	// Add an operator (+)
	opNode := &node.Node{Type: node.UASTBinaryOp}
	opNode.Props = map[string]string{"operator": "+"}
	opNode.Roles = []node.Role{node.RoleOperator}
	functionNode.AddChild(opNode)

	// Add operands (a, b)
	operand1 := &node.Node{Type: node.UASTIdentifier, Token: "a"}
	operand1.Roles = []node.Role{node.RoleVariable}
	functionNode.AddChild(operand1)

	operand2 := &node.Node{Type: node.UASTIdentifier, Token: "b"}
	operand2.Roles = []node.Role{node.RoleVariable}
	functionNode.AddChild(operand2)

	root := &node.Node{Type: node.UASTFile}
	root.AddChild(functionNode)

	traverser.Traverse(root)

	// Get results
	report := visitor.GetReport()
	
	metrics := report["functions"].([]map[string]interface{})
	assert.Equal(t, 1, len(metrics))
	
	fn := metrics[0]
	assert.Equal(t, "simpleFunction", fn["name"])
    
    // Debug assertions
    ops := fn["operators"].(map[string]int)
    operands := fn["operands"].(map[string]int)
    assert.Equal(t, 1, len(ops), "Operators count")
    assert.Equal(t, 3, len(operands), "Operands count")
	
	// 1 operator (+), 2 operands (a, b)
	// Distinct Operators: 1
	// Distinct Operands: 2
	// Vocabulary: 3
	// Length: 3
	// Volume: 3 * log2(3) approx 3 * 1.58 = 4.75
	
	assert.NotZero(t, fn["volume"])
}
