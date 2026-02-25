// Package couples provides couples functionality.
package couples

import (
	"context"
	"errors"
	"io"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	readBufferSize = 32 * 1024 // 32KB read buffer.
)

// ErrInvalidReversedPeopleDict indicates a type assertion failure for reversedPeopleDict.
var ErrInvalidReversedPeopleDict = errors.New("expected []string for reversedPeopleDict")

//

// HistoryAnalyzer identifies co-change coupling between files and developers.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	TreeDiff           *plumbing.TreeDiffAnalyzer
	Identity           *plumbing.IdentityDetector
	lastCommit         analyze.CommitLike
	merges             map[gitlib.Hash]bool
	reversedPeopleDict []string
	PeopleNumber       int
	seenFiles          map[string]bool
}

// NewHistoryAnalyzer creates a new HistoryAnalyzer.
func NewHistoryAnalyzer() *HistoryAnalyzer {
	a := &HistoryAnalyzer{}

	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID: "history/couples",
			Description: "The result is a square matrix, the value in each cell corresponds to the number of times " +
				"the pair of files appeared in the same commit or pair of developers committed to the same file.",
			Mode: analyze.ModeHistory,
		},
		Sequential: false,
		ComputeMetricsFn: func(report analyze.Report) (*ComputedMetrics, error) {
			if len(report) == 0 {
				return &ComputedMetrics{}, nil
			}

			return ComputeAllMetrics(report)
		},
		AggregatorFn: func(opts analyze.AggregatorOptions) analyze.Aggregator {
			return newAggregator(opts, a.PeopleNumber, a.reversedPeopleDict, a.lastCommit)
		},
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(ctx, ticks, a.reversedPeopleDict, a.PeopleNumber, a.lastCommit)
		},
	}

	return a
}

const (
	// CouplesMaximumMeaningfulContextSize is the maximum number of files in a commit to consider for coupling analysis.
	CouplesMaximumMeaningfulContextSize = 1000
)

// Name returns the name of the analyzer.
func (c *HistoryAnalyzer) Name() string {
	return "Couples"
}

// Flag returns the CLI flag for the analyzer.
func (c *HistoryAnalyzer) Flag() string {
	return "couples"
}

