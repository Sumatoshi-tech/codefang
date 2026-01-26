package rbtree //nolint:testpackage // tests require access to unexported fields (storage, gaps, minNode, etc.)

import (
	"math/rand"
	"os"
	"slices"
	"sort"
	"strconv"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Create a tree storing a set of integers.
func testNewIntSet() *RBTree {
	return NewRBTree(NewAllocator())
}

func testAssert(tb testing.TB, condition bool, message string) {
	tb.Helper()
	assert.True(tb, condition, message)
}

func boolInsert(tree *RBTree, item int) bool {
	status, _ := tree.Insert(Item{uint32(item), uint32(item)})

	return status
}

func TestEmpty(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	testAssert(t, tree.Len() == 0, "len!=0")
	testAssert(t, tree.Max().NegativeLimit(), "neglimit")
	testAssert(t, tree.Min().Limit(), "limit")
	testAssert(t, tree.FindGE(10).Limit(), "Not empty")
	testAssert(t, tree.FindLE(10).NegativeLimit(), "Not empty")
	testAssert(t, tree.Get(10) == nil, "Not empty")
	testAssert(t, tree.Limit().Equal(tree.Min()), "iter")
}

func TestFindGE(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	testAssert(t, boolInsert(tree, 10), "Insert1")
	testAssert(t, !boolInsert(tree, 10), "Insert2")
	testAssert(t, tree.Len() == 1, "len==1")
	testAssert(t, tree.FindGE(10).Item().Key == 10, "FindGE 10")
	testAssert(t, tree.FindGE(11).Limit(), "FindGE 11")
	assert.Equal(t, uint32(10), tree.FindGE(9).Item().Key, "FindGE 10")
}

func TestFindLE(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	testAssert(t, boolInsert(tree, 10), "insert1")
	testAssert(t, tree.FindLE(10).Item().Key == 10, "FindLE 10")
	testAssert(t, tree.FindLE(11).Item().Key == 10, "FindLE 11")
	testAssert(t, tree.FindLE(9).NegativeLimit(), "FindLE 9")
}

func TestGet(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	testAssert(t, boolInsert(tree, 10), "insert1")
	assert.Equal(t, uint32(10), *tree.Get(10), "Get 10")
	testAssert(t, tree.Get(9) == nil, "Get 9")
	testAssert(t, tree.Get(11) == nil, "Get 11")
}

func TestDelete(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	testAssert(t, !tree.DeleteWithKey(10), "del")
	testAssert(t, tree.Len() == 0, "dellen")
	testAssert(t, boolInsert(tree, 10), "ins")
	testAssert(t, tree.DeleteWithKey(10), "del")
	testAssert(t, tree.Len() == 0, "dellen")

	// Delete was deleting after the request if request not found.
	// Ensure this does not regress.
	testAssert(t, boolInsert(tree, 10), "ins")
	testAssert(t, !tree.DeleteWithKey(9), "del")
	testAssert(t, tree.Len() == 1, "dellen")
}

func iterToString(iter Iterator) string {
	result := ""

	for ; !iter.Limit(); iter = iter.Next() {
		if result != "" {
			result += ","
		}

		result += strconv.FormatUint(uint64(iter.Item().Key), 10)
	}

	return result
}

func reverseIterToString(iter Iterator) string {
	result := ""

	for ; !iter.NegativeLimit(); iter = iter.Prev() {
		if result != "" {
			result += ","
		}

		result += strconv.FormatUint(uint64(iter.Item().Key), 10)
	}

	return result
}

func TestIterator(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()

	for idx := 0; idx < 10; idx += 2 {
		boolInsert(tree, idx)
	}

	assert.Equal(t, "4,6,8", iterToString(tree.FindGE(3)))
	assert.Equal(t, "4,6,8", iterToString(tree.FindGE(4)))
	assert.Equal(t, "8", iterToString(tree.FindGE(8)))
	assert.Empty(t, iterToString(tree.FindGE(9)))
	assert.Equal(t, "2,0", reverseIterToString(tree.FindLE(3)))
	assert.Equal(t, "2,0", reverseIterToString(tree.FindLE(2)))
	assert.Equal(t, "0", reverseIterToString(tree.FindLE(0)))
}

// Randomized tests.

// oracle provides an interface similar to rbtree, but stores
// data in a sorted array.
type oracle struct {
	data []int
}

func newOracle() *oracle {
	return &oracle{data: make([]int, 0)}
}

func (o *oracle) Len() int {
	return len(o.data)
}

// Interface needed for sorting.
func (o *oracle) Less(idx1, idx2 int) bool {
	return o.data[idx1] < o.data[idx2]
}

func (o *oracle) Swap(idx1, idx2 int) {
	o.data[idx2], o.data[idx1] = o.data[idx1], o.data[idx2]
}

func (o *oracle) Insert(key int) bool {
	if slices.Contains(o.data, key) {
		return false
	}

	dataLen := len(o.data) + 1
	newData := make([]int, dataLen)
	copy(newData, o.data)
	newData[dataLen-1] = key
	o.data = newData
	sort.Sort(o)

	return true
}

func (o *oracle) RandomExistingKey(rng *rand.Rand) int {
	index := rng.Int31n(int32(len(o.data)))

	return o.data[index]
}

func (o *oracle) FindGE(tb testing.TB, key int) oracleIterator {
	tb.Helper()

	prev := int(-1)

	for idx, elem := range o.data {
		if elem <= prev {
			tb.Fatal("Nonsorted oracle ", elem, prev)
		}

		if elem >= key {
			return oracleIterator{o: o, index: idx}
		}
	}

	return oracleIterator{o: o, index: len(o.data)}
}

func (o *oracle) FindLE(tb testing.TB, key int) oracleIterator {
	tb.Helper()

	iter := o.FindGE(tb, key)

	if !iter.Limit() && o.data[iter.index] == key {
		return iter
	}

	return oracleIterator{o, iter.index - 1}
}

func (o *oracle) Delete(key int) bool {
	for idx, elem := range o.data {
		if elem != key {
			continue
		}

		newData := make([]int, len(o.data)-1)
		copy(newData, o.data[0:idx])
		copy(newData[idx:], o.data[idx+1:])
		o.data = newData

		return true
	}

	return false
}

// Test iterator.
type oracleIterator struct {
	o     *oracle
	index int
}

func (oiter oracleIterator) Limit() bool {
	return oiter.index >= len(oiter.o.data)
}

func (oiter oracleIterator) Min() bool {
	return oiter.index == 0
}

func (oiter oracleIterator) NegativeLimit() bool {
	return oiter.index < 0
}

func (oiter oracleIterator) Max() bool {
	return oiter.index == len(oiter.o.data)-1
}

func (oiter oracleIterator) Item() int {
	return oiter.o.data[oiter.index]
}

func (oiter oracleIterator) Next() oracleIterator {
	return oracleIterator{oiter.o, oiter.index + 1}
}

func (oiter oracleIterator) Prev() oracleIterator {
	return oracleIterator{oiter.o, oiter.index - 1}
}

func compareContents(tb testing.TB, oiter oracleIterator, titer Iterator) {
	tb.Helper()

	oi := oiter
	ti := titer

	// Test forward iteration.
	testAssert(tb, oi.NegativeLimit() == ti.NegativeLimit(), "rend")

	if oi.NegativeLimit() {
		oi = oi.Next()
		ti = ti.Next()
	}

	for !oi.Limit() && !ti.Limit() {
		if ti.Item().Key != uint32(oi.Item()) {
			tb.Fatal("Wrong item", ti.Item(), oi.Item())
		}

		oi = oi.Next()
		ti = ti.Next()
	}

	if !ti.Limit() {
		tb.Fatal("!ti.done", ti.Item())
	}

	if !oi.Limit() {
		tb.Fatal("!oi.done", oi.Item())
	}

	// Test reverse iteration.
	oi = oiter
	ti = titer

	testAssert(tb, oi.Limit() == ti.Limit(), "end")

	if oi.Limit() {
		oi = oi.Prev()
		ti = ti.Prev()
	}

	for !oi.NegativeLimit() && !ti.NegativeLimit() {
		if ti.Item().Key != uint32(oi.Item()) {
			tb.Fatal("Wrong item", ti.Item(), oi.Item())
		}

		oi = oi.Prev()
		ti = ti.Prev()
	}

	if !ti.NegativeLimit() {
		tb.Fatal("!ti.done", ti.Item())
	}

	if !oi.NegativeLimit() {
		tb.Fatal("!oi.done", oi.Item())
	}
}

func compareContentsFull(tb testing.TB, orc *oracle, tree *RBTree) {
	tb.Helper()
	compareContents(tb, orc.FindGE(tb, -1), tree.FindGE(0))
}

func TestRandomized(t *testing.T) {
	t.Parallel()

	const numKeys = 1000

	orc := newOracle()
	tree := testNewIntSet()
	rng := rand.New(rand.NewSource(0))

	for range 10000 {
		op := rng.Int31n(100)

		switch {
		case op < 50:
			key := rng.Int31n(numKeys)
			orc.Insert(int(key))
			boolInsert(tree, int(key))
			compareContentsFull(t, orc, tree)
		case op < 90 && orc.Len() > 0:
			key := orc.RandomExistingKey(rng)
			orc.Delete(key)

			if !tree.DeleteWithKey(uint32(key)) {
				t.Fatal("DeleteExisting", key)
			}

			compareContentsFull(t, orc, tree)
		case op < 95:
			key := int(rng.Int31n(numKeys))
			compareContents(t, orc.FindGE(t, key), tree.FindGE(uint32(key)))
		default:
			key := int(rng.Int31n(numKeys))
			compareContents(t, orc.FindLE(t, key), tree.FindLE(uint32(key)))
		}
	}
}

func TestAllocatorFreeZero(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()
	alloc.malloc()
	assert.Panics(t, func() { alloc.free(0) })
}

func TestCloneShallow(t *testing.T) {
	t.Parallel()

	alloc1 := NewAllocator()
	alloc1.malloc()

	tree := NewRBTree(alloc1)
	tree.Insert(Item{7, 7})
	tree.Insert(Item{8, 8})
	tree.DeleteWithKey(8)

	assert.Equal(t, []node{{}, {}, {color: black, item: Item{7, 7}}, {}}, alloc1.storage)
	assert.Equal(t, uint32(2), tree.minNode)
	assert.Equal(t, uint32(2), tree.maxNode)

	alloc2 := alloc1.Clone()
	clone := tree.CloneShallow(alloc2)

	assert.Equal(t, []node{{}, {}, {color: black, item: Item{7, 7}}, {}}, alloc2.storage)
	assert.Equal(t, uint32(2), clone.minNode)
	assert.Equal(t, uint32(2), clone.maxNode)
	assert.Equal(t, 4, alloc2.Size())

	tree.Insert(Item{10, 10})

	alloc3 := alloc1.Clone()
	clone = tree.CloneShallow(alloc3)

	assert.Equal(t, []node{
		{}, {},
		{right: 3, color: black, item: Item{7, 7}},
		{parent: 2, color: red, item: Item{10, 10}}}, alloc3.storage)
	assert.Equal(t, uint32(2), clone.minNode)
	assert.Equal(t, uint32(3), clone.maxNode)
	assert.Equal(t, 4, alloc3.Size())
	assert.Equal(t, 4, alloc2.Size())
}

func TestCloneDeep(t *testing.T) {
	t.Parallel()

	alloc1 := NewAllocator()
	alloc1.malloc()

	tree := NewRBTree(alloc1)
	tree.Insert(Item{7, 7})

	assert.Equal(t, []node{{}, {}, {color: black, item: Item{7, 7}}}, alloc1.storage)
	assert.Equal(t, uint32(2), tree.minNode)
	assert.Equal(t, uint32(2), tree.maxNode)

	alloc2 := NewAllocator()
	clone := tree.CloneDeep(alloc2)

	assert.Equal(t, []node{{}, {color: black, item: Item{7, 7}}}, alloc2.storage)
	assert.Equal(t, uint32(1), clone.minNode)
	assert.Equal(t, uint32(1), clone.maxNode)
	assert.Equal(t, 2, alloc2.Size())

	tree.Insert(Item{10, 10})

	alloc2 = NewAllocator()
	clone = tree.CloneDeep(alloc2)

	assert.Equal(t, []node{
		{},
		{right: 2, color: black, item: Item{7, 7}},
		{parent: 1, color: red, item: Item{10, 10}}}, alloc2.storage)
	assert.Equal(t, uint32(1), clone.minNode)
	assert.Equal(t, uint32(2), clone.maxNode)
	assert.Equal(t, 3, alloc2.Size())
}

func TestErase(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()
	tree := NewRBTree(alloc)

	for idx := range 10 {
		tree.Insert(Item{uint32(idx), uint32(idx)})
	}

	assert.Equal(t, 11, alloc.Used())
	tree.Erase()
	assert.Equal(t, 1, alloc.Used())
	assert.Equal(t, 11, alloc.Size())
}

func TestAllocatorHibernateBoot(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()

	for idx := range 10000 {
		nd := alloc.malloc()
		alloc.storage[nd].item.Key = uint32(idx)
		alloc.storage[nd].item.Value = uint32(idx)
		alloc.storage[nd].left = uint32(idx)
		alloc.storage[nd].right = uint32(idx)
		alloc.storage[nd].parent = uint32(idx)
		alloc.storage[nd].color = idx%2 == 0
	}

	for idx := range 10000 {
		alloc.gaps[uint32(idx)] = true // Makes no sense, only to test.
	}

	alloc.Hibernate()
	assert.PanicsWithValue(t, "cannot hibernate an already hibernated Allocator", alloc.Hibernate)
	assert.Nil(t, alloc.storage)
	assert.Nil(t, alloc.gaps)
	assert.Equal(t, 0, alloc.Size())
	assert.Equal(t, 10001, alloc.hibernatedStorageLen)
	assert.Equal(t, 10000, alloc.hibernatedGapsLen)
	assert.PanicsWithValue(t, "hibernated allocators cannot be used", func() { alloc.Used() })
	assert.PanicsWithValue(t, "hibernated allocators cannot be used", func() { alloc.malloc() })
	assert.PanicsWithValue(t, "hibernated allocators cannot be used", func() { alloc.free(0) })
	assert.PanicsWithValue(t, "cannot clone a hibernated allocator", func() { alloc.Clone() })

	alloc.Boot()
	assert.Equal(t, 0, alloc.hibernatedStorageLen)
	assert.Equal(t, 0, alloc.hibernatedGapsLen)

	for nd := 1; nd <= 10000; nd++ {
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].item.Key)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].item.Value)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].left)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].right)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].parent)
		assert.Equal(t, (nd-1)%2 == 0, alloc.storage[nd].color)
		assert.True(t, alloc.gaps[uint32(nd-1)])
	}
}

