// Package couples provides couples functionality.
package couples

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	readBufferSize = 32 * 1024 // 32KB read buffer.
)

// HistoryAnalyzer identifies co-change coupling between files and developers.
type HistoryAnalyzer struct {
	l                  interface{ Critical(args ...any) } //nolint:unused // used via dependency injection.
	Identity           *plumbing.IdentityDetector
	TreeDiff           *plumbing.TreeDiffAnalyzer
	files              map[string]map[string]int
	renames            *[]rename
	lastCommit         analyze.CommitLike
	merges             map[gitlib.Hash]bool
	people             []map[string]int
	peopleCommits      []int
	reversedPeopleDict []string
	PeopleNumber       int
}

type rename struct {
	FromName string
	ToName   string
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
			return errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error for type assertion failure.
		}

		c.reversedPeopleDict = rpd
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (c *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	c.people = make([]map[string]int, c.PeopleNumber+1)
	for i := range c.people {
		c.people[i] = map[string]int{}
	}

	c.peopleCommits = make([]int, c.PeopleNumber+1)
	c.files = map[string]map[string]int{}
	c.renames = &[]rename{}
	c.merges = map[gitlib.Hash]bool{}

	return nil
}

// ensureCapacity grows people and peopleCommits slices if needed.
// This handles incremental identity detection where PeopleNumber
// isn't known at Configure time.
func (c *HistoryAnalyzer) ensureCapacity(minSize int) {
	if minSize <= len(c.people) {
		return
	}

	// Grow people slice
	newPeople := make([]map[string]int, minSize)
	copy(newPeople, c.people)

	for i := len(c.people); i < minSize; i++ {
		newPeople[i] = make(map[string]int)
	}

	c.people = newPeople

	// Grow peopleCommits slice
	newPeopleCommits := make([]int, minSize)
	copy(newPeopleCommits, c.peopleCommits)
	c.peopleCommits = newPeopleCommits
}

// Consume processes a single commit with the provided dependency results.
func (c *HistoryAnalyzer) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit
	shouldConsume := true

	if commit.NumParents() > 1 {
		if c.merges[commit.Hash()] {
			shouldConsume = false
		} else {
			c.merges[commit.Hash()] = true
		}
	}

	mergeMode := ctx.IsMerge
	c.lastCommit = commit

	author := c.Identity.AuthorID
	if author == identity.AuthorMissing {
		author = c.PeopleNumber
	}

	// Grow slices dynamically if author ID exceeds current capacity.
	// This handles incremental identity detection where PeopleNumber
	// isn't known at Configure time.
	c.ensureCapacity(author + 1)

	if shouldConsume {
		c.peopleCommits[author]++
	}

	context := c.processTreeChanges(c.TreeDiff.Changes, mergeMode, author)
	c.updateFileCouplings(context)

	return nil
}

// processTreeChanges processes the tree diff changes and returns the list of files that form the coupling context.
//
//nolint:gocognit // complexity is inherent to multi-action change processing with merge mode.
func (c *HistoryAnalyzer) processTreeChanges(
	treeDiff gitlib.Changes, mergeMode bool, author int,
) []string {
	context := make([]string, 0, len(treeDiff))

	for _, change := range treeDiff {
		action := change.Action

		toName := change.To.Name
		fromName := change.From.Name

		switch action {
		case gitlib.Insert:
			if !mergeMode || c.files[toName] == nil {
				context = append(context, toName)
				c.people[author][toName]++
			}
		case gitlib.Delete:
			if !mergeMode {
				c.people[author][fromName]++
			}
		case gitlib.Modify:
			if fromName != toName {
				*c.renames = append(*c.renames, rename{ToName: toName, FromName: fromName})
			}

			if !mergeMode || c.files[toName] == nil {
				context = append(context, toName)
				c.people[author][toName]++
			}
		}
	}

	return context
}

// updateFileCouplings updates the file co-occurrence matrix based on the coupling context.
func (c *HistoryAnalyzer) updateFileCouplings(context []string) {
	if len(context) > CouplesMaximumMeaningfulContextSize {
		return
	}

	for _, file := range context {
		for _, otherFile := range context {
			lane, exists := c.files[file]
			if !exists {
				lane = map[string]int{}
				c.files[file] = lane
			}

			lane[otherFile]++
		}
	}
}

