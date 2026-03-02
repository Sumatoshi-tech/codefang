// Package interval provides an augmented interval tree for efficient
// range-overlap queries. It supports Insert, Delete, QueryOverlap, and
// QueryPoint operations with O(log N) insert/delete and O(log N + k)
// query time, where k is the number of overlapping intervals.
//
// The tree is backed by a red-black tree where each node stores the maximum
// right endpoint (maxHigh) in its subtree, enabling subtree pruning during
// overlap queries.
package interval

// Integer constrains interval endpoints to integer types.
type Integer interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~uintptr
}

// Interval represents a closed range [Low, High] with an associated Value.
type Interval[K Integer, V comparable] struct {
	Low   K
	High  K
	Value V
}

// Tree is an augmented interval tree supporting overlap queries.
type Tree[K Integer, V comparable] struct {
	root *node[K, V]
	size int
}

// node is an internal red-black tree node augmented with maxHigh.
type node[K Integer, V comparable] struct {
	interval    Interval[K, V]
	maxHigh     K
	left, right *node[K, V]
	parent      *node[K, V]
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
func New[K Integer, V comparable]() *Tree[K, V] {
	return &Tree[K, V]{}
}

// Len returns the number of intervals in the tree.
func (t *Tree[K, V]) Len() int {
	return t.size
}

// Clear removes all intervals from the tree.
func (t *Tree[K, V]) Clear() {
	t.root = nil
	t.size = 0
}

// Insert adds an interval [low, high] with the given value to the tree.
func (t *Tree[K, V]) Insert(low, high K, value V) {
	n := &node[K, V]{
		interval: Interval[K, V]{Low: low, High: high, Value: value},
		maxHigh:  high,
		color:    red,
	}

	t.bstInsert(n)
	t.insertFixup(n)
	t.size++
}

// Delete removes one interval matching [low, high, value] from the tree.
// Returns true if the interval was found and removed, false otherwise.
func (t *Tree[K, V]) Delete(low, high K, value V) bool {
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
func (t *Tree[K, V]) QueryOverlap(low, high K) []Interval[K, V] {
	if t.root == nil {
		return nil
	}

	var results []Interval[K, V]

	t.collectOverlap(t.root, low, high, &results)

	return results
}

// QueryPoint returns all intervals containing the given point.
// Equivalent to QueryOverlap(point, point).
func (t *Tree[K, V]) QueryPoint(point K) []Interval[K, V] {
	return t.QueryOverlap(point, point)
}

// bstInsert performs standard BST insertion by Low (then High for ties).
func (t *Tree[K, V]) bstInsert(n *node[K, V]) {
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
func (t *Tree[K, V]) findNode(low, high K, value V) *node[K, V] {
	target := Interval[K, V]{Low: low, High: high, Value: value}

	return t.findExact(t.root, target)
}

// findExact searches for an exact interval match in the subtree.
func (t *Tree[K, V]) findExact(n *node[K, V], target Interval[K, V]) *node[K, V] {
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
func (t *Tree[K, V]) deleteNode(n *node[K, V]) {
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
func (t *Tree[K, V]) transplant(u, v *node[K, V]) {
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
func detachFromParent[K Integer, V comparable](n *node[K, V], t *Tree[K, V]) {
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
func (t *Tree[K, V]) insertFixup(n *node[K, V]) {
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
func (t *Tree[K, V]) insertFixupCase(n, parent, gp *node[K, V], leftCase bool) *node[K, V] {
	uncle := childOf(gp, !leftCase)

	if nodeColor(uncle) == red {
		parent.color = black
		uncle.color = black
		gp.color = red

		return gp
	}

	// Check if n is the "inner" child.
	if n == childOf(parent, !leftCase) {
		t.rotate(parent, leftCase)
		n, parent = parent, n
	}

	parent.color = black
	gp.color = red
	t.rotate(gp, !leftCase)

	return n
}

// deleteFixup restores red-black properties after deletion (non-nil child case).
func (t *Tree[K, V]) deleteFixup(x *node[K, V]) {
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
func (t *Tree[K, V]) deleteFixupLeaf(parent *node[K, V], wasLeft bool) {
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
func (t *Tree[K, V]) deleteFixupCaseLeaf(parent *node[K, V], isLeft bool) fixupResult {
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
func (t *Tree[K, V]) fixupRecolor(parent, sibling *node[K, V], isLeft bool) fixupResult {
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
func (t *Tree[K, V]) fixupRotate(parent, sibling *node[K, V], isLeft bool) fixupResult {
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
func (t *Tree[K, V]) deleteFixupCase(parent *node[K, V], isLeft bool) *node[K, V] {
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
func (t *Tree[K, V]) rotate(n *node[K, V], left bool) {
	var pivot *node[K, V]

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
func (t *Tree[K, V]) collectOverlap(n *node[K, V], low, high K, results *[]Interval[K, V]) {
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
func compareIntervals[K Integer, V comparable](a, b Interval[K, V]) int {
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
func nodeColor[K Integer, V comparable](n *node[K, V]) color {
	if n == nil {
		return black
	}

	return n.color
}

// setBlack sets a node's color to black if it is non-nil.
func setBlack[K Integer, V comparable](n *node[K, V]) {
	if n != nil {
		n.color = black
	}
}

// childOf returns the left or right child of a node.
// When left is true, returns n.left; otherwise n.right.
func childOf[K Integer, V comparable](n *node[K, V], left bool) *node[K, V] {
	if n == nil {
		return nil
	}

	if left {
		return n.left
	}

	return n.right
}

// recalcMaxHigh recalculates a node's maxHigh from its interval and children.
func recalcMaxHigh[K Integer, V comparable](n *node[K, V]) {
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
func updateMaxHigh[K Integer, V comparable](n *node[K, V], high K) {
	if high > n.maxHigh {
		n.maxHigh = high
	}
}

// propagateMaxHigh recalculates maxHigh from the given node up to the root.
func (t *Tree[K, V]) propagateMaxHigh(n *node[K, V]) {
	for n != nil {
		recalcMaxHigh(n)
		n = n.parent
	}
}

// minimum returns the leftmost node in the subtree rooted at n.
func minimum[K Integer, V comparable](n *node[K, V]) *node[K, V] {
	for n.left != nil {
		n = n.left
	}

	return n
}
