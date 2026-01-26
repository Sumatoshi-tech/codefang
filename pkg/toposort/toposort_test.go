package toposort_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/toposort"
)

func index(list []string, val string) int {
	for idx, str := range list {
		if str == val {
			return idx
		}
	}

	return -1
}

// addNodes is a test helper to add multiple nodes at once.
func addNodes(graph *toposort.Graph, names ...string) {
	for _, name := range names {
		graph.AddNode(name)
	}
}

// Edge represents a directed edge from one node to another.
type Edge struct {
	From string
	To   string
}

func TestToposortDuplicatedNode(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
	graph.AddNode("a")

	if graph.AddNode("a") {
		t.Error("not raising duplicated node error")
	}
}

func TestToposortRemoveNotExistEdge(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
	if graph.RemoveEdge("a", "b") {
		t.Error("not raising not exist edge error")
	}
}

func TestToposortWikipedia(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
	addNodes(graph, "2", "3", "5", "7", "8", "9", "10", "11")

	// Correct edges list.
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

	for _, edge := range edges {
		graph.AddEdge(edge.From, edge.To)
	}

	result, ok := graph.Toposort()
	if !ok {
		t.Error("closed path detected in no closed pathed graph")
	}

	for _, edge := range edges {
		if fromIdx, toIdx := index(result, edge.From), index(result, edge.To); fromIdx > toIdx {
			t.Errorf("dependency failed: not satisfy %v(%v) > %v(%v)", edge.From, fromIdx, edge.To, toIdx)
		}
	}
}

func TestToposortCycle(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
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
	t.Parallel()

	graph := toposort.NewGraph()
	addNodes(graph, "1", "2", "3")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("3", "1")

	gc := graph.Copy()

	// Verify deep copy by modifying original and checking clone.
	graph.RemoveEdge("1", "2")

	// Clone should still have the edge.
	children := gc.FindChildren("1")
	assert.Equal(t, []string{"2"}, children)

	childrenOriginal := graph.FindChildren("1")
	assert.Equal(t, []string{}, childrenOriginal)
}

func TestToposortReindexNode(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
	addNodes(graph, "1", "2", "3")

	graph.AddEdge("1", "2")
	graph.AddEdge("2", "3")
	graph.AddEdge("3", "1")
	graph.AddEdge("1", "3")
	graph.RemoveEdge("1", "2")

	// ReindexNode is now a no-op but should be safe to call.
	graph.ReindexNode("1")

	children := graph.FindChildren("1")
	assert.Equal(t, []string{"3"}, children)
}

func TestToposortBreadthSort(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
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
	t.Parallel()

	graph := toposort.NewGraph()
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
	assert.Empty(t, cycle)
}

func TestToposortFindParents(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
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

	for _, parent := range parents {
		switch parent {
		case "3":
			checks[0] = true
		case "5":
			checks[1] = true
		}
	}

	assert.Equal(t, [2]bool{true, true}, checks)
}

func TestToposortFindChildren(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
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

	for _, child := range children {
		switch child {
		case "3":
			checks[0] = true
		case "4":
			checks[1] = true
		}
	}

	assert.Equal(t, [2]bool{true, true}, checks)
}

func TestToposortSerialize(t *testing.T) {
	t.Parallel()

	graph := toposort.NewGraph()
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
