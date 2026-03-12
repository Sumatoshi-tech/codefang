package burndown

import (
	"testing"
)

// NewFileWithTimeline creates a File with the given timeline.
func NewFileWithTimeline(timeline Timeline, updaters ...Updater) *File {
	return &File{timeline: timeline, updaters: updaters}
}

// TestTreapTimeline_ReplaceAndIterate verifies Replace/Iterate/Len/Nodes/Validate
// for the implicit treap timeline (same semantics as rbtree).
func TestTreapTimeline_ReplaceAndIterate(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, 1000)

	if tl.Len() != 1000 {
		t.Fatalf("Len: got %d want 1000", tl.Len())
	}

	tl.Validate()

	// Same sequence as TestMergeAdjacentSameValue: insert at 0, then at 50 and 60 with time 1.
	tl.Replace(0, 0, 100, 0)

	if tl.Len() != 1100 {
		t.Fatalf("after insert 100 at 0: Len got %d want 1100", tl.Len())
	}

	tl.Replace(50, 0, 10, 1)
	tl.Replace(60, 0, 40, 1)
	tl.Validate()

	valueAt := func(line int) int {
		var last int

		tl.Iterate(func(offset, length int, t TimeKey) bool {
			if offset <= line && line < offset+length {
				last = int(t)

				return false
			}

			return true
		})

		return last
	}
	if got := valueAt(50); got != 1 {
		t.Errorf("value at 50: got %d want 1", got)
	}

	if got := valueAt(55); got != 1 {
		t.Errorf("value at 55: got %d want 1", got)
	}

	if got := valueAt(60); got != 1 {
		t.Errorf("value at 60: got %d want 1", got)
	}
}

// TestSegments_RoundTrip verifies that Segments → ReconstructFromSegments preserves the timeline.
func TestSegments_RoundTrip(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, 1000)
	tl.Replace(100, 0, 50, 1)  // Insert 50 lines at pos 100 with time 1.
	tl.Replace(200, 30, 0, 2)  // Delete 30 lines at pos 200 with time 2.
	tl.Replace(500, 0, 100, 3) // Insert 100 lines at pos 500 with time 3.
	tl.Validate()

	originalFlat := tl.Flatten()
	originalLen := tl.Len()

	segs := tl.Segments()
	if len(segs) == 0 {
		t.Fatal("expected non-empty segments")
	}

	// Verify segments don't include TreeEnd.
	for _, s := range segs {
		if s.Value == TreeEnd {
			t.Error("segments should not include TreeEnd sentinel")
		}
	}

	// Reconstruct from segments.
	tl2 := &treapTimeline{}
	tl2.ReconstructFromSegments(segs)
	tl2.Validate()

	if tl2.Len() != originalLen {
		t.Errorf("Len mismatch: got %d, want %d", tl2.Len(), originalLen)
	}

	reconstructedFlat := tl2.Flatten()
	if len(reconstructedFlat) != len(originalFlat) {
		t.Fatalf("Flatten length mismatch: got %d, want %d", len(reconstructedFlat), len(originalFlat))
	}

	for i := range originalFlat {
		if originalFlat[i] != reconstructedFlat[i] {
			t.Errorf("Flatten[%d] mismatch: got %d, want %d", i, reconstructedFlat[i], originalFlat[i])

			break
		}
	}
}

// TestSegments_Empty verifies that an empty treap produces no segments.
func TestSegments_Empty(t *testing.T) {
	t.Parallel()

	tl := &treapTimeline{}

	segs := tl.Segments()
	if len(segs) != 0 {
		t.Errorf("expected 0 segments, got %d", len(segs))
	}

	// ReconstructFromSegments with empty should work.
	tl2 := &treapTimeline{}
	tl2.ReconstructFromSegments(nil)

	if tl2.Len() != 0 {
		t.Errorf("expected Len 0, got %d", tl2.Len())
	}
}

// TestSegments_SingleSegment verifies a single-segment file round-trips.
func TestSegments_SingleSegment(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(5, 42)

	segs := tl.Segments()
	if len(segs) != 1 {
		t.Fatalf("expected 1 segment, got %d", len(segs))
	}

	if segs[0].Length != 42 || segs[0].Value != 5 {
		t.Errorf("segment: got {%d, %d}, want {42, 5}", segs[0].Length, segs[0].Value)
	}

	tl2 := &treapTimeline{}
	tl2.ReconstructFromSegments(segs)
	tl2.Validate()

	if tl2.Len() != 42 {
		t.Errorf("Len: got %d, want 42", tl2.Len())
	}
}

// TestNewFileFromSegments verifies creating a File from segments.
func TestNewFileFromSegments(t *testing.T) {
	t.Parallel()

	// Create a file, modify it, extract segments, reconstruct.
	original := NewFile(1, 100)
	original.Update(2, 50, 20, 10) // Delete 10 at 50, insert 20 at 50 with time 2.

	segs := original.Segments()
	restored := NewFileFromSegments(segs)

	if restored.Len() != original.Len() {
		t.Errorf("Len mismatch: got %d, want %d", restored.Len(), original.Len())
	}

	// Compare Flatten outputs via ForEach.
	var origEntries, restoredEntries []struct{ line, value int }

	original.ForEach(func(line, value int) {
		origEntries = append(origEntries, struct{ line, value int }{line, value})
	})

	restored.ForEach(func(line, value int) {
		restoredEntries = append(restoredEntries, struct{ line, value int }{line, value})
	})

	if len(origEntries) != len(restoredEntries) {
		t.Fatalf("entry count mismatch: got %d, want %d", len(restoredEntries), len(origEntries))
	}

	for i := range origEntries {
		if origEntries[i] != restoredEntries[i] {
			t.Errorf("entry[%d] mismatch: got %+v, want %+v", i, restoredEntries[i], origEntries[i])

			break
		}
	}
}

// TestTreapTimeline_FileWithTimeline runs the same Update sequence as TestMergeAdjacentSameValue
// using NewFileWithTimeline(NewTreapTimeline(...)).
func TestTreapTimeline_FileWithTimeline(t *testing.T) {
	t.Parallel()

	timeline := NewTreapTimeline(0, 1000)
	file := NewFileWithTimeline(timeline)
	file.Update(0, 0, 100, 0)
	file.Update(1, 50, 10, 0)
	file.Update(1, 60, 40, 0)
	file.Validate()

	valueAt := func(line int) int {
		var last int

		file.ForEach(func(l, v int) {
			if l <= line {
				last = v
			}
		})

		return last
	}
	if got := valueAt(50); got != 1 {
		t.Errorf("value at 50: got %d want 1", got)
	}

	if got := valueAt(55); got != 1 {
		t.Errorf("value at 55: got %d want 1", got)
	}
}
