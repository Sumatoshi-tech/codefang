package node

import (
	"errors"
	"strconv"
)

// Pipeline errors.
var (
	errEmptyPipeline     = errors.New("empty pipeline")
	errReduceUnsupported = errors.New("only 'reduce(count)' is supported")
)

func lowerPipeline(pipelineNode *PipelineNode) (QueryFunc, error) {
	if len(pipelineNode.Stages) == 0 {
		return nil, errEmptyPipeline
	}

	funcs, buildErr := buildPipelineFuncs(pipelineNode.Stages)
	if buildErr != nil {
		return nil, buildErr
	}

	return func(nodes []*Node) []*Node {
		return runPipelineFuncs(funcs, nodes)
	}, nil
}

func buildPipelineFuncs(stages []DSLNode) ([]QueryFunc, error) {
	funcs := make([]QueryFunc, len(stages))

	for idx, stage := range stages {
		stageFunc, lowerErr := LowerDSL(stage)
		if lowerErr != nil {
			return nil, lowerErr
		}

		funcs[idx] = stageFunc
	}

	return funcs, nil
}

func runPipelineFuncs(funcs []QueryFunc, nodes []*Node) []*Node {
	results := nodes

	for _, fn := range funcs {
		results = fn(results)
	}

	return results
}

func lowerMap(mapNode *MapNode) (QueryFunc, error) {
	exprFunc, lowerErr := LowerDSL(mapNode.Expr)
	if lowerErr != nil {
		return nil, lowerErr
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
	testNode := &Node{
		Type:     "test",
		Children: []*Node{{Type: "c1"}, {Type: "c2"}},
	}

	res := exprFunc([]*Node{testNode})

	if len(res) != len(testNode.Children) {
		return false
	}

	for idx := range res {
		if res[idx] != testNode.Children[idx] {
			return false
		}
	}

	return true
}

func flattenChildren(nodes []*Node) []*Node {
	totalChildren := calculateTotalChildren(nodes)
	out := make([]*Node, 0, totalChildren)

	for _, targetNode := range nodes {
		out = append(out, targetNode.Children...)
	}

	return out
}

func mapOverNodes(exprFunc QueryFunc, nodes []*Node) []*Node {
	out := make([]*Node, 0, len(nodes))

	for _, targetNode := range nodes {
		out = append(out, exprFunc([]*Node{targetNode})...)
	}

	return out
}

func calculateTotalChildren(nodes []*Node) int {
	total := 0

	for _, targetNode := range nodes {
		total += len(targetNode.Children)
	}

	return total
}

func lowerRMap(rmapNode *RMapNode) (QueryFunc, error) {
	exprFunc, lowerErr := LowerDSL(rmapNode.Expr)
	if lowerErr != nil {
		return nil, lowerErr
	}

	return func(nodes []*Node) []*Node {
		return runRMap(exprFunc, nodes)
	}, nil
}

func runRMap(exprFunc QueryFunc, nodes []*Node) []*Node {
	estimatedSize := estimateRMapResultSize(nodes)
	out := make([]*Node, 0, estimatedSize)

	for _, targetNode := range nodes {
		out = append(out, mapNodeRecursively(exprFunc, targetNode)...)
	}

	return out
}

func mapNodeRecursively(exprFunc QueryFunc, targetNode *Node) []*Node {
	var out []*Node

	stack := []*Node{targetNode}

	for hasStack(stack) {
		curr := popStack(&stack)
		out = append(out, exprFunc([]*Node{curr})...)

		pushChildrenToStack(curr, &stack)
	}

	return out
}

func estimateRMapResultSize(nodes []*Node) int {
	total := 0

	for _, targetNode := range nodes {
		total += countNodesInTree(targetNode)
	}

	return total
}

func countNodesInTree(targetNode *Node) int {
	if targetNode == nil {
		return 0
	}

	count := 1

	for _, child := range targetNode.Children {
		count += countNodesInTree(child)
	}

	return count
}

func hasStack(stack []*Node) bool {
	return len(stack) > 0
}

func popStack(stack *[]*Node) *Node {
	last := (*stack)[len(*stack)-1]
	*stack = (*stack)[:len(*stack)-1]

	return last
}

func pushChildrenToStack(targetNode *Node, stack *[]*Node) {
	for idx := len(targetNode.Children) - 1; idx >= 0; idx-- {
		*stack = append(*stack, targetNode.Children[idx])
	}
}

func lowerFilter(filterNode *FilterNode) (QueryFunc, error) {
	predFunc, lowerErr := LowerDSL(filterNode.Expr)
	if lowerErr != nil {
		return nil, lowerErr
	}

	return func(nodes []*Node) []*Node {
		return runFilter(predFunc, nodes)
	}, nil
}

func runFilter(predFunc QueryFunc, nodes []*Node) []*Node {
	out := make([]*Node, 0, len(nodes))

	for _, targetNode := range nodes {
		if isPredicateTrue(predFunc, targetNode) {
			out = append(out, targetNode)
		}
	}

	return out
}

func isPredicateTrue(predFunc QueryFunc, targetNode *Node) bool {
	res := predFunc([]*Node{targetNode})

	return len(res) > 0 && res[0].Type == UASTLiteral && res[0].Token == boolTrue
}

func lowerRFilter(rfilterNode *RFilterNode) (QueryFunc, error) {
	predFunc, lowerErr := LowerDSL(rfilterNode.Expr)
	if lowerErr != nil {
		return nil, lowerErr
	}

	return func(nodes []*Node) []*Node {
		return runRFilter(predFunc, nodes)
	}, nil
}

func runRFilter(predFunc QueryFunc, nodes []*Node) []*Node {
	estimatedSize := estimateRMapResultSize(nodes)
	out := make([]*Node, 0, estimatedSize)

	for _, targetNode := range nodes {
		out = append(out, filterNodeRecursively(predFunc, targetNode)...)
	}

	return out
}

func filterNodeRecursively(predFunc QueryFunc, targetNode *Node) []*Node {
	var out []*Node

	stack := []*Node{targetNode}

	for hasStack(stack) {
		curr := popStack(&stack)

		if isPredicateTrue(predFunc, curr) {
			out = append(out, curr)
		}

		pushChildrenToStack(curr, &stack)
	}

	return out
}

func lowerReduce(reduceNode *ReduceNode) (QueryFunc, error) {
	if !isReduceCountCall(reduceNode.Expr) {
		return nil, errReduceUnsupported
	}

	return runReduce, nil
}

func isReduceCountCall(expr DSLNode) bool {
	call, ok := expr.(*CallNode)

	return ok && call.Name == "count"
}

func runReduce(nodes []*Node) []*Node {
	if len(nodes) == 0 {
		return []*Node{NewLiteralNode("0")}
	}

	return []*Node{NewLiteralNode(strconv.Itoa(len(nodes)))}
}
