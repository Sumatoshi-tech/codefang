// Package burndown provides file-level line interval tracking for burndown analysis.
package burndown

import (
	"fmt"
	"math"
)

// Updater is the function which is called back on File.Update().
type Updater = func(currentTime, previousTime, delta int)

// File encapsulates a Timeline (line-interval storage) and cumulative length counters via updaters.
// Users should call NewFile(); Len() returns line count; Update() mutates via the timeline and updaters.
// Dump() writes the tree to a string and Validate() checks integrity.
type File struct {
	timeline Timeline
	updaters []Updater
	index    *rangeIndex
}

// TreeEnd denotes the value of the last leaf in the tree.
const TreeEnd = math.MaxUint32

// TreeMaxBinPower is the binary power value which corresponds to the maximum tick which
// can be stored in the tree.
const TreeMaxBinPower = 14

// TreeMergeMark is the special day which disables the status updates and is used in File.Merge().
const TreeMergeMark = (1 << TreeMaxBinPower) - 1

// NewFile initializes a new instance of File struct using the default treap timeline.
//
// The time parameter is the starting value of the first node.
// The length parameter is the starting length of the tree (the key of the second and the last node).
// The updaters parameter lists the attached interval length mappings.
func NewFile(time, length int, updaters ...Updater) *File {
	if time < 0 || time > math.MaxUint32 {
		panic(fmt.Sprintf("time is out of allowed range: %d", time))
	}

	if length > math.MaxUint32 {
		panic(fmt.Sprintf("length is out of allowed range: %d", length))
	}

	timeline := NewTreapTimeline(time, length)

	file := &File{timeline: timeline, updaters: updaters}
	if length > 0 {
		file.updateTime(time, time, length)
	}

	return file
}

// NewFileWithTimeline creates a File with the given timeline (e.g. NewTreapTimeline).
// Used for tests and when using a timeline that does not require an allocator.
func NewFileWithTimeline(timeline Timeline, updaters ...Updater) *File {
	return &File{timeline: timeline, updaters: updaters}
}

// NewFileFromSegments creates a File from serialized segments without triggering updaters.
func NewFileFromSegments(segs []Segment, updaters ...Updater) *File {
	timeline := &treapTimeline{}
	timeline.ReconstructFromSegments(segs)

	return &File{timeline: timeline, updaters: updaters}
}

func (file *File) updateTime(currentTime, previousTime, delta int) {
	if previousTime&TreeMergeMark == TreeMergeMark {
		if currentTime == previousTime {
			return
		}

		panic("previousTime cannot be TreeMergeMark")
	}

	if currentTime&TreeMergeMark == TreeMergeMark {
		// Merge mode - we have already updated in one of the branches.
		return
	}

	for _, update := range file.updaters {
		update(currentTime, previousTime, delta)
	}
}

// CloneShallow copies the file (shallow copy of the timeline).
func (file *File) CloneShallow() *File {
	return &File{timeline: file.timeline.CloneShallow(), updaters: file.updaters}
}

// CloneDeep copies the file (deep copy of the timeline).
func (file *File) CloneDeep() *File {
	return &File{timeline: file.timeline.CloneDeep(), updaters: file.updaters}
}

// Delete deallocates the file.
func (file *File) Delete() {
	file.timeline.Erase()
}

// ShrinkPool trims the timeline's internal node pool to retain at most keep
// free nodes. Call between chunks to release excess pool memory to the GC.
func (file *File) ShrinkPool(keep int) {
	if tt, ok := file.timeline.(*treapTimeline); ok {
		tt.ShrinkPool(keep)
	}
}

// ReplaceUpdaters replaces the file's updaters with a new set.
func (file *File) ReplaceUpdaters(updaters []Updater) {
	file.updaters = updaters
}

// Segments returns the file's timeline segments as a compact slice.
func (file *File) Segments() []Segment {
	return file.timeline.Segments()
}

// ReconstructFromSegments rebuilds the file's timeline from a compact segment slice.
func (file *File) ReconstructFromSegments(segs []Segment) {
	file.timeline.ReconstructFromSegments(segs)
}

// Len returns the number of lines in the file.
func (file *File) Len() int {
	return file.timeline.Len()
}

// Nodes returns the number of segments/nodes in the file.
func (file *File) Nodes() int {
	return file.timeline.Nodes()
}

// Update modifies the timeline to reflect line changes and notifies updaters (deletions and insertions).
func (file *File) Update(time, pos, insLength, delLength int) {
	if time < 0 {
		panic("time may not be negative")
	}

	if time >= math.MaxUint32 {
		panic("time may not be >= MaxUint32")
	}

	if pos < 0 {
		panic("attempt to insert/delete at a negative position")
	}

	if pos > math.MaxUint32 {
		panic("pos may not be > MaxUint32")
	}

	if insLength < 0 || delLength < 0 {
		panic("insLength and delLength must be non-negative")
	}

	if insLength|delLength == 0 {
		return
	}

	if insLength > 0 {
		file.updateTime(time, time, insLength)
	}

	reports := file.timeline.Replace(pos, delLength, insLength, TimeKey(time))
	for _, d := range reports {
		file.updateTime(d.Current, d.Previous, d.Delta)
	}

	file.InvalidateIndex()
}

// MergeAdjacentSameValue coalesces consecutive segments with the same time (reduces node count).
func (file *File) MergeAdjacentSameValue() {
	file.timeline.MergeAdjacentSameValue()
}

// isMergeMarked checks if a line value has the merge mark bit set.
func isMergeMarked(value int) bool {
	return value&TreeMergeMark == TreeMergeMark
}

// Merge combines several prepared File-s together.
func (file *File) Merge(day int, others ...*File) {
	myself := file.timeline.Flatten()
	mergeOtherFiles(myself, others)
	file.resolveMergeConflicts(myself, day)
	file.timeline.Reconstruct(myself)
}

func mergeOtherFiles(myself []int, others []*File) {
	for _, other := range others {
		if other == nil {
			panic("merging with a nil file")
		}

		lines := other.timeline.Flatten()

		if len(myself) != len(lines) {
			panic(fmt.Sprintf("file corruption, lines number mismatch during merge %d != %d",
				len(myself), len(lines)))
		}

		for i, myLine := range myself {
			otherLine := lines[i]

			if isMergeMarked(otherLine) {
				continue
			}

			if isMergeMarked(myLine) || myLine&TreeMergeMark > otherLine&TreeMergeMark {
				myself[i] = otherLine
			}
		}
	}
}

func (file *File) resolveMergeConflicts(lines []int, day int) {
	for i, l := range lines {
		if isMergeMarked(l) {
			lines[i] = day
			file.updateTime(day, day, 1)
		}
	}
}

// Dump formats the underlying line interval tree into a string.
// Useful for error messages, panic()-s and debugging.
func (file *File) Dump() string {
	buffer := ""

	file.ForEach(func(line, value int) {
		buffer += fmt.Sprintf("%d %d\n", line, value)
	})

	return buffer
}

// Validate checks the timeline integrity (starts at 0, ends with TreeEnd, no duplicates/merge marks).
func (file *File) Validate() {
	file.timeline.Validate()
}

// ForEach visits each segment start in the timeline in order (line, value); value is -1 for TreeEnd.
func (file *File) ForEach(callback func(line, value int)) {
	file.timeline.Iterate(func(offset, _ int, t TimeKey) bool {
		v := int(t)
		if t == TreeEnd {
			v = -1
		}

		callback(offset, v)

		return true
	})
}
