package node

import (
	"errors"
	"fmt"
	"strings"
)

// ParseDSL parses a DSL query string and returns the root DSLNode AST.
// Returns an error for invalid syntax or unsupported constructs.
func ParseDSL(input string) (DSLNode, error) {
	parser := &QueryDSL{Buffer: input}

	if isParserInitFailed(parser.Init()) {
		return nil, createParserInitError(parser.Init())
	}

	if isParseFailed(parser.Parse()) {
		return nil, createParseError(parser.Parse())
	}

	ast := parser.AST()

	return ConvertAST(ast, input), nil
}

func isParserInitFailed(err error) bool {
	return err != nil
}

func createParserInitError(err error) error {
	return fmt.Errorf("parser initialization failed: %w", err)
}

func isParseFailed(err error) bool {
	return err != nil
}

// errUnknownInput is returned when the parser encounters unknown input.
var errUnknownInput = errors.New("parse error at 1:1: unknown input")

func createParseError(err error) error {
	errStr := err.Error()

	if isUnknownInputError(errStr) {
		return errUnknownInput
	}

	return fmt.Errorf("parse error: %w", err)
}

func isUnknownInputError(errStr string) bool {
	return strings.Contains(errStr, "parse error near Unknown")
}

// ConvertAST converts a *node32 parse tree to the legacy DSLNode AST.
func ConvertAST(root *node32, buffer string) DSLNode {
	if isNodeNilNode(root) {
		return nil
	}

	rule := rul3s[root.pegRule]

	return convertNodeByRule(root, rule, buffer)
}

func isNodeNilNode(n *node32) bool {
	return n == nil
}

func isNilDSLNode(n DSLNode) bool {
	return n == nil
}

func convertNodeByRule(n *node32, rule, buffer string) DSLNode {
	converter := getRuleConverter(rule)

	return converter(n, buffer)
}

type nodeConverter func(*node32, string) DSLNode

//nolint:gochecknoglobals // Package-level lookup table for rule converters.
var ruleConverters map[string]nodeConverter

//nolint:gochecknoinits // Initializes the ruleConverters lookup table.
func init() {
	ruleConverters = map[string]nodeConverter{
		"Query":       convertPipelineNode,
		"Pipeline":    convertPipelineNode,
		"Map":         convertMapNode,
		"RMap":        convertRMapNode,
		"Filter":      convertFilterNode,
		"RFilter":     convertRFilterNode,
		"Reduce":      convertReduceNode,
		"FieldAccess": convertFieldAccessNode,
		"Literal":     convertLiteralNode,
		"String":      convertStringNode,
		"Number":      convertNumberBooleanNode,
		"Boolean":     convertNumberBooleanNode,
		"Comparison":  convertComparisonNode,
		"OrExpr":      convertOrExprNode,
		"AndExpr":     convertAndExprNode,
		"NotExpr":     convertNotExprNode,
		"Membership":  convertMembershipNode,
	}
}

func getRuleConverter(rule string) nodeConverter {
	if converter, exists := ruleConverters[rule]; exists {
		return converter
	}

	return convertDefaultNode
}

func convertPipelineNode(n *node32, buffer string) DSLNode {
	stages := collectStages(n, buffer)

	if isSingleStage(stages) {
		return stages[0]
	}

	return &PipelineNode{Stages: stages}
}

func collectStages(n *node32, buffer string) []DSLNode {
	var stages []DSLNode

	for c := n.up; c != nil; c = c.next {
		stage := ConvertAST(c, buffer)

		if isNotNullStage(stage) {
			stages = append(stages, stage)
		}
	}

	return stages
}

func isNotNullStage(stage DSLNode) bool {
	return stage != nil
}

func isSingleStage(stages []DSLNode) bool {
	return len(stages) == 1
}

type exprNodeFactory struct {
	nodeType string
}

func newExprNodeFactory(nodeType string) *exprNodeFactory {
	return &exprNodeFactory{nodeType: nodeType}
}

func (f *exprNodeFactory) create(node *node32, buffer string) DSLNode {
	switch f.nodeType {
	case "Map":
		return &MapNode{Expr: ConvertAST(node.up, buffer)}
	case "RMap":
		return &RMapNode{Expr: ConvertAST(node.up, buffer)}
	case "Filter":
		return &FilterNode{Expr: ConvertAST(node.up, buffer)}
	case "RFilter":
		return &RFilterNode{Expr: ConvertAST(node.up, buffer)}
	default:
		return nil
	}
}

