// Package burndown provides file-level line interval tracking for burndown analysis.
package burndown

import (
	"fmt"
	"math"

	"github.com/Sumatoshi-tech/codefang/pkg/mathutil"
	"github.com/Sumatoshi-tech/codefang/pkg/rbtree"
)

// Updater is the function which is called back on File.Update().
type Updater = func(currentTime, previousTime, delta int)

// File encapsulates a balanced binary tree to store line intervals and
// a cumulative mapping of values to the corresponding length counters. Users
// are not supposed to create File-s directly; instead, they should call NewFile().
// NewFileFromTree() is the special constructor which is useful in the tests.
//
// Len() returns the number of lines in File.
//
// Update() mutates File by introducing tree structural changes and updating the
// length mapping.
//
// Dump() writes the tree to a string and Validate() checks the tree integrity.
type File struct {
	tree     *rbtree.RBTree
	updaters []Updater
}

// TreeEnd denotes the value of the last leaf in the tree.
const TreeEnd = math.MaxUint32

// TreeMaxBinPower is the binary power value which corresponds to the maximum tick which
// can be stored in the tree.
const TreeMaxBinPower = 14

// TreeMergeMark is the special day which disables the status updates and is used in File.Merge().
const TreeMergeMark = (1 << TreeMaxBinPower) - 1

// NewFile initializes a new instance of File struct.
//
// The time parameter is the starting value of the first node;
//
// The length parameter is the starting length of the tree (the key of the second and the
// last node);
//
// The updaters parameter lists the attached interval length mappings.
func NewFile(time, length int, allocator *rbtree.Allocator, updaters ...Updater) *File {
	file := &File{tree: rbtree.NewRBTree(allocator), updaters: updaters}
	file.updateTime(time, time, length)

	if time < 0 || time > math.MaxUint32 {
		panic(fmt.Sprintf("time is out of allowed range: %d", time))
	}

	if length > math.MaxUint32 {
		panic(fmt.Sprintf("length is out of allowed range: %d", length))
	}

	if length > 0 {
		file.tree.Insert(rbtree.Item{Key: 0, Value: uint32(time)}) //nolint:gosec // overflow guarded above.
	}

	file.tree.Insert(rbtree.Item{Key: uint32(length), Value: TreeEnd}) //nolint:gosec // overflow guarded above.

	return file
}

func (file *File) updateTime(currentTime, previousTime, delta int) {
	if previousTime&TreeMergeMark == TreeMergeMark {
		if currentTime == previousTime {
			return
		}

		panic("previousTime cannot be TreeMergeMark")
	}

	if currentTime&TreeMergeMark == TreeMergeMark {
		// Merge mode - we have already updated in one of the branches.
		return
	}

	for _, update := range file.updaters {
		update(currentTime, previousTime, delta)
	}
}

// CloneShallow copies the file. It performs a shallow copy of the tree: the allocator
// must be Clone()-d beforehand.
func (file *File) CloneShallow(allocator *rbtree.Allocator) *File {
	return &File{tree: file.tree.CloneShallow(allocator), updaters: file.updaters}
}

// CloneDeep copies the file. It performs a deep copy of the tree.
func (file *File) CloneDeep(allocator *rbtree.Allocator) *File {
	return &File{tree: file.tree.CloneDeep(allocator), updaters: file.updaters}
}

// Delete deallocates the file.
func (file *File) Delete() {
	file.tree.Erase()
}

// ReplaceUpdaters replaces the file's updaters with a new set.
func (file *File) ReplaceUpdaters(updaters []Updater) {
	file.updaters = updaters
}

// Len returns the number of lines in the file.
func (file *File) Len() int {
	return int(file.tree.Max().Item().Key)
}

// Nodes returns the number of RBTree nodes in the file.
func (file *File) Nodes() int {
	return file.tree.Len()
}

