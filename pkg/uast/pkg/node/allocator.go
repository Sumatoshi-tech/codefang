package node

// Allocator is a per-worker free-list allocator for [Node] and [Positions].
// It eliminates cross-goroutine [sync.Pool] contention by keeping a local free
// list per parse invocation. Not safe for concurrent use.
type Allocator struct {
	nodes []*Node
	pos   []*Positions
}

// GetNode returns a zeroed Node, reusing from the free list if available.
func (a *Allocator) GetNode() *Node {
	if count := len(a.nodes); count > 0 {
		nd := a.nodes[count-1]
		a.nodes = a.nodes[:count-1]

		return nd
	}

	return &Node{}
}

// PutNode clears the node and returns it to the free list.
func (a *Allocator) PutNode(target *Node) {
	target.ID = ""
	target.Type = ""
	target.Token = ""
	target.Roles = nil
	target.Pos = nil
	target.Props = nil
	target.Children = nil
	a.nodes = append(a.nodes, target)
}

// GetPositions returns a zeroed Positions, reusing from the free list if available.
func (a *Allocator) GetPositions() *Positions {
	if count := len(a.pos); count > 0 {
		positions := a.pos[count-1]
		a.pos = a.pos[:count-1]

		return positions
	}

	return &Positions{}
}

// PutPositions clears the positions and returns them to the free list.
func (a *Allocator) PutPositions(positions *Positions) {
	*positions = Positions{}
	a.pos = append(a.pos, positions)
}

// NewNode creates a Node from the free list and initializes all fields.
func (a *Allocator) NewNode(
	nodeID string, nodeType Type, token string,
	roles []Role, positions *Positions, props map[string]string,
) *Node {
	nd := a.GetNode()
	nd.ID = nodeID
	nd.Type = nodeType
	nd.Token = token
	nd.Roles = roles
	nd.Pos = positions
	nd.Props = props

	return nd
}

// NewPositions creates a Positions from the free list and initializes all fields.
func (a *Allocator) NewPositions(
	startLine, startCol, startOffset, endLine, endCol, endOffset uint,
) *Positions {
	positions := a.GetPositions()
	positions.StartLine = startLine
	positions.StartCol = startCol
	positions.StartOffset = startOffset
	positions.EndLine = endLine
	positions.EndCol = endCol
	positions.EndOffset = endOffset

	return positions
}

// ReleaseTree iteratively returns all nodes and positions in the tree to the
// allocator's free lists. Mirrors [ReleaseTree] but uses the local free list
// instead of the global [sync.Pool].
func (a *Allocator) ReleaseTree(root *Node) {
	if root == nil {
		return
	}

	stack := make([]*Node, 0, defaultStackCap)
	stack = append(stack, root)

	for len(stack) > 0 {
		current := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		stack = append(stack, current.Children...)

		if current.Pos != nil {
			a.PutPositions(current.Pos)
		}

		a.PutNode(current)
	}
}
