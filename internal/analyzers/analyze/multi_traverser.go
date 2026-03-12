package analyze

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Stack capacity constants for iterative traversal.
const (
	traverserStackInitCap   = 64
	traverserStackGrowth    = 32
	traverserVisitorInitCap = 4
)

// MultiAnalyzerTraverser manages multiple visitors for UAST traversal.
// Uses iterative depth-first traversal to avoid stack overflow on deep trees
// and eliminate function call overhead from recursion.
type MultiAnalyzerTraverser struct {
	visitors []NodeVisitor
}

// NewMultiAnalyzerTraverser creates a new MultiAnalyzerTraverser.
func NewMultiAnalyzerTraverser() *MultiAnalyzerTraverser {
	return &MultiAnalyzerTraverser{
		visitors: make([]NodeVisitor, 0, traverserVisitorInitCap),
	}
}

// RegisterVisitor registers a visitor to be called during traversal.
func (t *MultiAnalyzerTraverser) RegisterVisitor(v NodeVisitor) {
	t.visitors = append(t.visitors, v)
}

// traverseFrame represents a stack frame for iterative tree traversal.
// childIdx tracks which child to process next; a value equal to
// len(node.Children) means all children have been visited and OnExit
// should be called.
type traverseFrame struct {
	node     *node.Node
	depth    int
	childIdx int // Next child to push; -1 = OnEnter not yet called.
}

// Traverse performs iterative pre/post-order traversal of the UAST tree.
// Each node receives OnEnter before its children and OnExit after all
// children have been fully traversed — matching the previous recursive
// semantics without risk of stack overflow.
func (t *MultiAnalyzerTraverser) Traverse(root *node.Node) {
	if root == nil || len(t.visitors) == 0 {
		return
	}

	stack := make([]traverseFrame, 0, traverserStackInitCap)
	stack = append(stack, traverseFrame{node: root, depth: 0, childIdx: -1})

	visitors := t.visitors // Local copy avoids repeated field loads.

	for len(stack) > 0 {
		top := &stack[len(stack)-1]

		// First visit: fire OnEnter.
		if top.childIdx == -1 {
			for _, v := range visitors {
				v.OnEnter(top.node, top.depth)
			}

			top.childIdx = 0
		}

		// Push next unvisited child.
		if top.childIdx < len(top.node.Children) {
			child := top.node.Children[top.childIdx]
			top.childIdx++

			// Ensure stack capacity for the new frame.
			if len(stack) == cap(stack) {
				grown := make([]traverseFrame, len(stack), cap(stack)+traverserStackGrowth)
				copy(grown, stack)
				stack = grown
			}

			stack = append(stack, traverseFrame{
				node:     child,
				depth:    top.depth + 1,
				childIdx: -1,
			})

			continue
		}

		// All children visited: fire OnExit and pop.
		for _, v := range visitors {
			v.OnExit(top.node, top.depth)
		}

		stack = stack[:len(stack)-1]
	}
}
