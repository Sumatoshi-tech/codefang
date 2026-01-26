package toposort_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/toposort"
)

func TestIntGraph_Basic(t *testing.T) {
	t.Parallel()

	graph := toposort.NewIntGraph()
	graph.AddEdge(0, 1)
	graph.AddEdge(1, 2)

	sorted, ok := graph.TopoSort()
	assert.True(t, ok)
	assert.Equal(t, []int{0, 1, 2}, sorted)
}

func TestIntGraph_Cycle(t *testing.T) {
	t.Parallel()

	graph := toposort.NewIntGraph()
	graph.AddEdge(0, 1)
	graph.AddEdge(1, 0)

	_, ok := graph.TopoSort()
	assert.False(t, ok)
}

func TestIntGraph_Complex(t *testing.T) {
	t.Parallel()

	graph := toposort.NewIntGraph()
	// 3 -> 0.
	// 3 -> 1.
	// 0 -> 2.
	// 1 -> 2.
	graph.AddEdge(3, 0)
	graph.AddEdge(3, 1)
	graph.AddEdge(0, 2)
	graph.AddEdge(1, 2)

	sorted, ok := graph.TopoSort()
	assert.True(t, ok)

	// Expected order: 3, then 0/1 (sorted 0,1), then 2.
	assert.Equal(t, []int{3, 0, 1, 2}, sorted)
}

func TestIntGraph_Disconnected(t *testing.T) {
	t.Parallel()

	graph := toposort.NewIntGraph()
	graph.AddNode(2) // Creates 0, 1, 2.
	graph.AddEdge(0, 1)

	// 2 is isolated.

	sorted, ok := graph.TopoSort()
	assert.True(t, ok)
	assert.Len(t, sorted, 3)
	assert.Equal(t, []int{0, 1, 2}, sorted)
}

func TestIntGraph_FindCycle(t *testing.T) {
	t.Parallel()

	graph := toposort.NewIntGraph()
	// 0 -> 1 -> 2 -> 0.
	graph.AddEdge(0, 1)
	graph.AddEdge(1, 2)
	graph.AddEdge(2, 0)

	cycle := graph.FindCycle(0)
	assert.Equal(t, []int{0, 1, 2, 0}, cycle)
}
