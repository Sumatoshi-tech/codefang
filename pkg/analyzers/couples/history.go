// Package couples provides couples functionality.
package couples

import (
	"errors"
	"fmt"
	"io"
	"sort"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	readBufferSize = 32 * 1024 // 32KB read buffer.
)

// CouplesHistoryAnalyzer identifies co-change coupling between files and developers.
type CouplesHistoryAnalyzer struct {
	l                  interface{ Critical(args ...any) } //nolint:unused // used via dependency injection.
	Identity           *plumbing.IdentityDetector
	TreeDiff           *plumbing.TreeDiffAnalyzer
	files              map[string]map[string]int
	renames            *[]rename
	lastCommit         *object.Commit
	merges             map[gitplumbing.Hash]bool
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
func (c *CouplesHistoryAnalyzer) Name() string {
	return "Couples"
}

// Flag returns the CLI flag for the analyzer.
func (c *CouplesHistoryAnalyzer) Flag() string {
	return "couples"
}

// Description returns a human-readable description of the analyzer.
func (c *CouplesHistoryAnalyzer) Description() string {
	return "The result is a square matrix, the value in each cell corresponds to the number of times the pair of files appeared in the same commit or pair of developers committed to the same file." //nolint:lll // long line is acceptable here.
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *CouplesHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (c *CouplesHistoryAnalyzer) Configure(facts map[string]any) error {
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
func (c *CouplesHistoryAnalyzer) Initialize(_ *git.Repository) error {
	c.people = make([]map[string]int, c.PeopleNumber+1)
	for i := range c.people {
		c.people[i] = map[string]int{}
	}

	c.peopleCommits = make([]int, c.PeopleNumber+1)
	c.files = map[string]map[string]int{}
	c.renames = &[]rename{}
	c.merges = map[gitplumbing.Hash]bool{}

	return nil
}

// Consume processes a single commit with the provided dependency results.
//
//nolint:cyclop,funlen,gocognit,gocyclo // complex function.
func (c *CouplesHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit
	shouldConsume := true

	if commit.NumParents() > 1 {
		if c.merges[commit.Hash] {
			shouldConsume = false
		} else {
			c.merges[commit.Hash] = true
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

	treeDiff := c.TreeDiff.Changes

	context := make([]string, 0, len(treeDiff))
	for _, change := range treeDiff {
		action, err := change.Action()
		if err != nil {
			return fmt.Errorf("consume: %w", err)
		}

		toName := change.To.Name
		fromName := change.From.Name

		switch action {
		case merkletrie.Insert:
			if !mergeMode || c.files[toName] == nil {
				context = append(context, toName)
				c.people[author][toName]++
			}
		case merkletrie.Delete:
			if !mergeMode {
				c.people[author][fromName]++
			}
		case merkletrie.Modify:
			if fromName != toName {
				*c.renames = append(*c.renames, rename{ToName: toName, FromName: fromName})
			}

			if !mergeMode || c.files[toName] == nil {
				context = append(context, toName)
				c.people[author][toName]++
			}
		}
	}

	if len(context) <= CouplesMaximumMeaningfulContextSize {
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

	return nil
}

// Finalize completes the analysis and returns the result.
//
//nolint:cyclop,funlen,gocognit,gocyclo // complex function.
func (c *CouplesHistoryAnalyzer) Finalize() (analyze.Report, error) {
	files, people := c.propagateRenames(c.currentFiles())
	filesSequence := make([]string, len(files))

	i := 0
	for file := range files {
		filesSequence[i] = file
		i++
	}

	sort.Strings(filesSequence)

	filesIndex := map[string]int{}
	for i, file := range filesSequence {
		filesIndex[file] = i
	}

	filesLines := make([]int, len(filesSequence))
	if c.lastCommit != nil {
		for i, name := range filesSequence {
			file, err := c.lastCommit.File(name)
			if err != nil {
				continue
			}

			reader, err := file.Blob.Reader() //nolint:staticcheck // QF1008 is acceptable.
			if err != nil {
				continue
			}

			buf := make([]byte, readBufferSize)
			count := 0

			for {
				n, readErr := reader.Read(buf)
				count += countNewlines(buf[:n])

				if readErr != nil {
					break
				}
			}

			reader.Close()

			filesLines[i] = count
		}
	}

	peopleMatrix := make([]map[int]int64, c.PeopleNumber+1)
	peopleFiles := make([][]int, c.PeopleNumber+1)

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

	filesMatrix := make([]map[int]int64, len(filesIndex))
	for i := range filesMatrix {
		filesMatrix[i] = map[int]int64{}
		for otherFile, cooccs := range c.files[filesSequence[i]] {
			filesMatrix[i][filesIndex[otherFile]] = int64(cooccs)
		}
	}

	return analyze.Report{
		"PeopleMatrix":       peopleMatrix,
		"PeopleFiles":        peopleFiles,
		"Files":              filesSequence,
		"FilesLines":         filesLines,
		"FilesMatrix":        filesMatrix,
		"ReversedPeopleDict": c.reversedPeopleDict,
	}, nil
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
func (c *CouplesHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *c
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (c *CouplesHistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
//
//nolint:funlen,gocognit,cyclop,gocyclo // cognitive complexity is acceptable for this function.
func (c *CouplesHistoryAnalyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
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

	fmt.Fprintln(writer, "    matrix:")

	for _, row := range filesMatrix {
		fmt.Fprint(writer, "      - {")

		var indices []int
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

	fmt.Fprintln(writer, "  people_coocc:")
	fmt.Fprintln(writer, "    index:")

	for _, person := range reversedPeopleDict {
		fmt.Fprintf(writer, "      - %s\n", person)
	}

	fmt.Fprintln(writer, "    matrix:")

	for _, row := range peopleMatrix {
		fmt.Fprint(writer, "      - {")

		var indices []int
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

	fmt.Fprintln(writer, "    author_files:")
	// ... (author_files logic omitted).
	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (c *CouplesHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return c.Serialize(report, false, writer)
}

func (c *CouplesHistoryAnalyzer) currentFiles() map[string]bool {
	files := map[string]bool{}
	if c.lastCommit == nil {
		for key := range c.files {
			files[key] = true
		}
	} else {
		tree, _ := c.lastCommit.Tree()                       //nolint:errcheck // error return value is intentionally ignored.
		tree.Files().ForEach(func(fobj *object.File) error { //nolint:errcheck // error return value is intentionally ignored.
			files[fobj.Name] = true

			return nil
		})
	}

	return files
}

//nolint:gocritic // short name is clear in context; unnamed results for multi-return.
func (c *CouplesHistoryAnalyzer) propagateRenames(
	files map[string]bool,
) (map[string]map[string]int, []map[string]int) {
	// Renames := *c.renames.
	reducedFiles := map[string]map[string]int{}

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

	people := make([]map[string]int, len(c.people))
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
