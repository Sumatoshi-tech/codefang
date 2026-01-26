package rbtree

import (
	"errors"
	"fmt"
	"maps"
	"math"
	"os"
	"sync"

	gitbinary "github.com/go-git/go-git/v6/utils/binary"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
)

// ErrIncompleteRead is returned when a read does not return the expected number of bytes.
var ErrIncompleteRead = errors.New("incomplete read")

// growCapacityNumerator and growCapacityDenominator define the 3/2 growth factor for storage.
const (
	growCapacityNumerator   = 3
	growCapacityDenominator = 2
)

// Public definitions.

// Item is the object stored in each tree node.
type Item struct {
	Key   uint32
	Value uint32
}

// Allocator is the allocator for nodes in a RBTree.
type Allocator struct {
	storage              []node
	gaps                 map[uint32]bool
	hibernatedData       [7][]byte
	HibernationThreshold int
	hibernatedStorageLen int
	hibernatedGapsLen    int
}

// NewAllocator creates a new allocator for RBTree's nodes.
func NewAllocator() *Allocator {
	return &Allocator{
		storage:              []node{},
		gaps:                 map[uint32]bool{},
		hibernatedData:       [7][]byte{},
		HibernationThreshold: 0,
		hibernatedStorageLen: 0,
		hibernatedGapsLen:    0,
	}
}

// Size returns the currently allocated size.
func (allocator *Allocator) Size() int {
	return len(allocator.storage)
}

// Used returns the number of nodes contained in the allocator.
func (allocator *Allocator) Used() int {
	if allocator.storage == nil {
		panic("hibernated allocators cannot be used")
	}

	return len(allocator.storage) - len(allocator.gaps)
}

// Clone copies an existing RBTree allocator.
func (allocator *Allocator) Clone() *Allocator {
	if allocator.storage == nil {
		panic("cannot clone a hibernated allocator")
	}

	newAllocator := &Allocator{
		HibernationThreshold: allocator.HibernationThreshold,
		storage:              make([]node, len(allocator.storage), cap(allocator.storage)),
		gaps:                 map[uint32]bool{},
		hibernatedData:       [7][]byte{},
		hibernatedStorageLen: 0,
		hibernatedGapsLen:    0,
	}
	copy(newAllocator.storage, allocator.storage)
	maps.Copy(newAllocator.gaps, allocator.gaps)

	return newAllocator
}

// Hibernate compresses the allocated memory.
func (allocator *Allocator) Hibernate() {
	if allocator.hibernatedStorageLen > 0 {
		panic("cannot hibernate an already hibernated Allocator")
	}

	if len(allocator.storage) < allocator.HibernationThreshold {
		return
	}

	allocator.hibernatedStorageLen = len(allocator.storage)
	if allocator.hibernatedStorageLen == 0 {
		allocator.storage = nil

		return
	}

	buffers := [6][]uint32{}

	for idx := range buffers {
		buffers[idx] = make([]uint32, len(allocator.storage))
	}

	// We deinterleave to achieve a better compression ratio.
	for idx, nd := range allocator.storage {
		buffers[0][idx] = nd.item.Key
		buffers[1][idx] = nd.item.Value
		buffers[2][idx] = nd.left
		buffers[3][idx] = nd.parent
		buffers[4][idx] = nd.right

		if nd.color {
			buffers[5][idx] = 1
		}
	}

	allocator.storage = nil

	wg := &sync.WaitGroup{}
	wg.Add(len(buffers) + 1)

	for idx, buffer := range buffers {
		go func(bufIdx int, buf []uint32) {
			allocator.hibernatedData[bufIdx] = CompressUInt32Slice(buf)
			buffers[bufIdx] = nil

			wg.Done()
		}(idx, buffer)
	}

	// Compress gaps.
	go func() {
		if len(allocator.gaps) > 0 {
			allocator.hibernatedGapsLen = len(allocator.gaps)

			gapsBuffer := make([]uint32, len(allocator.gaps))
			idx := 0

			for key := range allocator.gaps {
				gapsBuffer[idx] = key
				idx++
			}

			allocator.hibernatedData[len(buffers)] = CompressUInt32Slice(gapsBuffer)
		}

		allocator.gaps = nil

		wg.Done()
	}()

	wg.Wait()
}

