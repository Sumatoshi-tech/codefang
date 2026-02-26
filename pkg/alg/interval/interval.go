// Package interval provides an augmented interval tree for efficient
// range-overlap queries. It supports Insert, Delete, QueryOverlap, and
// QueryPoint operations with O(log N) insert/delete and O(log N + k)
// query time, where k is the number of overlapping intervals.
//
// The tree is backed by a red-black tree where each node stores the maximum
// right endpoint (maxHigh) in its subtree, enabling subtree pruning during
// overlap queries.
package interval

// Interval represents a closed range [Low, High] with an associated Value.
type Interval struct {
	Low   uint32
	High  uint32
	Value uint32
}

// Tree is an augmented interval tree supporting overlap queries.
type Tree struct {
	root *node
	size int
}

// node is an internal red-black tree node augmented with maxHigh.
type node struct {
	interval    Interval
	maxHigh     uint32
	left, right *node
	parent      *node
	color       color
}

// color represents the red-black tree node color.
type color bool

// Red-black tree color constants.
const (
	red   color = false
	black color = true
)

// New creates an empty interval tree.
func New() *Tree {
	return &Tree{}
}

// Len returns the number of intervals in the tree.
func (t *Tree) Len() int {
	return t.size
}

// Clear removes all intervals from the tree.
func (t *Tree) Clear() {
	t.root = nil
	t.size = 0
}

// Insert adds an interval [low, high] with the given value to the tree.
func (t *Tree) Insert(low, high, value uint32) {
	n := &node{
		interval: Interval{Low: low, High: high, Value: value},
		maxHigh:  high,
		color:    red,
	}

	t.bstInsert(n)
	t.insertFixup(n)
	t.size++
}

// Delete removes one interval matching [low, high, value] from the tree.
// Returns true if the interval was found and removed, false otherwise.
func (t *Tree) Delete(low, high, value uint32) bool {
	n := t.findNode(low, high, value)
	if n == nil {
		return false
	}

	t.deleteNode(n)
	t.size--

	return true
}

// QueryOverlap returns all intervals that overlap with the query range [low, high].
// An interval [a, b] overlaps [low, high] when a <= high AND b >= low.
func (t *Tree) QueryOverlap(low, high uint32) []Interval {
	if t.root == nil {
		return nil
	}

	var results []Interval

	t.collectOverlap(t.root, low, high, &results)

	return results
}

// QueryPoint returns all intervals containing the given point.
// Equivalent to QueryOverlap(point, point).
func (t *Tree) QueryPoint(point uint32) []Interval {
	return t.QueryOverlap(point, point)
}

// bstInsert performs standard BST insertion by Low (then High for ties).
func (t *Tree) bstInsert(n *node) {
	if t.root == nil {
		t.root = n

		return
	}

	current := t.root

	for {
		updateMaxHigh(current, n.interval.High)

		if compareIntervals(n.interval, current.interval) < 0 {
			if current.left == nil {
				current.left = n
				n.parent = current

				return
			}

			current = current.left
		} else {
			if current.right == nil {
				current.right = n
				n.parent = current

				return
			}

			current = current.right
		}
	}
}

// findNode locates the node matching the exact interval and value.
func (t *Tree) findNode(low, high, value uint32) *node {
	target := Interval{Low: low, High: high, Value: value}

	return t.findExact(t.root, target)
}

// findExact searches for an exact interval match in the subtree.
func (t *Tree) findExact(n *node, target Interval) *node {
	if n == nil {
		return nil
	}

	cmp := compareIntervals(target, n.interval)

	if cmp == 0 && n.interval.Value == target.Value {
		return n
	}

	if cmp < 0 {
		return t.findExact(n.left, target)
	}

	// cmp > 0, or cmp == 0 but value doesn't match — search right subtree.
	// Also check left for duplicate keys with different values.
	if cmp == 0 {
		if found := t.findExact(n.left, target); found != nil {
			return found
		}
	}

	return t.findExact(n.right, target)
}

// deleteNode removes a node from the tree using standard RB-tree deletion.
func (t *Tree) deleteNode(n *node) {
	// If node has two children, swap with in-order successor.
	if n.left != nil && n.right != nil {
		succ := minimum(n.right)
		n.interval = succ.interval
		n = succ
	}

	// n now has at most one child.
	child := n.left
	if child == nil {
		child = n.right
	}

	needFixup := n.color == black

	t.transplant(n, child)
	t.propagateMaxHigh(n.parent)

	if !needFixup {
		return
	}

	if child != nil {
		t.deleteFixup(child)

		return
	}

	if n.parent != nil {
		t.deleteFixupLeaf(n.parent, n == n.parent.left)
		detachFromParent(n, t)
	}
}

