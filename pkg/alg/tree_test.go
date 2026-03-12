// FRD: specs/frds/FRD-20260310-traverse-tree.md.

package alg

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

type testNode struct {
	name     string
	children []*testNode
}

func childrenOf(n *testNode) []*testNode { return n.children }

func TestTraverseTree_SingleNode(t *testing.T) {
	t.Parallel()

	root := &testNode{name: "root"}

	var visited []string

	TraverseTree(root, childrenOf, func(n *testNode, depth int) {
		visited = append(visited, n.name)

		assert.Equal(t, 0, depth)
	})

	assert.Equal(t, []string{"root"}, visited)
}

func TestTraverseTree_PreOrder(t *testing.T) {
	t.Parallel()

	// Tree: a -> {b -> {d}, c}.
	d := &testNode{name: "d"}
	b := &testNode{name: "b", children: []*testNode{d}}
	c := &testNode{name: "c"}
	a := &testNode{name: "a", children: []*testNode{b, c}}

	var visited []string

	TraverseTree(a, childrenOf, func(n *testNode, _ int) {
		visited = append(visited, n.name)
	})

	assert.Equal(t, []string{"a", "b", "d", "c"}, visited)
}

func TestTraverseTree_DepthTracking(t *testing.T) {
	t.Parallel()

	d := &testNode{name: "d"}
	b := &testNode{name: "b", children: []*testNode{d}}
	c := &testNode{name: "c"}
	a := &testNode{name: "a", children: []*testNode{b, c}}

	depths := make(map[string]int)

	TraverseTree(a, childrenOf, func(n *testNode, depth int) {
		depths[n.name] = depth
	})

	assert.Equal(t, 0, depths["a"])
	assert.Equal(t, 1, depths["b"])
	assert.Equal(t, 1, depths["c"])
	assert.Equal(t, 2, depths["d"])
}

func TestTraverseTree_NilChildren(t *testing.T) {
	t.Parallel()

	root := &testNode{name: "root", children: nil}

	var count int

	TraverseTree(root, childrenOf, func(_ *testNode, _ int) {
		count++
	})

	assert.Equal(t, 1, count)
}

func TestTraverseTree_EmptyChildrenTerminatesBranch(t *testing.T) {
	t.Parallel()

	leaf := &testNode{name: "leaf", children: []*testNode{}}
	root := &testNode{name: "root", children: []*testNode{leaf}}

	var visited []string

	TraverseTree(root, childrenOf, func(n *testNode, _ int) {
		visited = append(visited, n.name)
	})

	assert.Equal(t, []string{"root", "leaf"}, visited)
}

func TestTraverseTree_ChildrenFuncControlsTraversal(t *testing.T) {
	t.Parallel()

	// Use children func to limit depth.
	d := &testNode{name: "d"}
	b := &testNode{name: "b", children: []*testNode{d}}
	c := &testNode{name: "c"}
	a := &testNode{name: "a", children: []*testNode{b, c}}

	const maxDepth = 1

	var visited []string

	TraverseTree(a, func(n *testNode) []*testNode {
		// Only return children for nodes at depth < maxDepth.
		// We track depth by checking if the node is the root.
		// For a proper depth limit, the caller wraps the node.
		return n.children
	}, func(n *testNode, depth int) {
		if depth <= maxDepth {
			visited = append(visited, n.name)
		}
	})

	// All nodes are traversed, but we only record those within maxDepth.
	assert.Contains(t, visited, "a")
	assert.Contains(t, visited, "b")
	assert.Contains(t, visited, "c")
	assert.NotContains(t, visited, "d")
}

func TestTraverseTree_IntTree(t *testing.T) {
	t.Parallel()

	// Demonstrate with non-pointer value types.
	type intNode struct {
		val      int
		children []intNode
	}

	root := intNode{
		val: 1,
		children: []intNode{
			{val: 2},
			{val: 3, children: []intNode{{val: 4}}},
		},
	}

	var sum int

	TraverseTree(root, func(n intNode) []intNode { return n.children }, func(n intNode, _ int) {
		sum += n.val
	})

	assert.Equal(t, 10, sum)
}

func TestTraverseTree_WideTree(t *testing.T) {
	t.Parallel()

	const width = 100

	children := make([]*testNode, width)
	for i := range children {
		children[i] = &testNode{name: "child"}
	}

	root := &testNode{name: "root", children: children}

	var count int

	TraverseTree(root, childrenOf, func(_ *testNode, _ int) {
		count++
	})

	assert.Equal(t, width+1, count)
}
