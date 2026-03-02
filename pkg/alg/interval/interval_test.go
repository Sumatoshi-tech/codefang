package interval

// FRD: specs/frds/FRD-20260302-generic-interval-tree.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Test constants.
const (
	testLow10    = 10
	testHigh20   = 20
	testLow15    = 15
	testHigh25   = 25
	testLow30    = 30
	testHigh40   = 40
	testLow5     = 5
	testHigh35   = 35
	testValue1   = 1
	testValue2   = 2
	testValue3   = 3
	testValue4   = 4
	testPoint12  = 12
	testPoint50  = 50
	testCount100 = 100
	testLow50    = 50
	testHigh60   = 60
)

// TestNew verifies empty tree creation.
func TestNew(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	assert.NotNil(t, tree)
	assert.Equal(t, 0, tree.Len())
}

// TestInsert_Len verifies length tracking after inserts.
func TestInsert_Len(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	assert.Equal(t, 1, tree.Len())

	tree.Insert(testLow30, testHigh40, testValue2)
	assert.Equal(t, 2, tree.Len())
}

// TestInsert_QueryOverlap_Basic verifies basic insert and query.
func TestInsert_QueryOverlap_Basic(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	results := tree.QueryOverlap(testLow15, testHigh25)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testLow10), results[0].Low)
	assert.Equal(t, uint32(testHigh20), results[0].High)
	assert.Equal(t, uint32(testValue1), results[0].Value)
}

// TestQueryOverlap_NoMatch verifies no results when no overlap.
func TestQueryOverlap_NoMatch(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	results := tree.QueryOverlap(testLow30, testHigh40)
	assert.Empty(t, results)
}

// TestQueryOverlap_EmptyTree verifies query on empty tree.
func TestQueryOverlap_EmptyTree(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()

	results := tree.QueryOverlap(testLow10, testHigh20)
	assert.Nil(t, results)
}

// TestQueryOverlap_MultipleResults verifies multiple overlapping intervals.
func TestQueryOverlap_MultipleResults(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow15, testHigh25, testValue2)
	tree.Insert(testLow30, testHigh40, testValue3)

	// Query [12, 18] should overlap [10,20] and [15,25] but not [30,40].
	results := tree.QueryOverlap(testPoint12, 18)
	assert.Len(t, results, 2)
}

// TestQueryPoint_Basic verifies point query.
func TestQueryPoint_Basic(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow30, testHigh40, testValue2)

	results := tree.QueryPoint(testPoint12)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testValue1), results[0].Value)
}

// TestQueryPoint_Boundary verifies point query at interval boundaries.
func TestQueryPoint_Boundary(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	// Point at Low boundary.
	results := tree.QueryPoint(testLow10)
	require.Len(t, results, 1)

	// Point at High boundary.
	results = tree.QueryPoint(testHigh20)
	require.Len(t, results, 1)
}

// TestQueryPoint_NoMatch verifies point query with no match.
func TestQueryPoint_NoMatch(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	results := tree.QueryPoint(testPoint50)
	assert.Empty(t, results)
}

// TestDelete_Basic verifies basic delete and re-query.
func TestDelete_Basic(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	deleted := tree.Delete(testLow10, testHigh20, testValue1)
	assert.True(t, deleted)
	assert.Equal(t, 0, tree.Len())

	results := tree.QueryOverlap(testLow10, testHigh20)
	assert.Empty(t, results)
}

// TestDelete_NonExistent verifies deleting a non-existent interval.
func TestDelete_NonExistent(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)

	deleted := tree.Delete(testLow30, testHigh40, testValue2)
	assert.False(t, deleted)
	assert.Equal(t, 1, tree.Len())
}

// TestDelete_EmptyTree verifies delete on empty tree.
func TestDelete_EmptyTree(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()

	deleted := tree.Delete(testLow10, testHigh20, testValue1)
	assert.False(t, deleted)
}

// TestDelete_PreservesOthers verifies delete doesn't affect other intervals.
func TestDelete_PreservesOthers(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow30, testHigh40, testValue2)

	tree.Delete(testLow10, testHigh20, testValue1)

	results := tree.QueryOverlap(testLow30, testHigh40)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testValue2), results[0].Value)
}

// TestClear verifies clear removes all intervals.
func TestClear(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow30, testHigh40, testValue2)
	assert.Equal(t, 2, tree.Len())

	tree.Clear()
	assert.Equal(t, 0, tree.Len())

	results := tree.QueryOverlap(0, 100)
	assert.Empty(t, results)
}

