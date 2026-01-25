package halstead

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// OperatorOperandDetector handles detection of operators and operands in UAST nodes
type OperatorOperandDetector struct{}

// NewOperatorOperandDetector creates a new detector
func NewOperatorOperandDetector() *OperatorOperandDetector {
	return &OperatorOperandDetector{}
}

// CollectOperatorsAndOperands recursively collects operators and operands from a node
func (d *OperatorOperandDetector) CollectOperatorsAndOperands(node *node.Node, operators map[string]int, operands map[string]int) {
	if node == nil {
		return
	}

	if d.IsOperator(node) {
		operator := d.GetOperatorName(node)
		operators[string(operator)]++
	} else if d.IsOperand(node) {
		operand := d.GetOperandName(node)
		operands[string(operand)]++
	}

	for _, child := range node.Children {
		d.CollectOperatorsAndOperands(child, operators, operands)
	}
}

// IsOperator determines if a node represents an operator in Halstead complexity analysis
func (d *OperatorOperandDetector) IsOperator(n *node.Node) bool {
	if n == nil {
		return false
	}

	// Check if the node type indicates an operator (binary operations, assignments, calls, etc.)
	operatorTypes := map[node.Type]bool{
		node.UASTBinaryOp:   true,
		node.UASTUnaryOp:    true,
		node.UASTAssignment: true,
		node.UASTCall:       true,
		node.UASTIndex:      true,
		node.UASTSlice:      true,
	}
	if operatorTypes[n.Type] {
		return true
	}

	// Check if the node has operator-related roles (operator, assignment, function call)
	operatorRoles := map[node.Role]bool{
		node.RoleOperator:   true,
		node.RoleAssignment: true,
		node.RoleCall:       true,
	}
	for _, role := range n.Roles {
		if operatorRoles[role] {
			return true
		}
	}

	return false
}

// IsOperand determines if a node represents an operand in Halstead complexity analysis
func (d *OperatorOperandDetector) IsOperand(n *node.Node) bool {
	if n == nil {
		return false
	}

	// Check if the node type indicates an operand (identifiers, literals, variables, etc.)
	operandTypes := map[node.Type]bool{
		node.UASTIdentifier: true,
		node.UASTLiteral:    true,
		node.UASTVariable:   true,
		node.UASTParameter:  true,
		node.UASTField:      true,
	}
	if operandTypes[n.Type] {
		return true
	}

	// Check if the node has operand-related roles (names, literals, variables, parameters)
	operandRoles := map[node.Role]bool{
		node.RoleName:      true,
		node.RoleLiteral:   true,
		node.RoleVariable:  true,
		node.RoleParameter: true,
		node.RoleArgument:  true,
		node.RoleValue:     true,
	}
	for _, role := range n.Roles {
		if operandRoles[role] {
			return true
		}
	}

	return false
}

// GetOperatorName extracts the operator name from a node
func (d *OperatorOperandDetector) GetOperatorName(n *node.Node) node.Type {
	if n == nil {
		return ""
	}

	if n.Token != "" {
		return node.Type(n.Token)
	}

	if op, ok := n.Props["operator"]; ok {
		return node.Type(op)
	}

	return n.Type
}

// GetOperandName extracts the operand name from a node
func (d *OperatorOperandDetector) GetOperandName(n *node.Node) node.Type {
	if n == nil {
		return ""
	}

	if n.Token != "" {
		return node.Type(n.Token)
	}

	if name, ok := n.Props["name"]; ok {
		return node.Type(name)
	}

	if value, ok := n.Props["value"]; ok {
		return node.Type(value)
	}

	return n.Type
}
