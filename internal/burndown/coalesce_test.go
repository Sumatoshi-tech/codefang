package burndown

import (
	"testing"
)

// Test constants for coalescing tests.
const (
	// coalesceTestTimeA is the first time value used in coalescing tests.
	coalesceTestTimeA = 1

	// coalesceTestTimeB is the second time value used in coalescing tests.
	coalesceTestTimeB = 2

	// coalesceTestTimeC is the third time value used in coalescing tests.
	coalesceTestTimeC = 3

	// coalesceTestLen10 is a segment length used in coalescing tests.
	coalesceTestLen10 = 10

	// coalesceTestLen20 is a segment length used in coalescing tests.
	coalesceTestLen20 = 20

	// coalesceTestLen30 is a segment length used in coalescing tests.
	coalesceTestLen30 = 30

	// coalesceTestLen50 is a segment length used in coalescing tests.
	coalesceTestLen50 = 50

	// coalesceTestLen100 is a segment length used in coalescing tests.
	coalesceTestLen100 = 100

	// coalesceTestLen1000 is the initial file length for coalescing tests.
	coalesceTestLen1000 = 1000

	// coalesceTestHeavyNodes is the number of same-value nodes in the heavy coalescing test.
	coalesceTestHeavyNodes = 800

	// coalesceTestReplacePos is the position for Replace-after-coalesce tests.
	coalesceTestReplacePos = 50

	// coalesceTestReplaceDel is the deletion length for Replace-after-coalesce tests.
	coalesceTestReplaceDel = 5

	// coalesceTestReplaceIns is the insertion length for Replace-after-coalesce tests.
	coalesceTestReplaceIns = 3

	// coalesceTestReplaceTime is the time for Replace-after-coalesce tests.
	coalesceTestReplaceTime = 99
)

// TestCoalesceSegments_MergesAdjacent verifies that adjacent segments with the same value are merged.
func TestCoalesceSegments_MergesAdjacent(t *testing.T) {
	t.Parallel()

	segs := []Segment{
		{Length: coalesceTestLen10, Value: coalesceTestTimeA},
		{Length: coalesceTestLen20, Value: coalesceTestTimeA},
		{Length: coalesceTestLen30, Value: coalesceTestTimeB},
	}

	result := coalesceSegments(segs)

	if len(result) != 2 {
		t.Fatalf("expected 2 segments, got %d", len(result))
	}

	if result[0].Length != coalesceTestLen10+coalesceTestLen20 || result[0].Value != coalesceTestTimeA {
		t.Errorf("segment 0: got {%d, %d}, want {%d, %d}",
			result[0].Length, result[0].Value,
			coalesceTestLen10+coalesceTestLen20, coalesceTestTimeA)
	}

	if result[1].Length != coalesceTestLen30 || result[1].Value != coalesceTestTimeB {
		t.Errorf("segment 1: got {%d, %d}, want {%d, %d}",
			result[1].Length, result[1].Value,
			coalesceTestLen30, coalesceTestTimeB)
	}
}

// TestCoalesceSegments_NoMerge verifies that segments with different values are not merged.
func TestCoalesceSegments_NoMerge(t *testing.T) {
	t.Parallel()

	segs := []Segment{
		{Length: coalesceTestLen10, Value: coalesceTestTimeA},
		{Length: coalesceTestLen20, Value: coalesceTestTimeB},
		{Length: coalesceTestLen30, Value: coalesceTestTimeC},
	}

	result := coalesceSegments(segs)

	if len(result) != len(segs) {
		t.Fatalf("expected %d segments, got %d", len(segs), len(result))
	}

	for i := range segs {
		if result[i] != segs[i] {
			t.Errorf("segment %d: got %+v, want %+v", i, result[i], segs[i])
		}
	}
}

// TestCoalesceSegments_Empty verifies that an empty input returns an empty output.
func TestCoalesceSegments_Empty(t *testing.T) {
	t.Parallel()

	result := coalesceSegments(nil)

	if len(result) != 0 {
		t.Errorf("expected 0 segments, got %d", len(result))
	}
}

// TestCoalesceSegments_SingleSegment verifies that a single segment is returned unchanged.
func TestCoalesceSegments_SingleSegment(t *testing.T) {
	t.Parallel()

	segs := []Segment{{Length: coalesceTestLen50, Value: coalesceTestTimeA}}

	result := coalesceSegments(segs)

	if len(result) != 1 {
		t.Fatalf("expected 1 segment, got %d", len(result))
	}

	if result[0] != segs[0] {
		t.Errorf("got %+v, want %+v", result[0], segs[0])
	}
}