// Description returns a human-readable description of the analyzer.
func (c *HistoryAnalyzer) Description() string {
	return c.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (c *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID: "history/couples",
		Description: "The result is a square matrix, the value in each cell corresponds to the number of times " +
			"the pair of files appeared in the same commit or pair of developers committed to the same file.",
		Mode: analyze.ModeHistory,
	}
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (c *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[identity.FactIdentityDetectorPeopleCount].(int); exists {
		c.PeopleNumber = val

		rpd, ok := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
		if !ok {
			return ErrInvalidReversedPeopleDict
		}

		c.reversedPeopleDict = rpd
	}

	return nil
}

// MapDependencies returns the required plumbing analyzers.
func (c *HistoryAnalyzer) MapDependencies() []string {
	return []string{}
}

// Initialize prepares the analyzer for processing commits.
func (c *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	c.seenFiles = map[string]bool{}
	c.merges = map[gitlib.Hash]bool{}

	return nil
}

// ensureCapacity grows people and peopleCommits slices if needed.
// This handles incremental identity detection where PeopleNumber
// isn't known at Configure time.

// Consume processes a single commit and returns a TC with coupling data.
func (c *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	commit := ac.Commit

	if commit.NumParents() > 1 {
		if c.merges[commit.Hash()] {
			return analyze.TC{Data: &CommitData{}}, nil
		}

		c.merges[commit.Hash()] = true
	}

	mergeMode := ac.IsMerge
	c.lastCommit = commit

	author := c.Identity.AuthorID
	if author == identity.AuthorMissing {
		author = c.PeopleNumber
	}

	data := CommitData{
		CouplingFiles: []string{},
		AuthorFiles:   make(map[string]int),
		Renames:       []RenamePair{},
		CommitCounted: true,
	}

	// Skip oversized changesets — mass changes (formatting, license headers,
	// dependency bumps) produce noise rather than meaningful coupling signal.
	if len(c.TreeDiff.Changes) > CouplesMaximumMeaningfulContextSize {
		return analyze.TC{Data: &data}, nil
	}

	for _, change := range c.TreeDiff.Changes {
		c.processChange(change, mergeMode, author, &data)
	}

	return analyze.TC{Data: &data}, nil
}

func (c *HistoryAnalyzer) processChange(change *gitlib.Change, mergeMode bool, author int, data *CommitData) {
	action := change.Action

	name := change.To.Name
	if action == gitlib.Delete {
		name = change.From.Name
	}

	if action == gitlib.Modify && change.To.Name != change.From.Name {
		data.Renames = append(data.Renames, RenamePair{
			FromName: change.From.Name,
			ToName:   change.To.Name,
		})
		name = change.To.Name
	}

	if mergeMode && action == gitlib.Delete {
		return
	}

	if !mergeMode {
		if action != gitlib.Delete {
			data.CouplingFiles = append(data.CouplingFiles, name)
		}

		c.seenFiles[name] = true

		if author != identity.AuthorMissing {
			data.AuthorFiles[name] = 1
		}

		return
	}

	if !c.seenFiles[name] {
		// Merge mode: only add to coupling context if file not seen before.
		data.CouplingFiles = append(data.CouplingFiles, name)
	}

	// Always record author touch, even for previously-seen files in merge mode.
	// This mirrors the devs analyzer pattern: coupling dedup ≠ ownership dedup.
	if author != identity.AuthorMissing {
		data.AuthorFiles[name] = 1
	}
}

// updateFileCouplings updates the file co-occurrence matrix based on the coupling context.

// mergeFileCouplings additively merges two file coupling maps.
func mergeFileCouplings(existing, incoming map[string]int) map[string]int {
	for k, v := range incoming {
		existing[k] += v
	}

	return existing
}

// buildFilesIndex creates a sorted sequence of file names and a map from file name to index.
func buildFilesIndex(files map[string]map[string]int) (sequence []string, index map[string]int) {
	filesSequence := make([]string, 0, len(files))
	for file := range files {
		filesSequence = append(filesSequence, file)
	}

	sort.Strings(filesSequence)

	filesIndex := make(map[string]int, len(filesSequence))
	for i, file := range filesSequence {
		filesIndex[file] = i
	}

	return filesSequence, filesIndex
}

// computePeopleMatrix builds the people co-occurrence matrix and the per-person file lists.
func computePeopleMatrix(
	people []map[string]int, filesIndex map[string]int, peopleNumber int,
) (matrix []map[int]int64, peopleFiles [][]int) {
	peopleMatrix := make([]map[int]int64, peopleNumber+1)
	peopleFiles = make([][]int, peopleNumber+1)

	for i := range peopleMatrix {
		peopleMatrix[i] = map[int]int64{}

		for file, commits := range people[i] {
			if fi, exists := filesIndex[file]; exists {
				peopleFiles[i] = append(peopleFiles[i], fi)
			}

			for j, otherFiles := range people {
				otherCommits := otherFiles[file]

				delta := min(otherCommits, commits)

				if delta > 0 {
					peopleMatrix[i][j] += int64(delta)
				}
			}
		}

		sort.Ints(peopleFiles[i])
	}

	return peopleMatrix, peopleFiles
}

// computeFilesMatrix builds the file co-occurrence matrix from the raw coupling data.
func computeFilesMatrix(
	rawFiles map[string]map[string]int, filesSequence []string, filesIndex map[string]int,
) []map[int]int64 {
	filesMatrix := make([]map[int]int64, len(filesIndex))

	for i := range filesMatrix {
		filesMatrix[i] = map[int]int64{}

		for otherFile, cooccs := range rawFiles[filesSequence[i]] {
			filesMatrix[i][filesIndex[otherFile]] = int64(cooccs)
		}
	}

	return filesMatrix
}

func countNewlines(p []byte) int {
	count := 0

	for _, b := range p {
		if b == '\n' {
			count++
		}
	}

	return count
}

// SequentialOnly returns false because couples analysis can be parallelized.
func (c *HistoryAnalyzer) SequentialOnly() bool { return false }

// CPUHeavy returns false because coupling analysis is lightweight file-pair bookkeeping.
func (c *HistoryAnalyzer) CPUHeavy() bool { return false }

// SnapshotPlumbing captures the current plumbing output state for one commit.
func (c *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:  c.TreeDiff.Changes,
		AuthorID: c.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (c *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	s, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	c.TreeDiff.Changes = s.Changes
	c.Identity.AuthorID = s.AuthorID
}

// ReleaseSnapshot releases any resources owned by the snapshot.
func (c *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets its own independent copies of mutable state (slices and maps).
func (c *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &HistoryAnalyzer{
			Identity:           &plumbing.IdentityDetector{},
			TreeDiff:           &plumbing.TreeDiffAnalyzer{},
			PeopleNumber:       c.PeopleNumber,
			reversedPeopleDict: c.reversedPeopleDict,
			seenFiles:          make(map[string]bool),
		}
		if c.BaseHistoryAnalyzer != nil {
			clone.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
				Desc:             c.Desc,
				Sequential:       c.Sequential,
				ComputeMetricsFn: c.ComputeMetricsFn,
				AggregatorFn:     c.AggregatorFn,
				TicksToReportFn:  c.TicksToReportFn,
			}
		}
		// Initialize independent state for each fork.
		clone.merges = make(map[gitlib.Hash]bool)

		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (c *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		c.mergeMerges(other.merges)

		// Keep the latest lastCommit for aggregator/ticksToReport file line counts.
		if other.lastCommit != nil {
			c.lastCommit = other.lastCommit
		}
	}
}

// mergeMerges combines merge commit tracking from another analyzer.
func (c *HistoryAnalyzer) mergeMerges(other map[gitlib.Hash]bool) {
	for hash := range other {
		c.merges[hash] = true
	}
}

// Serialize writes the analysis result to the given writer.
// Text and plot formats are handled here; JSON/YAML/Binary delegate to the base.
func (c *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatText {
		return c.generateText(result, writer)
	}

	if format == analyze.FormatPlot {
		return c.generatePlot(result, writer)
	}

	return c.BaseHistoryAnalyzer.Serialize(result, format, writer)
}

// NewAggregator creates a new aggregator for this analyzer.
func (c *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return newAggregator(opts, c.PeopleNumber, c.reversedPeopleDict, c.lastCommit)
}
