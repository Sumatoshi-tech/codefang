package analyze

import (
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// TC is a per-commit result emitted by a HistoryAnalyzer.
// Each Consume() call produces one TC representing the analyzer's
// output for that commit. Data holds an analyzer-specific payload;
// concrete types are documented per-analyzer.
type TC struct {
	// CommitHash identifies the analyzed commit.
	CommitHash gitlib.Hash

	// Tick is the time-bucket index this commit belongs to.
	Tick int

	// AuthorID is the numeric identity of the commit author.
	AuthorID int

	// Timestamp is the commit's author time.
	Timestamp time.Time

	// Data carries the analyzer-specific per-commit payload.
	// The concrete type is defined by each analyzer.
	Data any
}

// TICK is an aggregated tick-level result produced by an Aggregator.
// It represents the merged output of all TCs within one time bucket.
// Data holds an analyzer-specific aggregated payload.
type TICK struct {
	// Tick is the time-bucket index.
	Tick int

	// StartTime is the earliest commit timestamp in this tick.
	StartTime time.Time

	// EndTime is the latest commit timestamp in this tick.
	EndTime time.Time

	// Data carries the analyzer-specific aggregated payload.
	// The concrete type is defined by each aggregator.
	Data any
}
