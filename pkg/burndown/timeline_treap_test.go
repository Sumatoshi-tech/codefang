package burndown

import (
	"testing"
)

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