// Boot performs the opposite of Hibernate() - decompresses and restores the allocated memory.
func (allocator *Allocator) Boot() {
	if allocator.storage == nil && allocator.hibernatedStorageLen == 0 {
		allocator.storage = []node{}
		allocator.gaps = map[uint32]bool{}

		return
	}

	if allocator.hibernatedStorageLen == 0 {
		// Not hibernated.
		return
	}

	if allocator.hibernatedData[0] == nil {
		panic("cannot boot a serialized Allocator")
	}

	allocator.gaps = map[uint32]bool{}
	buffers := [6][]uint32{}

	wg := &sync.WaitGroup{}
	wg.Add(len(buffers) + 1)

	for idx := range buffers {
		go func(bufIdx int) {
			buffers[bufIdx] = make([]uint32, allocator.hibernatedStorageLen)
			DecompressUInt32Slice(allocator.hibernatedData[bufIdx], buffers[bufIdx])
			allocator.hibernatedData[bufIdx] = nil

			wg.Done()
		}(idx)
	}

	go func() {
		if allocator.hibernatedGapsLen > 0 {
			gapData := allocator.hibernatedData[len(buffers)]
			buffer := make([]uint32, allocator.hibernatedGapsLen)
			DecompressUInt32Slice(gapData, buffer)

			for _, key := range buffer {
				allocator.gaps[key] = true
			}

			allocator.hibernatedData[len(buffers)] = nil
			allocator.hibernatedGapsLen = 0
		}

		wg.Done()
	}()

	wg.Wait()

	capSize := (allocator.hibernatedStorageLen * growCapacityNumerator) / growCapacityDenominator
	allocator.storage = make([]node, allocator.hibernatedStorageLen, capSize)

	for idx := range allocator.storage {
		nd := &allocator.storage[idx]
		nd.item.Key = buffers[0][idx]
		nd.item.Value = buffers[1][idx]
		nd.left = buffers[2][idx]
		nd.parent = buffers[3][idx]
		nd.right = buffers[4][idx]
		nd.color = buffers[5][idx] > 0
	}

	allocator.hibernatedStorageLen = 0
}

// Serialize writes the hibernated allocator on disk.
func (allocator *Allocator) Serialize(path string) error {
	if allocator.storage != nil {
		panic("serialization requires the hibernated state")
	}

	file, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}

	defer file.Close()

	err = gitbinary.WriteVariableWidthInt(file, int64(allocator.hibernatedStorageLen))
	if err != nil {
		return fmt.Errorf("write storage len: %w", err)
	}

	err = gitbinary.WriteVariableWidthInt(file, int64(allocator.hibernatedGapsLen))
	if err != nil {
		return fmt.Errorf("write gaps len: %w", err)
	}

	for idx, hse := range allocator.hibernatedData {
		err = gitbinary.WriteVariableWidthInt(file, int64(len(hse)))
		if err != nil {
			return fmt.Errorf("write data len %d: %w", idx, err)
		}

		_, err = file.Write(hse)
		if err != nil {
			return fmt.Errorf("write data %d: %w", idx, err)
		}

		allocator.hibernatedData[idx] = nil
	}

	return nil
}

// Deserialize reads a hibernated allocator from disk.
func (allocator *Allocator) Deserialize(path string) error {
	if allocator.storage != nil {
		panic("deserialization requires the hibernated state")
	}

	file, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("open file: %w", err)
	}

	defer file.Close()

	storageLen, err := gitbinary.ReadVariableWidthInt(file)
	if err != nil {
		return fmt.Errorf("read storage len: %w", err)
	}

	allocator.hibernatedStorageLen = int(storageLen)

	gapsLen, err := gitbinary.ReadVariableWidthInt(file)
	if err != nil {
		return fmt.Errorf("read gaps len: %w", err)
	}

	allocator.hibernatedGapsLen = int(gapsLen)

	for idx := range allocator.hibernatedData {
		dataLen, readErr := gitbinary.ReadVariableWidthInt(file)
		if readErr != nil {
			return fmt.Errorf("read data len %d: %w", idx, readErr)
		}

		allocator.hibernatedData[idx] = make([]byte, int(dataLen))

		bytesRead, readErr := file.Read(allocator.hibernatedData[idx])
		if readErr != nil {
			return fmt.Errorf("read data %d: %w", idx, readErr)
		}

		if bytesRead != int(dataLen) {
			return fmt.Errorf("%w %d: %d instead of %d", ErrIncompleteRead, idx, bytesRead, int(dataLen))
		}
	}

	return nil
}

