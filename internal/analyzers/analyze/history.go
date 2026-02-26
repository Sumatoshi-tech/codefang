package analyze

import (
	"context"
	"errors"
	"io"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// ErrNotImplemented is returned by stub methods that are not yet implemented.
var ErrNotImplemented = errors.New("not implemented")

// Serialization format constants.
const (
	FormatYAML   = "yaml"
	FormatJSON   = "json"
	FormatBinary = "binary"
	FormatPlot   = "plot"
)

// CommitIdentity provides commit identification methods.
type CommitIdentity interface {
	Hash() gitlib.Hash
	Author() gitlib.Signature
	Committer() gitlib.Signature
	Message() string
}

// CommitParents provides access to parent commits.
type CommitParents interface {
	NumParents() int
	Parent(n int) (*gitlib.Commit, error)
}

// CommitLike is an interface for commit-like objects (real or mock).
// It composes CommitIdentity and CommitParents with tree/file access.
type CommitLike interface {
	CommitIdentity
	CommitParents
	Tree() (*gitlib.Tree, error)
	Files() (*gitlib.FileIter, error)
	File(path string) (*gitlib.File, error)
}

// Context provides information about the current step in the analysis.
type Context struct {
	Time    time.Time
	Commit  CommitLike
	Index   int
	IsMerge bool

	// Pipeline data - populated by the runtime when using the optimized pipeline.
	// These fields are optional and may be nil if using the legacy processing path.

	// Changes contains the tree diff changes for this commit.
	Changes gitlib.Changes

	// BlobCache maps blob hashes to cached blobs.
	// Populated by the runtime pipeline for efficient blob access.
	BlobCache map[gitlib.Hash]*gitlib.CachedBlob

	// FileDiffs maps file paths to diff data for modified files.
	// Populated by the runtime pipeline using native C diff computation.
	FileDiffs map[string]plumbing.FileDiffData

	// UASTChanges contains pre-computed UAST changes for this commit.
	// Populated by the UAST pipeline stage when enabled.
	UASTChanges []uast.Change
}

// HistoryAnalyzer interface defines the contract for history-based analyzers.
type HistoryAnalyzer interface {
	Analyzer

	// Core analysis methods.
	Initialize(repository *gitlib.Repository) error

	// Consumption. Returns a TC with per-commit result data.
	// Plumbing analyzers return zero-value TC (Data: nil).
	Consume(ctx context.Context, ac *Context) (TC, error)

	// Memory sizing for the planner.
	// WorkingStateSize returns the estimated bytes of analyzer-internal
	// working state accumulated per commit (maps, treaps, matrices).
	WorkingStateSize() int64
	// AvgTCSize returns the estimated bytes of TC payload emitted per commit.
	AvgTCSize() int64

	// Aggregation. NewAggregator creates a per-analyzer aggregator that
	// collects TCs into TICKs. Returns nil when no aggregator is available.
	NewAggregator(opts AggregatorOptions) Aggregator
	// SerializeTICKs writes aggregated TICKs in the given format.
	// Returns ErrNotImplemented when not yet wired.
	SerializeTICKs(ticks []TICK, format string, writer io.Writer) error

	// ReportFromTICKs converts aggregated TICKs into a Report.
	// Returns ErrNotImplemented for analyzers without aggregators.
	// ctx is used for cancellation and tracing (e.g. tree.Files() I/O).
	ReportFromTICKs(ctx context.Context, ticks []TICK) (Report, error)

	// Branching support.
	Fork(n int) []HistoryAnalyzer
	Merge(branches []HistoryAnalyzer)

	// Formatting/Serialization.
	// Format can be: "yaml", "json", or "binary" (protobuf).
	Serialize(result Report, format string, writer io.Writer) error
}

// PlumbingSnapshot is an opaque snapshot of plumbing state for one commit.
// The framework treats this as an opaque value; concrete snapshot types
// are defined in the plumbing package.
type PlumbingSnapshot any

// Parallelizable is optionally implemented by leaf analyzers that support
// parallel execution via the framework's Fork/Merge worker pool.
// The framework uses these methods instead of type-switching on concrete types.
type Parallelizable interface {
	// SequentialOnly returns true if this analyzer cannot be parallelized
	// (e.g. it tracks cumulative state across all commits).
	SequentialOnly() bool

	// CPUHeavy returns true if this analyzer's Consume() is CPU-intensive
	// (e.g. UAST processing) and benefits from W parallel workers.
	// Lightweight analyzers return false and run on the main goroutine
	// to avoid fork/merge overhead.
	CPUHeavy() bool

	// SnapshotPlumbing captures the current plumbing output state.
	// Called once per commit after core analyzers have run.
	// The returned value is opaque to the framework.
	SnapshotPlumbing() PlumbingSnapshot

	// ApplySnapshot restores plumbing state from a previously captured snapshot.
	// Called on forked copies before Consume().
	ApplySnapshot(snapshot PlumbingSnapshot)

	// ReleaseSnapshot releases any resources owned by the snapshot
	// (e.g. UAST trees). Called once per snapshot after all leaves
	// in the worker have consumed it.
	ReleaseSnapshot(snapshot PlumbingSnapshot)
}
