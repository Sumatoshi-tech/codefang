// Package couples provides couples functionality.
package couples

import (
	"context"
	"errors"
	"io"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	readBufferSize = 32 * 1024 // 32KB read buffer.

	// seenFilesBloomExpected is the expected number of unique file paths for the Bloom filter.
	// Overestimating is cheap (extra memory is ~10 bits per element); underestimating
	// degrades the false-positive rate. 100K covers very large monorepos.
	seenFilesBloomExpected = 100_000

	// seenFilesBloomFP is the target false-positive rate for the seen-files Bloom filter.
	// A false positive conservatively excludes a file from the coupling context in merge mode,
	// which has negligible impact on coupling quality.
	seenFilesBloomFP = 0.01
)

// ErrInvalidReversedPeopleDict indicates a type assertion failure for reversedPeopleDict.
var ErrInvalidReversedPeopleDict = errors.New("expected []string for reversedPeopleDict")

//

// HistoryAnalyzer identifies co-change coupling between files and developers.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]
	common.IdentityMixin

	TreeDiff     *plumbing.TreeDiffAnalyzer
	lastCommit   analyze.CommitLike
	merges       *analyze.MergeTracker
	PeopleNumber int
	seenFiles    *bloom.Filter

	// TopKPerFile limits the number of file coupling pairs emitted by WriteToStoreFromAggregator.
	// Zero uses DefaultTopKPerFile.
	TopKPerFile int
	// MinEdgeWeight is the minimum co-change count for an edge to be emitted.
	// Zero uses DefaultMinEdgeWeight.
	MinEdgeWeight int64
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
			return newAggregator(opts, a.PeopleNumber, a.GetReversedPeopleDict(), a.lastCommit)
		},
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(ctx, ticks, a.GetReversedPeopleDict(), a.PeopleNumber, a.lastCommit)
		},
		SerializeTextFn: func(result analyze.Report, writer io.Writer) error {
			return a.generateText(result, writer)
		},
		SerializePlotFn: func(result analyze.Report, writer io.Writer) error {
			return a.generatePlot(result, writer)
		},
	}

	return a
}

const (
	// CouplesMaximumMeaningfulContextSize is the maximum number of files in a commit
	// to consider for coupling analysis. Commits exceeding this threshold are skipped
	// because they are typically bulk operations (vendor updates, mass renames,
	// formatting) that produce noise rather than meaningful coupling signal.
	// Memory impact: N files → N² coupling entries × ~200 bytes.
	// At 200 files: 40K entries ≈ 8 MB. At 1000 files: 1M entries ≈ 200 MB.
	CouplesMaximumMeaningfulContextSize = 200
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
	if val, ok := pkgplumbing.GetPeopleCount(facts); ok {
		c.PeopleNumber = val

		rpd, rpdOK := pkgplumbing.GetReversedPeopleDict(facts)
		if !rpdOK {
			return ErrInvalidReversedPeopleDict
		}

		c.ReversedPeopleDict = rpd
	}

	return nil
}

// MapDependencies returns the required plumbing analyzers.
func (c *HistoryAnalyzer) MapDependencies() []string {
	return []string{}
}

// newSeenFilesFilter creates a Bloom filter for tracking seen file paths.
func newSeenFilesFilter() *bloom.Filter {
	// Error is structurally impossible: constants are valid.
	f, err := bloom.NewWithEstimates(seenFilesBloomExpected, seenFilesBloomFP)
	if err != nil {
		panic("couples: seen-files bloom filter initialization failed: " + err.Error())
	}

	return f
}

// Initialize prepares the analyzer for processing commits.
func (c *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	c.seenFiles = newSeenFilesFilter()
	c.merges = analyze.NewMergeTracker()

	return nil
}

// ensureCapacity grows people and peopleCommits slices if needed.
// This handles incremental identity detection where PeopleNumber
// isn't known at Configure time.