func (allocator *Allocator) malloc() uint32 {
	if allocator.storage == nil {
		panic("hibernated allocators cannot be used")
	}

	if len(allocator.gaps) > 0 {
		var key uint32

		for key = range allocator.gaps {
			break
		}

		delete(allocator.gaps, key)

		return key
	}

	nodeLen := len(allocator.storage)
	if nodeLen == 0 {
		// Zero is reserved.
		allocator.storage = append(allocator.storage, node{
			item: Item{Key: 0, Value: 0}, parent: 0, left: 0, right: 0, color: false,
		})
		nodeLen = 1
	}

	if nodeLen == negativeLimitNode-1 {
		// [math.MaxUint32] is reserved.
		panic("the size of my RBTree allocator has reached the maximum value for uint32, sorry")
	}

	doAssert(nodeLen < negativeLimitNode)

	allocator.storage = append(allocator.storage, node{
		item: Item{Key: 0, Value: 0}, parent: 0, left: 0, right: 0, color: false,
	})

	return safeconv.MustIntToUint32(nodeLen)
}

func (allocator *Allocator) free(nodeIdx uint32) {
	if allocator.storage == nil {
		panic("hibernated allocators cannot be used")
	}

	if nodeIdx == 0 {
		panic("node #0 is special and cannot be deallocated")
	}

	_, exists := allocator.gaps[nodeIdx]
	doAssert(!exists)

	allocator.storage[nodeIdx] = node{
		item: Item{Key: 0, Value: 0}, parent: 0, left: 0, right: 0, color: false,
	}

	allocator.gaps[nodeIdx] = true
}

// RBTree is a red-black tree with an API similar to C++ STL's.
//
// The implementation is inspired (read: stolen) from:
// http://en.literateprograms.org/Red-black_tree_(C)#chunk use:private function prototypes.
//
// The code was optimized for the simple integer types of Key and Value.
// The code was further optimized for using allocators.
// Credits: Yaz Saito.
type RBTree struct {
	// Nodes allocator.
	allocator *Allocator

	// Root of the tree.
	root uint32

	// The minimum and maximum nodes under the tree.
	minNode, maxNode uint32

	// Number of nodes under root, including the root.
	count int32
}

// NewRBTree creates a new red-black binary tree.
func NewRBTree(allocator *Allocator) *RBTree {
	return &RBTree{allocator: allocator, root: 0, minNode: 0, maxNode: 0, count: 0}
}

func (tree *RBTree) storage() []node {
	return tree.allocator.storage
}

// Allocator returns the bound nodes allocator.
func (tree *RBTree) Allocator() *Allocator {
	return tree.allocator
}

// Len returns the number of elements in the tree.
func (tree *RBTree) Len() int {
	return int(tree.count)
}

// CloneShallow performs a shallow copy of the tree - the nodes are assumed to already exist in the allocator.
func (tree *RBTree) CloneShallow(allocator *Allocator) *RBTree {
	clone := *tree
	clone.allocator = allocator

	return &clone
}

// CloneDeep performs a deep copy of the tree - the nodes are created from scratch.
func (tree *RBTree) CloneDeep(allocator *Allocator) *RBTree {
	clone := &RBTree{
		count:     tree.count,
		allocator: allocator,
		root:      0,
		minNode:   0,
		maxNode:   0,
	}

	nodeMap := map[uint32]uint32{}
	originStorage := tree.storage()

	for iter := tree.Min(); !iter.Limit(); iter = iter.Next() {
		newNode := allocator.malloc()
		cloneNode := &allocator.storage[newNode]
		cloneNode.item = *iter.Item()
		cloneNode.color = originStorage[iter.node].color
		nodeMap[iter.node] = newNode
	}

	cloneStorage := allocator.storage

	for iter := tree.Min(); !iter.Limit(); iter = iter.Next() {
		cloneNode := &cloneStorage[nodeMap[iter.node]]
		originNode := originStorage[iter.node]
		cloneNode.left = nodeMap[originNode.left]
		cloneNode.right = nodeMap[originNode.right]
		cloneNode.parent = nodeMap[originNode.parent]
	}

	clone.root = nodeMap[tree.root]
	clone.minNode = nodeMap[tree.minNode]
	clone.maxNode = nodeMap[tree.maxNode]

	return clone
}

// Erase removes all the nodes from the tree.
func (tree *RBTree) Erase() {
	nodes := make([]uint32, 0, tree.count)

	for iter := tree.Min(); !iter.Limit(); iter = iter.Next() {
		nodes = append(nodes, iter.node)
	}

	for _, nd := range nodes {
		tree.allocator.free(nd)
	}

	tree.root = 0
	tree.minNode = 0
	tree.maxNode = 0
	tree.count = 0
}

// Get is a convenience function for finding an element equal to Key. Returns
// nil if not found.
func (tree *RBTree) Get(key uint32) *uint32 {
	nodeIdx, exact := tree.findGE(key)
	if exact {
		return &tree.storage()[nodeIdx].item.Value
	}

	return nil
}

