package node

import "fmt"

func lowerPipeline(n *PipelineNode) (QueryFunc, error) {
	if isEmptyStages(n.Stages) {
		return nil, fmt.Errorf("empty pipeline")
	}
	funcs, err := buildPipelineFuncs(n.Stages)
	if hasError(err) {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		return runPipelineFuncs(funcs, nodes)
	}, nil
}

func isEmptyStages(stages []DSLNode) bool {
	return len(stages) == 0
}

func buildPipelineFuncs(stages []DSLNode) ([]QueryFunc, error) {
	funcs := make([]QueryFunc, len(stages))
	for i, stage := range stages {
		f, err := LowerDSL(stage)
		if hasError(err) {
			return nil, err
		}
		funcs[i] = f
	}
	return funcs, nil
}

func runPipelineFuncs(funcs []QueryFunc, nodes []*Node) []*Node {
	results := nodes
	for _, f := range funcs {
		results = f(results)
	}
	return results
}

func lowerMap(n *MapNode) (QueryFunc, error) {
	exprFunc, err := LowerDSL(n.Expr)
	if hasError(err) {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		return runMap(exprFunc, nodes)
	}, nil
}

func runMap(exprFunc QueryFunc, nodes []*Node) []*Node {
	if isExactChildrenField(exprFunc) {
		return flattenChildren(nodes)
	}
	return mapOverNodes(exprFunc, nodes)
}

func isExactChildrenField(exprFunc QueryFunc) bool {
	testNode := &Node{Type: "test", Children: []*Node{{Type: "c1"}, {Type: "c2"}}}
	res := exprFunc([]*Node{testNode})
	if len(res) != len(testNode.Children) {
		return false
	}
	for i := range res {
		if res[i] != testNode.Children[i] {
			return false
		}
	}
	return true
}

func flattenChildren(nodes []*Node) []*Node {
	totalChildren := calculateTotalChildren(nodes)
	out := make([]*Node, 0, totalChildren)
	for _, node := range nodes {
		out = append(out, node.Children...)
	}
	return out
}

func mapOverNodes(exprFunc QueryFunc, nodes []*Node) []*Node {
	out := make([]*Node, 0, len(nodes))
	for _, node := range nodes {
		out = append(out, exprFunc([]*Node{node})...)
	}
	return out
}

func calculateTotalChildren(nodes []*Node) int {
	total := 0
	for _, node := range nodes {
		total += len(node.Children)
	}
	return total
}

func lowerRMap(n *RMapNode) (QueryFunc, error) {
	exprFunc, err := LowerDSL(n.Expr)
	if hasError(err) {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		return runRMap(exprFunc, nodes)
	}, nil
}

func runRMap(exprFunc QueryFunc, nodes []*Node) []*Node {
	estimatedSize := estimateRMapResultSize(nodes)
	out := make([]*Node, 0, estimatedSize)
	for _, node := range nodes {
		out = append(out, mapNodeRecursively(exprFunc, node)...)
	}
	return out
}

func mapNodeRecursively(exprFunc QueryFunc, node *Node) []*Node {
	var out []*Node
	stack := []*Node{node}
	for hasStack(stack) {
		curr := popStack(&stack)
		out = append(out, exprFunc([]*Node{curr})...)
		pushChildrenToStack(curr, &stack)
	}
	return out
}

func estimateRMapResultSize(nodes []*Node) int {
	total := 0
	for _, node := range nodes {
		total += countNodesInTree(node)
	}
	return total
}

func countNodesInTree(node *Node) int {
	if isNilNode(node) {
		return 0
	}
	count := 1
	for _, child := range node.Children {
		count += countNodesInTree(child)
	}
	return count
}

func isNilNode(node *Node) bool {
	return node == nil
}

func hasStack(stack []*Node) bool {
	return len(stack) > 0
}

func popStack(stack *[]*Node) *Node {
	last := (*stack)[len(*stack)-1]
	*stack = (*stack)[:len(*stack)-1]
	return last
}

func pushChildrenToStack(node *Node, stack *[]*Node) {
	for i := len(node.Children) - 1; i >= 0; i-- {
		*stack = append(*stack, node.Children[i])
	}
}

func lowerFilter(n *FilterNode) (QueryFunc, error) {
	predFunc, err := LowerDSL(n.Expr)
	if hasError(err) {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		return runFilter(predFunc, nodes)
	}, nil
}

func hasError(err error) bool {
	return err != nil
}

func runFilter(predFunc QueryFunc, nodes []*Node) []*Node {
	out := make([]*Node, 0, len(nodes))
	for _, node := range nodes {
		if isPredicateTrue(predFunc, node) {
			out = append(out, node)
		}
	}
	return out
}

func isPredicateTrue(predFunc QueryFunc, node *Node) bool {
	res := predFunc([]*Node{node})
	return len(res) > 0 && res[0].Type == "Literal" && res[0].Token == "true"
}

func lowerRFilter(n *RFilterNode) (QueryFunc, error) {
	predFunc, err := LowerDSL(n.Expr)
	if hasError(err) {
		return nil, err
	}
	return func(nodes []*Node) []*Node {
		return runRFilter(predFunc, nodes)
	}, nil
}

func runRFilter(predFunc QueryFunc, nodes []*Node) []*Node {
	estimatedSize := estimateRMapResultSize(nodes)
	out := make([]*Node, 0, estimatedSize)
	for _, node := range nodes {
		out = append(out, filterNodeRecursively(predFunc, node)...)
	}
	return out
}

func filterNodeRecursively(predFunc QueryFunc, node *Node) []*Node {
	var out []*Node
	stack := []*Node{node}
	for hasStack(stack) {
		curr := popStack(&stack)
		if isPredicateTrue(predFunc, curr) {
			out = append(out, curr)
		}
		pushChildrenToStack(curr, &stack)
	}
	return out
}

func lowerReduce(n *ReduceNode) (QueryFunc, error) {
	if !isReduceCountCall(n.Expr) {
		return nil, fmt.Errorf("only 'reduce(count)' is supported")
	}
	return func(nodes []*Node) []*Node {
		return runReduce(nodes)
	}, nil
}

func isReduceCountCall(expr DSLNode) bool {
	call, ok := expr.(*CallNode)
	return ok && call.Name == "count"
}

func runReduce(nodes []*Node) []*Node {
	if isEmptyNodes(nodes) {
		return []*Node{{Type: "Literal", Token: "0"}}
	}
	return []*Node{{Type: "Literal", Token: fmt.Sprint(len(nodes))}}
}

func isEmptyNodes(nodes []*Node) bool {
	return len(nodes) == 0
}
