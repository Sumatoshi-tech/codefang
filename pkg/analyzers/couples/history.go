package couples

import (
	"fmt"
	"io"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
)

type CouplesHistoryAnalyzer struct {
	// Configuration
	PeopleNumber int

	// Dependencies
	Identity *plumbing.IdentityDetector
	TreeDiff *plumbing.TreeDiffAnalyzer

	// State
	people             []map[string]int
	peopleCommits      []int
	files              map[string]map[string]int
	renames            *[]rename
	lastCommit         *object.Commit
	reversedPeopleDict []string
	merges             map[gitplumbing.Hash]bool

	// Internal
	l interface {
		Critical(args ...interface{})
	}
}

type rename struct {
	FromName string
	ToName   string
}

const (
	CouplesMaximumMeaningfulContextSize = 1000
)

func (c *CouplesHistoryAnalyzer) Name() string {
	return "Couples"
}

func (c *CouplesHistoryAnalyzer) Flag() string {
	return "couples"
}

func (c *CouplesHistoryAnalyzer) Description() string {
	return "The result is a square matrix, the value in each cell corresponds to the number of times the pair of files appeared in the same commit or pair of developers committed to the same file."
}

func (c *CouplesHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

func (c *CouplesHistoryAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[identity.FactIdentityDetectorPeopleCount].(int); exists {
		c.PeopleNumber = val
		c.reversedPeopleDict = facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
	}
	return nil
}

func (c *CouplesHistoryAnalyzer) Initialize(repository *git.Repository) error {
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
			return err
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
			reader, err := file.Blob.Reader()
			if err != nil {
				continue
			}
			buf := make([]byte, 32*1024)
			count := 0
			for {
				n, err := reader.Read(buf)
				count += countNewlines(buf[:n])
				if err != nil {
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
				delta := otherCommits
				if otherCommits > commits {
					delta = commits
				}
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

func (c *CouplesHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *c
		res[i] = &clone
	}
	return res
}

func (c *CouplesHistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (c *CouplesHistoryAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	peopleMatrix := result["PeopleMatrix"].([]map[int]int64)
	_ = result["PeopleFiles"].([][]int)
	files := result["Files"].([]string)
	filesLines := result["FilesLines"].([]int)
	filesMatrix := result["FilesMatrix"].([]map[int]int64)
	reversedPeopleDict := result["ReversedPeopleDict"].([]string)

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
	// ... (author_files logic omitted)
	return nil
}

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
		tree, _ := c.lastCommit.Tree()
		tree.Files().ForEach(func(fobj *object.File) error {
			files[fobj.Name] = true
			return nil
		})
	}
	return files
}

func (c *CouplesHistoryAnalyzer) propagateRenames(files map[string]bool) (map[string]map[string]int, []map[string]int) {
	// renames := *c.renames
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
