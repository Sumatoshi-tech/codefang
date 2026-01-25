package toposort

import (
	"bytes"
	"fmt"
	"sort"
)

// Graph represents a directed acyclic graph.
type Graph struct {
	symbols  *SymbolTable
	intGraph *IntGraph
}

// NewGraph initializes a new Graph.
func NewGraph() *Graph {
	return &Graph{
		symbols:  NewSymbolTable(),
		intGraph: NewIntGraph(),
	}
}

// Copy clones the graph and returns the independent copy.
func (g *Graph) Copy() *Graph {
	clone := NewGraph()
	// Deep copy logic
	// For SymbolTable, we can iterate if we expose iteration or just re-add nodes/edges
	// Re-adding edges is easier if we can iterate edges.
	// But SymbolTable doesn't expose iteration easily.

	// Efficient copy:
	// Copy symbols
	clone.symbols.lock.Lock()
	g.symbols.lock.RLock()
	for k, v := range g.symbols.strToID {
		clone.symbols.strToID[k] = v
	}
	clone.symbols.idToStr = make([]string, len(g.symbols.idToStr))
	copy(clone.symbols.idToStr, g.symbols.idToStr)
	g.symbols.lock.RUnlock()
	clone.symbols.lock.Unlock()

	// Copy IntGraph
	clone.intGraph.EnsureCapacity(len(g.intGraph.nodes))
	for u, neighbors := range g.intGraph.nodes {
		if neighbors != nil {
			clone.intGraph.nodes[u] = make([]int, len(neighbors))
			copy(clone.intGraph.nodes[u], neighbors)
		}
	}
	clone.intGraph.inDegree = make([]int, len(g.intGraph.inDegree))
	copy(clone.intGraph.inDegree, g.intGraph.inDegree)
	clone.intGraph.nodeCount = g.intGraph.nodeCount

	return clone
}

// AddNode inserts a new node into the graph.
func (g *Graph) AddNode(name string) bool {
	// Check if node exists
	g.symbols.lock.RLock()
	_, exists := g.symbols.strToID[name]
	g.symbols.lock.RUnlock()

	if exists {
		return false
	}

	id := g.symbols.Intern(name)
	return g.intGraph.AddNode(id)
}

// AddEdge inserts the link from "from" node to "to" node.
func (g *Graph) AddEdge(from, to string) int {
	u := g.symbols.Intern(from)
	v := g.symbols.Intern(to)

	// Ensure nodes exist in graph (IntGraph.AddEdge handles capacity but AddNode logic might be needed for consistency)
	g.intGraph.AddNode(u)
	g.intGraph.AddNode(v)

	if g.intGraph.AddEdge(u, v) {
		return g.intGraph.inDegree[v]
	}
	// Edge already exists, return current in-degree
	return g.intGraph.inDegree[v]
}

// ReindexNode updates the internal representation of the node after edge removals.
// In the new implementation, this might be a no-op or we might need to compact IDs?
// The original ReindexNode resorted children and updated their values in the map.
// Since we use int IDs and unordered/ordered lists, we might not need this.
// However, if the caller relies on deterministic behavior that ReindexNode provided...
// Original: "sort.Strings(keys); for i, key := range keys { children[key] = i + 1 }"
// This seems to be assigning values 1..N to children in the map?
// Wait, `m[to] = len(m) + 1` in `AddEdge`. The value in `outputs[from][to]` seems to be the insertion order index (1-based).
// `ReindexNode` re-assigns these indices based on sorted key order.
// Does anything use these values? Toposort uses keys.
// The values in `outputs` map seem unused by `Toposort`.
// Let's check `FindParents` etc.
// They iterate keys.
// So `ReindexNode` might be for some specific usage or just legacy maintenance of map values.
// We can make it a no-op if we don't expose these values.
func (g *Graph) ReindexNode(node string) {
	// No-op in integer implementation as we don't maintain edge indices in map
}

// RemoveEdge deletes the link from "from" node to "to" node.
func (g *Graph) RemoveEdge(from, to string) bool {
	// Resolve IDs
	// We need to be careful not to create new IDs if they don't exist
	g.symbols.lock.RLock()
	u, ok1 := g.symbols.strToID[from]
	v, ok2 := g.symbols.strToID[to]
	g.symbols.lock.RUnlock()

	if !ok1 || !ok2 {
		return false
	}

	return g.intGraph.RemoveEdge(u, v)
}

// Toposort sorts the nodes in the graph in topological order.
func (g *Graph) Toposort() ([]string, bool) {
	ids, ok := g.intGraph.TopoSort()

	result := make([]string, len(ids))
	for i, id := range ids {
		result[i] = g.symbols.Resolve(id)
	}

	return result, ok
}

