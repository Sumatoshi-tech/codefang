package toposort

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func index(s []string, v string) int {
	for i, s := range s {
		if s == v {
			return i
		}
	}
	return -1
}

// addNodes is a test helper to add multiple nodes at once
func addNodes(g *Graph, names ...string) {
	for _, name := range names {
		g.AddNode(name)
	}
}

type Edge struct {
	From string
	To   string
}

func TestToposortDuplicatedNode(t *testing.T) {
	graph := NewGraph()
	graph.AddNode("a")
	if graph.AddNode("a") {
		t.Error("not raising duplicated node error")
	}

}

func TestToposortRemoveNotExistEdge(t *testing.T) {
	graph := NewGraph()
	if graph.RemoveEdge("a", "b") {
		t.Error("not raising not exist edge error")
	}
}

func TestToposortWikipedia(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "2", "3", "5", "7", "8", "9", "10", "11")

	edges := []Edge{
		{"7", "8"},
		{"7", "11"},

		{"5", "11"},

		{"5", "8"}, // Missing from original? No, looking at original: {"5", "11"}, next is empty line then 56?
		// Checking original:
		// 54: {"5", "11"},
		// 55:
		// 56: {"3", "8"},
		// Ah, I might have missed one or read it wrong. Let me double check original code block in thought trace.
		// Original:
		// {"7", "8"}, {"7", "11"},
		// {"5", "11"},
		// {"3", "8"}, {"3", "10"},
		// ...
	}
	// Re-reading original test content:
	/*
		edges := []Edge{
			{"7", "8"},
			{"7", "11"},

			{"5", "11"},

			{"3", "8"},
			{"3", "10"},

			{"11", "2"},
			{"11", "9"},
			{"11", "10"},

			{"8", "9"},
		}
	*/

	// Correct edges list
	edges = []Edge{
		{"7", "8"},
		{"7", "11"},
		{"5", "11"},
		{"3", "8"},
		{"3", "10"},
		{"11", "2"},
		{"11", "9"},
		{"11", "10"},
		{"8", "9"},
	}

	for _, e := range edges {
		graph.AddEdge(e.From, e.To)
	}

	result, ok := graph.Toposort()
	if !ok {
		t.Error("closed path detected in no closed pathed graph")
	}

	for _, e := range edges {
		if i, j := index(result, e.From), index(result, e.To); i > j {
			t.Errorf("dependency failed: not satisfy %v(%v) > %v(%v)", e.From, i, e.To, j)
		}
	}
}

func TestToposortCycle(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("3", "1")

	_, ok := graph.Toposort()
	if ok {
		t.Error("closed path not detected in closed pathed graph")
	}
}

func TestToposortCopy(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("3", "1")

	gc := graph.Copy()
	// Verify deep copy by modifying original and checking clone
	graph.RemoveEdge("1", "2")

	// Clone should still have the edge
	children := gc.FindChildren("1")
	assert.Equal(t, []string{"2"}, children)

	childrenOriginal := graph.FindChildren("1")
	assert.Equal(t, []string{}, childrenOriginal)
}

func TestToposortReindexNode(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("3", "1")
	graph.AddEdge("1", "3")
	graph.RemoveEdge("1", "2")

	// ReindexNode is now a no-op but should be safe to call
	graph.ReindexNode("1")

	children := graph.FindChildren("1")
	assert.Equal(t, []string{"3"}, children)
}

func TestToposortBreadthSort(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "0", "1", "2", "3", "4")

	graph.AddEdge("0", "1")
	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("1", "3")
	graph.AddEdge("3", "4")
	graph.AddEdge("4", "1")
	order := graph.BreadthSort()
	var expected [5]string
	if order[2] == "2" {
		expected = [...]string{"0", "1", "2", "3", "4"}
	} else {
		expected = [...]string{"0", "1", "3", "2", "4"}
	}
	assert.Equal(t, expected[:], order)
}

func TestToposortFindCycle(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3", "4", "5")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("2", "4")
	graph.AddEdge("3", "1")
	graph.AddEdge("5", "1")

	cycle := graph.FindCycle("2")
	expected := [...]string{"2", "3", "1"}
	assert.Equal(t, expected[:], cycle)
	cycle = graph.FindCycle("5")
	assert.Len(t, cycle, 0)
}

func TestToposortFindParents(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3", "4", "5")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("2", "4")
	graph.AddEdge("3", "1")
	graph.AddEdge("5", "1")

	parents := graph.FindParents("2")
	expected := [...]string{"1"}
	assert.Equal(t, expected[:], parents)
	parents = graph.FindParents("1")
	assert.Len(t, parents, 2)
	checks := [2]bool{}
	for _, p := range parents {
		if p == "3" {
			checks[0] = true
		} else if p == "5" {
			checks[1] = true
		}
	}
	assert.Equal(t, [2]bool{true, true}, checks)
}

func TestToposortFindChildren(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3", "4", "5")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("2", "4")
	graph.AddEdge("3", "1")
	graph.AddEdge("5", "1")

	children := graph.FindChildren("1")
	expected := [...]string{"2"}
	assert.Equal(t, expected[:], children)
	children = graph.FindChildren("2")
	assert.Len(t, children, 2)
	checks := [2]bool{}
	for _, p := range children {
		if p == "3" {
			checks[0] = true
		} else if p == "4" {
			checks[1] = true
		}
	}
	assert.Equal(t, [2]bool{true, true}, checks)
}

func TestToposortSerialize(t *testing.T) {
	graph := NewGraph()
	addNodes(graph, "1", "2", "3", "4", "5")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("2", "4")
	graph.AddEdge("3", "1")
	graph.AddEdge("5", "1")

	order := [...]string{"5", "4", "3", "2", "1"}
	gv := graph.Serialize(order[:])
	assert.Equal(t, `digraph Codefang {
  "4 1" -> "3 2"
  "3 2" -> "2 3"
  "3 2" -> "1 4"
  "2 3" -> "4 1"
  "0 5" -> "4 1"
}`, gv)
}