// Min creates an iterator that points to the minimum item in the tree.
// If the tree is empty, returns Limit().
func (tree *RBTree) Min() Iterator {
	return Iterator{tree, tree.minNode}
}

// Max creates an iterator that points at the maximum item in the tree.
//
// If the tree is empty, returns NegativeLimit().
func (tree *RBTree) Max() Iterator {
	if tree.maxNode == 0 {
		return Iterator{tree, negativeLimitNode}
	}

	return Iterator{tree, tree.maxNode}
}

// Limit creates an iterator that points beyond the maximum item in the tree.
func (tree *RBTree) Limit() Iterator {
	return Iterator{tree, 0}
}

// NegativeLimit creates an iterator that points before the minimum item in the tree.
func (tree *RBTree) NegativeLimit() Iterator {
	return Iterator{tree, negativeLimitNode}
}

// FindGE finds the smallest element N such that N >= Key, and returns the
// iterator pointing to the element. If no such element is found,
// returns tree.Limit().
func (tree *RBTree) FindGE(key uint32) Iterator {
	nodeIdx, _ := tree.findGE(key)

	return Iterator{tree, nodeIdx}
}

// FindLE finds the largest element N such that N <= Key, and returns the
// iterator pointing to the element. If no such element is found,
// returns iter.NegativeLimit().
func (tree *RBTree) FindLE(key uint32) Iterator {
	nodeIdx, exact := tree.findGE(key)
	if exact {
		return Iterator{tree, nodeIdx}
	}

	if nodeIdx != 0 {
		return Iterator{tree, doPrev(nodeIdx, tree.storage())}
	}

	if tree.maxNode == 0 {
		return Iterator{tree, negativeLimitNode}
	}

	return Iterator{tree, tree.maxNode}
}

// Insert an item. If the item is already in the tree, do nothing and
// return false. Else return true.
//
//nolint:gocognit // RB-tree insertion with rebalancing is inherently complex.
func (tree *RBTree) Insert(item Item) (bool, Iterator) {
	// Delay creating n until it is found to be inserted.
	nodeIdx := tree.doInsert(item)
	if nodeIdx == 0 {
		return false, Iterator{}
	}

	alloc := tree.storage()
	insN := nodeIdx

	alloc[nodeIdx].color = red

	for {
		// Case 1: N is at the root.
		if alloc[nodeIdx].parent == 0 {
			alloc[nodeIdx].color = black

			break
		}

		// Case 2: The parent is black, so the tree already
		// satisfies the RB properties.
		if alloc[alloc[nodeIdx].parent].color {
			break
		}

		// Case 3: parent and uncle are both red.
		// Then paint both black and make grandparent red.
		grandparent := alloc[alloc[nodeIdx].parent].parent

		var uncle uint32
		if isLeftChild(alloc[nodeIdx].parent, alloc) {
			uncle = alloc[grandparent].right
		} else {
			uncle = alloc[grandparent].left
		}

		if uncle != 0 && !alloc[uncle].color {
			alloc[alloc[nodeIdx].parent].color = black
			alloc[uncle].color = black
			alloc[grandparent].color = red
			nodeIdx = grandparent

			continue
		}

		// Case 4: parent is red, uncle is black (1).
		if isRightChild(nodeIdx, alloc) && isLeftChild(alloc[nodeIdx].parent, alloc) {
			tree.rotateLeft(alloc[nodeIdx].parent)
			nodeIdx = alloc[nodeIdx].left

			continue
		}

		if isLeftChild(nodeIdx, alloc) && isRightChild(alloc[nodeIdx].parent, alloc) {
			tree.rotateRight(alloc[nodeIdx].parent)
			nodeIdx = alloc[nodeIdx].right

			continue
		}

		// Case 5: parent is red, uncle is black (2).
		alloc[alloc[nodeIdx].parent].color = black
		alloc[grandparent].color = red

		if isLeftChild(nodeIdx, alloc) {
			tree.rotateRight(grandparent)
		} else {
			tree.rotateLeft(grandparent)
		}

		break
	}

	return true, Iterator{tree, insN}
}

// DeleteWithKey deletes an item with the given Key. Returns true iff the item was
// found.
func (tree *RBTree) DeleteWithKey(key uint32) bool {
	nodeIdx, exact := tree.findGE(key)
	if exact {
		tree.doDelete(nodeIdx)

		return true
	}

	return false
}

// DeleteWithIterator deletes the current item.
//
// REQUIRES: !iter.Limit() && !iter.NegativeLimit().
func (tree *RBTree) DeleteWithIterator(iter Iterator) {
	doAssert(!iter.Limit() && !iter.NegativeLimit())
	tree.doDelete(iter.node)
}

