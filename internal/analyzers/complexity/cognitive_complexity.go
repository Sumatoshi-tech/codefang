package complexity

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// CognitiveComplexityCalculator implements the SonarSource cognitive complexity algorithm.
type CognitiveComplexityCalculator struct {
	complexity   int
	sourceCtx    functionSourceContext
	functionName string
}

// NewCognitiveComplexityCalculator creates a new cognitive complexity calculator.
func NewCognitiveComplexityCalculator() *CognitiveComplexityCalculator {
	return &CognitiveComplexityCalculator{}
}

// CalculateCognitiveComplexity calculates cognitive complexity according to SonarSource specification.
func (c *CognitiveComplexityCalculator) CalculateCognitiveComplexity(fn *node.Node) int {
	calculator := &CognitiveComplexityCalculator{
		sourceCtx: newFunctionSourceContext(fn),
	}

	calculator.functionName = calculator.extractFunctionName(fn)

	for idx, child := range fn.Children {
		calculator.walkNode(child, fn, idx, 0)
	}

	return calculator.complexity
}

func (c *CognitiveComplexityCalculator) walkNode(curr, parent *node.Node, childIdx, nesting int) {
	if curr == nil {
		return
	}

	switch curr.Type {
	case node.UASTIf:
		c.processIfNode(curr, parent, childIdx, nesting)

		return
	case node.UASTLoop, node.UASTSwitch, node.UASTTry, node.UASTCatch, node.UASTMatch:
		c.addNestingIncrement(nesting)

		for idx, child := range curr.Children {
			c.walkNode(child, curr, idx, nesting+1)
		}

		return
	case node.UASTLambda:
		for idx, child := range curr.Children {
			c.walkNode(child, curr, idx, nesting+1)
		}

		return
	case node.UASTCall:
		if c.isRecursiveCall(curr) {
			c.complexity++
		}
	}

	for idx, child := range curr.Children {
		c.walkNode(child, curr, idx, nesting)
	}
}

func (c *CognitiveComplexityCalculator) processIfNode(ifNode, parent *node.Node, childIdx, nesting int) {
	if isElseIfNode(parent, ifNode, childIdx) {
		c.complexity++
	} else {
		c.addNestingIncrement(nesting)
	}

	if len(ifNode.Children) > 0 {
		c.addLogicalSequenceComplexity(ifNode.Children[0])
		c.walkNode(ifNode.Children[0], ifNode, 0, nesting)
	}

	if len(ifNode.Children) > 1 {
		c.walkNode(ifNode.Children[1], ifNode, 1, nesting+1)
	}

	for idx := 2; idx < len(ifNode.Children); idx++ {
		child := ifNode.Children[idx]

		switch child.Type {
		case node.UASTIf:
			c.walkNode(child, ifNode, idx, nesting)
		case node.UASTBlock:
			// Sonar/gocognit model: else branch adds one structural increment.
			c.complexity++
			c.walkNode(child, ifNode, idx, nesting)
		default:
			c.walkNode(child, ifNode, idx, nesting)
		}
	}
}

func (c *CognitiveComplexityCalculator) addNestingIncrement(nesting int) {
	c.complexity += nesting + 1
}

func (c *CognitiveComplexityCalculator) addLogicalSequenceComplexity(expr *node.Node) {
	var operators []string
	c.collectLogicalOperators(expr, &operators)

	if len(operators) == 0 {
		return
	}

	c.complexity++

	lastOp := operators[0]
	for _, op := range operators[1:] {
		if op != lastOp {
			c.complexity++
			lastOp = op
		}
	}
}

func (c *CognitiveComplexityCalculator) collectLogicalOperators(curr *node.Node, operators *[]string) {
	if curr == nil {
		return
	}

	if curr.Type == node.UASTBinaryOp && len(curr.Children) >= 2 {
		c.collectLogicalOperators(curr.Children[0], operators)

		op := c.sourceCtx.binaryOperator(curr)
		if isLogicalOperatorToken(op) {
			*operators = append(*operators, op)
		}

		c.collectLogicalOperators(curr.Children[1], operators)

		return
	}

	for _, child := range curr.Children {
		c.collectLogicalOperators(child, operators)
	}
}

func (c *CognitiveComplexityCalculator) extractFunctionName(fn *node.Node) string {
	if fn == nil {
		return ""
	}

	if name, ok := common.ExtractEntityName(fn); ok && name != "" {
		return name
	}

	if fn.Props != nil {
		if name := fn.Props["name"]; name != "" {
			return name
		}
	}

	return ""
}

func (c *CognitiveComplexityCalculator) isRecursiveCall(callNode *node.Node) bool {
	if callNode == nil || c.functionName == "" {
		return false
	}

	callName := c.extractCallName(callNode)
	if callName == "" {
		return false
	}

	return callName == c.functionName
}

func (c *CognitiveComplexityCalculator) extractCallName(callNode *node.Node) string {
	if callNode == nil {
		return ""
	}

	if callNode.Props != nil {
		if name := callNode.Props["name"]; name != "" {
			return name
		}
	}

	for _, child := range callNode.Children {
		if child == nil {
			continue
		}

		if child.HasAnyRole(node.RoleName) && child.Token != "" {
			return child.Token
		}
	}

	if len(callNode.Children) > 0 && callNode.Children[0] != nil {
		if token := callNode.Children[0].Token; token != "" {
			return token
		}
	}

	return ""
}
