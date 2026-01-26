package toposort

import (
	"bytes"
	"fmt"
	"maps"
	"slices"
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
func (graph *Graph) Copy() *Graph {
	clone := NewGraph()
	// Deep copy logic.
	// For SymbolTable, we can iterate if we expose iteration or just re-add nodes/edges.
	// Re-adding edges is easier if we can iterate edges.
	// But SymbolTable doesn't expose iteration easily.

	// Efficient copy:
	// Copy symbols.
	clone.symbols.lock.Lock()
	graph.symbols.lock.RLock()

	maps.Copy(clone.symbols.strToID, graph.symbols.strToID)

	clone.symbols.idToStr = make([]string, len(graph.symbols.idToStr))
	copy(clone.symbols.idToStr, graph.symbols.idToStr)
	graph.symbols.lock.RUnlock()
	clone.symbols.lock.Unlock()

	// Copy IntGraph.
	clone.intGraph.EnsureCapacity(len(graph.intGraph.nodes))

	for nodeIdx, neighbors := range graph.intGraph.nodes {
		if neighbors != nil {
			clone.intGraph.nodes[nodeIdx] = make([]int, len(neighbors))
			copy(clone.intGraph.nodes[nodeIdx], neighbors)
		}
	}

	clone.intGraph.inDegree = make([]int, len(graph.intGraph.inDegree))
	copy(clone.intGraph.inDegree, graph.intGraph.inDegree)
	clone.intGraph.nodeCount = graph.intGraph.nodeCount

	return clone
}

// AddNode inserts a new node into the graph.
func (graph *Graph) AddNode(name string) bool {
	// Check if node exists.
	graph.symbols.lock.RLock()
	_, exists := graph.symbols.strToID[name]
	graph.symbols.lock.RUnlock()

	if exists {
		return false
	}

	id := graph.symbols.Intern(name)

	return graph.intGraph.AddNode(id)
}

// AddEdge inserts the link from "from" node to "to" node.
func (graph *Graph) AddEdge(from, to string) int {
	src := graph.symbols.Intern(from)
	dst := graph.symbols.Intern(to)

	// Ensure nodes exist in graph (IntGraph.AddEdge handles capacity but AddNode logic might be needed for consistency).
	graph.intGraph.AddNode(src)
	graph.intGraph.AddNode(dst)

	if graph.intGraph.AddEdge(src, dst) {
		return graph.intGraph.inDegree[dst]
	}

	// Edge already exists, return current in-degree.
	return graph.intGraph.inDegree[dst]
}

// ReindexNode updates the internal representation of the node after edge removals.
// In the new implementation, this might be a no-op or we might need to compact IDs.
// The original ReindexNode resorted children and updated their values in the map.
// Since we use int IDs and unordered/ordered lists, we might not need this.
// However, if the caller relies on deterministic behavior that ReindexNode provided...
// Original: "sort.Strings(keys); for i, key := range keys { children[key] = i + 1 }".
// This seems to be assigning values 1..N to children in the map.
// Wait, `m[to] = len(m) + 1` in `AddEdge`. The value in `outputs[from][to]` seems to be the insertion order index (1-based).
// `ReindexNode` re-assigns these indices based on sorted key order.
// Does anything use these values? Toposort uses keys.
// The values in `outputs` map seem unused by `Toposort`.
// Let's check `FindParents` etc.
// They iterate keys.
// So `ReindexNode` might be for some specific usage or just legacy maintenance of map values.
// We can make it a no-op if we don't expose these values.
func (graph *Graph) ReindexNode(_ string) {
	// No-op in integer implementation as we don't maintain edge indices in map.
}

// RemoveEdge deletes the link from "from" node to "to" node.
func (graph *Graph) RemoveEdge(from, to string) bool {
	// Resolve IDs.
	// We need to be careful not to create new IDs if they don't exist.
	graph.symbols.lock.RLock()
	src, ok1 := graph.symbols.strToID[from]
	dst, ok2 := graph.symbols.strToID[to]
	graph.symbols.lock.RUnlock()

	if !ok1 || !ok2 {
		return false
	}

	return graph.intGraph.RemoveEdge(src, dst)
}

// Toposort sorts the nodes in the graph in topological order.
func (graph *Graph) Toposort() ([]string, bool) {
	ids, ok := graph.intGraph.TopoSort()

	result := make([]string, len(ids))
	for idx, id := range ids {
		result[idx] = graph.symbols.Resolve(id)
	}

	return result, ok
}

// BreadthSort sorts the nodes in the graph in BFS order.
func (graph *Graph) BreadthSort() []string {
	// Reimplement BFS using IntGraph logic (or adapt IntGraph to support BFS).
	// For now, implement here using IntGraph internals or similar logic.
	// Similar to Toposort but BFS exploration.
	// Original BFS starts with nodes having 0 in-degree.
	// We can implement BFS in IntGraph or here.
	// Let's implement here for now using ids.
	nodeCount := len(graph.intGraph.nodes)
	inDegree := make([]int, nodeCount)
	copy(inDegree, graph.intGraph.inDegree)

	queue := make([]int, 0)

	// Find roots (in-degree 0).
	for idx := range nodeCount {
		// Only valid nodes.
		// We can check if node name resolves to non-empty string to ensure it's a valid node.
		if graph.symbols.Resolve(idx) != "" && inDegree[idx] == 0 {
			queue = append(queue, idx)
		}
	}

	// Sort initial queue by name to match string-based behavior (lexicographical).
	sort.Slice(queue, func(i, j int) bool {
		return graph.symbols.Resolve(queue[i]) < graph.symbols.Resolve(queue[j])
	})

	visited := make(map[int]bool)
	result := make([]string, 0, nodeCount)

	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]

		if !visited[cur] {
			visited[cur] = true
			result = append(result, graph.symbols.Resolve(cur))

			children := graph.intGraph.nodes[cur]

			// Sort children by name.
			childIDs := make([]int, len(children))
			copy(childIDs, children)
			sort.Slice(childIDs, func(i, j int) bool {
				return graph.symbols.Resolve(childIDs[i]) < graph.symbols.Resolve(childIDs[j])
			})

			queue = append(queue, childIDs...)
		}
	}

	return result
}

