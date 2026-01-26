// Package filehistory provides file history functionality.
package filehistory

import (
	"errors"
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
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
	lastCommit *object.Commit
	merges     map[gitplumbing.Hash]bool
	//nolint:unused // used via reflection or external caller.
	// Internal.
	l interface {
		Errorf(format string, args ...any)
	}
}

// FileHistory holds the change history for a single file.
type FileHistory struct {
	People map[int]pkgplumbing.LineStats
	Hashes []gitplumbing.Hash
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
	return "Each file path is mapped to the list of commits which touch that file " +
		"and the mapping from involved developers to the corresponding line statistics."
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
func (h *Analyzer) Initialize(_ *git.Repository) error {
	h.files = map[string]*FileHistory{}
	h.merges = map[gitplumbing.Hash]bool{}

	return nil
}

// shouldConsumeCommit checks whether a commit should be processed.
// It returns false for duplicate merge commits and non-merge context merges.
func (h *Analyzer) shouldConsumeCommit(ctx *analyze.Context) bool {
	commit := ctx.Commit

	if commit.NumParents() > 1 {
		if h.merges[commit.Hash] {
			return false
		}

		h.merges[commit.Hash] = true
	}

	return !ctx.IsMerge
}

// processFileChanges updates file histories based on the tree diff changes for the given commit.
//
//nolint:gocognit // complexity is inherent to multi-action file history tracking with renames.
func (h *Analyzer) processFileChanges(changes object.Changes, commit *object.Commit) error {
	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			return fmt.Errorf("consume: %w", err)
		}

		var fh *FileHistory
		if action != merkletrie.Delete {
			fh = h.files[change.To.Name]
		} else {
			fh = h.files[change.From.Name]
		}

		if fh == nil {
			fh = &FileHistory{}
			if action != merkletrie.Delete {
				h.files[change.To.Name] = fh
			} else {
				h.files[change.From.Name] = fh
			}
		}

		switch action {
		case merkletrie.Insert:
			fh.Hashes = []gitplumbing.Hash{commit.Hash}
		case merkletrie.Delete:
			fh.Hashes = append(fh.Hashes, commit.Hash)
		case merkletrie.Modify:
			if change.From.Name != change.To.Name {
				if oldFH, ok := h.files[change.From.Name]; ok {
					delete(h.files, change.From.Name)
					h.files[change.To.Name] = oldFH
					fh = oldFH
				}
			}

			fh.Hashes = append(fh.Hashes, commit.Hash)
		}
	}

	return nil
}

// aggregateLineStats merges line statistics from the current commit into file histories.
func (h *Analyzer) aggregateLineStats(lineStats map[object.ChangeEntry]pkgplumbing.LineStats, author int) {
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
func (h *Analyzer) Consume(ctx *analyze.Context) error {
	if !h.shouldConsumeCommit(ctx) {
		return nil
	}

	h.lastCommit = ctx.Commit

	err := h.processFileChanges(h.TreeDiff.Changes, ctx.Commit)
	if err != nil {
		return err
	}

	h.aggregateLineStats(h.LineStats.LineStats, h.Identity.AuthorID)

	return nil
}

// Finalize completes the analysis and returns the result.
func (h *Analyzer) Finalize() (analyze.Report, error) {
	files := map[string]FileHistory{}

	if h.lastCommit != nil { //nolint:nestif // complex tree traversal with nested iteration
		fileIter, err := h.lastCommit.Files()
		if err == nil {
			iterErr := fileIter.ForEach(func(file *object.File) error {
				if fh := h.files[file.Name]; fh != nil {
					files[file.Name] = *fh
				}

				return nil
			})
			if iterErr != nil {
				return nil, fmt.Errorf("iterating files: %w", iterErr)
			}
		}
	}

	return analyze.Report{"Files": files}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
func (h *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *h
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (h *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (h *Analyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
	files, ok := result["Files"].(map[string]FileHistory)
	if !ok {
		return errors.New("expected map[string]FileHistory for files") //nolint:err113 // descriptive error for type assertion failure.
	}

	keys := make([]string, 0, len(files))
	for key := range files {
		keys = append(keys, key)
	}

	sort.Strings(keys)

	for _, key := range keys {
		fmt.Fprintf(writer, "  - %s:\n", key)
		file := files[key]
		hashes := file.Hashes

		strhashes := make([]string, len(hashes))
		for i, hash := range hashes {
			strhashes[i] = "\"" + hash.String() + "\""
		}

		sort.Strings(strhashes)
		fmt.Fprintf(writer, "    commits: [%s]\n", strings.Join(strhashes, ","))

		strpeople := make([]string, 0, len(file.People))
		for key, val := range file.People {
			strpeople = append(strpeople, fmt.Sprintf("%d:[%d,%d,%d]", key, val.Added, val.Removed, val.Changed))
		}

		sort.Strings(strpeople)
		fmt.Fprintf(writer, "    people: {%s}\n", strings.Join(strpeople, ","))
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (h *Analyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, false, writer)
}
