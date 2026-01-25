package node

import "fmt"

// OperatorRegistry manages operator handlers
type OperatorRegistry struct {
	handlers map[string]OperatorHandler
}

type OperatorHandler func(*CallNode) (QueryFunc, error)

func NewOperatorRegistry() *OperatorRegistry {
	registry := &OperatorRegistry{
		handlers: make(map[string]OperatorHandler),
	}

	// Register all operators
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

func (r *OperatorRegistry) Register(name string, handler OperatorHandler) {
	r.handlers[name] = handler
}

func (r *OperatorRegistry) Handle(n *CallNode) (QueryFunc, error) {
	handler, exists := r.handlers[n.Name]
	if !exists {
		return nil, fmt.Errorf("unsupported call operator: %s", n.Name)
	}
	return handler(n)
}

var globalOperatorRegistry = NewOperatorRegistry()

// logicalCombiner defines how to combine two boolean results
type logicalCombiner func(left, right bool) bool

// lowerLogicalOp creates a QueryFunc for logical operations (and/or)
func lowerLogicalOp(n *CallNode, combiner logicalCombiner) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])
	if err != nil {
		return nil, err
	}
	rightFunc, err := LowerDSL(n.Args[1])
	if err != nil {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))
		for _, node := range nodes {
			leftResult := leftFunc([]*Node{node})
			rightResult := rightFunc([]*Node{node})
			leftTrue := len(leftResult) > 0 && leftResult[0].Type == UASTLiteral && leftResult[0].Token == "true"
			rightTrue := len(rightResult) > 0 && rightResult[0].Type == UASTLiteral && rightResult[0].Token == "true"
			if combiner(leftTrue, rightTrue) {
				out = append(out, NewLiteralNode("true"))
			} else {
				out = append(out, NewLiteralNode("false"))
			}
		}
		return out
	}, nil
}

// Operator implementations
func lowerLogicalOr(n *CallNode) (QueryFunc, error) {
	return lowerLogicalOp(n, func(left, right bool) bool { return left || right })
}

func lowerLogicalAnd(n *CallNode) (QueryFunc, error) {
	return lowerLogicalOp(n, func(left, right bool) bool { return left && right })
}

// equalityComparator defines how to compare two token values
type equalityComparator func(left, right string) bool

// lowerEqualityOp creates a QueryFunc for equality operations (eq/ne)
func lowerEqualityOp(n *CallNode, comparator equalityComparator, emptyResult bool) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])
	if err != nil {
		return nil, err
	}
	rightFunc, err := LowerDSL(n.Args[1])
	if err != nil {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		out := make([]*Node, 0, len(nodes))
		for _, node := range nodes {
			left := leftFunc([]*Node{node})
			right := rightFunc([]*Node{node})
			if len(left) == 0 || len(right) == 0 {
				if emptyResult {
					out = append(out, NewLiteralNode("true"))
				} else {
					out = append(out, NewLiteralNode("false"))
				}
			} else if comparator(left[0].Token, right[0].Token) {
				out = append(out, NewLiteralNode("true"))
			} else {
				out = append(out, NewLiteralNode("false"))
			}
		}
		return out
	}, nil
}

func lowerEquality(n *CallNode) (QueryFunc, error) {
	return lowerEqualityOp(n, func(l, r string) bool { return l == r }, false)
}

func lowerNotEqual(n *CallNode) (QueryFunc, error) {
	return lowerEqualityOp(n, func(l, r string) bool { return l != r }, true)
}

func lowerNot(n *CallNode) (QueryFunc, error) {
	argFunc, err := LowerDSL(n.Args[0])
	if err != nil {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		var out []*Node
		for _, node := range nodes {
			result := argFunc([]*Node{node})
			if len(result) == 0 || result[0].Type != UASTLiteral || result[0].Token != "true" {
				out = append(out, NewLiteralNode("true"))
			} else {
				out = append(out, NewLiteralNode("false"))
			}
		}
		return out
	}, nil
}

func lowerGreaterThan(n *CallNode) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])

	if err != nil {
		return nil, err
	}

	rightFunc, err := LowerDSL(n.Args[1])

	if err != nil {
		return nil, err
	}

	return compareFunc(leftFunc, rightFunc, ">"), nil
}

func lowerGreaterThanOrEqual(n *CallNode) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])

	if err != nil {
		return nil, err
	}

	rightFunc, err := LowerDSL(n.Args[1])

	if err != nil {
		return nil, err
	}

	return compareFunc(leftFunc, rightFunc, ">="), nil
}

func lowerLessThan(n *CallNode) (QueryFunc, error) {
	l, err := LowerDSL(n.Args[0])

	if err != nil {
		return nil, err
	}

	r, err := LowerDSL(n.Args[1])

	if err != nil {
		return nil, err
	}

	return compareFunc(l, r, "<"), nil
}

func lowerLessThanOrEqual(n *CallNode) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])

	if err != nil {
		return nil, err
	}

	rightFunc, err := LowerDSL(n.Args[1])

	if err != nil {
		return nil, err
	}

	return compareFunc(leftFunc, rightFunc, "<="), nil
}

func lowerMembership(n *CallNode) (QueryFunc, error) {
	leftFunc, err := LowerDSL(n.Args[0])
	if err != nil {
		return nil, err
	}
	rightFunc, err := LowerDSL(n.Args[1])
	if err != nil {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		var out []*Node
		for _, node := range nodes {
			result := checkMembership(leftFunc, rightFunc, node)
			out = append(out, NewLiteralNode(result))
		}
		return out
	}, nil
}

func compareFunc(leftFunc, rightFunc func([]*Node) []*Node, operator string) QueryFunc {
	return func(nodes []*Node) []*Node {
		var out []*Node
		for _, node := range nodes {
			l := leftFunc([]*Node{node})
			r := rightFunc([]*Node{node})

			if tokensCompare(l, r, operator) {
				out = append(out, NewLiteralNode("true"))
				continue
			}

			out = append(out, NewLiteralNode("false"))
		}
		return out
	}
}