func convertMapNode(n *node32, buffer string) DSLNode {
	return newExprNodeFactory("Map").create(n, buffer)
}

func convertRMapNode(n *node32, buffer string) DSLNode {
	return newExprNodeFactory("RMap").create(n, buffer)
}

func convertFilterNode(n *node32, buffer string) DSLNode {
	return newExprNodeFactory("Filter").create(n, buffer)
}

func convertRFilterNode(n *node32, buffer string) DSLNode {
	return newExprNodeFactory("RFilter").create(n, buffer)
}

func convertReduceNode(n *node32, buffer string) DSLNode {
	if hasUpNode(n) {
		name := extractNodeName(n, buffer)

		return &ReduceNode{Expr: &CallNode{Name: name, Args: nil}}
	}

	return &ReduceNode{Expr: nil}
}

func hasUpNode(n *node32) bool {
	return n.up != nil
}

func extractNodeName(n *node32, buffer string) string {
	return extractNodeText(n.up, buffer)
}

func extractNodeText(node *node32, buffer string) string {
	return buffer[node.begin:node.end]
}

//nolint:iface // Returns DSLNode by design.
func convertFieldAccessNode(n *node32, buffer string) DSLNode {
	var fields []string

	for c := n.up; c != nil; c = c.next {
		if isIdentifierRule(c) {
			fields = append(fields, buffer[c.begin:c.end])
		}
	}

	if len(fields) > 0 {
		return &FieldNode{Fields: fields}
	}

	return nil
}

func isIdentifierRule(c *node32) bool {
	return rul3s[c.pegRule] == "Identifier"
}

func convertLiteralNode(n *node32, buffer string) DSLNode {
	if hasUpNode(n) {
		val := ConvertAST(n.up, buffer)

		if isLiteralNode(val) {
			return val
		}

		return &LiteralNode{Value: val}
	}

	return nil
}

//nolint:iface // Returns DSLNode by design.
func convertStringNode(n *node32, buffer string) DSLNode {
	val := buffer[n.begin:n.end]

	if isQuotedString(val) {
		val = removeQuotes(val)
	}

	return &LiteralNode{Value: val}
}

func isQuotedString(val string) bool {
	return len(val) >= 2 && (val[0] == '"' || val[0] == '\'')
}

func removeQuotes(val string) string {
	return val[1 : len(val)-1]
}

func convertNumberBooleanNode(n *node32, buffer string) DSLNode {
	return createLiteralFromNode(n, buffer)
}

//nolint:iface // Returns DSLNode by design.
func createLiteralFromNode(n *node32, buffer string) DSLNode {
	return &LiteralNode{Value: buffer[n.begin:n.end]}
}

//nolint:iface // Returns DSLNode by design.
func convertComparisonNode(n *node32, buffer string) DSLNode {
	left, right, op := extractComparisonParts(n, buffer)
	left = wrapAsLiteralIfNeeded(left)
	right = wrapAsLiteralIfNeeded(right)

	return &CallNode{Name: op, Args: []DSLNode{left, right}}
}

func extractComparisonParts(n *node32, buffer string) (left, right DSLNode, op string) {
	parts := &comparisonParts{}

	for c := n.up; c != nil; c = c.next {
		parts.processChild(c, buffer)
	}

	return parts.left, parts.right, parts.op
}

type comparisonParts struct {
	left, right DSLNode
	op          string
	valueCount  int
}

func (p *comparisonParts) processChild(child *node32, buffer string) {
	rule := rul3s[child.pegRule]

	switch rule {
	case "Value":
		p.processValue(child, buffer)
	case "CompOp":
		p.op = buffer[child.begin:child.end]
	}
}

func (p *comparisonParts) processValue(c *node32, buffer string) {
	if isFirstValue(p.valueCount) {
		p.left = ConvertAST(c, buffer)
		p.valueCount++
	} else if isSecondValue(p.valueCount) {
		p.right = ConvertAST(c, buffer)
		p.valueCount++
	}
}

func isFirstValue(valueCount int) bool {
	return valueCount == 0
}

func isSecondValue(valueCount int) bool {
	return valueCount == 1
}