// Update modifies the underlying tree to adapt to the specified line changes.
//
// The time parameter is the time when the requested changes are made. Sets the values of the
// inserted nodes.
//
// The pos parameter is the index of the line at which the changes are introduced.
//
// The insLength parameter is the number of inserted lines after pos.
//
// The delLength parameter is the number of removed lines after pos. Deletions come before
// the insertions.
//
// The code inside this function is probably the most important one throughout
// the project. It is extensively covered with tests. If you find a bug, please
// add the corresponding case in file_test.go.
func (file *File) Update(time, pos, insLength, delLength int) {
	if time < 0 {
		panic("time may not be negative")
	}

	if time >= math.MaxUint32 {
		panic("time may not be >= MaxUint32")
	}

	if pos < 0 {
		panic("attempt to insert/delete at a negative position")
	}

	if pos > math.MaxUint32 {
		panic("pos may not be > MaxUint32")
	}

	if insLength < 0 || delLength < 0 {
		panic("insLength and delLength must be non-negative")
	}

	if insLength|delLength == 0 {
		return
	}

	tree := file.tree

	if tree.Len() < 2 && tree.Min().Item().Key != 0 {
		panic("invalid tree state")
	}

	if uint32(pos) > tree.Max().Item().Key { //nolint:gosec // overflow guarded above.
		panic(fmt.Sprintf("attempt to insert after the end of the file: %d < %d",
			tree.Max().Item().Key, pos))
	}

	iter := tree.FindLE(uint32(pos)) //nolint:gosec // overflow guarded above.
	origin := *iter.Item()
	prevOrigin := origin

	{
		prevIter := iter.Prev()
		if prevIter.Item() != nil {
			prevOrigin = *prevIter.Item()
		}
	}

	if insLength > 0 {
		file.updateTime(time, time, insLength)
	}

	if delLength == 0 {
		file.updateInsertOnly(tree, &iter, &origin, time, pos, insLength)

		return
	}

	file.updateWithDeletions(tree, &iter, &origin, &prevOrigin, time, pos, insLength, delLength)
}

func (file *File) updateInsertOnly(
	tree *rbtree.RBTree, iter *rbtree.Iterator, origin *rbtree.Item,
	time, pos, insLength int,
) {
	// Simple case with insertions only.
	//nolint:gosec // overflow guarded by caller's range checks.
	if origin.Key < uint32(pos) || (origin.Value == uint32(time) && (pos == 0 || uint32(pos) == origin.Key)) {
		*iter = iter.Next()
	}

	for ; !iter.Limit(); *iter = iter.Next() {
		iter.Item().Key += uint32(insLength) //nolint:gosec // guarded by caller.
	}

	if origin.Value != uint32(time) { //nolint:gosec // guarded by caller.
		tree.Insert(rbtree.Item{Key: uint32(pos), Value: uint32(time)}) //nolint:gosec // guarded by caller.

		if origin.Key < uint32(pos) { //nolint:gosec // guarded by caller.
			tree.Insert(rbtree.Item{Key: uint32(pos + insLength), Value: origin.Value}) //nolint:gosec // guarded by caller.
		}
	}
}

func (file *File) updateWithDeletions(
	tree *rbtree.RBTree, iter *rbtree.Iterator, origin, prevOrigin *rbtree.Item,
	time, pos, insLength, delLength int,
) {
	file.deleteOverlappingNodes(tree, iter, origin, prevOrigin, time, pos, insLength, delLength)
	previous := file.prepareInsertion(tree, iter, origin, time, pos, insLength, delLength)
	file.updateSubsequentKeys(iter, origin, pos, insLength, delLength)
	file.finalizeInterval(tree, origin, prevOrigin, previous, time, pos, insLength)
}