// transplant replaces node u with node v in the tree.
func (t *Tree) transplant(u, v *node) {
	switch {
	case u.parent == nil:
		t.root = v
	case u == u.parent.left:
		u.parent.left = v
	default:
		u.parent.right = v
	}

	if v != nil {
		v.parent = u.parent
	}
}

// detachFromParent removes any remaining reference to n from its parent.
func detachFromParent(n *node, t *Tree) {
	if n.parent == nil {
		return
	}

	switch n {
	case n.parent.left:
		n.parent.left = nil
	case n.parent.right:
		n.parent.right = nil
	}

	t.propagateMaxHigh(n.parent)
}

// insertFixup restores red-black properties after insertion.
func (t *Tree) insertFixup(n *node) {
	for n != t.root && nodeColor(n.parent) == red {
		parent := n.parent

		grandparent := parent.parent
		if grandparent == nil {
			break
		}

		isLeft := parent == grandparent.left
		n = t.insertFixupCase(n, parent, grandparent, isLeft)
	}

	t.root.color = black
}

// insertFixupCase handles one side of the insert fixup.
// When leftCase is true, parent is grandparent.left; otherwise parent is grandparent.right.
func (t *Tree) insertFixupCase(n, parent, grandparent *node, leftCase bool) *node {
	uncle := childOf(grandparent, !leftCase)

	if nodeColor(uncle) == red {
		parent.color = black
		uncle.color = black
		grandparent.color = red

		return grandparent
	}

	// Check if n is the "inner" child.
	if n == childOf(parent, !leftCase) {
		t.rotate(parent, leftCase)
		n, parent = parent, n
	}

	parent.color = black
	grandparent.color = red
	t.rotate(grandparent, !leftCase)

	return n
}

// deleteFixup restores red-black properties after deletion (non-nil child case).
func (t *Tree) deleteFixup(x *node) {
	for x != t.root && nodeColor(x) == black {
		if x.parent == nil {
			break
		}

		isLeft := x == x.parent.left
		x = t.deleteFixupCase(x.parent, isLeft)
	}

	if x != nil {
		x.color = black
	}
}

// deleteFixupLeaf restores red-black properties when a black leaf was deleted.
func (t *Tree) deleteFixupLeaf(parent *node, wasLeft bool) {
	for parent != nil {
		result := t.deleteFixupCaseLeaf(parent, wasLeft)
		if result.done {
			break
		}

		// Move up the tree.
		if parent.parent != nil {
			wasLeft = parent == parent.parent.left
		}

		parent = parent.parent
	}
}

// fixupResult captures whether the fixup loop should continue.
type fixupResult struct {
	done bool
}

// deleteFixupCaseLeaf handles one iteration of delete fixup for a nil leaf.
func (t *Tree) deleteFixupCaseLeaf(parent *node, isLeft bool) fixupResult {
	sibling := childOf(parent, !isLeft)
	if sibling == nil {
		return fixupResult{done: false}
	}

	if nodeColor(sibling) == red {
		sibling.color = black
		parent.color = red
		t.rotate(parent, isLeft)

		sibling = childOf(parent, !isLeft)

		if sibling == nil {
			return fixupResult{done: true}
		}
	}

	return t.fixupRecolor(parent, sibling, isLeft)
}

// fixupRecolor handles the recolor/rotation sub-cases of delete fixup.
func (t *Tree) fixupRecolor(parent, sibling *node, isLeft bool) fixupResult {
	outerChild := childOf(sibling, !isLeft)
	innerChild := childOf(sibling, isLeft)

	if nodeColor(innerChild) == black && nodeColor(outerChild) == black {
		sibling.color = red

		if parent.color == red {
			parent.color = black

			return fixupResult{done: true}
		}

		return fixupResult{done: false}
	}

	return t.fixupRotate(parent, sibling, isLeft)
}

// fixupRotate performs the final rotation case of delete fixup.
func (t *Tree) fixupRotate(parent, sibling *node, isLeft bool) fixupResult {
	outerChild := childOf(sibling, !isLeft)

	if nodeColor(outerChild) == black {
		setBlack(childOf(sibling, isLeft))
		sibling.color = red
		t.rotate(sibling, !isLeft)

		sibling = childOf(parent, !isLeft)
		outerChild = childOf(sibling, !isLeft)
	}

	if sibling != nil {
		sibling.color = parent.color
	}

	parent.color = black

	setBlack(outerChild)
	t.rotate(parent, isLeft)

	return fixupResult{done: true}
}

