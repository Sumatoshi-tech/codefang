package analyze

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// MultiAnalyzerTraverser manages multiple visitors for UAST traversal.
type MultiAnalyzerTraverser struct {
	visitors []NodeVisitor
}

// NewMultiAnalyzerTraverser creates a new MultiAnalyzerTraverser.
func NewMultiAnalyzerTraverser() *MultiAnalyzerTraverser {
	return &MultiAnalyzerTraverser{
		visitors: make([]NodeVisitor, 0),
	}
}

// RegisterVisitor registers a visitor to be called during traversal.
func (t *MultiAnalyzerTraverser) RegisterVisitor(v NodeVisitor) {
	t.visitors = append(t.visitors, v)
}

// Traverse starts traversal from the root node.
func (t *MultiAnalyzerTraverser) Traverse(root *node.Node) {
	if root == nil {
		return
	}

	t.traverseRecursive(root, 0)
}

func (t *MultiAnalyzerTraverser) traverseRecursive(current *node.Node, depth int) {
	// Call global visitors.
	for _, v := range t.visitors {
		v.OnEnter(current, depth)
	}

	for _, child := range current.Children {
		t.traverseRecursive(child, depth+1)
	}

	// Call global visitors (exit).
	for _, v := range t.visitors {
		v.OnExit(current, depth)
	}
}
