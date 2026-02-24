package burndown

// CommitResult carries the per-commit burndown deltas emitted as TC.Data.
// Each field contains the changes produced by a single commit's diff application.
type CommitResult struct {
	// GlobalDeltas: curTick -> prevTick -> lineCountDelta.
	GlobalDeltas sparseHistory

	// PeopleDeltas: authorID -> curTick -> prevTick -> lineCountDelta.
	PeopleDeltas map[int]sparseHistory

	// MatrixDeltas: indexed by oldAuthor, maps newAuthor -> lineCountDelta.
	MatrixDeltas []map[int]int64

	// FileDeltas: pathID -> curTick -> prevTick -> lineCountDelta.
	FileDeltas map[PathID]sparseHistory

	// FileOwnership: pathID -> authorID -> surviving line count.
	// Computed from live file segments (not sparse history) when both
	// TrackFiles and PeopleNumber > 0. This is a snapshot of current
	// state, not a delta — each commit's snapshot replaces the previous.
	FileOwnership map[PathID]map[int]int
}

// TickResult holds the aggregated burndown state for a single tick,
// stored as TICK.Data by the aggregator.
type TickResult struct {
	GlobalHistory   sparseHistory
	PeopleHistories []sparseHistory
	Matrix          []map[int]int64
	FileHistories   map[PathID]sparseHistory
	FileOwnership   map[PathID]map[int]int
}

// deltaBuffer holds per-commit delta accumulation for a single shard.
// Reset at the start of each Consume() call.
type deltaBuffer struct {
	globalDeltas sparseHistory
	peopleDeltas map[int]sparseHistory
	matrixDeltas []map[int]int64
	fileDeltas   map[PathID]sparseHistory
}