// Iterator allows scanning tree elements in sort order.
//
// Iterator invalidation rule is the same as C++ std::map<>'s. That
// is, if you delete the element that an iterator points to, the
// iterator becomes invalid. For other operation types, the iterator
// remains valid.
type Iterator struct {
	tree *RBTree
	node uint32
}

// Equal checks for the underlying nodes equality.
func (iter Iterator) Equal(other Iterator) bool {
	return iter.node == other.node
}

// Limit checks if the iterator points beyond the max element in the tree.
func (iter Iterator) Limit() bool {
	return iter.node == 0
}

// Min checks if the iterator points to the minimum element in the tree.
func (iter Iterator) Min() bool {
	return iter.node == iter.tree.minNode
}

// Max checks if the iterator points to the maximum element in the tree.
func (iter Iterator) Max() bool {
	return iter.node == iter.tree.maxNode
}

// NegativeLimit checks if the iterator points before the minimum element in the tree.
func (iter Iterator) NegativeLimit() bool {
	return iter.node == negativeLimitNode
}

// Item returns the current element. Allows mutating the node
// (key to be changed with care!).
//
// The result is nil if iter.Limit() || iter.NegativeLimit().
func (iter Iterator) Item() *Item {
	if iter.Limit() || iter.NegativeLimit() {
		return nil
	}

	return &iter.tree.storage()[iter.node].item
}

// Next creates a new iterator that points to the successor of the current element.
//
// REQUIRES: !iter.Limit().
func (iter Iterator) Next() Iterator {
	doAssert(!iter.Limit())

	if iter.NegativeLimit() {
		return Iterator{iter.tree, iter.tree.minNode}
	}

	return Iterator{iter.tree, doNext(iter.node, iter.tree.storage())}
}

// Prev creates a new iterator that points to the predecessor of the current
// node.
//
// REQUIRES: !iter.NegativeLimit().
func (iter Iterator) Prev() Iterator {
	doAssert(!iter.NegativeLimit())

	if !iter.Limit() {
		return Iterator{iter.tree, doPrev(iter.node, iter.tree.storage())}
	}

	if iter.tree.maxNode == 0 {
		return Iterator{iter.tree, negativeLimitNode}
	}

	return Iterator{iter.tree, iter.tree.maxNode}
}

func doAssert(condition bool) {
	if !condition {
		panic("rbtree internal assertion failed")
	}
}

const (
	red               = false
	black             = true
	negativeLimitNode = math.MaxUint32
)

type node struct {
	item                Item
	parent, left, right uint32
	color               bool // Black or red.
}

// Internal node attribute accessors.
func getColor(nodeIdx uint32, allocator []node) bool {
	if nodeIdx == 0 {
		return black
	}

	return allocator[nodeIdx].color
}

func isLeftChild(nodeIdx uint32, allocator []node) bool {
	return nodeIdx == allocator[allocator[nodeIdx].parent].left
}

func isRightChild(nodeIdx uint32, allocator []node) bool {
	return nodeIdx == allocator[allocator[nodeIdx].parent].right
}

func sibling(nodeIdx uint32, allocator []node) uint32 {
	doAssert(allocator[nodeIdx].parent != 0)

	if isLeftChild(nodeIdx, allocator) {
		return allocator[allocator[nodeIdx].parent].right
	}

	return allocator[allocator[nodeIdx].parent].left
}

// Return the minimum node that's larger than N. Return nil if no such
// node is found.
func doNext(nodeIdx uint32, allocator []node) uint32 {
	if allocator[nodeIdx].right != 0 {
		cursor := allocator[nodeIdx].right

		for allocator[cursor].left != 0 {
			cursor = allocator[cursor].left
		}

		return cursor
	}

	for nodeIdx != 0 {
		parentIdx := allocator[nodeIdx].parent
		if parentIdx == 0 {
			return 0
		}

		if isLeftChild(nodeIdx, allocator) {
			return parentIdx
		}

		nodeIdx = parentIdx
	}

	return 0
}

// Return the maximum node that's smaller than N. Return nil if no
// such node is found.
func doPrev(nodeIdx uint32, allocator []node) uint32 {
	if allocator[nodeIdx].left != 0 {
		return maxPredecessor(nodeIdx, allocator)
	}

	for nodeIdx != 0 {
		parentIdx := allocator[nodeIdx].parent
		if parentIdx == 0 {
			break
		}

		if isRightChild(nodeIdx, allocator) {
			return parentIdx
		}

		nodeIdx = parentIdx
	}

	return negativeLimitNode
}

