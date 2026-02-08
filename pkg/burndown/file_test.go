package burndown

import (
	"testing"
)

// BenchmarkFileUpdateManyEdits simulates many edits (different times = no merge benefit) on a growing file.
func BenchmarkFileUpdateManyEdits(b *testing.B) {
	file := NewFile(0, 10000)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Simulate edits at different "times" (commits) so adjacent intervals rarely match.
		time := i % 1000
		pos := (i * 31) % 9000
		if pos < 0 {
			pos = -pos
		}
		file.Update(time, pos, 5, 2)
	}
}

// TestMergeAdjacentSameValue verifies that merge does not increase node count and preserves
// effective line→time (value at any line unchanged). Treap implementation uses no-op merge.
func TestMergeAdjacentSameValue(t *testing.T) {
	t.Parallel()
	file := NewFile(0, 1000)
	// Build (0,0), (50,1), (60,1), (100,0), ... so (50,1) and (60,1) are adjacent same-value.
	file.Update(0, 0, 100, 0)
	file.Update(1, 50, 10, 0)
	file.Update(1, 60, 40, 0)

	before := file.Nodes()
	valueAtLine := func(f *File, line int) int {
		var last int
		f.ForEach(func(l, v int) {
			if l <= line {
				last = v
			}
		})
		return last
	}
	sampleLines := []int{0, 25, 50, 55, 60, 70, 100, 500}
	beforeValues := make([]int, len(sampleLines))
	for i, ln := range sampleLines {
		beforeValues[i] = valueAtLine(file, ln)
	}

	file.MergeAdjacentSameValue()

	after := file.Nodes()
	if after > before {
		t.Errorf("merge should not increase nodes: before=%d after=%d", before, after)
	}
	for i, ln := range sampleLines {
		if got := valueAtLine(file, ln); got != beforeValues[i] {
			t.Errorf("merge must preserve value at line %d: before=%d after=%d", ln, beforeValues[i], got)
		}
	}
	file.Validate()
}
