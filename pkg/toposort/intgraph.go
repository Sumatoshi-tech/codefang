// Package toposort provides topological sorting for directed acyclic graphs.
package toposort

import (
	"slices"
	"sort"
)

// IntGraph represents a directed acyclic graph using integer IDs.
// It is optimized for performance and memory usage.
type IntGraph struct {
	// Nodes is an adjacency list where nodes[u] contains a list of v for edges u -> v.
	nodes [][]int
	// InDegree stores the number of incoming edges for each node.
	inDegree []int
	// NodeCount tracks the number of active nodes.
	nodeCount int
}

// NewIntGraph creates a new IntGraph.
func NewIntGraph() *IntGraph {
	return &IntGraph{
		nodes:     make([][]int, 0),
		inDegree:  make([]int, 0),
		nodeCount: 0,
	}
}

// EnsureCapacity ensures the graph can hold at least `n` nodes.
func (graph *IntGraph) EnsureCapacity(nodeCapacity int) {
	if nodeCapacity > len(graph.nodes) {
		newNodes := make([][]int, nodeCapacity)
		copy(newNodes, graph.nodes)
		graph.nodes = newNodes

		newInDegree := make([]int, nodeCapacity)
		copy(newInDegree, graph.inDegree)
		graph.inDegree = newInDegree
	}
}

// AddNode adds a node with the given ID.
// Returns true if the node was added (newly tracked capacity), false otherwise.
// Note: In this implementation, adding a node ID essentially ensures capacity.
func (graph *IntGraph) AddNode(id int) bool {
	if id >= len(graph.nodes) {
		graph.EnsureCapacity(id + 1)
		graph.nodeCount = id + 1 // Simplified: assumes contiguous usage or simply max ID.

		return true
	}

	return false
}

// AddEdge adds a directed edge from src to dst.
// Returns true if the edge was added, false if it already existed.
func (graph *IntGraph) AddEdge(src, dst int) bool {
	graph.EnsureCapacity(max(src, dst) + 1)

	// Check if edge already exists.
	if slices.Contains(graph.nodes[src], dst) {
		return false
	}

	graph.nodes[src] = append(graph.nodes[src], dst)
	graph.inDegree[dst]++

	return true
}

// RemoveEdge removes the edge from src to dst.
func (graph *IntGraph) RemoveEdge(src, dst int) bool {
	if src >= len(graph.nodes) || dst >= len(graph.nodes) {
		return false
	}

	for idx, neighbor := range graph.nodes[src] {
		if neighbor == dst {
			// Remove dst from src's adjacency list.
			graph.nodes[src] = append(graph.nodes[src][:idx], graph.nodes[src][idx+1:]...)
			graph.inDegree[dst]--

			return true
		}
	}

	return false
}

// TopoSort performs topological sort using Kahn's algorithm.
// Returns sorted node IDs and a boolean indicating success (true) or cycle detected (false).
func (graph *IntGraph) TopoSort() ([]int, bool) {
	nodeCount := len(graph.nodes)
	if nodeCount == 0 {
		return []int{}, true
	}

	// Copy in-degrees to avoid modifying the graph state during sort (optional, but good practice if reuse is needed).
	// But standard Toposort often consumes the graph or uses temp structure.
	// We'll use a temp in-degree array.
	inDegree := make([]int, nodeCount)
	copy(inDegree, graph.inDegree)

	// Initialize queue with nodes having 0 in-degree.
	queue := make([]int, 0)

	for idx := range nodeCount {
		// We only care about nodes that have been "added" or are within range.
		// Since we use slice indices as IDs, all indices < len(nodes) are valid potential nodes.
		// However, some indices might represent unused IDs if IDs are sparse.
		// Ideally IntGraph assumes dense IDs from 0 to N-1.
		if inDegree[idx] == 0 {
			queue = append(queue, idx)
		}
	}

	// Sort initial queue for deterministic output.
	sort.Ints(queue)

	result := make([]int, 0, nodeCount)

	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]

		result = append(result, cur)

		for _, neighbor := range graph.nodes[cur] {
			inDegree[neighbor]--
			if inDegree[neighbor] == 0 {
				// Insert neighbor into queue maintaining sorted order for determinism.
				// Kahn's algorithm doesn't require sorted queue, but for stable/deterministic output we might want it.
				// The original string implementation sorts `S` at the start and when adding `m`.
				// To match that behavior (lexicographical sort of available nodes), we need to keep queue sorted.
				insertSorted(&queue, neighbor)
			}
		}
	}

	// Check for cycles.
	if len(result) != graph.activeNodeCount() {
		// If we processed fewer nodes than active nodes (nodes with edges or explicitly added), there's a cycle.
		// Note: nodes with 0 in-degree and 0 out-degree are included in result.
		// But "unused" gaps in ID space (if any) would have 0 in-degree and be included.
		// So len(result) should equal len(g.nodes) if all IDs are used.
		// If we rely on sparse IDs, this check is tricky.
		// For now, assume IDs are 0..N-1.
		return result, false
	}

	return result, true
}

// FindCycle returns a cycle in the graph containing the start node.
// Returns empty slice if no cycle found.
//
//nolint:gocognit // BFS cycle detection with path reconstruction is inherently complex.
func (graph *IntGraph) FindCycle(start int) []int {
	if start >= len(graph.nodes) {
		return []int{}
	}

	pathMap := make(map[int]int) // Node to parent mapping.

	// BFS traversal.
	bfsQueue := []int{start}
	pathMap[start] = -1 // Root sentinel.

	for len(bfsQueue) > 0 {
		cur := bfsQueue[0]
		bfsQueue = bfsQueue[1:]

		for _, neighbor := range graph.nodes[cur] {
			if neighbor == start {
				// Found cycle: cur -> start.
				cycle := []int{start}
				curr := cur

				for curr != start && curr != -1 {
					cycle = append(cycle, curr)
					curr = pathMap[curr]
				}

				cycle = append(cycle, start)

				// Reverse to get start -> ... -> cur -> start.
				for left, right := 0, len(cycle)-1; left < right; left, right = left+1, right-1 {
					cycle[left], cycle[right] = cycle[right], cycle[left]
				}

				return cycle
			}

			if _, visited := pathMap[neighbor]; !visited {
				pathMap[neighbor] = cur
				bfsQueue = append(bfsQueue, neighbor)
			}
		}
	}

	return []int{}
}

// activeNodeCount returns counts of nodes involved in the graph.
// Since we blindly iterate 0..len(g.nodes), all are considered active.
func (graph *IntGraph) activeNodeCount() int {
	return len(graph.nodes)
}

// insertSorted inserts val into sorted slice s.
func insertSorted(sortedSlice *[]int, val int) {
	idx := sort.SearchInts(*sortedSlice, val)
	*sortedSlice = append(*sortedSlice, 0)
	copy((*sortedSlice)[idx+1:], (*sortedSlice)[idx:])
	(*sortedSlice)[idx] = val
}
