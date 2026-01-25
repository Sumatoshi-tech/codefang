package toposort

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestIntGraph_Basic(t *testing.T) {
	g := NewIntGraph()
	g.AddEdge(0, 1)
	g.AddEdge(1, 2)

	sorted, ok := g.TopoSort()
	assert.True(t, ok)
	assert.Equal(t, []int{0, 1, 2}, sorted)
}

func TestIntGraph_Cycle(t *testing.T) {
	g := NewIntGraph()
	g.AddEdge(0, 1)
	g.AddEdge(1, 0)

	_, ok := g.TopoSort()
	assert.False(t, ok)
}

func TestIntGraph_Complex(t *testing.T) {
	g := NewIntGraph()
	// 3 -> 0
	// 3 -> 1
	// 0 -> 2
	// 1 -> 2
	g.AddEdge(3, 0)
	g.AddEdge(3, 1)
	g.AddEdge(0, 2)
	g.AddEdge(1, 2)

	sorted, ok := g.TopoSort()
	assert.True(t, ok)
	
	// Expected order: 3, then 0/1 (sorted 0,1), then 2
	assert.Equal(t, []int{3, 0, 1, 2}, sorted)
}

func TestIntGraph_Disconnected(t *testing.T) {
	g := NewIntGraph()
	g.AddNode(2) // Creates 0, 1, 2
	g.AddEdge(0, 1)
	
	// 2 is isolated
	
	sorted, ok := g.TopoSort()
	assert.True(t, ok)
	assert.Equal(t, 3, len(sorted))
	assert.Equal(t, []int{0, 1, 2}, sorted)
}

func TestIntGraph_FindCycle(t *testing.T) {
	g := NewIntGraph()
	// 0 -> 1 -> 2 -> 0
	g.AddEdge(0, 1)
	g.AddEdge(1, 2)
	g.AddEdge(2, 0)

	cycle := g.FindCycle(0)
	assert.Equal(t, []int{0, 1, 2, 0}, cycle)
}
