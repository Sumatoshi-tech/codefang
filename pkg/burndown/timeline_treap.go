// Package burndown: implicit treap Timeline (position = implicit key). No key shifting on Replace.

package burndown

import (
	"fmt"
	"math"
)

// treapNode is a single segment: length lines with value Value. Position is implicit (sum of left subtree sizes).
type treapNode struct {
	left, right *treapNode
	length      int
	value       TimeKey
	size        int
	priority    uint32
}

func (n *treapNode) recalcSize() {
	n.size = n.length
	if n.left != nil {
		n.size += n.left.size
	}
	if n.right != nil {
		n.size += n.right.size
	}
}

// treapTimeline implements Timeline using an implicit treap (position = size of left subtree).
// Replace is O(log N) without shifting keys; no updateSubsequentKeys.
type treapTimeline struct {
	root         *treapNode
	totalLength  int
	nextPriority uint32
}

// NewTreapTimeline creates a Timeline backed by an implicit treap with initial [0, length) at time t.
func NewTreapTimeline(time, length int) Timeline {
	if time < 0 || time > math.MaxUint32 {
		panic(fmt.Sprintf("time out of range: %d", time))
	}
	if length < 0 || length > math.MaxUint32 {
		panic(fmt.Sprintf("length out of range: %d", length))
	}
	t := &treapTimeline{totalLength: length}
	if length > 0 {
		t.root = t.merge(t.newNode(length, TimeKey(time)), t.newNode(0, TreeEnd))
	}
	return t
}

func (tt *treapTimeline) newNode(length int, value TimeKey) *treapNode {
	tt.nextPriority++
	return &treapNode{length: length, value: value, size: length, priority: tt.nextPriority}
}

func (tt *treapTimeline) merge(l, r *treapNode) *treapNode {
	if l == nil {
		return r
	}
	if r == nil {
		return l
	}
	if l.priority >= r.priority {
		l.right = tt.merge(l.right, r)
		l.recalcSize()
		return l
	}
	r.left = tt.merge(l, r.left)
	r.recalcSize()
	return r
}

// splitByLines splits so left has the first pos lines (0-indexed), right has the rest.
func (tt *treapTimeline) splitByLines(root *treapNode, pos int) (left, right *treapNode) {
	if root == nil {
		return nil, nil
	}
	leftSize := 0
	if root.left != nil {
		leftSize = root.left.size
	}
	if pos <= leftSize {
		l, r := tt.splitByLines(root.left, pos)
		root.left = r
		root.recalcSize()
		return l, root
	}
	if pos >= leftSize+root.length {
		l, r := tt.splitByLines(root.right, pos-leftSize-root.length)
		root.right = l
		root.recalcSize()
		return root, r
	}
	// Split inside root's segment: [leftSize, leftSize+root.length) at pos.
	leftPart := tt.newNode(pos-leftSize, root.value)
	rightPart := tt.newNode(leftSize+root.length-pos, root.value)
	l := tt.merge(root.left, leftPart)
	r := tt.merge(rightPart, root.right)
	return l, r
}

func (tt *treapTimeline) collectReports(n *treapNode, currentTime int, reports []DeltaReport) []DeltaReport {
	if n == nil {
		return reports
	}
	reports = tt.collectReports(n.left, currentTime, reports)
	if n.length > 0 && n.value != TreeEnd {
		reports = append(reports, DeltaReport{Current: currentTime, Previous: int(n.value), Delta: -n.length})
	}
	reports = tt.collectReports(n.right, currentTime, reports)
	return reports
}

// Replace applies delete [pos, pos+delLines) then insert insLines at pos with time t.
func (tt *treapTimeline) Replace(pos, delLines, insLines int, t TimeKey) []DeltaReport {
	if tt.root == nil {
		if pos != 0 || delLines != 0 {
			panic("Replace on empty timeline with non-zero pos or delLines")
		}
		if insLines > 0 {
			tt.root = tt.merge(tt.newNode(insLines, t), tt.newNode(0, TreeEnd))
			tt.totalLength = insLines
		}
		return nil
	}
	if pos > tt.totalLength {
		panic(fmt.Sprintf("Replace pos %d > Len %d", pos, tt.totalLength))
	}
	if pos+delLines > tt.totalLength {
		panic(fmt.Sprintf("Replace [%d,%d) out of range (Len %d)", pos, pos+delLines, tt.totalLength))
	}

	left, right := tt.splitByLines(tt.root, pos)
	midSeg, right2 := tt.splitByLines(right, delLines)

	var reports []DeltaReport
	reports = tt.collectReports(midSeg, int(t), reports)

	var mid *treapNode
	if insLines > 0 {
		mid = tt.newNode(insLines, t)
	}
	tt.root = tt.merge(left, tt.merge(mid, right2))
	tt.totalLength += insLines - delLines
	return reports
}