// deleteFixupCase handles one iteration of delete fixup for a real child node.
func (t *Tree) deleteFixupCase(parent *node, isLeft bool) *node {
	sibling := childOf(parent, !isLeft)
	if sibling == nil {
		return parent
	}

	if nodeColor(sibling) == red {
		sibling.color = black
		parent.color = red
		t.rotate(parent, isLeft)

		sibling = childOf(parent, !isLeft)
	}

	if sibling == nil {
		return parent
	}

	outerChild := childOf(sibling, !isLeft)
	innerChild := childOf(sibling, isLeft)

	if nodeColor(innerChild) == black && nodeColor(outerChild) == black {
		sibling.color = red

		return parent
	}

	if nodeColor(outerChild) == black {
		setBlack(innerChild)

		sibling.color = red
		t.rotate(sibling, !isLeft)

		sibling = childOf(parent, !isLeft)
	}

	if sibling != nil {
		sibling.color = parent.color
	}

	parent.color = black

	setBlack(childOf(sibling, !isLeft))
	t.rotate(parent, isLeft)

	return t.root
}

// rotate performs a rotation at node n. When left is true, rotates left;
// otherwise rotates right. Maintains maxHigh augmentation.
func (t *Tree) rotate(n *node, left bool) {
	var pivot *node

	if left {
		pivot = n.right
		n.right = pivot.left

		if pivot.left != nil {
			pivot.left.parent = n
		}

		pivot.left = n
	} else {
		pivot = n.left
		n.left = pivot.right

		if pivot.right != nil {
			pivot.right.parent = n
		}

		pivot.right = n
	}

	pivot.parent = n.parent

	switch {
	case n.parent == nil:
		t.root = pivot
	case n == n.parent.left:
		n.parent.left = pivot
	default:
		n.parent.right = pivot
	}

	n.parent = pivot

	// Recalculate maxHigh bottom-up: n first, then pivot.
	recalcMaxHigh(n)
	recalcMaxHigh(pivot)
}

// collectOverlap recursively collects intervals overlapping [low, high].
func (t *Tree) collectOverlap(n *node, low, high uint32, results *[]Interval) {
	if n == nil {
		return
	}

	// Prune: if maxHigh in this subtree is less than query low, no overlap possible.
	if n.maxHigh < low {
		return
	}

	// Search left subtree.
	t.collectOverlap(n.left, low, high, results)

	// Check current node's interval: overlaps when a <= high AND b >= low.
	if n.interval.Low <= high && n.interval.High >= low {
		*results = append(*results, n.interval)
	}

	// Prune right: if node's Low > high, no right child can overlap.
	if n.interval.Low > high {
		return
	}

	// Search right subtree.
	t.collectOverlap(n.right, low, high, results)
}

// compareIntervals compares two intervals for BST ordering.
// Primary sort by Low, secondary by High.
func compareIntervals(a, b Interval) int {
	if a.Low != b.Low {
		if a.Low < b.Low {
			return -1
		}

		return 1
	}

	if a.High != b.High {
		if a.High < b.High {
			return -1
		}

		return 1
	}

	return 0
}

// nodeColor returns the color of a node, treating nil as black.
func nodeColor(n *node) color {
	if n == nil {
		return black
	}

	return n.color
}

// setBlack sets a node's color to black if it is non-nil.
func setBlack(n *node) {
	if n != nil {
		n.color = black
	}
}

// childOf returns the left or right child of a node.
// When left is true, returns n.left; otherwise n.right.
func childOf(n *node, left bool) *node {
	if n == nil {
		return nil
	}

	if left {
		return n.left
	}

	return n.right
}

// recalcMaxHigh recalculates a node's maxHigh from its interval and children.
func recalcMaxHigh(n *node) {
	if n == nil {
		return
	}

	m := n.interval.High

	if n.left != nil && n.left.maxHigh > m {
		m = n.left.maxHigh
	}

	if n.right != nil && n.right.maxHigh > m {
		m = n.right.maxHigh
	}

	n.maxHigh = m
}

// updateMaxHigh updates a node's maxHigh if the given value is larger.
func updateMaxHigh(n *node, high uint32) {
	if high > n.maxHigh {
		n.maxHigh = high
	}
}

// propagateMaxHigh recalculates maxHigh from the given node up to the root.
func (t *Tree) propagateMaxHigh(n *node) {
	for n != nil {
		recalcMaxHigh(n)
		n = n.parent
	}
}

// minimum returns the leftmost node in the subtree rooted at n.
func minimum(n *node) *node {
	for n.left != nil {
		n = n.left
	}

	return n
}
