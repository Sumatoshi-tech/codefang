package halstead

import (
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// operatorTypes maps UAST types that represent operators in Halstead analysis.
var operatorTypes = map[node.Type]bool{
	node.UASTBinaryOp:   true,
	node.UASTUnaryOp:    true,
	node.UASTAssignment: true,
	node.UASTCall:       true,
	node.UASTIndex:      true,
	node.UASTSlice:      true,
	node.UASTReturn:     true,
}

// operatorRoles maps UAST roles that represent operators in Halstead analysis.
var operatorRoles = map[node.Role]bool{
	node.RoleOperator:   true,
	node.RoleAssignment: true,
	node.RoleCall:       true,
	node.RoleReturn:     true,
}

// operandTypes maps UAST types that represent operands in Halstead analysis.
var operandTypes = map[node.Type]bool{
	node.UASTIdentifier: true,
	node.UASTLiteral:    true,
	node.UASTField:      true,
}

// operandRoles maps UAST roles that represent operands in Halstead analysis.
var operandRoles = map[node.Role]bool{
	node.RoleName:     true,
	node.RoleLiteral:  true,
	node.RoleVariable: true,
	node.RoleArgument: true,
}

// declarationTypes maps UAST types that represent declarations.
var declarationTypes = map[node.Type]bool{
	node.UASTFunction:     true,
	node.UASTFunctionDecl: true,
	node.UASTMethod:       true,
	node.UASTParameter:    true,
	node.UASTVariable:     true,
	node.UASTField:        true,
	node.UASTImport:       true,
	node.UASTPackage:      true,
	node.UASTStruct:       true,
	node.UASTClass:        true,
	node.UASTInterface:    true,
	node.UASTEnum:         true,
}

// tokenOperatorSet maps exact operator tokens for O(1) lookup.
var tokenOperatorSet = map[string]bool{
	"===": true, "!==": true, "==": true, "!=": true,
	"<=": true, ">=": true, "&&": true, "||": true,
	"<<=": true, ">>=": true, "<<": true, ">>": true,
	"**": true, ":=": true,
	"+=": true, "-=": true, "*=": true, "/=": true,
	"%=": true, "&=": true, "|=": true, "^=": true,
	"+": true, "-": true, "*": true, "/": true,
	"%": true, "=": true, "<": true, ">": true,
	"&": true, "|": true, "^": true, "!": true,
}

// tokenOperatorsByLength lists operators sorted longest-first for containment matching.
var tokenOperatorsByLength = []string{
	"===", "!==", "<<=", ">>=",
	"==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "**", ":=",
	"+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
	"+", "-", "*", "/", "%", "=", "<", ">", "&", "|", "^", "!",
}

// OperatorOperandDetector handles detection of operators and operands in UAST nodes.
type OperatorOperandDetector struct{}

// NewOperatorOperandDetector creates a new detector.
func NewOperatorOperandDetector() *OperatorOperandDetector {
	return &OperatorOperandDetector{}
}

// CollectOperatorsAndOperands recursively collects operators and operands from a node.
func (d *OperatorOperandDetector) CollectOperatorsAndOperands(
	nd *node.Node, operators, operands map[string]int,
) {
	d.walkNodes(nd, nil, operators, operands)
}

func (d *OperatorOperandDetector) walkNodes(
	nd, parent *node.Node, operators, operands map[string]int,
) {
	if nd == nil {
		return
	}

	if d.IsOperator(nd) {
		d.recordOperator(nd, operators)
	} else {
		d.recordOperand(nd, parent, operands)
	}

	for _, child := range nd.Children {
		d.walkNodes(child, nd, operators, operands)
	}
}

func (d *OperatorOperandDetector) recordOperator(target *node.Node, operators map[string]int) {
	operator := d.GetOperatorName(target)
	if operator == "" {
		return
	}

	operators[string(operator)]++
}

func (d *OperatorOperandDetector) recordOperand(target, parent *node.Node, operands map[string]int) {
	if !d.IsOperand(target) || !d.shouldCountOperand(target, parent) {
		return
	}

	operand := d.GetOperandName(target)
	if operand == "" {
		return
	}

	operands[string(operand)]++
}

// IsOperator determines if a node represents an operator in Halstead complexity analysis.
func (d *OperatorOperandDetector) IsOperator(target *node.Node) bool {
	if target == nil {
		return false
	}

	if operatorTypes[target.Type] {
		return true
	}

	for _, role := range target.Roles {
		if operatorRoles[role] {
			return true
		}
	}

	return false
}

// IsOperand determines if a node represents an operand in Halstead complexity analysis.
func (d *OperatorOperandDetector) IsOperand(target *node.Node) bool {
	if target == nil {
		return false
	}

	if operandTypes[target.Type] {
		return true
	}

	for _, role := range target.Roles {
		if operandRoles[role] {
			return true
		}
	}

	return false
}

// GetOperatorName extracts the operator name from a node.
func (d *OperatorOperandDetector) GetOperatorName(target *node.Node) node.Type {
	if target == nil {
		return ""
	}

	if op, ok := target.Props["operator"]; ok {
		return node.Type(op)
	}

	if op, ok := extractOperatorFromToken(target.Token); ok {
		return node.Type(op)
	}

	if target.Token != "" {
		return node.Type(target.Token)
	}

	return target.Type
}

// GetOperandName extracts the operand name from a node.
func (d *OperatorOperandDetector) GetOperandName(target *node.Node) node.Type {
	if target == nil {
		return ""
	}

	if target.Token != "" {
		return node.Type(target.Token)
	}

	if name, ok := target.Props["name"]; ok {
		return node.Type(name)
	}

	if value, ok := target.Props["value"]; ok {
		return node.Type(value)
	}

	return ""
}

func (d *OperatorOperandDetector) shouldCountOperand(target, parent *node.Node) bool {
	if target == nil {
		return false
	}

	if isDeclarationIdentifier(target, parent) {
		return false
	}

	operand := d.GetOperandName(target)

	return operand != ""
}

func isDeclarationIdentifier(target, parent *node.Node) bool {
	if target == nil || parent == nil {
		return false
	}

	if target.Type != node.UASTIdentifier || !target.HasAnyRole(node.RoleName) {
		return false
	}

	if parent.HasAnyRole(node.RoleDeclaration, node.RoleParameter, node.RoleImport, node.RoleType) {
		return true
	}

	return declarationTypes[parent.Type]
}

func extractOperatorFromToken(token string) (string, bool) {
	if strings.TrimSpace(token) == "" {
		return "", false
	}

	// O(1) exact match against the operator set.
	if tokenOperatorSet[token] {
		return token, true
	}

	// Containment check: longest operators first to match "===" before "==".
	for _, op := range tokenOperatorsByLength {
		if strings.Contains(token, " "+op+" ") {
			return op, true
		}
	}

	return "", false
}
