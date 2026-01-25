package analyze

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/stretchr/testify/assert"
)

type mockVisitor struct {
	enterCalls int
	exitCalls  int
}

func (m *mockVisitor) OnEnter(n *node.Node, depth int) {
	m.enterCalls++
}

func (m *mockVisitor) OnExit(n *node.Node, depth int) {
	m.exitCalls++
}

func TestMultiAnalyzerTraverser_RegisterVisitor(t *testing.T) {
	traverser := NewMultiAnalyzerTraverser()
	visitor := &mockVisitor{}
	traverser.RegisterVisitor(visitor)

	assert.Equal(t, 1, len(traverser.visitors))
}

func TestMultiAnalyzerTraverser_Traverse(t *testing.T) {
	traverser := NewMultiAnalyzerTraverser()
	visitor := &mockVisitor{}
	traverser.RegisterVisitor(visitor)

	root := &node.Node{
		Children: []*node.Node{
			{},
			{},
		},
	}

	traverser.Traverse(root)

	// Root + 2 children = 3 visits
	assert.Equal(t, 3, visitor.enterCalls)
	assert.Equal(t, 3, visitor.exitCalls)
}

func TestMultiAnalyzerTraverser_RegisterHook(t *testing.T) {
	traverser := NewMultiAnalyzerTraverser()
	visitor := &mockVisitor{}
	
	// Register hook for type "Function"
	traverser.RegisterHook("Function", visitor)

	root := &node.Node{
		Type: "Class",
		Children: []*node.Node{
			{Type: "Function"},
			{Type: "Method"},
		},
	}

	traverser.Traverse(root)

	// Should only visit the "Function" node
	assert.Equal(t, 1, visitor.enterCalls)
	assert.Equal(t, 1, visitor.exitCalls)
}
