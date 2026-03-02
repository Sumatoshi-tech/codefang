package halstead

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Test constants for detector benchmarks.
const (
	// detectorBenchNodes is the number of nodes to process in detector benchmarks.
	detectorBenchNodes = 1000

	// detectorTestOperator is the operator string used in detector tests.
	detectorTestOperator = "+"

	// detectorTestOperand is the operand string used in detector tests.
	detectorTestOperand = "x"

	// detectorTestToken is the token string used in extractOperatorFromToken tests.
	detectorTestToken = "a + b"

	// detectorTestExactToken is the exact operator token used in detector tests.
	detectorTestExactToken = "=="
)

// --- IsOperator Benchmarks ---.

func BenchmarkIsOperator(b *testing.B) {
	detector := NewOperatorOperandDetector()

	opNode := &node.Node{Type: node.UASTBinaryOp}
	opNode.Props = map[string]string{"operator": detectorTestOperator}
	opNode.Roles = []node.Role{node.RoleOperator}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		detector.IsOperator(opNode)
	}
}

func BenchmarkIsOperator_RoleBased(b *testing.B) {
	detector := NewOperatorOperandDetector()

	// Node that is only matched by role, not type.
	roleNode := &node.Node{Type: "CustomType"}
	roleNode.Roles = []node.Role{node.RoleAssignment}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		detector.IsOperator(roleNode)
	}
}

func BenchmarkIsOperator_NotOperator(b *testing.B) {
	detector := NewOperatorOperandDetector()

	// Node that is neither operator type nor role.
	plainNode := &node.Node{Type: node.UASTIdentifier, Token: detectorTestOperand}
	plainNode.Roles = []node.Role{node.RoleVariable}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		detector.IsOperator(plainNode)
	}
}

// --- IsOperand Benchmarks ---.

func BenchmarkIsOperand(b *testing.B) {
	detector := NewOperatorOperandDetector()

	opndNode := &node.Node{Type: node.UASTIdentifier, Token: detectorTestOperand}
	opndNode.Roles = []node.Role{node.RoleVariable}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		detector.IsOperand(opndNode)
	}
}

func BenchmarkIsOperand_RoleBased(b *testing.B) {
	detector := NewOperatorOperandDetector()

	roleNode := &node.Node{Type: "CustomType"}
	roleNode.Roles = []node.Role{node.RoleLiteral}

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		detector.IsOperand(roleNode)
	}
}

// --- extractOperatorFromToken Benchmarks ---.

func BenchmarkExtractOperatorFromToken_Containment(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		extractOperatorFromToken(detectorTestToken)
	}
}

func BenchmarkExtractOperatorFromToken_ExactMatch(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		extractOperatorFromToken(detectorTestExactToken)
	}
}

func BenchmarkExtractOperatorFromToken_NoMatch(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		extractOperatorFromToken(detectorTestOperand)
	}
}

// --- Full Traversal Benchmark ---.

func BenchmarkHalsteadDetector_FullTraversal(b *testing.B) {
	detector := NewOperatorOperandDetector()

	// Build an AST with detectorBenchNodes nodes.
	root := &node.Node{Type: node.UASTFunction}
	root.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

	for i := range detectorBenchNodes {
		if i%2 == 0 {
			opNode := &node.Node{Type: node.UASTBinaryOp}
			opNode.Props = map[string]string{"operator": detectorTestOperator}
			opNode.Roles = []node.Role{node.RoleOperator}
			root.AddChild(opNode)
		} else {
			opndNode := &node.Node{Type: node.UASTIdentifier, Token: detectorTestOperand}
			opndNode.Roles = []node.Role{node.RoleVariable}
			root.AddChild(opndNode)
		}
	}

	operators := make(map[string]int)
	operands := make(map[string]int)

	b.ReportAllocs()
	b.ResetTimer()

	for range b.N {
		clear(operators)
		clear(operands)
		detector.CollectOperatorsAndOperands(root, operators, operands)
	}
}
