// Package filehistory provides file history functionality.
package filehistory

import (
	"context"
	"io"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// HistoryAnalyzer tracks file-level change history across commits.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	// Dependencies.
	Identity  *plumbing.IdentityDetector
	TreeDiff  *plumbing.TreeDiffAnalyzer
	LineStats *plumbing.LinesStatsCalculator

	// State.
	files          map[string]*FileHistory
	lastCommitHash gitlib.Hash
	repo           *gitlib.Repository
	merges         *analyze.MergeTracker
}

// FileHistory holds the change history for a single file.
type FileHistory struct {
	People map[int]pkgplumbing.LineStats
	Hashes []gitlib.Hash
}

// NewAnalyzer creates a new file history analyzer.
func NewAnalyzer() *HistoryAnalyzer {
	ha := &HistoryAnalyzer{
		Identity:  &plumbing.IdentityDetector{},
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		LineStats: &plumbing.LinesStatsCalculator{},
		files:     make(map[string]*FileHistory),
		merges:    analyze.NewMergeTracker(),
	}

	ha.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: ComputeAllMetrics,
		TicksToReportFn: func(ctx context.Context, t []analyze.TICK) analyze.Report {
			return TicksToReport(ctx, t, ha.repo)
		},
	}

	return ha
}

// Name returns the name of the analyzer.
func (h *HistoryAnalyzer) Name() string {
	return "FileHistoryAnalysis"
}

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string {
	return "file-history"
}