func TestAllocatorHibernateBootEmpty(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()
	alloc.Hibernate()
	alloc.Boot()
	assert.NotNil(t, alloc.gaps)
	assert.Equal(t, 0, alloc.Size())
	assert.Equal(t, 0, alloc.Used())
}

func TestAllocatorHibernateBootThreshold(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()
	alloc.malloc()
	alloc.HibernationThreshold = 3
	assert.Equal(t, 3, alloc.Clone().HibernationThreshold)

	alloc.Hibernate()
	assert.Equal(t, 0, alloc.hibernatedStorageLen)

	alloc.Boot()
	alloc.malloc()
	alloc.Hibernate()
	assert.Equal(t, 0, alloc.hibernatedGapsLen)
	assert.Equal(t, 3, alloc.hibernatedStorageLen)

	alloc.Boot()
	assert.Equal(t, 3, alloc.Size())
	assert.Equal(t, 3, alloc.Used())
	assert.NotNil(t, alloc.gaps)
}

// TestRBTreeAllocator tests the Allocator() accessor method.
func TestRBTreeAllocator(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()
	tree := NewRBTree(alloc)

	// Verify Allocator() returns the same allocator.
	assert.Equal(t, alloc, tree.Allocator())

	// Insert some elements and verify allocator is still accessible.
	tree.Insert(Item{5, 5})
	tree.Insert(Item{10, 10})
	assert.Equal(t, alloc, tree.Allocator())
	assert.Equal(t, 3, alloc.Used()) // 1 reserved + 2 nodes.
}

