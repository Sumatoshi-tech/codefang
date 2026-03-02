package burndown

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Range query test constants.
const (
	rqInitialLength   = 100
	rqLargeFileLength = 10000
	rqBenchFileLength = 100000
	rqBenchEditCount  = 1000
	rqBenchTimeModulo = 100
	rqBenchSpacing    = 97
	rqBenchInsert     = 3
	rqBenchDelete     = 2
)

// TestQueryRange_Basic verifies basic range query on a simple file.
func TestQueryRange_Basic(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)

	// Initial file: all lines owned by time 0.
	results := file.QueryRange(0, rqInitialLength)
	require.Len(t, results, 1)
	assert.Equal(t, 0, results[0].StartLine)
	assert.Equal(t, rqInitialLength, results[0].EndLine)
	assert.Equal(t, 0, results[0].Owner)
}

// TestQueryRange_AfterUpdate verifies query reflects timeline changes.
func TestQueryRange_AfterUpdate(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)

	// Insert 10 lines at position 50 with time 1.
	file.Update(1, 50, 10, 0)

	// Query the entire file.
	results := file.QueryRange(0, file.Len())
	require.NotEmpty(t, results)

	// Verify time=1 segment exists in results.
	found := false

	for _, seg := range results {
		if seg.Owner == 1 {
			found = true

			assert.Equal(t, 50, seg.StartLine)
			assert.Equal(t, 60, seg.EndLine)
		}
	}

	assert.True(t, found, "should find segment with owner=1")
}

// TestQueryRange_PartialOverlap verifies partial range overlap.
func TestQueryRange_PartialOverlap(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)
	file.Update(1, 50, 10, 0)

	// Query range [55, 65) — should overlap the time=1 segment [50, 60).
	results := file.QueryRange(55, 65)
	require.NotEmpty(t, results)

	hasOwner1 := false

	for _, seg := range results {
		if seg.Owner == 1 {
			hasOwner1 = true
		}
	}

	assert.True(t, hasOwner1, "should find time=1 segment in partial overlap")
}

// TestQueryRange_NoOverlap verifies empty result when no overlap.
func TestQueryRange_NoOverlap(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)

	// Query beyond file range.
	results := file.QueryRange(200, 300)
	assert.Empty(t, results)
}

// TestQueryRange_EmptyFile verifies query on zero-length file.
func TestQueryRange_EmptyFile(t *testing.T) {
	t.Parallel()

	file := NewFile(0, 0)

	results := file.QueryRange(0, 10)
	assert.Empty(t, results)
}

// TestQueryRange_LazyRebuild verifies index is rebuilt after Update.
func TestQueryRange_LazyRebuild(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)

	// First query builds index.
	results1 := file.QueryRange(0, rqInitialLength)
	require.Len(t, results1, 1)

	// Update invalidates index.
	file.Update(1, 50, 10, 0)

	// Second query rebuilds.
	results2 := file.QueryRange(0, file.Len())
	assert.Greater(t, len(results2), 1, "should have more segments after update")
}

// TestQueryRange_LargeFile verifies correctness on a larger file.
func TestQueryRange_LargeFile(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqLargeFileLength)

	// Apply several updates.
	for i := 1; i <= 10; i++ {
		pos := i * 500
		file.Update(i, pos, 50, 0)
	}

	// Query a range that should overlap multiple segments.
	results := file.QueryRange(0, file.Len())
	assert.NotEmpty(t, results)

	// Verify total covered lines match file length.
	totalLines := 0
	for _, seg := range results {
		totalLines += seg.EndLine - seg.StartLine
	}

	assert.Equal(t, file.Len(), totalLines, "total segment lines should match file length")
}

// TestQueryRange_ExcludesTreeEnd verifies TreeEnd sentinel is excluded.
func TestQueryRange_ExcludesTreeEnd(t *testing.T) {
	t.Parallel()

	file := NewFile(0, rqInitialLength)

	results := file.QueryRange(0, rqInitialLength+10)
	for _, seg := range results {
		assert.NotEqual(t, -1, seg.Owner, "TreeEnd should not appear in results")
		assert.NotEqual(t, int(TreeEnd), seg.Owner, "TreeEnd should not appear in results")
	}
}

// BenchmarkQueryRange_IntervalTree benchmarks range query using interval tree.
func BenchmarkQueryRange_IntervalTree(b *testing.B) {
	file := NewFile(0, rqBenchFileLength)

	for i := 1; i <= rqBenchEditCount; i++ {
		pos := (i * rqBenchSpacing) % (rqBenchFileLength - rqBenchInsert)
		file.Update(i%rqBenchTimeModulo, pos, rqBenchInsert, rqBenchDelete)
	}

	// Warm up the index.
	file.QueryRange(0, rqBenchFileLength)

	b.ResetTimer()

	for range b.N {
		file.QueryRange(rqBenchFileLength/4, rqBenchFileLength/2)
	}
}

// BenchmarkQueryRange_LinearScan benchmarks linear scan as baseline comparison.
func BenchmarkQueryRange_LinearScan(b *testing.B) {
	file := NewFile(0, rqBenchFileLength)

	for i := 1; i <= rqBenchEditCount; i++ {
		pos := (i * rqBenchSpacing) % (rqBenchFileLength - rqBenchInsert)
		file.Update(i%rqBenchTimeModulo, pos, rqBenchInsert, rqBenchDelete)
	}

	queryStart := rqBenchFileLength / 4
	queryEnd := rqBenchFileLength / 2

	b.ResetTimer()

	for range b.N {
		var results []OwnershipSegment

		file.timeline.Iterate(func(offset, length int, t TimeKey) bool {
			if t == TreeEnd || length <= 0 {
				return true
			}

			segEnd := offset + length

			if offset < queryEnd && segEnd > queryStart {
				results = append(results, OwnershipSegment{
					StartLine: offset,
					EndLine:   segEnd,
					Owner:     int(t),
				})
			}

			return true
		})
	}
}