// Finalize completes the analysis and returns the result.
func (c *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	files, people := c.propagateRenames(c.currentFiles())
	filesSequence, filesIndex := buildFilesIndex(files)
	filesLines := c.computeFilesLines(filesSequence)

	// Use the actual people count from accumulated data rather than PeopleNumber,
	// which may be 0 when IdentityDetector.PeopleCount fact was not provided.
	effectivePeopleNumber := c.PeopleNumber

	if len(people) > effectivePeopleNumber+1 {
		effectivePeopleNumber = len(people) - 1
	}

	peopleMatrix, peopleFiles := computePeopleMatrix(people, filesIndex, effectivePeopleNumber)
	filesMatrix := computeFilesMatrix(c.files, filesSequence, filesIndex)

	return analyze.Report{
		"PeopleMatrix":       peopleMatrix,
		"PeopleFiles":        peopleFiles,
		"Files":              filesSequence,
		"FilesLines":         filesLines,
		"FilesMatrix":        filesMatrix,
		"ReversedPeopleDict": c.reversedPeopleDict,
	}, nil
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

// computeFilesLines counts the number of newlines in each file at the last commit.
func (c *HistoryAnalyzer) computeFilesLines(filesSequence []string) []int {
	filesLines := make([]int, len(filesSequence))

	if c.lastCommit == nil {
		return filesLines
	}

	for i, name := range filesSequence {
		file, err := c.lastCommit.File(name)
		if err != nil {
			continue
		}

		blob, err := file.Blob()
		if err != nil {
			continue
		}

		reader := blob.Reader()

		buf := make([]byte, readBufferSize)
		count := 0

		for {
			n, readErr := reader.Read(buf)
			count += countNewlines(buf[:n])

			if readErr != nil {
				break
			}
		}

		filesLines[i] = count

		blob.Free()
	}

	return filesLines
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
		}
		// Initialize independent state for each fork
		clone.files = make(map[string]map[string]int)
		clone.renames = &[]rename{}
		clone.merges = make(map[gitlib.Hash]bool)

		clone.people = make([]map[string]int, c.PeopleNumber+1)
		for j := range clone.people {
			clone.people[j] = make(map[string]int)
		}

		clone.peopleCommits = make([]int, c.PeopleNumber+1)

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

		c.mergeFiles(other.files)
		c.mergePeople(other.people)
		c.mergePeopleCommits(other.peopleCommits)
		c.mergeMerges(other.merges)
		c.mergeRenames(other.renames)

		// Keep the latest lastCommit so Finalize can compute file line counts.
		if other.lastCommit != nil {
			c.lastCommit = other.lastCommit
		}
	}
}

// mergeFiles combines file coupling counts from another analyzer.
func (c *HistoryAnalyzer) mergeFiles(other map[string]map[string]int) {
	for file, otherCouplings := range other {
		if c.files[file] == nil {
			c.files[file] = make(map[string]int)
		}

		for otherFile, count := range otherCouplings {
			c.files[file][otherFile] += count
		}
	}
}

// mergePeople combines per-person file touch counts from another analyzer.
func (c *HistoryAnalyzer) mergePeople(other []map[string]int) {
	// Grow if the forked branch discovered more authors than we expected.
	if len(other) > len(c.people) {
		grown := make([]map[string]int, len(other))
		copy(grown, c.people)

		for i := len(c.people); i < len(other); i++ {
			grown[i] = make(map[string]int)
		}

		c.people = grown
	}

	for i, otherFiles := range other {
		for file, count := range otherFiles {
			c.people[i][file] += count
		}
	}
}

// mergePeopleCommits combines per-person commit counts from another analyzer.
func (c *HistoryAnalyzer) mergePeopleCommits(other []int) {
	// Grow if the forked branch discovered more authors than we expected.
	if len(other) > len(c.peopleCommits) {
		grown := make([]int, len(other))
		copy(grown, c.peopleCommits)
		c.peopleCommits = grown
	}

	for i, count := range other {
		c.peopleCommits[i] += count
	}
}

// mergeMerges combines merge commit tracking from another analyzer.
func (c *HistoryAnalyzer) mergeMerges(other map[gitlib.Hash]bool) {
	for hash := range other {
		c.merges[hash] = true
	}
}

// mergeRenames combines rename tracking from another analyzer.
func (c *HistoryAnalyzer) mergeRenames(other *[]rename) {
	if other != nil {
		*c.renames = append(*c.renames, *other...)
	}
}

// Serialize writes the analysis result to the given writer.
func (c *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return c.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return c.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return c.generatePlot(result, writer)
	case analyze.FormatBinary:
		return c.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (c *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
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

func (c *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
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

func (c *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
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
func (c *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return c.Serialize(report, analyze.FormatYAML, writer)
}

func (c *HistoryAnalyzer) currentFiles() map[string]bool {
	files := map[string]bool{}

	if c.lastCommit == nil {
		for key := range c.files {
			files[key] = true
		}

		return files
	}

	c.collectTreeFiles(files)

	return files
}

// collectTreeFiles populates the files map from the last commit's tree.
// Best-effort: errors are silently ignored since this is a best-effort enumeration.
func (c *HistoryAnalyzer) collectTreeFiles(files map[string]bool) {
	tree, treeErr := c.lastCommit.Tree()
	if treeErr != nil {
		return
	}

	//nolint:errcheck // best-effort enumeration; errors are intentionally ignored
	tree.Files().ForEach(func(fobj *gitlib.File) error {
		files[fobj.Name] = true

		return nil
	})
}

func (c *HistoryAnalyzer) propagateRenames(
	files map[string]bool,
) (reducedFiles map[string]map[string]int, people []map[string]int) {
	// Renames := *c.renames.
	reducedFiles = map[string]map[string]int{}

	for file := range files {
		fmap := map[string]int{}

		refmap := c.files[file]
		for other := range files {
			refval := refmap[other]
			if refval > 0 {
				fmap[other] = refval
			}
		}

		if len(fmap) > 0 {
			reducedFiles[file] = fmap
		}
	}

	people = make([]map[string]int, len(c.people))
	for i, counts := range c.people {
		reducedCounts := map[string]int{}
		people[i] = reducedCounts

		for file := range files {
			count := counts[file]
			if count > 0 {
				reducedCounts[file] = count
			}
		}
	}

	return reducedFiles, people
}