// Description returns a human-readable description of the analyzer.
func (h *HistoryAnalyzer) Description() string {
	return h.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (h *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID: "history/file-history",
		Description: "Each file path is mapped to the list of commits which touch that file " +
			"and the mapping from involved developers to the corresponding line statistics.",
		Mode: analyze.ModeHistory,
	}
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (h *HistoryAnalyzer) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (h *HistoryAnalyzer) Initialize(repo *gitlib.Repository) error {
	h.files = map[string]*FileHistory{}
	h.merges = analyze.NewMergeTracker()
	h.repo = repo

	return nil
}

// shouldConsumeCommit checks whether a commit should be processed.
// It returns false for duplicate merge commits and non-merge context merges.
func (h *HistoryAnalyzer) shouldConsumeCommit(ctx *analyze.Context) bool {
	commit := ctx.Commit

	if commit.NumParents() > 1 {
		if h.merges.SeenOrAdd(commit.Hash()) {
			return false
		}
	}

	return !ctx.IsMerge
}

// buildCommitData produces the TC payload from plumbing state without mutating h.files.
func (h *HistoryAnalyzer) buildCommitData(changes gitlib.Changes, commit analyze.CommitLike, author int) *CommitData {
	data := &CommitData{}

	router := &plumbing.ChangeRouter{
		OnInsert: func(change *gitlib.Change) error {
			data.PathActions = append(data.PathActions, PathAction{
				Path:       change.To.Name,
				Action:     gitlib.Insert,
				CommitHash: commit.Hash(),
			})

			return nil
		},
		OnDelete: func(change *gitlib.Change) error {
			data.PathActions = append(data.PathActions, PathAction{
				Path:       change.From.Name,
				Action:     gitlib.Delete,
				CommitHash: commit.Hash(),
			})

			return nil
		},
		OnModify: func(change *gitlib.Change) error {
			data.PathActions = append(data.PathActions, PathAction{
				Path:       change.To.Name,
				Action:     gitlib.Modify,
				CommitHash: commit.Hash(),
			})

			return nil
		},
		OnRename: func(from, to string, _ *gitlib.Change) error {
			data.PathActions = append(data.PathActions, PathAction{
				FromPath:   from,
				ToPath:     to,
				Action:     gitlib.Modify,
				CommitHash: commit.Hash(),
			})

			return nil
		},
	}

	_ = router.Route(changes) //nolint:errcheck // errors are always nil from our handlers.

	for changeEntry, stats := range h.LineStats.LineStats {
		data.LineStatUpdates = append(data.LineStatUpdates, LineStatUpdate{
			Path:     changeEntry.Name,
			AuthorID: author,
			Stats:    stats,
		})
	}

	return data
}

// processFileChanges updates file histories based on the tree diff changes for the given commit.
func (h *HistoryAnalyzer) processFileChanges(changes gitlib.Changes, commit analyze.CommitLike) {
	router := &plumbing.ChangeRouter{
		OnInsert: func(change *gitlib.Change) error {
			h.processAction(change, commit)

			return nil
		},
		OnDelete: func(change *gitlib.Change) error {
			h.processAction(change, commit)

			return nil
		},
		OnModify: func(change *gitlib.Change) error {
			h.processAction(change, commit)

			return nil
		},
		OnRename: func(from, to string, _ *gitlib.Change) error {
			fh := h.getOrCreateFileHistory(from)
			if oldFH, ok := h.files[from]; ok {
				delete(h.files, from)
				h.files[to] = oldFH
				fh = oldFH
			}

			fh.Hashes = append(fh.Hashes, commit.Hash())

			return nil
		},
	}

	_ = router.Route(changes) //nolint:errcheck // errors are always nil from our handlers.
}

// processAction handles Insert, Delete, and simple Modify actions.
func (h *HistoryAnalyzer) processAction(change *gitlib.Change, commit analyze.CommitLike) {
	name := change.To.Name
	if change.Action == gitlib.Delete {
		name = change.From.Name
	}

	fh := h.getOrCreateFileHistory(name)

	if change.Action == gitlib.Insert {
		fh.Hashes = []gitlib.Hash{commit.Hash()}
	} else {
		fh.Hashes = append(fh.Hashes, commit.Hash())
	}
}

// getOrCreateFileHistory retrieves or creates a FileHistory for the given name.
func (h *HistoryAnalyzer) getOrCreateFileHistory(name string) *FileHistory {
	fh := h.files[name]
	if fh == nil {
		fh = &FileHistory{}
		h.files[name] = fh
	}

	return fh
}

// aggregateLineStats merges line statistics from the current commit into file histories.
func (h *HistoryAnalyzer) aggregateLineStats(lineStats map[gitlib.ChangeEntry]pkgplumbing.LineStats, author int) {
	for changeEntry, stats := range lineStats {
		file := h.files[changeEntry.Name]
		if file == nil {
			file = &FileHistory{}
			h.files[changeEntry.Name] = file
		}

		people := file.People
		if people == nil {
			people = map[int]pkgplumbing.LineStats{}
			file.People = people
		}

		oldStats := people[author]
		people[author] = pkgplumbing.LineStats{
			Added:   oldStats.Added + stats.Added,
			Removed: oldStats.Removed + stats.Removed,
			Changed: oldStats.Changed + stats.Changed,
		}
	}
}

// Consume processes a single commit with the provided dependency results.
// Emits a TC with CommitData for the aggregator. Also maintains local state
// for Fork/Merge parallel path (workers merge state; main uses aggregator).
func (h *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	if !h.shouldConsumeCommit(ac) {
		return analyze.TC{}, nil
	}

	if ac.Commit != nil {
		h.lastCommitHash = ac.Commit.Hash()
	}

	h.processFileChanges(h.TreeDiff.Changes, ac.Commit)
	h.aggregateLineStats(h.LineStats.LineStats, h.Identity.AuthorID)

	data := h.buildCommitData(h.TreeDiff.Changes, ac.Commit, h.Identity.AuthorID)

	return analyze.TC{
		CommitHash: ac.Commit.Hash(),
		Data:       data,
	}, nil
}

// SequentialOnly returns false because file history analysis can be parallelized.
func (h *HistoryAnalyzer) SequentialOnly() bool { return false }

// CPUHeavy returns false because file history tracking is lightweight bookkeeping.
func (h *HistoryAnalyzer) CPUHeavy() bool { return false }

// SnapshotPlumbing captures the current plumbing output state for one commit.
func (h *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   h.TreeDiff.Changes,
		LineStats: h.LineStats.LineStats,
		AuthorID:  h.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.TreeDiff.Changes = snapshot.Changes
	h.LineStats.LineStats = snapshot.LineStats
	h.Identity.AuthorID = snapshot.AuthorID
}

// ReleaseSnapshot releases any resources owned by the snapshot.
func (h *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets its own independent copies of mutable state.
func (h *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := NewAnalyzer()
		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (h *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		h.mergeFiles(other.files)
		// Merge trackers are not combined: each fork processes a disjoint
		// subset of commits, so merge dedup state stays independent.

		// Keep the latest lastCommitHash so Finalize can filter deleted files.
		if !other.lastCommitHash.IsZero() {
			h.lastCommitHash = other.lastCommitHash
		}
	}
}

// mergeFiles combines file histories from another analyzer.
func (h *HistoryAnalyzer) mergeFiles(other map[string]*FileHistory) {
	for name, otherFH := range other {
		if h.files[name] == nil {
			h.files[name] = &FileHistory{
				People: make(map[int]pkgplumbing.LineStats),
			}
		}

		fh := h.files[name]

		// Initialize People map if needed.
		if fh.People == nil {
			fh.People = make(map[int]pkgplumbing.LineStats)
		}

		// Merge people stats (sum).
		for person, stats := range otherFH.People {
			existing := fh.People[person]
			fh.People[person] = pkgplumbing.LineStats{
				Added:   existing.Added + stats.Added,
				Removed: existing.Removed + stats.Removed,
				Changed: existing.Changed + stats.Changed,
			}
		}

		// Append hashes.
		fh.Hashes = append(fh.Hashes, otherFH.Hashes...)
	}
}

// Serialize writes the analysis result to the given writer.
func (h *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatPlot {
		return h.generatePlot(result, writer)
	}

	if h.BaseHistoryAnalyzer != nil {
		return h.BaseHistoryAnalyzer.Serialize(result, format, writer)
	}

	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: ComputeAllMetrics,
	}).Serialize(result, format, writer)
}