// FindCycle returns the cycle in the graph which contains "seed" node.
func (graph *Graph) FindCycle(seed string) []string {
	graph.symbols.lock.RLock()
	id, exists := graph.symbols.strToID[seed]
	graph.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	cycleIDs := graph.intGraph.FindCycle(id)

	// Legacy compatibility: return path without closing loop repetition.
	if len(cycleIDs) > 1 && cycleIDs[0] == cycleIDs[len(cycleIDs)-1] {
		cycleIDs = cycleIDs[:len(cycleIDs)-1]
	}

	result := make([]string, len(cycleIDs))
	for idx, cid := range cycleIDs {
		result[idx] = graph.symbols.Resolve(cid)
	}

	return result
}

// FindParents returns the other ends of incoming edges.
func (graph *Graph) FindParents(to string) []string {
	graph.symbols.lock.RLock()
	targetID, exists := graph.symbols.strToID[to]
	graph.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	var parents []string
	// Inefficient: iterate all nodes to find edges to targetID.
	// IntGraph doesn't store reverse edges (parents).
	// Current IntGraph is optimized for forward traversal.
	// But we can iterate.

	for nodeIdx, children := range graph.intGraph.nodes {
		if slices.Contains(children, targetID) {
			parents = append(parents, graph.symbols.Resolve(nodeIdx))
		}
	}

	sort.Strings(parents)

	return parents
}

// FindChildren returns the other ends of outgoing edges.
func (graph *Graph) FindChildren(from string) []string {
	graph.symbols.lock.RLock()
	src, exists := graph.symbols.strToID[from]
	graph.symbols.lock.RUnlock()

	if !exists {
		return []string{}
	}

	if src >= len(graph.intGraph.nodes) {
		return []string{}
	}

	childrenIDs := graph.intGraph.nodes[src]

	children := make([]string, len(childrenIDs))
	for idx, neighbor := range childrenIDs {
		children[idx] = graph.symbols.Resolve(neighbor)
	}

	sort.Strings(children)

	return children
}

// Serialize outputs the graph in Graphviz format.
func (graph *Graph) Serialize(sorted []string) string {
	node2index := map[string]int{}
	for index, node := range sorted {
		node2index[node] = index
	}

	var buffer bytes.Buffer

	buffer.WriteString("digraph Codefang {\n")

	nodesFrom := graph.symbols.idToStr // All nodes.
	sortedNodesFrom := make([]string, len(nodesFrom))
	copy(sortedNodesFrom, nodesFrom)
	sort.Strings(sortedNodesFrom)

	for _, nodeFrom := range sortedNodesFrom {
		children := graph.FindChildren(nodeFrom)
		for _, nodeTo := range children {
			buffer.WriteString(fmt.Sprintf("  \"%d %s\" -> \"%d %s\"\n",
				node2index[nodeFrom], nodeFrom, node2index[nodeTo], nodeTo))
		}
	}

	buffer.WriteString("}")

	return buffer.String()
}
