// Package filehistory provides file history functionality.
package filehistory

import (
	"context"
	"encoding/json"
	"fmt"
	"io"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// Analyzer tracks file-level change history across commits.
type Analyzer struct {
	// Dependencies.
	Identity  *plumbing.IdentityDetector
	TreeDiff  *plumbing.TreeDiffAnalyzer
	LineStats *plumbing.LinesStatsCalculator

	// State.
	files      map[string]*FileHistory
	lastCommit analyze.CommitLike
	merges     map[gitlib.Hash]bool
}

// FileHistory holds the change history for a single file.
type FileHistory struct {
	People map[int]pkgplumbing.LineStats
	Hashes []gitlib.Hash
}

// Name returns the name of the analyzer.
func (h *Analyzer) Name() string {
	return "FileHistoryAnalysis"
}

// Flag returns the CLI flag for the analyzer.
func (h *Analyzer) Flag() string {
	return "file-history"
}

// Description returns a human-readable description of the analyzer.
func (h *Analyzer) Description() string {
	return h.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (h *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID: "history/file-history",
		Description: "Each file path is mapped to the list of commits which touch that file " +
			"and the mapping from involved developers to the corresponding line statistics.",
		Mode: analyze.ModeHistory,
	}
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (h *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (h *Analyzer) Initialize(_ *gitlib.Repository) error {
	h.files = map[string]*FileHistory{}
	h.merges = map[gitlib.Hash]bool{}

	return nil
}

// shouldConsumeCommit checks whether a commit should be processed.
// It returns false for duplicate merge commits and non-merge context merges.
func (h *Analyzer) shouldConsumeCommit(ctx *analyze.Context) bool {
	commit := ctx.Commit

	if commit.NumParents() > 1 {
		if h.merges[commit.Hash()] {
			return false
		}

		h.merges[commit.Hash()] = true
	}

	return !ctx.IsMerge
}

// processFileChanges updates file histories based on the tree diff changes for the given commit.
func (h *Analyzer) processFileChanges(changes gitlib.Changes, commit analyze.CommitLike) error {
	for _, change := range changes {
		h.processOneFileChange(change, commit)
	}

	return nil
}

// processOneFileChange handles a single file change for history tracking.
func (h *Analyzer) processOneFileChange(change *gitlib.Change, commit analyze.CommitLike) {
	action := change.Action
	fh := h.getOrCreateFileHistory(change)

	switch action {
	case gitlib.Insert:
		fh.Hashes = []gitlib.Hash{commit.Hash()}
	case gitlib.Delete:
		fh.Hashes = append(fh.Hashes, commit.Hash())
	case gitlib.Modify:
		fh = h.handleModifyRename(change, fh)
		fh.Hashes = append(fh.Hashes, commit.Hash())
	}
}

// getOrCreateFileHistory retrieves or creates a FileHistory for the given change.
func (h *Analyzer) getOrCreateFileHistory(change *gitlib.Change) *FileHistory {
	name := change.To.Name
	if change.Action == gitlib.Delete {
		name = change.From.Name
	}

	fh := h.files[name]
	if fh == nil {
		fh = &FileHistory{}
		h.files[name] = fh
	}

	return fh
}

// handleModifyRename handles the rename portion of a Modify action.
func (h *Analyzer) handleModifyRename(change *gitlib.Change, fh *FileHistory) *FileHistory {
	if change.From.Name != change.To.Name {
		if oldFH, ok := h.files[change.From.Name]; ok {
			delete(h.files, change.From.Name)
			h.files[change.To.Name] = oldFH

			return oldFH
		}
	}

	return fh
}

// aggregateLineStats merges line statistics from the current commit into file histories.
func (h *Analyzer) aggregateLineStats(lineStats map[gitlib.ChangeEntry]pkgplumbing.LineStats, author int) {
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
func (h *Analyzer) Consume(_ context.Context, ac *analyze.Context) error {
	if !h.shouldConsumeCommit(ac) {
		return nil
	}

	h.lastCommit = ac.Commit

	err := h.processFileChanges(h.TreeDiff.Changes, ac.Commit)
	if err != nil {
		return err
	}

	h.aggregateLineStats(h.LineStats.LineStats, h.Identity.AuthorID)

	return nil
}

// Finalize completes the analysis and returns the result.
func (h *Analyzer) Finalize() (analyze.Report, error) {
	files := map[string]FileHistory{}

	if h.lastCommit != nil {
		err := h.collectFinalFiles(files)
		if err != nil {
			return nil, err
		}
	}

	return analyze.Report{"Files": files}, nil
}

// collectFinalFiles populates the files map with histories of files present in the last commit.
func (h *Analyzer) collectFinalFiles(files map[string]FileHistory) error {
	fileIter, err := h.lastCommit.Files()
	if err != nil {
		return fmt.Errorf("listing files: %w", err)
	}

	iterErr := fileIter.ForEach(func(file *gitlib.File) error {
		if fh := h.files[file.Name]; fh != nil {
			files[file.Name] = *fh
		}

		return nil
	})
	if iterErr != nil {
		return fmt.Errorf("iterating files: %w", iterErr)
	}

	return nil
}

// SequentialOnly returns false because file history analysis can be parallelized.
func (h *Analyzer) SequentialOnly() bool { return false }

// CPUHeavy returns false because file history tracking is lightweight bookkeeping.
func (h *Analyzer) CPUHeavy() bool { return false }

// SnapshotPlumbing captures the current plumbing output state for one commit.
func (h *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   h.TreeDiff.Changes,
		LineStats: h.LineStats.LineStats,
		AuthorID:  h.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.TreeDiff.Changes = snapshot.Changes
	h.LineStats.LineStats = snapshot.LineStats
	h.Identity.AuthorID = snapshot.AuthorID
}

// ReleaseSnapshot releases any resources owned by the snapshot.
func (h *Analyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets its own independent copies of mutable state.
func (h *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &Analyzer{
			Identity:  &plumbing.IdentityDetector{},
			TreeDiff:  &plumbing.TreeDiffAnalyzer{},
			LineStats: &plumbing.LinesStatsCalculator{},
		}
		// Initialize independent state for each fork.
		clone.files = make(map[string]*FileHistory)
		clone.merges = make(map[gitlib.Hash]bool)

		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (h *Analyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*Analyzer)
		if !ok {
			continue
		}

		h.mergeFiles(other.files)
		h.mergeMerges(other.merges)

		// Keep the latest lastCommit so Finalize can filter deleted files.
		if other.lastCommit != nil {
			h.lastCommit = other.lastCommit
		}
	}
}

// mergeFiles combines file histories from another analyzer.
func (h *Analyzer) mergeFiles(other map[string]*FileHistory) {
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

// mergeMerges combines merge commit tracking from another analyzer.
func (h *Analyzer) mergeMerges(other map[gitlib.Hash]bool) {
	for hash := range other {
		h.merges[hash] = true
	}
}

// Serialize writes the analysis result to the given writer.
func (h *Analyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return h.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return h.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return h.generatePlot(result, writer)
	case analyze.FormatBinary:
		return h.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (h *Analyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(metrics)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (h *Analyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("yaml write: %w", err)
	}

	return nil
}

func (h *Analyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (h *Analyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, analyze.FormatYAML, writer)
}
