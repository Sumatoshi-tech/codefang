package analyze

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// MultiAnalyzerTraverser manages multiple visitors for UAST traversal
type MultiAnalyzerTraverser struct {
	visitors []NodeVisitor
	hooks    map[string][]NodeVisitor
}

// NewMultiAnalyzerTraverser creates a new MultiAnalyzerTraverser
func NewMultiAnalyzerTraverser() *MultiAnalyzerTraverser {
	return &MultiAnalyzerTraverser{
		visitors: make([]NodeVisitor, 0),
		hooks:    make(map[string][]NodeVisitor),
	}
}

// RegisterVisitor registers a visitor to be called during traversal
func (t *MultiAnalyzerTraverser) RegisterVisitor(v NodeVisitor) {
	t.visitors = append(t.visitors, v)
}

// RegisterHook registers a visitor for a specific node type
func (t *MultiAnalyzerTraverser) RegisterHook(nodeType string, v NodeVisitor) {
	if t.hooks[nodeType] == nil {
		t.hooks[nodeType] = make([]NodeVisitor, 0)
	}
	t.hooks[nodeType] = append(t.hooks[nodeType], v)
}

// Traverse starts traversal from the root node
func (t *MultiAnalyzerTraverser) Traverse(root *node.Node) {
	if root == nil {
		return
	}
	t.traverseRecursive(root, 0)
}

func (t *MultiAnalyzerTraverser) traverseRecursive(n *node.Node, depth int) {
	// Call global visitors
	for _, v := range t.visitors {
		v.OnEnter(n, depth)
	}

	// Call type-specific hooks
	if hooks, ok := t.hooks[string(n.Type)]; ok {
		for _, v := range hooks {
			v.OnEnter(n, depth)
		}
	}

	for _, child := range n.Children {
		t.traverseRecursive(child, depth+1)
	}

	// Call type-specific hooks (exit)
	if hooks, ok := t.hooks[string(n.Type)]; ok {
		for _, v := range hooks {
			v.OnExit(n, depth)
		}
	}

	// Call global visitors (exit)
	for _, v := range t.visitors {
		v.OnExit(n, depth)
	}
}