// BreadthSort sorts the nodes in the graph in BFS order.
func (g *Graph) BreadthSort() []string {
	// Reimplement BFS using IntGraph logic (or adapt IntGraph to support BFS)
	// For now, implement here using IntGraph internals or similar logic

	// Similar to Toposort but BFS exploration
	// Original BFS starts with nodes having 0 in-degree.

	// We can implement BFS in IntGraph or here.
	// Let's implement here for now using ids.

	n := len(g.intGraph.nodes)
	inDegree := make([]int, n)
	copy(inDegree, g.intGraph.inDegree)

	queue := make([]int, 0)
	// Find roots (in-degree 0)
	for i := 0; i < n; i++ {
		// Only valid nodes?
		// We can check if node name resolves to non-empty string to ensure it's a valid node
		if g.symbols.Resolve(i) != "" && inDegree[i] == 0 {
			queue = append(queue, i)
		}
	}

	// Sort initial queue by name to match string-based behavior (lexicographical)
	sort.Slice(queue, func(i, j int) bool {
		return g.symbols.Resolve(queue[i]) < g.symbols.Resolve(queue[j])
	})

	visited := make(map[int]bool)
	result := make([]string, 0, n)

	for len(queue) > 0 {
		u := queue[0]
		queue = queue[1:]

		if !visited[u] {
			visited[u] = true
			result = append(result, g.symbols.Resolve(u))

			children := g.intGraph.nodes[u]
			// Sort children by name
			childIDs := make([]int, len(children))
			copy(childIDs, children)
			sort.Slice(childIDs, func(i, j int) bool {
				return g.symbols.Resolve(childIDs[i]) < g.symbols.Resolve(childIDs[j])
			})

			for _, v := range childIDs {
				queue = append(queue, v)
			}
		}
	}

	return result
}

// FindCycle returns the cycle in the graph which contains "seed" node.
func (g *Graph) FindCycle(seed string) []string {
	g.symbols.lock.RLock()
	id, exists := g.symbols.strToID[seed]
	g.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	cycleIDs := g.intGraph.FindCycle(id)

	// Legacy compatibility: return path without closing loop repetition
	if len(cycleIDs) > 1 && cycleIDs[0] == cycleIDs[len(cycleIDs)-1] {
		cycleIDs = cycleIDs[:len(cycleIDs)-1]
	}

	result := make([]string, len(cycleIDs))
	for i, cid := range cycleIDs {
		result[i] = g.symbols.Resolve(cid)
	}
	return result
}

// FindParents returns the other ends of incoming edges.
func (g *Graph) FindParents(to string) []string {
	g.symbols.lock.RLock()
	targetID, exists := g.symbols.strToID[to]
	g.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	var parents []string
	// Inefficient: iterate all nodes to find edges to targetID
	// IntGraph doesn't store reverse edges (parents).
	// Current IntGraph is optimized for forward traversal.
	// But we can iterate.

	for u, children := range g.intGraph.nodes {
		for _, v := range children {
			if v == targetID {
				parents = append(parents, g.symbols.Resolve(u))
				break
			}
		}
	}

	sort.Strings(parents)
	return parents
}

// FindChildren returns the other ends of outgoing edges.
func (g *Graph) FindChildren(from string) []string {
	g.symbols.lock.RLock()
	u, exists := g.symbols.strToID[from]
	g.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	if u >= len(g.intGraph.nodes) {
		return []string{}
	}

	childrenIDs := g.intGraph.nodes[u]
	children := make([]string, len(childrenIDs))
	for i, v := range childrenIDs {
		children[i] = g.symbols.Resolve(v)
	}

	sort.Strings(children)
	return children
}

// Serialize outputs the graph in Graphviz format.
func (g *Graph) Serialize(sorted []string) string {
	node2index := map[string]int{}
	for index, node := range sorted {
		node2index[node] = index
	}
	var buffer bytes.Buffer
	buffer.WriteString("digraph Codefang {\n")

	nodesFrom := g.symbols.idToStr // All nodes
	sortedNodesFrom := make([]string, len(nodesFrom))
	copy(sortedNodesFrom, nodesFrom)
	sort.Strings(sortedNodesFrom)

	for _, nodeFrom := range sortedNodesFrom {
		children := g.FindChildren(nodeFrom)
		for _, nodeTo := range children {
			buffer.WriteString(fmt.Sprintf("  \"%d %s\" -> \"%d %s\"\n",
				node2index[nodeFrom], nodeFrom, node2index[nodeTo], nodeTo))
		}
	}
	buffer.WriteString("}")
	return buffer.String()
}