// TestCoalesceSegments_AllSameValue verifies that all segments with the same value merge into one.
func TestCoalesceSegments_AllSameValue(t *testing.T) {
	t.Parallel()

	segs := []Segment{
		{Length: coalesceTestLen10, Value: coalesceTestTimeA},
		{Length: coalesceTestLen20, Value: coalesceTestTimeA},
		{Length: coalesceTestLen30, Value: coalesceTestTimeA},
	}

	result := coalesceSegments(segs)

	if len(result) != 1 {
		t.Fatalf("expected 1 segment, got %d", len(result))
	}

	expectedLen := coalesceTestLen10 + coalesceTestLen20 + coalesceTestLen30

	if result[0].Length != expectedLen || result[0].Value != coalesceTestTimeA {
		t.Errorf("got {%d, %d}, want {%d, %d}",
			result[0].Length, result[0].Value, expectedLen, coalesceTestTimeA)
	}
}

// TestMergeAdjacentSameValue_ReducesNodes verifies that coalescing reduces node count.
func TestMergeAdjacentSameValue_ReducesNodes(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	// Insert lines at adjacent positions with the same time value.
	// This creates fragmentation: two adjacent segments both with value 1.
	tl.Replace(coalesceTestReplacePos, 0, coalesceTestLen10, coalesceTestTimeA)
	tl.Replace(coalesceTestReplacePos+coalesceTestLen10, 0, coalesceTestLen20, coalesceTestTimeA)

	before := tl.Nodes()

	tl.MergeAdjacentSameValue()

	after := tl.Nodes()

	if after >= before {
		t.Errorf("expected fewer nodes after coalescing: before=%d, after=%d", before, after)
	}

	tl.Validate()
}

// TestMergeAdjacentSameValue_PreservesLen verifies that total line count is unchanged.
func TestMergeAdjacentSameValue_PreservesLen(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	tl.Replace(coalesceTestReplacePos, 0, coalesceTestLen10, coalesceTestTimeA)
	tl.Replace(coalesceTestReplacePos+coalesceTestLen10, 0, coalesceTestLen20, coalesceTestTimeA)

	before := tl.Len()

	tl.MergeAdjacentSameValue()

	after := tl.Len()

	if after != before {
		t.Errorf("Len changed: before=%d, after=%d", before, after)
	}
}

// TestMergeAdjacentSameValue_PreservesIterate verifies that Iterate output is identical.
func TestMergeAdjacentSameValue_PreservesIterate(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	tl.Replace(coalesceTestReplacePos, 0, coalesceTestLen10, coalesceTestTimeA)
	tl.Replace(coalesceTestReplacePos+coalesceTestLen10, 0, coalesceTestLen20, coalesceTestTimeA)

	beforeFlat := tl.Flatten()

	tl.MergeAdjacentSameValue()

	afterFlat := tl.Flatten()

	if len(afterFlat) != len(beforeFlat) {
		t.Fatalf("Flatten length changed: before=%d, after=%d", len(beforeFlat), len(afterFlat))
	}

	for i := range beforeFlat {
		if beforeFlat[i] != afterFlat[i] {
			t.Errorf("Flatten[%d] changed: before=%d, after=%d", i, beforeFlat[i], afterFlat[i])

			break
		}
	}
}

// TestMergeAdjacentSameValue_EmptyTimeline verifies no panic on empty timeline.
func TestMergeAdjacentSameValue_EmptyTimeline(t *testing.T) {
	t.Parallel()

	tl := &treapTimeline{}

	// Must not panic.
	tl.MergeAdjacentSameValue()

	if tl.Len() != 0 {
		t.Errorf("expected Len 0, got %d", tl.Len())
	}

	if tl.Nodes() != 0 {
		t.Errorf("expected 0 nodes, got %d", tl.Nodes())
	}
}