// TestNegativeLimit tests the NegativeLimit() iterator method.
func TestNegativeLimit(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	boolInsert(tree, 5)
	boolInsert(tree, 10)
	boolInsert(tree, 15)

	// NegativeLimit should be before first element.
	iter := tree.NegativeLimit()
	assert.True(t, iter.NegativeLimit())
	assert.False(t, iter.Limit())

	// Next from NegativeLimit should give first element.
	iter = iter.Next()
	assert.False(t, iter.NegativeLimit())
	assert.Equal(t, uint32(5), iter.Item().Key)

	// Empty tree should still work.
	emptyTree := testNewIntSet()
	emptyIter := emptyTree.NegativeLimit()
	assert.True(t, emptyIter.NegativeLimit())
}

// TestDeleteWithIterator tests the DeleteWithIterator() method.
func TestDeleteWithIterator(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	boolInsert(tree, 5)
	boolInsert(tree, 10)
	boolInsert(tree, 15)

	assert.Equal(t, 3, tree.Len())

	// Find middle element and delete via iterator.
	iter := tree.FindGE(10)
	assert.Equal(t, uint32(10), iter.Item().Key)

	tree.DeleteWithIterator(iter)
	assert.Equal(t, 2, tree.Len())
	assert.Nil(t, tree.Get(10))

	// Verify remaining elements.
	assert.NotNil(t, tree.Get(5))
	assert.NotNil(t, tree.Get(15))

	// Delete first element via iterator.
	minIter := tree.Min()
	tree.DeleteWithIterator(minIter)
	assert.Equal(t, 1, tree.Len())
	assert.Nil(t, tree.Get(5))

	// Delete last element via iterator.
	maxIter := tree.Max()
	tree.DeleteWithIterator(maxIter)
	assert.Equal(t, 0, tree.Len())
	assert.Nil(t, tree.Get(15))
}