func (file *File) deleteOverlappingNodes(
	tree *rbtree.RBTree, iter *rbtree.Iterator, origin, prevOrigin *rbtree.Item,
	time, pos, insLength, delLength int,
) {
	for {
		node := iter.Item()
		nextIter := iter.Next()

		if nextIter.Limit() {
			if uint32(pos+delLength) > node.Key { //nolint:gosec // guarded by caller.
				panic("attempt to delete after the end of the file")
			}

			break
		}

		delta := mathutil.Min(int(nextIter.Item().Key), pos+delLength) - mathutil.Max(int(node.Key), pos)

		//nolint:gosec // overflow guarded by caller's range checks.
		if delta == 0 && insLength == 0 && origin.Key == uint32(pos) && prevOrigin.Value == node.Value {
			*origin = *node

			tree.DeleteWithIterator(*iter)
			*iter = nextIter
		}

		if delta <= 0 {
			break
		}

		file.updateTime(time, int(node.Value), -delta)

		if node.Key >= uint32(pos) { //nolint:gosec // guarded by caller.
			*origin = *node

			tree.DeleteWithIterator(*iter)
		}

		*iter = nextIter
	}
}

//nolint:nestif // insertion preparation has intertwined conditions.
func (file *File) prepareInsertion(
	tree *rbtree.RBTree, iter *rbtree.Iterator, origin *rbtree.Item,
	time, pos, insLength, delLength int,
) *rbtree.Item {
	if insLength > 0 && (origin.Value != uint32(time) || origin.Key == uint32(pos)) { //nolint:gosec // guarded by caller.
		if iter.Item().Value == uint32(time) && int(iter.Item().Key)-delLength == pos { //nolint:gosec // guarded by caller.
			prev := iter.Prev()

			if prev.NegativeLimit() || prev.Item().Value != uint32(time) { //nolint:gosec // guarded by caller.
				iter.Item().Key = uint32(pos) //nolint:gosec // guarded by caller.
			} else {
				tree.DeleteWithIterator(*iter)
				*iter = prev
			}

			origin.Value = uint32(time) //nolint:gosec // Cancels the insertion after applying the delta.
		} else {
			_, *iter = tree.Insert(rbtree.Item{Key: uint32(pos), Value: uint32(time)}) //nolint:gosec // guarded by caller.
		}

		return nil
	}

	// Rollback 1 position back, see deletion cycle above.
	*iter = iter.Prev()

	return iter.Item()
}

func (file *File) updateSubsequentKeys(
	iter *rbtree.Iterator, origin *rbtree.Item, pos, insLength, delLength int,
) {
	delta := insLength - delLength
	if delta == 0 {
		return
	}

	for *iter = iter.Next(); !iter.Limit(); *iter = iter.Next() {
		iter.Item().Key = uint32(int(iter.Item().Key) + delta) //nolint:gosec // delta is bounded.
	}

	if origin.Key > uint32(pos) { //nolint:gosec // guarded by caller.
		origin.Key = uint32(int(origin.Key) + delta) //nolint:gosec // delta is bounded.
	}
}

func (file *File) finalizeInterval(
	tree *rbtree.RBTree, origin, prevOrigin *rbtree.Item, previous *rbtree.Item,
	time, pos, insLength int,
) {
	if insLength > 0 {
		if origin.Value != uint32(time) { //nolint:gosec // guarded by caller.
			tree.Insert(rbtree.Item{Key: uint32(pos + insLength), Value: origin.Value}) //nolint:gosec // guarded by caller.
		} else if pos == 0 {
			tree.Insert(rbtree.Item{Key: uint32(pos), Value: uint32(time)}) //nolint:gosec // guarded by caller.
		}

		return
	}

	if (uint32(pos) > origin.Key && previous != nil && previous.Value != origin.Value) || //nolint:gosec // guarded by caller.
		(uint32(pos) == origin.Key && origin.Value != prevOrigin.Value) || //nolint:gosec // guarded by caller.
		pos == 0 {
		tree.Insert(rbtree.Item{Key: uint32(pos), Value: origin.Value})
	}
}

// isMergeMarked checks if a line value has the merge mark bit set.
func isMergeMarked(value int) bool {
	return value&TreeMergeMark == TreeMergeMark
}