// Consume processes a single commit and returns a TC with coupling data.
func (c *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	commit := ac.Commit

	if commit.NumParents() > 1 {
		if c.merges.SeenOrAdd(commit.Hash()) {
			return analyze.TC{Data: &CommitData{}}, nil
		}
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

	return analyze.TC{
		Data:       &data,
		CommitHash: ac.Commit.Hash(),
	}, nil
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

		c.seenFiles.Add([]byte(name))

		if author != identity.AuthorMissing {
			data.AuthorFiles[name] = 1
		}

		return
	}

	if !c.seenFiles.Test([]byte(name)) {
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
//
// Uses an inverted index (file → list of (devID, commits) pairs) to avoid the
// O(D² × F) triple loop. Instead: O(D × F) to build the index, then O(sum of
// per-file developer pairs) to compute the matrix, which is much smaller for
// codebases where most files are touched by few developers.
// devCommit records a developer's commit count on a file.
type devCommit struct {
	devID   int
	commits int
}

func computePeopleMatrix(
	people []map[string]int, filesIndex map[string]int, peopleNumber int,
) (matrix []map[int]int64, peopleFiles [][]int) {
	peopleFiles = buildPeopleFileIndices(people, filesIndex, peopleNumber)
	invertedIndex := buildInvertedIndex(people, filesIndex, peopleNumber)
	matrix = accumulateMatrix(invertedIndex, peopleNumber)

	return matrix, peopleFiles
}

// buildPeopleFileIndices builds sorted per-person file index lists.
func buildPeopleFileIndices(
	people []map[string]int, filesIndex map[string]int, peopleNumber int,
) [][]int {
	peopleFiles := make([][]int, peopleNumber+1)

	for i := range people {
		if i > peopleNumber {
			break
		}

		for file := range people[i] {
			if fi, exists := filesIndex[file]; exists {
				peopleFiles[i] = append(peopleFiles[i], fi)
			}
		}

		sort.Ints(peopleFiles[i])
	}

	return peopleFiles
}

// buildInvertedIndex creates a file → [(devID, commits)] mapping.
func buildInvertedIndex(
	people []map[string]int, filesIndex map[string]int, peopleNumber int,
) map[string][]devCommit {
	invertedIndex := make(map[string][]devCommit, len(filesIndex))

	for i, files := range people {
		if i > peopleNumber {
			break
		}

		for file, commits := range files {
			if commits > 0 {
				invertedIndex[file] = append(invertedIndex[file], devCommit{devID: i, commits: commits})
			}
		}
	}

	return invertedIndex
}

// accumulateMatrix computes the co-occurrence matrix from the inverted index.
// For each file, accumulates min(commits_i, commits_j) for all developer pairs,
// including self-coupling (diagonal entries) where i == j.
func accumulateMatrix(invertedIndex map[string][]devCommit, peopleNumber int) []map[int]int64 {
	matrix := make([]map[int]int64, peopleNumber+1)
	for i := range matrix {
		matrix[i] = map[int]int64{}
	}

	for _, devs := range invertedIndex {
		for a := range devs {
			for b := range devs {
				delta := int64(min(devs[a].commits, devs[b].commits))
				if delta > 0 {
					matrix[devs[a].devID][devs[b].devID] += delta
				}
			}
		}
	}

	return matrix
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
			IdentityMixin: common.IdentityMixin{
				Identity:           &plumbing.IdentityDetector{},
				ReversedPeopleDict: c.GetReversedPeopleDict(),
			},
			TreeDiff:     &plumbing.TreeDiffAnalyzer{},
			PeopleNumber: c.PeopleNumber,
			seenFiles:    newSeenFilesFilter(),
		}
		if c.BaseHistoryAnalyzer != nil {
			clone.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
				Desc:             c.Desc,
				Sequential:       c.Sequential,
				ComputeMetricsFn: c.ComputeMetricsFn,
				AggregatorFn:     c.AggregatorFn,
				TicksToReportFn:  c.TicksToReportFn,
				SerializeTextFn:  c.SerializeTextFn,
				SerializePlotFn:  c.SerializePlotFn,
			}
		}
		// Initialize independent state for each fork.
		clone.merges = analyze.NewMergeTracker()

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

		// Merge trackers are not combined: each fork processes a disjoint
		// subset of commits, so merge dedup state stays independent.

		// Keep the latest lastCommit for aggregator/ticksToReport file line counts.
		if other.lastCommit != nil {
			c.lastCommit = other.lastCommit
		}
	}
}

// NewAggregator creates a new aggregator for this analyzer.
func (c *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return newAggregator(opts, c.PeopleNumber, c.GetReversedPeopleDict(), c.lastCommit)
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It extracts per-commit coupling summary data for the unified timeseries output.
func (c *HistoryAnalyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitStats, ok := report["commit_stats"].(map[string]*CommitSummary)
	if !ok || len(commitStats) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitStats))

	for hash, cs := range commitStats {
		result[hash] = map[string]any{
			"files_touched": cs.FilesTouched,
			"author_id":     cs.AuthorID,
		}
	}

	return result
}