// SerializeTICKs delegates to BaseHistoryAnalyzer for JSON/YAML/binary; FormatPlot uses ReportFromTICKs and generatePlot.
func (h *HistoryAnalyzer) SerializeTICKs(ticks []analyze.TICK, format string, writer io.Writer) error {
	if format == analyze.FormatPlot {
		report, err := h.ReportFromTICKs(context.Background(), ticks)
		if err != nil {
			return err
		}

		return h.generatePlot(report, writer)
	}

	if h.BaseHistoryAnalyzer != nil {
		return h.BaseHistoryAnalyzer.SerializeTICKs(ticks, format, writer)
	}

	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: ComputeAllMetrics,
		TicksToReportFn: func(ctx context.Context, t []analyze.TICK) analyze.Report {
			report, err := h.ReportFromTICKs(ctx, t)
			if err != nil {
				return nil
			}

			return report
		},
	}).SerializeTICKs(ticks, format, writer)
}

// FormatReport writes the formatted analysis report to the given writer.
func (h *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, analyze.FormatYAML, writer)
}

// NewAggregator creates an aggregator for this analyzer.
func (h *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return NewAggregator(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (h *HistoryAnalyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return TicksToReport(ctx, ticks, h.repo), nil
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It extracts per-commit file change summary data for the unified timeseries output.
func (h *HistoryAnalyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitStats, ok := report["commit_stats"].(map[string]*FileHistoryCommitSummary)
	if !ok || len(commitStats) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitStats))

	for hash, cs := range commitStats {
		result[hash] = map[string]any{
			"files_touched": cs.FilesTouched,
			"lines_added":   cs.LinesAdded,
			"lines_removed": cs.LinesRemoved,
			"lines_changed": cs.LinesChanged,
			"inserts":       cs.Inserts,
			"deletes":       cs.Deletes,
			"modifies":      cs.Modifies,
		}
	}

	return result
}
