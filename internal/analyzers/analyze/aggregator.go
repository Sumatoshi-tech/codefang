package analyze

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// AggregatorFunc is the factory signature for creating an Aggregator
// from options. Concrete analyzers provide this via their registration.
type AggregatorFunc func(opts AggregatorOptions) (Aggregator, error)

// Aggregator transforms a stream of TCs into TICKs.
// It is driven by a single goroutine; callers must serialize calls.
//
// Lifecycle: Add() repeatedly → FlushTick() per tick boundary →
// Spill()/Collect() as needed → Close().
//
// Close is idempotent and safe to call after errors.
type Aggregator interface {
	// Add ingests a single per-commit result.
	Add(tc TC) error

	// FlushTick finalizes and returns the aggregated result for the
	// given tick. Precondition: tick >= 0.
	FlushTick(tick int) (TICK, error)

	// FlushAllTicks returns TICKs for all ticks that have accumulated data.
	// For per-tick aggregators (byTick map), ticks are sorted ascending.
	// For cumulative aggregators, returns a single TICK with all state.
	// Returns nil, nil when the aggregator has no data.
	FlushAllTicks() ([]TICK, error)

	// Spill writes accumulated state to disk to free memory.
	// Returns the number of bytes freed. A SpillBudget of 0 in
	// AggregatorOptions means no spill limit (keep everything in memory).
	Spill() (int64, error)

	// Collect reloads previously spilled state back into memory.
	Collect() error

	// EstimatedStateSize returns the current in-memory footprint
	// of the aggregator's accumulated state in bytes.
	EstimatedStateSize() int64

	// SpillState returns the current on-disk spill state for checkpoint persistence.
	SpillState() AggregatorSpillInfo

	// RestoreSpillState points the aggregator at a previously-saved spill directory.
	// Called on checkpoint resume before any Add() calls.
	RestoreSpillState(info AggregatorSpillInfo)

	// Close releases all resources. Idempotent.
	Close() error
}

// CommitStatsDrainer allows extracting and clearing per-commit data
// between chunks during streaming timeseries NDJSON output.
// Aggregators that store per-commit summary data implement this to
// enable per-chunk flushing and prevent OOM on large repos.
type CommitStatsDrainer interface {
	// DrainCommitStats returns per-commit summary data and per-tick commit
	// ordering, then clears these maps from the aggregator. Cumulative state
	// (coupling matrices, burndown histories, etc.) remains intact.
	//
	// The returned commitData maps commit hash (hex) to a JSON-serializable
	// summary (same shape as ExtractCommitTimeSeries output for this analyzer).
	DrainCommitStats() (commitData map[string]any, commitsByTick map[int][]gitlib.Hash)
}

// AggregatorSpillInfo describes the on-disk spill state of an Aggregator.
// Used by the checkpoint system to save and restore spill directories.
type AggregatorSpillInfo struct {
	// Dir is the directory containing spill files. Empty if no spills occurred.
	Dir string `json:"dir,omitempty"`

	// Count is the number of spill files written.
	Count int `json:"count,omitempty"`
}

// ConfigTmpDir is the facts key for the global temporary directory override.
// When set, analyzers should use this directory for spill and hibernation files
// instead of [os.TempDir].
const ConfigTmpDir = "TmpDir"

// AggregatorOptions configures an Aggregator instance.
// A zero-value AggregatorOptions is valid and means:
// no spill limit, no spill directory, no sampling, default granularity.
type AggregatorOptions struct {
	// SpillBudget is the maximum bytes of aggregator state to keep
	// in memory before spilling to disk. Zero means no limit.
	SpillBudget int64

	// SpillDir is the directory for spill files. Empty means the
	// system default temporary directory.
	SpillDir string

	// Sampling is the commit sampling rate. Zero means no sampling
	// (process every commit).
	Sampling int

	// Granularity is the tick granularity in hours. Zero means the
	// pipeline default.
	Granularity int
}