// TestAdjacentNonOverlapping verifies adjacent intervals don't overlap.
func TestAdjacentNonOverlapping(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(21, testHigh40, testValue2)

	// Query the gap — should not find [10,20] or [21,40].
	// Actually [10,20] and [21,40] are adjacent. Query exactly at 20 should find first.
	results := tree.QueryPoint(testHigh20)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testValue1), results[0].Value)

	// Query at 21 should find second.
	results = tree.QueryPoint(21)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testValue2), results[0].Value)
}

// TestZeroWidthInterval verifies point intervals (Low == High) work.
func TestZeroWidthInterval(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow15, testLow15, testValue1)

	results := tree.QueryPoint(testLow15)
	require.Len(t, results, 1)

	results = tree.QueryPoint(testLow10)
	assert.Empty(t, results)
}

// TestLargeScale verifies correctness with many intervals.
func TestLargeScale(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()

	// Insert 10K intervals: [i*10, i*10+5] for i in [0, 10000).
	const (
		intervalCount   = 10000
		intervalWidth   = 5
		intervalSpacing = 10
	)

	for i := range intervalCount {
		low := uint32(i * intervalSpacing)
		high := low + intervalWidth

		tree.Insert(low, high, uint32(i))
	}

	assert.Equal(t, intervalCount, tree.Len())

	// Query a range that overlaps exactly 100 intervals.
	// Intervals [0,5], [10,15], ..., [990,995] all have Low < 1000.
	// Query [0, 995] should overlap all with Low 0..990, i.e., 100 intervals.
	results := tree.QueryOverlap(0, 995)
	assert.Len(t, results, testCount100)

	// Query a point — should find exactly 1.
	results = tree.QueryPoint(testLow50 * intervalSpacing)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testLow50), results[0].Value)
}

// TestDeleteMultiple verifies deleting multiple intervals one by one.
func TestDeleteMultiple(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()

	const count = 20

	for i := range count {
		tree.Insert(uint32(i*testLow10), uint32(i*testLow10+testLow5), uint32(i))
	}

	assert.Equal(t, count, tree.Len())

	// Delete all intervals.
	for i := range count {
		ok := tree.Delete(uint32(i*testLow10), uint32(i*testLow10+testLow5), uint32(i))
		assert.True(t, ok, "delete failed at index %d", i)
	}

	assert.Equal(t, 0, tree.Len())
}

// TestInsertDuplicateIntervals verifies duplicate interval handling.
func TestInsertDuplicateIntervals(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow10, testHigh20, testValue1)
	assert.Equal(t, 2, tree.Len())

	results := tree.QueryOverlap(testLow10, testHigh20)
	assert.Len(t, results, 2)

	// Delete one — should leave one.
	tree.Delete(testLow10, testHigh20, testValue1)
	assert.Equal(t, 1, tree.Len())

	results = tree.QueryOverlap(testLow10, testHigh20)
	assert.Len(t, results, 1)
}

// TestCompareIntervals verifies interval comparison ordering.
func TestCompareIntervals(t *testing.T) {
	t.Parallel()

	a := Interval[uint32, uint32]{Low: testLow10, High: testHigh20}
	b := Interval[uint32, uint32]{Low: testLow15, High: testHigh25}

	assert.Negative(t, compareIntervals(a, b))
	assert.Positive(t, compareIntervals(b, a))
	assert.Equal(t, 0, compareIntervals(a, a))

	// Same Low, different High.
	c := Interval[uint32, uint32]{Low: testLow10, High: testHigh25}
	assert.Negative(t, compareIntervals(a, c))
	assert.Positive(t, compareIntervals(c, a))
}

// TestNodeColor verifies nil node is black.
func TestNodeColor(t *testing.T) {
	t.Parallel()

	assert.Equal(t, black, nodeColor[uint32, uint32](nil))

	n := &node[uint32, uint32]{color: red}
	assert.Equal(t, red, nodeColor(n))

	n.color = black
	assert.Equal(t, black, nodeColor(n))
}

// TestWideOverlap verifies query that spans many intervals.
func TestWideOverlap(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Insert(testLow15, testHigh25, testValue2)
	tree.Insert(testLow30, testHigh40, testValue3)
	tree.Insert(testLow5, testHigh35, testValue4)

	// Wide query should find all four.
	results := tree.QueryOverlap(0, testLow50)
	assert.Len(t, results, 4)
}

