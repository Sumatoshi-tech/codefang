// Package couples provides couples functionality.
package couples

import (
	"errors"
	"fmt"
	"io"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
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
	return "The result is a square matrix, the value in each cell corresponds to the number of times " +
		"the pair of files appeared in the same commit or pair of developers committed to the same file."
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
	peopleMatrix, peopleFiles := computePeopleMatrix(people, filesIndex, c.PeopleNumber)
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

// Fork creates a copy of the analyzer for parallel processing.
func (c *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *c
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (c *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (c *HistoryAnalyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
	peopleMatrix, ok := result["PeopleMatrix"].([]map[int]int64)
	if !ok {
		return errors.New("expected []map[int]int64 for peopleMatrix") //nolint:err113 // descriptive error for type assertion failure.
	}

	files, ok := result["Files"].([]string)
	if !ok {
		return errors.New("expected []string for files") //nolint:err113 // descriptive error for type assertion failure.
	}

	filesLines, ok := result["FilesLines"].([]int)
	if !ok {
		return errors.New("expected []int for filesLines") //nolint:err113 // descriptive error for type assertion failure.
	}

	filesMatrix, ok := result["FilesMatrix"].([]map[int]int64)
	if !ok {
		return errors.New("expected []map[int]int64 for filesMatrix") //nolint:err113 // descriptive error for type assertion failure.
	}

	reversedPeopleDict, ok := result["ReversedPeopleDict"].([]string)
	if !ok {
		return errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error for type assertion failure.
	}

	fmt.Fprintln(writer, "  files_coocc:")
	fmt.Fprintln(writer, "    index:")

	for _, file := range files {
		fmt.Fprintf(writer, "      - %s\n", file)
	}

	fmt.Fprintln(writer, "    lines:")

	for _, l := range filesLines {
		fmt.Fprintf(writer, "      - %d\n", l)
	}

	writeMatrixSection(writer, filesMatrix)

	fmt.Fprintln(writer, "  people_coocc:")
	fmt.Fprintln(writer, "    index:")

	for _, person := range reversedPeopleDict {
		fmt.Fprintf(writer, "      - %s\n", person)
	}

	writeMatrixSection(writer, peopleMatrix)

	fmt.Fprintln(writer, "    author_files:")
	// ... (author_files logic omitted).
	return nil
}

// writeMatrixSection writes a YAML "matrix:" section with sorted sparse row data.
func writeMatrixSection(writer io.Writer, matrix []map[int]int64) {
	fmt.Fprintln(writer, "    matrix:")

	for _, row := range matrix {
		fmt.Fprint(writer, "      - {")

		indices := make([]int, 0, len(row))
		for k := range row {
			indices = append(indices, k)
		}

		sort.Ints(indices)

		for i, k := range indices {
			fmt.Fprintf(writer, "%d: %d", k, row[k])

			if i < len(indices)-1 {
				fmt.Fprint(writer, ", ")
			}
		}

		fmt.Fprintln(writer, "}")
	}
}

// FormatReport writes the formatted analysis report to the given writer.
func (c *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return c.Serialize(report, false, writer)
}

func (c *HistoryAnalyzer) currentFiles() map[string]bool {
	files := map[string]bool{}
	if c.lastCommit == nil { //nolint:nestif // complex tree traversal with nested iteration
		for key := range c.files {
			files[key] = true
		}
	} else {
		tree, treeErr := c.lastCommit.Tree()
		if treeErr == nil {
			iterErr := tree.Files().ForEach(func(fobj *gitlib.File) error {
				files[fobj.Name] = true

				return nil
			})
			// Best-effort enumeration; callback never returns an error.
			if iterErr != nil {
				return files
			}
		}
	}

	return files
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
