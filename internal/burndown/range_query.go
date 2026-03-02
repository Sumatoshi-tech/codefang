package burndown

import "github.com/Sumatoshi-tech/codefang/pkg/alg/interval"

// OwnershipSegment represents a line range with a single owner (time value).
type OwnershipSegment struct {
	StartLine int
	EndLine   int
	Owner     int
}

// rangeIndex holds the lazy interval tree index for range queries.
type rangeIndex struct {
	tree  *interval.Tree[uint32, uint32]
	dirty bool
}

// QueryRange returns all ownership segments that overlap the line range [startLine, endLine).
// The interval tree index is rebuilt lazily when the timeline has been modified.
// TreeEnd sentinel segments are excluded from results.
func (file *File) QueryRange(startLine, endLine int) []OwnershipSegment {
	file.ensureIndex()

	if file.index == nil || file.index.tree.Len() == 0 {
		return nil
	}

	intervals := file.index.tree.QueryOverlap(uint32(startLine), uint32(endLine-1))

	results := make([]OwnershipSegment, 0, len(intervals))
	for _, iv := range intervals {
		results = append(results, OwnershipSegment{
			StartLine: int(iv.Low),
			EndLine:   int(iv.High) + 1,
			Owner:     int(iv.Value),
		})
	}

	return results
}

// InvalidateIndex marks the interval tree index as needing a rebuild.
// Called automatically by Update.
func (file *File) InvalidateIndex() {
	if file.index != nil {
		file.index.dirty = true
	}
}

// ensureIndex rebuilds the interval tree index if it is dirty or uninitialized.
func (file *File) ensureIndex() {
	if file.index == nil {
		file.index = &rangeIndex{tree: interval.New[uint32, uint32](), dirty: true}
	}

	if !file.index.dirty {
		return
	}

	file.rebuildIndex()
}

// rebuildIndex reconstructs the interval tree from the current timeline segments.
func (file *File) rebuildIndex() {
	file.index.tree.Clear()

	file.timeline.Iterate(func(offset, length int, t TimeKey) bool {
		// Skip TreeEnd sentinel and zero-length segments.
		if t == TreeEnd || length <= 0 {
			return true
		}

		low := uint32(offset)
		high := uint32(offset + length - 1)

		file.index.tree.Insert(low, high, t)

		return true
	})

	file.index.dirty = false
}