// TestMergeAdjacentSameValue_AlreadyOptimal verifies that an optimal treap is not modified.
func TestMergeAdjacentSameValue_AlreadyOptimal(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	tl.Replace(coalesceTestLen100, 0, coalesceTestLen50, coalesceTestTimeA)

	before := tl.Nodes()
	beforeFlat := tl.Flatten()

	tl.MergeAdjacentSameValue()

	after := tl.Nodes()
	afterFlat := tl.Flatten()

	if after != before {
		t.Errorf("node count changed on already-optimal treap: before=%d, after=%d", before, after)
	}

	for i := range beforeFlat {
		if beforeFlat[i] != afterFlat[i] {
			t.Errorf("Flatten[%d] changed: before=%d, after=%d", i, beforeFlat[i], afterFlat[i])

			break
		}
	}
}

// TestMergeAdjacentSameValue_ReplaceAfterCoalesce verifies that Replace works correctly after coalescing.
func TestMergeAdjacentSameValue_ReplaceAfterCoalesce(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	tl.Replace(coalesceTestReplacePos, 0, coalesceTestLen10, coalesceTestTimeA)
	tl.Replace(coalesceTestReplacePos+coalesceTestLen10, 0, coalesceTestLen20, coalesceTestTimeA)

	tl.MergeAdjacentSameValue()
	tl.Validate()

	lenBefore := tl.Len()

	// Replace should work correctly on the coalesced tree.
	reports := tl.Replace(coalesceTestReplacePos, coalesceTestReplaceDel, coalesceTestReplaceIns, coalesceTestReplaceTime)

	tl.Validate()

	expectedLen := lenBefore + coalesceTestReplaceIns - coalesceTestReplaceDel

	if tl.Len() != expectedLen {
		t.Errorf("Len after Replace: got %d, want %d", tl.Len(), expectedLen)
	}

	// Reports should contain delta for deleted lines.
	if len(reports) == 0 {
		t.Error("expected non-empty reports from Replace after coalesce")
	}
}

// TestMergeAdjacentSameValue_Idempotent verifies that calling MergeAdjacentSameValue twice is a no-op.
func TestMergeAdjacentSameValue_Idempotent(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, coalesceTestLen1000)
	tl.Replace(coalesceTestReplacePos, 0, coalesceTestLen10, coalesceTestTimeA)
	tl.Replace(coalesceTestReplacePos+coalesceTestLen10, 0, coalesceTestLen20, coalesceTestTimeA)

	tl.MergeAdjacentSameValue()

	nodesAfterFirst := tl.Nodes()
	flatAfterFirst := tl.Flatten()

	tl.MergeAdjacentSameValue()

	nodesAfterSecond := tl.Nodes()
	flatAfterSecond := tl.Flatten()

	if nodesAfterSecond != nodesAfterFirst {
		t.Errorf("second coalesce changed node count: %d -> %d", nodesAfterFirst, nodesAfterSecond)
	}

	for i := range flatAfterFirst {
		if flatAfterFirst[i] != flatAfterSecond[i] {
			t.Errorf("Flatten[%d] changed on second coalesce: %d -> %d", i, flatAfterFirst[i], flatAfterSecond[i])

			break
		}
	}
}

// TestMergeAdjacentSameValue_AllSameValue verifies heavy fragmentation collapses maximally.
func TestMergeAdjacentSameValue_AllSameValue(t *testing.T) {
	t.Parallel()

	// Build a treap where all lines have the same time value by inserting many small segments.
	tl := NewTreapTimeline(coalesceTestTimeA, coalesceTestLen10)

	for range coalesceTestHeavyNodes - 1 {
		tl.Replace(0, 0, coalesceTestLen10, coalesceTestTimeA)
	}

	nodesBefore := tl.Nodes()

	if nodesBefore < coalesceTestHeavyNodes/2 {
		t.Fatalf("expected more fragmentation: only %d nodes for %d insertions", nodesBefore, coalesceTestHeavyNodes)
	}

	tl.MergeAdjacentSameValue()
	tl.Validate()

	nodesAfter := tl.Nodes()

	// Should be exactly 2 nodes: one data segment + TreeEnd.
	expectedNodes := 2

	if nodesAfter != expectedNodes {
		t.Errorf("expected %d nodes after coalescing all-same-value, got %d", expectedNodes, nodesAfter)
	}

	// Verify total length is preserved.
	expectedLen := coalesceTestHeavyNodes * coalesceTestLen10

	if tl.Len() != expectedLen {
		t.Errorf("Len: got %d, want %d", tl.Len(), expectedLen)
	}
}