// TestIteratorMinMax tests the Iterator.Min() and Iterator.Max() methods.
func TestIteratorMinMax(t *testing.T) {
	t.Parallel()

	tree := testNewIntSet()
	boolInsert(tree, 5)
	boolInsert(tree, 10)
	boolInsert(tree, 15)

	// Min iterator should have Min() == true.
	minIter := tree.Min()
	assert.True(t, minIter.Min())
	assert.False(t, minIter.Max())
	assert.Equal(t, uint32(5), minIter.Item().Key)

	// Max iterator should have Max() == true.
	maxIter := tree.Max()
	assert.True(t, maxIter.Max())
	assert.False(t, maxIter.Min())
	assert.Equal(t, uint32(15), maxIter.Item().Key)

	// Middle element should have both false.
	midIter := tree.FindGE(10)
	assert.False(t, midIter.Min())
	assert.False(t, midIter.Max())
	assert.Equal(t, uint32(10), midIter.Item().Key)

	// Single element tree - same element is both min and max.
	singleTree := testNewIntSet()
	boolInsert(singleTree, 42)

	singleMin := singleTree.Min()
	assert.True(t, singleMin.Min())
	assert.True(t, singleMin.Max())

	singleMax := singleTree.Max()
	assert.True(t, singleMax.Min())
	assert.True(t, singleMax.Max())
}