func wrapAsLiteralIfNeeded(node DSLNode) DSLNode {
	if isStringNode(node) {
		return &LiteralNode{Value: node}
	}

	return node
}

func isStringNode(node DSLNode) bool {
	_, ok := node.(string)

	return ok
}

func convertOrExprNode(n *node32, buffer string) DSLNode {
	args := collectOrExprArgs(n, buffer)

	if isSingleArg(args) {
		return args[0]
	}

	return foldOrExprArgs(args)
}

func collectOrExprArgs(n *node32, buffer string) []DSLNode {
	var args []DSLNode

	for c := n.up; c != nil; c = c.next {
		child := ConvertAST(c, buffer)

		if isNotNullStage(child) {
			args = append(args, child)
		}
	}

	return args
}

func isSingleArg(args []DSLNode) bool {
	return len(args) == 1
}

func foldOrExprArgs(args []DSLNode) DSLNode {
	cur := args[0]

	for i := 1; i < len(args); i++ {
		cur = &CallNode{Name: "||", Args: []DSLNode{cur, args[i]}}
	}

	return cur
}

func convertAndExprNode(n *node32, buffer string) DSLNode {
	args := collectAndExprArgs(n, buffer)

	if isSingleArg(args) {
		return args[0]
	}

	return foldAndExprArgs(args)
}

func collectAndExprArgs(n *node32, buffer string) []DSLNode {
	var args []DSLNode

	for c := n.up; c != nil; c = c.next {
		child := ConvertAST(c, buffer)

		if isNotNullStage(child) {
			args = append(args, child)
		}
	}

	return args
}

func foldAndExprArgs(args []DSLNode) DSLNode {
	cur := args[0]

	for i := 1; i < len(args); i++ {
		cur = &CallNode{Name: "&&", Args: []DSLNode{cur, args[i]}}
	}

	return cur
}

//nolint:iface // Returns DSLNode by design.
func convertMembershipNode(n *node32, buffer string) DSLNode {
	left, right := extractMembershipParts(n, buffer)

	if isIncompleteMembership(left, right) {
		return nil
	}

	return &CallNode{Name: "has", Args: []DSLNode{left, right}}
}

func extractMembershipParts(n *node32, buffer string) (left, right DSLNode) {
	for c := n.up; c != nil; c = c.next {
		rule := rul3s[c.pegRule]

		if isFieldAccessRule(rule) && isNilDSLNode(left) {
			left = ConvertAST(c, buffer)
		} else if isValueRule(rule) && isNilDSLNode(right) {
			right = ConvertAST(c, buffer)
		}
	}

	return left, right
}

func isFieldAccessRule(rule string) bool {
	return rule == "FieldAccess"
}

func isValueRule(rule string) bool {
	return rule == "Value"
}

func isIncompleteMembership(left, right DSLNode) bool {
	return left == nil || right == nil
}

func convertDefaultNode(n *node32, buffer string) DSLNode {
	if hasUpNode(n) {
		return ConvertAST(n.up, buffer)
	}

	return nil
}

func convertNotExprNode(exprNode *node32, buffer string) DSLNode {
	text := buffer[exprNode.begin:exprNode.end]

	if strings.HasPrefix(text, "!") {
		return convertNotExpression(exprNode, buffer)
	}

	return convertNormalExpression(exprNode, buffer)
}

//nolint:iface // Returns DSLNode by design.
func convertNotExpression(n *node32, buffer string) DSLNode {
	child := findPrimaryExpression(n, buffer)

	if child != nil {
		return &CallNode{Name: "!", Args: []DSLNode{child}}
	}

	return nil
}

func findPrimaryExpression(n *node32, buffer string) DSLNode {
	for c := n.up; c != nil; c = c.next {
		rule := rul3s[c.pegRule]

		if rule == "PrimaryExpr" {
			return ConvertAST(c, buffer)
		}
	}

	return nil
}

func convertNormalExpression(n *node32, buffer string) DSLNode {
	children := collectValidChildren(n, buffer)

	if len(children) > 0 {
		return children[0]
	}

	return nil
}

func collectValidChildren(n *node32, buffer string) []DSLNode {
	var children []DSLNode

	for c := n.up; c != nil; c = c.next {
		child := ConvertAST(c, buffer)

		if child != nil {
			children = append(children, child)
		}
	}

	return children
}
