// Package burndown: Timeline interface for line-interval storage (Track A refactor).

package burndown

// TimeKey is the time (tick) associated with a line interval. Same semantics as tree Value.
type TimeKey = uint32

// DeltaReport is a single (currentTime, previousTime, delta) for updater callbacks.
type DeltaReport struct {
	Current  int
	Previous int
	Delta    int
}

// Timeline stores line intervals by (implicit or explicit) position and supports
// Replace without O(N) key shifting. Default implementation: implicit treap.
type Timeline interface {
	// Replace applies delete [pos, pos+delLines) then insert insLines at pos with time t.
	// Returns delta reports for the caller to apply to updaters (e.g. from deleted intervals).
	Replace(pos, delLines, insLines int, t TimeKey) []DeltaReport
	// Iterate calls fn(offset, length, timeKey) for each segment in order; return false to stop.
	Iterate(fn func(offset int, length int, t TimeKey) bool)
	// Len returns total line count (file length).
	Len() int
	// Nodes returns the number of segments/nodes (for diagnostics).
	Nodes() int
	// Validate panics if invariants are violated.
	Validate()
	// CloneShallow returns a shallow copy of the timeline.
	CloneShallow() *treapTimeline
	// CloneDeep returns a deep copy of the timeline.
	CloneDeep() *treapTimeline
	// Erase clears all nodes (for Delete).
	Erase()
	// Flatten returns line→time as a slice (for Merge).
	Flatten() []int
	// Reconstruct rebuilds from line→time slice (for Merge).
	Reconstruct(lines []int)
	// MergeAdjacentSameValue coalesces consecutive segments with the same time (reduces node count).
	// No-op for implementations that do not benefit (e.g. implicit treap).
	MergeAdjacentSameValue()
	// Segments returns the treap's segments as a compact slice (excludes TreeEnd sentinel).
	Segments() []Segment
	// ReconstructFromSegments rebuilds from a compact segment slice (inverse of Segments).
	ReconstructFromSegments(segs []Segment)
}