// Return the predecessor of "n".
func maxPredecessor(nodeIdx uint32, allocator []node) uint32 {
	doAssert(allocator[nodeIdx].left != 0)

	cursor := allocator[nodeIdx].left

	for allocator[cursor].right != 0 {
		cursor = allocator[cursor].right
	}

	return cursor
}

// Tree methods.

// Private methods.

func (tree *RBTree) recomputeMinNode() {
	alloc := tree.storage()
	tree.minNode = tree.root

	if tree.minNode != 0 {
		for alloc[tree.minNode].left != 0 {
			tree.minNode = alloc[tree.minNode].left
		}
	}
}

func (tree *RBTree) recomputeMaxNode() {
	alloc := tree.storage()
	tree.maxNode = tree.root

	if tree.maxNode != 0 {
		for alloc[tree.maxNode].right != 0 {
			tree.maxNode = alloc[tree.maxNode].right
		}
	}
}

func (tree *RBTree) maybeSetMinNode(nodeIdx uint32) {
	alloc := tree.storage()

	if tree.minNode == 0 {
		tree.minNode = nodeIdx
		tree.maxNode = nodeIdx
	} else if alloc[nodeIdx].item.Key < alloc[tree.minNode].item.Key {
		tree.minNode = nodeIdx
	}
}

func (tree *RBTree) maybeSetMaxNode(nodeIdx uint32) {
	alloc := tree.storage()

	if tree.maxNode == 0 {
		tree.minNode = nodeIdx
		tree.maxNode = nodeIdx
	} else if alloc[nodeIdx].item.Key > alloc[tree.maxNode].item.Key {
		tree.maxNode = nodeIdx
	}
}

// Try inserting "item" into the tree. Return nil if the item is
// already in the tree. Otherwise return a new (leaf) node.
func (tree *RBTree) doInsert(item Item) uint32 {
	if tree.root == 0 {
		nodeIdx := tree.allocator.malloc()
		tree.storage()[nodeIdx].item = item
		tree.root = nodeIdx
		tree.minNode = nodeIdx
		tree.maxNode = nodeIdx
		tree.count++

		return nodeIdx
	}

	parent := tree.root
	storageSlice := tree.storage()

	for {
		parentNode := storageSlice[parent]
		comp := int(item.Key) - int(parentNode.item.Key)

		switch {
		case comp == 0:
			return 0
		case comp < 0:
			if parentNode.left == 0 {
				nodeIdx := tree.allocator.malloc()
				storageSlice = tree.storage()
				newNode := &storageSlice[nodeIdx]
				newNode.item = item
				newNode.parent = parent
				storageSlice[parent].left = nodeIdx
				tree.count++
				tree.maybeSetMinNode(nodeIdx)

				return nodeIdx
			}

			parent = parentNode.left
		default:
			if parentNode.right == 0 {
				nodeIdx := tree.allocator.malloc()
				storageSlice = tree.storage()
				newNode := &storageSlice[nodeIdx]
				newNode.item = item
				newNode.parent = parent
				storageSlice[parent].right = nodeIdx
				tree.count++
				tree.maybeSetMaxNode(nodeIdx)

				return nodeIdx
			}

			parent = parentNode.right
		}
	}
}

// Find a node whose item >= Key. The 2nd return Value is true iff the
// node.item==Key. Returns (nil, false) if all nodes in the tree are <
// Key.
func (tree *RBTree) findGE(key uint32) (uint32, bool) { //nolint:revive // intentional private/public pair
	alloc := tree.storage()
	nodeIdx := tree.root

	for {
		if nodeIdx == 0 {
			return 0, false
		}

		comp := int(key) - int(alloc[nodeIdx].item.Key)

		switch {
		case comp == 0:
			return nodeIdx, true
		case comp < 0:
			if alloc[nodeIdx].left == 0 {
				return nodeIdx, false
			}

			nodeIdx = alloc[nodeIdx].left
		default:
			if alloc[nodeIdx].right == 0 {
				succ := doNext(nodeIdx, alloc)
				if succ == 0 {
					return 0, false
				}

				return succ, key == alloc[succ].item.Key
			}

			nodeIdx = alloc[nodeIdx].right
		}
	}
}

