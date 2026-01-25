package toposort

import "sort"

// IntGraph represents a directed acyclic graph using integer IDs.
// It is optimized for performance and memory usage.
type IntGraph struct {
	// nodes is an adjacency list where nodes[u] contains a list of v for edges u -> v
	nodes [][]int
	// inDegree stores the number of incoming edges for each node
	inDegree []int
	// nodeCount tracks the number of active nodes
	nodeCount int
}

// NewIntGraph creates a new IntGraph.
func NewIntGraph() *IntGraph {
	return &IntGraph{
		nodes:    make([][]int, 0),
		inDegree: make([]int, 0),
	}
}

// EnsureCapacity ensures the graph can hold at least `n` nodes.
func (g *IntGraph) EnsureCapacity(n int) {
	if n > len(g.nodes) {
		newNodes := make([][]int, n)
		copy(newNodes, g.nodes)
		g.nodes = newNodes

		newInDegree := make([]int, n)
		copy(newInDegree, g.inDegree)
		g.inDegree = newInDegree
	}
}

// AddNode adds a node with the given ID.
// Returns true if the node was added (newly tracked capacity), false otherwise.
// Note: In this implementation, adding a node ID essentially ensures capacity.
func (g *IntGraph) AddNode(id int) bool {
	if id >= len(g.nodes) {
		g.EnsureCapacity(id + 1)
		g.nodeCount = id + 1 // Simplified: assumes contiguous usage or simply max ID
		return true
	}
	return false
}

// AddEdge adds a directed edge from u to v.
// Returns true if the edge was added, false if it already existed.
func (g *IntGraph) AddEdge(u, v int) bool {
	g.EnsureCapacity(max(u, v) + 1)
	
	// Check if edge already exists
	for _, neighbor := range g.nodes[u] {
		if neighbor == v {
			return false
		}
	}

	g.nodes[u] = append(g.nodes[u], v)
	g.inDegree[v]++
	return true
}

// RemoveEdge removes the edge from u to v.
func (g *IntGraph) RemoveEdge(u, v int) bool {
	if u >= len(g.nodes) || v >= len(g.nodes) {
		return false
	}

	for i, neighbor := range g.nodes[u] {
		if neighbor == v {
			// Remove v from u's adjacency list
			g.nodes[u] = append(g.nodes[u][:i], g.nodes[u][i+1:]...)
			g.inDegree[v]--
			return true
		}
	}
	return false
}

// TopoSort performs topological sort using Kahn's algorithm.
// Returns sorted node IDs and a boolean indicating success (true) or cycle detected (false).
func (g *IntGraph) TopoSort() ([]int, bool) {
	n := len(g.nodes)
	if n == 0 {
		return []int{}, true
	}

	// Copy in-degrees to avoid modifying the graph state during sort (optional, but good practice if reuse is needed)
	// But standard Toposort often consumes the graph or uses temp structure.
	// We'll use a temp in-degree array.
	inDegree := make([]int, n)
	copy(inDegree, g.inDegree)

	// Initialize queue with nodes having 0 in-degree
	queue := make([]int, 0)
	for i := 0; i < n; i++ {
		// We only care about nodes that have been "added" or are within range.
		// Since we use slice indices as IDs, all indices < len(nodes) are valid potential nodes.
		// However, some indices might represent unused IDs if IDs are sparse.
		// Ideally IntGraph assumes dense IDs from 0 to N-1.
		if inDegree[i] == 0 {
			queue = append(queue, i)
		}
	}
	
	// Sort initial queue for deterministic output
	sort.Ints(queue)

	result := make([]int, 0, n)
	for len(queue) > 0 {
		u := queue[0]
		queue = queue[1:]
		result = append(result, u)

		for _, v := range g.nodes[u] {
			inDegree[v]--
			if inDegree[v] == 0 {
				// Insert v into queue maintaining sorted order for determinism?
				// Kahn's algorithm doesn't require sorted queue, but for stable/deterministic output we might want it.
				// The original string implementation sorts `S` at the start and when adding `m`.
				// To match that behavior (lexicographical sort of available nodes), we need to keep queue sorted.
				insertSorted(&queue, v)
			}
		}
	}

	// Check for cycles
	if len(result) != g.activeNodeCount() {
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
func (g *IntGraph) FindCycle(start int) []int {
	if start >= len(g.nodes) {
		return []int{}
	}

	pathMap := make(map[int]int) // node -> parent
	
	// BFS
	q := []int{start}
	pathMap[start] = -1 // Root
	
	for len(q) > 0 {
		u := q[0]
		q = q[1:]
		
		for _, v := range g.nodes[u] {
			if v == start {
				// Found cycle: u -> start
				cycle := []int{start}
				curr := u
				for curr != start && curr != -1 {
					cycle = append(cycle, curr)
					curr = pathMap[curr]
				}
				cycle = append(cycle, start)
				
				// Reverse to get start -> ... -> u -> start
				for i, j := 0, len(cycle)-1; i < j; i, j = i+1, j-1 {
					cycle[i], cycle[j] = cycle[j], cycle[i]
				}
				return cycle
			}
			
			if _, visited := pathMap[v]; !visited {
				pathMap[v] = u
				q = append(q, v)
			}
		}
	}
	
	return []int{}
}

// activeNodeCount returns counts of nodes involved in the graph
// Since we blindly iterate 0..len(g.nodes), all are considered active.
func (g *IntGraph) activeNodeCount() int {
	return len(g.nodes)
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// insertSorted inserts v into sorted slice s
func insertSorted(s *[]int, v int) {
	i := sort.SearchInts(*s, v)
	*s = append(*s, 0)
	copy((*s)[i+1:], (*s)[i:])
	(*s)[i] = v
}