// Merge combines several prepared File-s together.
func (file *File) Merge(day int, others ...*File) {
	myself := file.flatten()
	mergeOtherFiles(myself, others)
	file.resolveMergeConflicts(myself, day)
	file.reconstructTree(myself)
}

func mergeOtherFiles(myself []int, others []*File) {
	for _, other := range others {
		if other == nil {
			panic("merging with a nil file")
		}

		lines := other.flatten()

		if len(myself) != len(lines) {
			panic(fmt.Sprintf("file corruption, lines number mismatch during merge %d != %d",
				len(myself), len(lines)))
		}

		for i, myLine := range myself {
			otherLine := lines[i]

			if isMergeMarked(otherLine) {
				continue
			}

			if isMergeMarked(myLine) || myLine&TreeMergeMark > otherLine&TreeMergeMark {
				myself[i] = otherLine
			}
		}
	}
}

func (file *File) resolveMergeConflicts(lines []int, day int) {
	for i, l := range lines {
		if isMergeMarked(l) {
			lines[i] = day
			file.updateTime(day, day, 1)
		}
	}
}

func (file *File) reconstructTree(lines []int) {
	file.tree.Erase()
	tree := rbtree.NewRBTree(file.tree.Allocator())

	for i, v := range lines {
		if i == 0 || v != lines[i-1] {
			tree.Insert(rbtree.Item{Key: uint32(i), Value: uint32(v)}) //nolint:gosec // values are bounded by tree structure.
		}
	}

	tree.Insert(rbtree.Item{Key: uint32(len(lines)), Value: TreeEnd}) //nolint:gosec // len is bounded.
	file.tree = tree
}

// Dump formats the underlying line interval tree into a string.
// Useful for error messages, panic()-s and debugging.
func (file *File) Dump() string {
	buffer := ""

	file.ForEach(func(line, value int) {
		buffer += fmt.Sprintf("%d %d\n", line, value)
	})

	return buffer
}

// Validate checks the underlying line interval tree integrity.
// The checks are as follows:
//
// 1. The minimum key must be 0 because the first line index is always 0.
//
// 2. The last node must carry TreeEnd value. This is the maintained invariant
// which marks the ending of the last line interval.
//
// 3. Node keys must monotonically increase and never duplicate.
func (file *File) Validate() {
	if file.tree.Min().Item().Key != 0 {
		panic("the tree must start with key 0")
	}

	if file.tree.Max().Item().Value != TreeEnd {
		panic(fmt.Sprintf("the last value in the tree must be %d", TreeEnd))
	}

	prevKey := uint32(math.MaxUint32)

	for iter := file.tree.Min(); !iter.Limit(); iter = iter.Next() {
		node := iter.Item()

		if node.Key == prevKey {
			panic(fmt.Sprintf("duplicate tree key: %d", node.Key))
		}

		if node.Value == TreeMergeMark {
			panic(fmt.Sprintf("unmerged lines left: %d", node.Key))
		}

		prevKey = node.Key
	}
}

// ForEach visits each node in the underlying tree, in ascending key order.
func (file *File) ForEach(callback func(line, value int)) {
	for iter := file.tree.Min(); !iter.Limit(); iter = iter.Next() {
		item := iter.Item()
		key := int(item.Key)

		var value int

		if item.Value == math.MaxUint32 {
			value = -1
		} else {
			value = int(item.Value)
		}

		callback(key, value)
	}
}

// flatten represents the file as a slice of lines, each line's value being the corresponding day.
func (file *File) flatten() []int {
	lines := make([]int, 0, file.Len())
	val := uint32(math.MaxUint32)

	for iter := file.tree.Min(); !iter.Limit(); iter = iter.Next() {
		for i := uint32(len(lines)); i < iter.Item().Key; i++ { //nolint:gosec // len is bounded by tree keys.
			lines = append(lines, int(val))
		}

		val = iter.Item().Value
	}

	return lines
}
