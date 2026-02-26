package burndown

import (
	"math"
	"testing"
)

// Test constants for xorshift64 and randomized priority tests.
const (
	// prioTestSeed is a non-zero seed for deterministic PRNG tests.
	prioTestSeed = uint64(0xDEAD_BEEF_CAFE_BABE)

	// prioTestSequenceLen is the number of PRNG values to generate in sequence tests.
	prioTestSequenceLen = 1000

	// prioTestBucketCount is the number of buckets for distribution tests.
	prioTestBucketCount = 16

	// prioTestDistributionLen is the number of values to generate for distribution tests.
	prioTestDistributionLen = 100000

	// prioTestMinBucketFraction is the minimum fraction of values per bucket (uniform = 1/16 = 6.25%).
	prioTestMinBucketFraction = 0.03

	// prioTestMaxBucketFraction is the maximum fraction of values per bucket.
	prioTestMaxBucketFraction = 0.10

	// prioTestDepthInserts is the number of sequential inserts for depth tests.
	prioTestDepthInserts = 10000

	// prioTestDepthMultiplier is the multiplier for max allowed depth (3 * log2(N)).
	prioTestDepthMultiplier = 3

	// prioTestCloneFileLen is the file length for clone PRNG tests.
	prioTestCloneFileLen = 500

	// prioTestCloneOps is the number of Replace operations for clone divergence tests.
	prioTestCloneOps = 100

	// prioTestCloneInsLen is the insertion length for clone Replace tests.
	prioTestCloneInsLen = 3

	// prioTestCloneDelLen is the deletion length for clone Replace tests.
	prioTestCloneDelLen = 2

	// prioTestCloneTimeMod is the time modulo for clone Replace tests.
	prioTestCloneTimeMod = 20

	// prioTestClonePosMod is the position modulo for clone Replace tests.
	prioTestClonePosMod = 400

	// prioTestClonePosMultiplier is the position multiplier for clone Replace tests.
	prioTestClonePosMultiplier = 31
)

// TestXorshift64_NonZero verifies that xorshift64 produces non-zero output from a non-zero seed.
func TestXorshift64_NonZero(t *testing.T) {
	t.Parallel()

	state := prioTestSeed

	for range prioTestSequenceLen {
		val := xorshift64(&state)
		if val != 0 {
			return
		}
	}

	t.Error("xorshift64 produced only zeros from non-zero seed")
}

// TestXorshift64_Deterministic verifies that the same seed produces the same sequence.
func TestXorshift64_Deterministic(t *testing.T) {
	t.Parallel()

	state1 := prioTestSeed
	state2 := prioTestSeed

	for range prioTestSequenceLen {
		v1 := xorshift64(&state1)
		v2 := xorshift64(&state2)

		if v1 != v2 {
			t.Fatalf("determinism broken: got %d and %d from same seed", v1, v2)
		}
	}
}

// TestXorshift64_StateAdvances verifies that state changes after each call.
func TestXorshift64_StateAdvances(t *testing.T) {
	t.Parallel()

	state := prioTestSeed
	prev := state

	for range prioTestSequenceLen {
		xorshift64(&state)

		if state == prev {
			t.Fatal("state did not advance")
		}

		prev = state
	}
}

// TestXorshift64_Distribution verifies that output covers a reasonable range.
func TestXorshift64_Distribution(t *testing.T) {
	t.Parallel()

	state := prioTestSeed
	buckets := make([]int, prioTestBucketCount)
	bucketSize := (math.MaxUint32 + 1) / prioTestBucketCount

	for range prioTestDistributionLen {
		val := xorshift64(&state)

		bucket := int(val) / bucketSize
		if bucket >= prioTestBucketCount {
			bucket = prioTestBucketCount - 1
		}

		buckets[bucket]++
	}

	for i, count := range buckets {
		fraction := float64(count) / float64(prioTestDistributionLen)

		if fraction < prioTestMinBucketFraction || fraction > prioTestMaxBucketFraction {
			t.Errorf("bucket %d: fraction %.4f outside [%.2f, %.2f]",
				i, fraction, prioTestMinBucketFraction, prioTestMaxBucketFraction)
		}
	}
}

// TestMaxDepth_NilRoot verifies that maxDepth returns 0 for an empty treap.
func TestMaxDepth_NilRoot(t *testing.T) {
	t.Parallel()

	tl := &treapTimeline{}

	if depth := tl.maxDepth(); depth != 0 {
		t.Errorf("expected maxDepth 0 for nil root, got %d", depth)
	}
}

// TestMaxDepth_SingleNode verifies maxDepth for a single-node treap.
func TestMaxDepth_SingleNode(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, 0)

	// Empty timeline has nil root, depth 0.
	if depth := tl.maxDepth(); depth != 0 {
		t.Errorf("expected maxDepth 0 for empty timeline, got %d", depth)
	}
}

// TestMaxDepth_NonEmpty verifies maxDepth for a non-empty treap is positive.
func TestMaxDepth_NonEmpty(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, prioTestCloneFileLen)

	depth := tl.maxDepth()
	if depth < 1 {
		t.Errorf("expected maxDepth >= 1 for non-empty timeline, got %d", depth)
	}
}

// TestRandomPriority_Depth10K verifies that 10K sequential inserts produce
// a tree with depth < 3 * log2(10K) ≈ 42.
func TestRandomPriority_Depth10K(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, 1)

	for i := range prioTestDepthInserts {
		tl.Replace(0, 0, 1, TimeKey(i%prioTestCloneTimeMod))
	}

	depth := tl.maxDepth()
	maxAllowed := prioTestDepthMultiplier * int(math.Log2(float64(prioTestDepthInserts)))

	if depth > maxAllowed {
		t.Errorf("tree depth %d exceeds max allowed %d (3 * log2(%d))",
			depth, maxAllowed, prioTestDepthInserts)
	}
}

// TestCloneDeep_PreservesPRNG verifies that CloneDeep produces independent PRNG evolution.
func TestCloneDeep_PreservesPRNG(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, prioTestCloneFileLen)

	// Perform some operations to advance the PRNG.
	for i := range prioTestCloneOps {
		pos := (i * prioTestClonePosMultiplier) % prioTestClonePosMod

		tl.Replace(pos, prioTestCloneDelLen, prioTestCloneInsLen, TimeKey(i%prioTestCloneTimeMod))
	}

	clone := tl.CloneDeep()

	// Both should be valid.
	tl.Validate()
	clone.Validate()

	// Modify clone independently.
	clone.Replace(0, prioTestCloneDelLen, prioTestCloneInsLen, TimeKey(1))
	clone.Validate()

	// Original should remain valid and unchanged in length.
	tl.Validate()

	if tl.Len() == clone.Len() {
		t.Error("expected different Len after modifying clone but not original")
	}
}