// Delete N from the tree.
func (tree *RBTree) doDelete(nodeIdx uint32) {
	alloc := tree.storage()

	if alloc[nodeIdx].left != 0 && alloc[nodeIdx].right != 0 {
		pred := maxPredecessor(nodeIdx, alloc)
		tree.swapNodes(nodeIdx, pred)
	}

	doAssert(alloc[nodeIdx].left == 0 || alloc[nodeIdx].right == 0)

	child := alloc[nodeIdx].right
	if child == 0 {
		child = alloc[nodeIdx].left
	}

	if alloc[nodeIdx].color {
		alloc[nodeIdx].color = getColor(child, alloc)
		tree.deleteCase1(nodeIdx)
	}

	tree.replaceNode(nodeIdx, child)

	if alloc[nodeIdx].parent == 0 && child != 0 {
		alloc[child].color = black
	}

	tree.allocator.free(nodeIdx)
	tree.count--

	if tree.count == 0 {
		tree.minNode = 0
		tree.maxNode = 0
	} else {
		if tree.minNode == nodeIdx {
			tree.recomputeMinNode()
		}

		if tree.maxNode == nodeIdx {
			tree.recomputeMaxNode()
		}
	}
}

// Move n to the pred's place, and vice versa.
//
//nolint:gocognit,nestif // RB-tree node swapping is inherently complex with many pointer adjustments.
func (tree *RBTree) swapNodes(nodeIdx, pred uint32) {
	doAssert(pred != nodeIdx)

	alloc := tree.storage()
	isLeft := isLeftChild(pred, alloc)
	tmp := alloc[pred]

	tree.replaceNode(nodeIdx, pred)
	alloc[pred].color = alloc[nodeIdx].color

	if tmp.parent == nodeIdx {
		// Swap the positions of nodeIdx and pred.
		if isLeft {
			alloc[pred].left = nodeIdx
			alloc[pred].right = alloc[nodeIdx].right

			if alloc[pred].right != 0 {
				alloc[alloc[pred].right].parent = pred
			}
		} else {
			alloc[pred].left = alloc[nodeIdx].left

			if alloc[pred].left != 0 {
				alloc[alloc[pred].left].parent = pred
			}

			alloc[pred].right = nodeIdx
		}

		alloc[nodeIdx].item = tmp.item
		alloc[nodeIdx].parent = pred

		alloc[nodeIdx].left = tmp.left
		if alloc[nodeIdx].left != 0 {
			alloc[alloc[nodeIdx].left].parent = nodeIdx
		}

		alloc[nodeIdx].right = tmp.right
		if alloc[nodeIdx].right != 0 {
			alloc[alloc[nodeIdx].right].parent = nodeIdx
		}
	} else {
		alloc[pred].left = alloc[nodeIdx].left

		if alloc[pred].left != 0 {
			alloc[alloc[pred].left].parent = pred
		}

		alloc[pred].right = alloc[nodeIdx].right

		if alloc[pred].right != 0 {
			alloc[alloc[pred].right].parent = pred
		}

		if isLeft {
			alloc[tmp.parent].left = nodeIdx
		} else {
			alloc[tmp.parent].right = nodeIdx
		}

		alloc[nodeIdx].item = tmp.item
		alloc[nodeIdx].parent = tmp.parent
		alloc[nodeIdx].left = tmp.left

		if alloc[nodeIdx].left != 0 {
			alloc[alloc[nodeIdx].left].parent = nodeIdx
		}

		alloc[nodeIdx].right = tmp.right

		if alloc[nodeIdx].right != 0 {
			alloc[alloc[nodeIdx].right].parent = nodeIdx
		}
	}

	alloc[nodeIdx].color = tmp.color
}

func (tree *RBTree) deleteCase1(nodeIdx uint32) {
	alloc := tree.storage()

	for alloc[nodeIdx].parent != 0 {
		if !getColor(sibling(nodeIdx, alloc), alloc) {
			alloc[alloc[nodeIdx].parent].color = red
			alloc[sibling(nodeIdx, alloc)].color = black

			if nodeIdx == alloc[alloc[nodeIdx].parent].left {
				tree.rotateLeft(alloc[nodeIdx].parent)
			} else {
				tree.rotateRight(alloc[nodeIdx].parent)
			}
		}

		if getColor(alloc[nodeIdx].parent, alloc) &&
			getColor(sibling(nodeIdx, alloc), alloc) &&
			getColor(alloc[sibling(nodeIdx, alloc)].left, alloc) &&
			getColor(alloc[sibling(nodeIdx, alloc)].right, alloc) { //nolint:whitespace // conflicts with wsl_v5 leading-whitespace.
			alloc[sibling(nodeIdx, alloc)].color = red
			nodeIdx = alloc[nodeIdx].parent

			continue
		}

		// Case 4.
		if !getColor(alloc[nodeIdx].parent, alloc) &&
			getColor(sibling(nodeIdx, alloc), alloc) &&
			getColor(alloc[sibling(nodeIdx, alloc)].left, alloc) &&
			getColor(alloc[sibling(nodeIdx, alloc)].right, alloc) { //nolint:whitespace // conflicts with wsl_v5 leading-whitespace.
			alloc[sibling(nodeIdx, alloc)].color = red
			alloc[alloc[nodeIdx].parent].color = black
		} else {
			tree.deleteCase5(nodeIdx)
		}

		break
	}
}