// TestDeleteAndReinsert verifies delete followed by re-insert.
func TestDeleteAndReinsert(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh20, testValue1)
	tree.Delete(testLow10, testHigh20, testValue1)
	assert.Equal(t, 0, tree.Len())

	tree.Insert(testLow10, testHigh20, testValue2)
	assert.Equal(t, 1, tree.Len())

	results := tree.QueryOverlap(testLow10, testHigh20)
	require.Len(t, results, 1)
	assert.Equal(t, uint32(testValue2), results[0].Value)
}

// TestMaxHighMaintenance verifies maxHigh is maintained correctly.
func TestMaxHighMaintenance(t *testing.T) {
	t.Parallel()

	tree := New[uint32, uint32]()
	tree.Insert(testLow10, testHigh60, testValue1)
	tree.Insert(testLow30, testHigh40, testValue2)

	// Root should have maxHigh >= 60.
	require.NotNil(t, tree.root)
	assert.GreaterOrEqual(t, tree.root.maxHigh, uint32(testHigh60))

	// After deleting [10,60], maxHigh should be 40.
	tree.Delete(testLow10, testHigh60, testValue1)
	require.NotNil(t, tree.root)
	assert.Equal(t, uint32(testHigh40), tree.root.maxHigh)
}

// Int key test constants.
const (
	testIntLow100   = 100
	testIntHigh200  = 200
	testIntLow150   = 150
	testIntHigh250  = 250
	testIntLow300   = 300
	testIntHigh400  = 400
	testIntValueA   = "alpha"
	testIntValueB   = "beta"
	testIntValueC   = "gamma"
	testIntPoint175 = 175
)

// TestGeneric_IntKeys verifies the tree works with int keys and string values.
func TestGeneric_IntKeys(t *testing.T) {
	t.Parallel()

	tree := New[int, string]()
	tree.Insert(testIntLow100, testIntHigh200, testIntValueA)
	tree.Insert(testIntLow150, testIntHigh250, testIntValueB)
	tree.Insert(testIntLow300, testIntHigh400, testIntValueC)
	assert.Equal(t, 3, tree.Len())

	// Point query at 175 should find both [100,200] and [150,250].
	results := tree.QueryPoint(testIntPoint175)
	assert.Len(t, results, 2)

	// Query [300,400] should find only gamma.
	results = tree.QueryOverlap(testIntLow300, testIntHigh400)
	require.Len(t, results, 1)
	assert.Equal(t, testIntValueC, results[0].Value)

	// Delete alpha, verify it's gone.
	ok := tree.Delete(testIntLow100, testIntHigh200, testIntValueA)
	assert.True(t, ok)
	assert.Equal(t, 2, tree.Len())

	results = tree.QueryPoint(testIntPoint175)
	require.Len(t, results, 1)
	assert.Equal(t, testIntValueB, results[0].Value)
}

// Int64 key test constants.
const (
	testInt64Low1B   int64 = 1_000_000_000
	testInt64High2B  int64 = 2_000_000_000
	testInt64Low15B  int64 = 1_500_000_000
	testInt64High25B int64 = 2_500_000_000
	testInt64Low3B   int64 = 3_000_000_000
	testInt64High4B  int64 = 4_000_000_000
	testInt64Value1  int64 = 1
	testInt64Value2  int64 = 2
	testInt64Value3  int64 = 3
	testInt64Point   int64 = 1_750_000_000
)

// TestGeneric_Int64Keys verifies the tree works with int64 keys.
func TestGeneric_Int64Keys(t *testing.T) {
	t.Parallel()

	tree := New[int64, int64]()
	tree.Insert(testInt64Low1B, testInt64High2B, testInt64Value1)
	tree.Insert(testInt64Low15B, testInt64High25B, testInt64Value2)
	tree.Insert(testInt64Low3B, testInt64High4B, testInt64Value3)
	assert.Equal(t, 3, tree.Len())

	// Point query at 1.75B should find [1B,2B] and [1.5B,2.5B].
	results := tree.QueryPoint(testInt64Point)
	assert.Len(t, results, 2)

	// Non-overlapping query should return empty.
	results = tree.QueryOverlap(testInt64High4B+1, testInt64High4B+testInt64Low1B)
	assert.Empty(t, results)

	// Delete and verify.
	ok := tree.Delete(testInt64Low15B, testInt64High25B, testInt64Value2)
	assert.True(t, ok)
	assert.Equal(t, 2, tree.Len())

	results = tree.QueryPoint(testInt64Point)
	require.Len(t, results, 1)
	assert.Equal(t, testInt64Value1, results[0].Value)

	// Clear and verify.
	tree.Clear()
	assert.Equal(t, 0, tree.Len())
}
