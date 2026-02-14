package node

import (
	"errors"
	"fmt"
)

// Boolean result constants.
const (
	boolTrue  = "true"
	boolFalse = "false"
)

// errUnsupportedOperator is returned for unknown call operators.
var errUnsupportedOperator = errors.New("unsupported call operator")

// OperatorRegistry manages operator handlers.
type OperatorRegistry struct {
	handlers map[string]OperatorHandler
}

// OperatorHandler is a function that lowers a CallNode to a QueryFunc.
type OperatorHandler func(*CallNode) (QueryFunc, error)

// NewOperatorRegistry creates a new OperatorRegistry with all built-in operators.
func NewOperatorRegistry() *OperatorRegistry {
	registry := &OperatorRegistry{
		handlers: make(map[string]OperatorHandler),
	}

	// Register all operators.
	registry.Register("||", lowerLogicalOr)
	registry.Register("&&", lowerLogicalAnd)
	registry.Register("==", lowerEquality)
	registry.Register("!=", lowerNotEqual)
	registry.Register("!", lowerNot)
	registry.Register(">", lowerGreaterThan)
	registry.Register(">=", lowerGreaterThanOrEqual)
	registry.Register("<", lowerLessThan)
	registry.Register("<=", lowerLessThanOrEqual)
	registry.Register("has", lowerMembership)

	return registry
}

// Register adds an operator handler by name.
func (registry *OperatorRegistry) Register(name string, handler OperatorHandler) {
	registry.handlers[name] = handler
}

// Handle dispatches a CallNode to the appropriate operator handler.
func (registry *OperatorRegistry) Handle(callNode *CallNode) (QueryFunc, error) {
	handler, exists := registry.handlers[callNode.Name]
	if !exists {
		return nil, fmt.Errorf("%w: %s", errUnsupportedOperator, callNode.Name)
	}

	return handler(callNode)
}

var globalOperatorRegistry = NewOperatorRegistry()

// logicalCombiner defines how to combine two boolean results.
type logicalCombiner func(left, right bool) bool

// lowerLogicalOp creates a QueryFunc for logical operations (and/or).
func lowerLogicalOp(callNode *CallNode, combiner logicalCombiner) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))

		for _, targetNode := range nodes {
			leftResult := leftFunc([]*Node{targetNode})
			rightResult := rightFunc([]*Node{targetNode})
			leftTrue := len(leftResult) > 0 && leftResult[0].Type == UASTLiteral && leftResult[0].Token == boolTrue
			rightTrue := len(rightResult) > 0 && rightResult[0].Type == UASTLiteral && rightResult[0].Token == boolTrue

			if combiner(leftTrue, rightTrue) {
				out = append(out, NewLiteralNode(boolTrue))
			} else {
				out = append(out, NewLiteralNode(boolFalse))
			}
		}

		return out
	}, nil
}

// Operator implementations.
func lowerLogicalOr(callNode *CallNode) (QueryFunc, error) {
	return lowerLogicalOp(callNode, func(left, right bool) bool { return left || right })
}

func lowerLogicalAnd(callNode *CallNode) (QueryFunc, error) {
	return lowerLogicalOp(callNode, func(left, right bool) bool { return left && right })
}

// equalityComparator defines how to compare two token values.
type equalityComparator func(left, right string) bool

// lowerEqualityOp creates a QueryFunc for equality operations (eq/ne).
func lowerEqualityOp(callNode *CallNode, comparator equalityComparator, emptyResult bool) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))

		for _, targetNode := range nodes {
			left := leftFunc([]*Node{targetNode})
			right := rightFunc([]*Node{targetNode})

			switch {
			case len(left) == 0 || len(right) == 0:
				if emptyResult {
					out = append(out, NewLiteralNode(boolTrue))
				} else {
					out = append(out, NewLiteralNode(boolFalse))
				}
			case comparator(left[0].Token, right[0].Token):
				out = append(out, NewLiteralNode(boolTrue))
			default:
				out = append(out, NewLiteralNode(boolFalse))
			}
		}

		return out
	}, nil
}

func lowerEquality(callNode *CallNode) (QueryFunc, error) {
	return lowerEqualityOp(callNode, func(left, right string) bool { return left == right }, false)
}

func lowerNotEqual(callNode *CallNode) (QueryFunc, error) {
	return lowerEqualityOp(callNode, func(left, right string) bool { return left != right }, true)
}

func lowerNot(callNode *CallNode) (QueryFunc, error) {
	argFunc, lowerErr := LowerDSL(callNode.Args[0])
	if lowerErr != nil {
		return nil, lowerErr
	}

	return func(nodes []*Node) []*Node {
		var out []*Node

		for _, targetNode := range nodes {
			result := argFunc([]*Node{targetNode})

			if len(result) == 0 || result[0].Type != UASTLiteral || result[0].Token != boolTrue {
				out = append(out, NewLiteralNode(boolTrue))
			} else {
				out = append(out, NewLiteralNode(boolFalse))
			}
		}

		return out
	}, nil
}

func lowerGreaterThan(callNode *CallNode) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return compareFunc(leftFunc, rightFunc, ">"), nil
}

func lowerGreaterThanOrEqual(callNode *CallNode) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return compareFunc(leftFunc, rightFunc, ">="), nil
}

func lowerLessThan(callNode *CallNode) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return compareFunc(leftFunc, rightFunc, "<"), nil
}

func lowerLessThanOrEqual(callNode *CallNode) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return compareFunc(leftFunc, rightFunc, "<="), nil
}

func lowerMembership(callNode *CallNode) (QueryFunc, error) {
	leftFunc, leftErr := LowerDSL(callNode.Args[0])
	if leftErr != nil {
		return nil, leftErr
	}

	rightFunc, rightErr := LowerDSL(callNode.Args[1])
	if rightErr != nil {
		return nil, rightErr
	}

	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))

		for _, targetNode := range nodes {
			result := checkMembership(leftFunc, rightFunc, targetNode)
			out = append(out, NewLiteralNode(result))
		}

		return out
	}, nil
}

func compareFunc(leftFunc, rightFunc func([]*Node) []*Node, operator string) QueryFunc {
	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))

		for _, targetNode := range nodes {
			left := leftFunc([]*Node{targetNode})
			right := rightFunc([]*Node{targetNode})

			if tokensCompare(left, right, operator) {
				out = append(out, NewLiteralNode(boolTrue))

				continue
			}

			out = append(out, NewLiteralNode(boolFalse))
		}

		return out
	}
}