func (tt *treapTimeline) iterate(n *treapNode, offset int, fn func(offset int, length int, t TimeKey) bool) (int, bool) {
	if n == nil {
		return offset, true
	}
	off, ok := tt.iterate(n.left, offset, fn)
	if !ok {
		return off, false
	}
	if n.length > 0 {
		if !fn(off, n.length, n.value) {
			return off, false
		}
	}
	return tt.iterate(n.right, off+n.length, fn)
}

func (tt *treapTimeline) Iterate(fn func(offset int, length int, t TimeKey) bool) {
	tt.iterate(tt.root, 0, fn)
}

func (tt *treapTimeline) Len() int {
	return tt.totalLength
}

func (tt *treapTimeline) nodeCount(n *treapNode) int {
	if n == nil {
		return 0
	}
	return 1 + tt.nodeCount(n.left) + tt.nodeCount(n.right)
}

func (tt *treapTimeline) Nodes() int {
	return tt.nodeCount(tt.root)
}

func (tt *treapTimeline) Validate() {
	if tt.root == nil {
		if tt.totalLength != 0 {
			panic("empty root but totalLength != 0")
		}
		return
	}
	// First segment must start at 0 (implicit in treap). Last segment must be TreeEnd.
	var lastVal TimeKey
	var check func(n *treapNode)
	check = func(n *treapNode) {
		if n == nil {
			return
		}
		check(n.left)
		if n.value == TreeMergeMark {
			panic(fmt.Sprintf("unmerged lines left at segment length %d", n.length))
		}
		lastVal = n.value
		check(n.right)
	}
	check(tt.root)
	if lastVal != TreeEnd {
		panic(fmt.Sprintf("last value must be TreeEnd, got %d", lastVal))
	}
}

func (tt *treapTimeline) cloneShallow() *treapTimeline {
	return &treapTimeline{root: tt.root, totalLength: tt.totalLength, nextPriority: tt.nextPriority}
}

func (tt *treapTimeline) cloneDeepNode(n *treapNode) *treapNode {
	if n == nil {
		return nil
	}
	tt.nextPriority++
	c := &treapNode{length: n.length, value: n.value, size: n.size, priority: tt.nextPriority}
	c.left = tt.cloneDeepNode(n.left)
	c.right = tt.cloneDeepNode(n.right)
	return c
}

func (tt *treapTimeline) CloneShallow() Timeline {
	return tt.cloneShallow()
}

func (tt *treapTimeline) CloneDeep() Timeline {
	out := &treapTimeline{totalLength: tt.totalLength, nextPriority: tt.nextPriority}
	out.root = out.cloneDeepNode(tt.root)
	return out
}

func (tt *treapTimeline) Erase() {
	tt.root = nil
	tt.totalLength = 0
}

func (tt *treapTimeline) Flatten() []int {
	lines := make([]int, 0, tt.totalLength)
	tt.iterate(tt.root, 0, func(offset, length int, t TimeKey) bool {
		for i := 0; i < length; i++ {
			lines = append(lines, int(t))
		}
		return true
	})
	return lines
}

func (tt *treapTimeline) Reconstruct(lines []int) {
	tt.root = nil
	tt.totalLength = len(lines)
	if len(lines) == 0 {
		return
	}
	type seg struct {
		length int
		value  TimeKey
	}
	var segs []seg
	for i := 0; i < len(lines); {
		v := TimeKey(lines[i])
		j := i + 1
		for j < len(lines) && lines[j] == lines[i] {
			j++
		}
		segs = append(segs, seg{length: j - i, value: v})
		i = j
	}
	var buildFromSegs func(start, end int) *treapNode
	buildFromSegs = func(start, end int) *treapNode {
		if start >= end {
			return nil
		}
		mid := (start + end) / 2
		s := segs[mid]
		tt.nextPriority++
		n := &treapNode{length: s.length, value: s.value, priority: tt.nextPriority}
		n.left = buildFromSegs(start, mid)
		n.right = buildFromSegs(mid+1, end)
		n.recalcSize()
		return n
	}
	tt.root = buildFromSegs(0, len(segs))
	tt.root = tt.merge(tt.root, tt.newNode(0, TreeEnd))
}

// MergeAdjacentSameValue is a no-op for treap; segment coalescing is optional and not required for correctness.
func (tt *treapTimeline) MergeAdjacentSameValue() {}