func (tree *RBTree) deleteCase5(nodeIdx uint32) {
	alloc := tree.storage()

	if nodeIdx == alloc[alloc[nodeIdx].parent].left &&
		getColor(sibling(nodeIdx, alloc), alloc) &&
		!getColor(alloc[sibling(nodeIdx, alloc)].left, alloc) &&
		getColor(alloc[sibling(nodeIdx, alloc)].right, alloc) { //nolint:whitespace // conflicts with wsl_v5 leading-whitespace.
		alloc[sibling(nodeIdx, alloc)].color = red
		alloc[alloc[sibling(nodeIdx, alloc)].left].color = black
		tree.rotateRight(sibling(nodeIdx, alloc))
	} else if nodeIdx == alloc[alloc[nodeIdx].parent].right &&
		getColor(sibling(nodeIdx, alloc), alloc) &&
		!getColor(alloc[sibling(nodeIdx, alloc)].right, alloc) &&
		getColor(alloc[sibling(nodeIdx, alloc)].left, alloc) { //nolint:whitespace // conflicts with wsl_v5 leading-whitespace.
		alloc[sibling(nodeIdx, alloc)].color = red
		alloc[alloc[sibling(nodeIdx, alloc)].right].color = black
		tree.rotateLeft(sibling(nodeIdx, alloc))
	}

	// Case 6.
	alloc[sibling(nodeIdx, alloc)].color = getColor(alloc[nodeIdx].parent, alloc)
	alloc[alloc[nodeIdx].parent].color = black

	if nodeIdx == alloc[alloc[nodeIdx].parent].left {
		doAssert(!getColor(alloc[sibling(nodeIdx, alloc)].right, alloc))
		alloc[alloc[sibling(nodeIdx, alloc)].right].color = black
		tree.rotateLeft(alloc[nodeIdx].parent)
	} else {
		doAssert(!getColor(alloc[sibling(nodeIdx, alloc)].left, alloc))
		alloc[alloc[sibling(nodeIdx, alloc)].left].color = black
		tree.rotateRight(alloc[nodeIdx].parent)
	}
}

func (tree *RBTree) replaceNode(oldn, newn uint32) {
	alloc := tree.storage()

	if alloc[oldn].parent == 0 {
		tree.root = newn
	} else {
		if oldn == alloc[alloc[oldn].parent].left {
			alloc[alloc[oldn].parent].left = newn
		} else {
			alloc[alloc[oldn].parent].right = newn
		}
	}

	if newn != 0 {
		alloc[newn].parent = alloc[oldn].parent
	}
}

// rotateDirection performs a tree rotation in the specified direction.
// IsLeft=true performs left rotation, isLeft=false performs right rotation.
//
// Left rotation:
//
//	  X              Y
//	A   Y    =>    X   C
//	  B C        A B
//
// Right rotation:
//
//	    Y            X
//	  X   C  =>    A   Y
//	A B              B C
//
//nolint:dupword // ASCII art diagrams contain intentional repeated letters.
func (tree *RBTree) rotateDirection(pivot uint32, isLeft bool) {
	alloc := tree.storage()

	// Get the child in the opposite direction of rotation.
	var child uint32
	if isLeft {
		child = alloc[pivot].right
	} else {
		child = alloc[pivot].left
	}

	// Move the inner subtree.
	var innerSubtree uint32
	if isLeft {
		innerSubtree = alloc[child].left
		alloc[pivot].right = innerSubtree
	} else {
		innerSubtree = alloc[child].right
		alloc[pivot].left = innerSubtree
	}

	if innerSubtree != 0 {
		alloc[innerSubtree].parent = pivot
	}

	// Update parent links.
	alloc[child].parent = alloc[pivot].parent

	if alloc[pivot].parent == 0 {
		tree.root = child
	} else {
		if isLeftChild(pivot, alloc) {
			alloc[alloc[pivot].parent].left = child
		} else {
			alloc[alloc[pivot].parent].right = child
		}
	}

	// Complete the rotation.
	if isLeft {
		alloc[child].left = pivot
	} else {
		alloc[child].right = pivot
	}

	alloc[pivot].parent = child
}

func (tree *RBTree) rotateLeft(nodeIdx uint32) {
	tree.rotateDirection(nodeIdx, true)
}

func (tree *RBTree) rotateRight(nodeIdx uint32) {
	tree.rotateDirection(nodeIdx, false)
}