func TestAllocatorSerializeDeserialize(t *testing.T) {
	t.Parallel()

	alloc := NewAllocator()

	for idx := range 10000 {
		nd := alloc.malloc()
		alloc.storage[nd].item.Key = uint32(idx)
		alloc.storage[nd].item.Value = uint32(idx)
		alloc.storage[nd].left = uint32(idx)
		alloc.storage[nd].right = uint32(idx)
		alloc.storage[nd].parent = uint32(idx)
		alloc.storage[nd].color = idx%2 == 0
	}

	for idx := range 10000 {
		alloc.gaps[uint32(idx)] = true // Makes no sense, only to test.
	}

	assert.PanicsWithValue(t, "serialization requires the hibernated state",
		func() {
			err := alloc.Serialize("...")
			_ = err
		})
	assert.PanicsWithValue(t, "deserialization requires the hibernated state",
		func() {
			err := alloc.Deserialize("...")
			_ = err
		})

	alloc.Hibernate()

	file, err := os.CreateTemp(t.TempDir(), "")
	require.NoError(t, err)

	name := file.Name()

	require.NoError(t, file.Close())
	require.Error(t, alloc.Serialize("/tmp/xxx/yyy"))
	require.NoError(t, alloc.Serialize(name))
	assert.Nil(t, alloc.storage)
	assert.Nil(t, alloc.gaps)

	for _, data := range alloc.hibernatedData {
		assert.Nil(t, data)
	}

	assert.Equal(t, 10001, alloc.hibernatedStorageLen)
	assert.Equal(t, 10000, alloc.hibernatedGapsLen)
	assert.PanicsWithValue(t, "cannot boot a serialized Allocator", alloc.Boot)
	require.Error(t, alloc.Deserialize("/tmp/xxx/yyy"))
	require.NoError(t, alloc.Deserialize(name))

	for _, data := range alloc.hibernatedData {
		assert.NotEmpty(t, data)
	}

	alloc.Boot()
	assert.Equal(t, 0, alloc.hibernatedStorageLen)
	assert.Equal(t, 0, alloc.hibernatedGapsLen)

	for _, data := range alloc.hibernatedData {
		assert.Nil(t, data)
	}

	for nd := 1; nd <= 10000; nd++ {
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].item.Key)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].item.Value)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].left)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].right)
		assert.Equal(t, uint32(nd-1), alloc.storage[nd].parent)
		assert.Equal(t, (nd-1)%2 == 0, alloc.storage[nd].color)
		assert.True(t, alloc.gaps[uint32(nd-1)])
	}

	alloc.Hibernate()

	require.NoError(t, os.Truncate(name, 100))
	require.Error(t, alloc.Deserialize(name))
	require.NoError(t, os.Truncate(name, 4))
	require.Error(t, alloc.Deserialize(name))
	require.NoError(t, os.Truncate(name, 0))
	require.Error(t, alloc.Deserialize(name))
}
