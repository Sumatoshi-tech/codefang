package alg

const defaultTreeStackCap = 32

// TraverseTree performs an iterative pre-order DFS over a tree.
// children returns the children of a node; visit is called for each node
// with its depth (root depth is 0). An empty children slice terminates
// the branch.
func TraverseTree[T any](root T, children func(T) []T, visit func(node T, depth int)) {
	type frame struct {
		node  T
		depth int
	}

	stack := make([]frame, 0, defaultTreeStackCap)
	stack = append(stack, frame{node: root, depth: 0})

	for len(stack) > 0 {
		top := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		visit(top.node, top.depth)

		kids := children(top.node)
		childDepth := top.depth + 1

		for i := len(kids) - 1; i >= 0; i-- {
			stack = append(stack, frame{node: kids[i], depth: childDepth})
		}
	}
}
