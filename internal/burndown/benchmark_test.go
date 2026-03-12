package burndown

import (
	"testing"
)

// Benchmark constants for treap coalescing and pool benchmarks.
const (
	// benchCoalesceFileLen is the initial file length for coalescing benchmarks.
	benchCoalesceFileLen = 50000

	// benchCoalesceEdits is the number of edit operations per benchmark iteration.
	benchCoalesceEdits = 10000

	// benchCoalesceInsLen is the insertion length per edit in benchmarks.
	benchCoalesceInsLen = 5

	// benchCoalesceDelLen is the deletion length per edit in benchmarks.
	benchCoalesceDelLen = 2

	// benchCoalesceTimeModulo is the time modulo to create adjacent same-value segments.
	benchCoalesceTimeModulo = 50

	// benchCoalescePosModulo is the position modulo for edit placement.
	benchCoalescePosModulo = 40000

	// benchCoalescePosMultiplier is the position multiplier for pseudo-random placement.
	benchCoalescePosMultiplier = 31

	// benchCoalesceInterval is how often to coalesce (every N edits).
	benchCoalesceInterval = 500

	// benchPoolWarmupEdits is the number of warmup edits to fill the pool before benchmarking.
	benchPoolWarmupEdits = 500

	// benchPoolFileLen is the initial file length for pool benchmarks.
	benchPoolFileLen = 5000

	// benchPoolPosMod is the position modulo for pool benchmark edits.
	benchPoolPosMod = 4000

	// benchPrioFileLen is the initial file length for priority benchmarks.
	benchPrioFileLen = 5000

	// benchPrioWarmupEdits is the warmup edits for priority benchmarks.
	benchPrioWarmupEdits = 500

	// benchPrioPosMod is the position modulo for priority benchmark edits.
	benchPrioPosMod = 4000
)

// BenchmarkReplaceWithCoalescing measures Replace throughput with periodic coalescing.
func BenchmarkReplaceWithCoalescing(b *testing.B) {
	for range b.N {
		tl := NewTreapTimeline(0, benchCoalesceFileLen)

		for i := range benchCoalesceEdits {
			time := TimeKey(i % benchCoalesceTimeModulo)

			pos := (i * benchCoalescePosMultiplier) % benchCoalescePosModulo
			if pos < 0 {
				pos = -pos
			}

			tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, time)

			if (i+1)%benchCoalesceInterval == 0 {
				tl.MergeAdjacentSameValue()
			}
		}
	}
}

// BenchmarkReplaceWithoutCoalescing measures Replace throughput without coalescing (baseline).
func BenchmarkReplaceWithoutCoalescing(b *testing.B) {
	for range b.N {
		tl := NewTreapTimeline(0, benchCoalesceFileLen)

		for i := range benchCoalesceEdits {
			time := TimeKey(i % benchCoalesceTimeModulo)

			pos := (i * benchCoalescePosMultiplier) % benchCoalescePosModulo
			if pos < 0 {
				pos = -pos
			}

			tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, time)
		}
	}
}

// BenchmarkReplace_Pooled measures per-Replace allocs after pool warmup.
// The pool should serve most allocations from the free-list, reducing allocs/op.
func BenchmarkReplace_Pooled(b *testing.B) {
	tl := NewTreapTimeline(0, benchPoolFileLen)

	// Warmup: fill the pool with recycled nodes.
	for i := range benchPoolWarmupEdits {
		pos := (i * benchCoalescePosMultiplier) % benchPoolPosMod

		tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, TimeKey(i%benchCoalesceTimeModulo))
	}

	b.ResetTimer()

	for i := range b.N {
		pos := (i * benchCoalescePosMultiplier) % benchPoolPosMod

		tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, TimeKey(i%benchCoalesceTimeModulo))
	}
}

// BenchmarkReplace_RandomPriority measures Replace throughput with xorshift64 random priorities.
// This is the current default; compared against BenchmarkReplace_Pooled for parity.
func BenchmarkReplace_RandomPriority(b *testing.B) {
	tl := NewTreapTimeline(0, benchPrioFileLen)

	// Warmup: fill the pool with recycled nodes.
	for i := range benchPrioWarmupEdits {
		pos := (i * benchCoalescePosMultiplier) % benchPrioPosMod

		tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, TimeKey(i%benchCoalesceTimeModulo))
	}

	b.ResetTimer()

	for i := range b.N {
		pos := (i * benchCoalescePosMultiplier) % benchPrioPosMod

		tl.Replace(pos, benchCoalesceDelLen, benchCoalesceInsLen, TimeKey(i%benchCoalesceTimeModulo))
	}
}
