package plumbing

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// IdentityDetector maps commit authors to canonical developer identities.
type IdentityDetector struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	PeopleDict         map[string]int
	PeopleDictPath     string
	ReversedPeopleDict []string
	AuthorID           int
	ExactSignatures    bool
}

const (
	// ConfigIdentityDetectorPeopleDictPath is the configuration key for the people dictionary file path.
	ConfigIdentityDetectorPeopleDictPath = "IdentityDetector.PeopleDictPath"
	// ConfigIdentityDetectorExactSignatures is the configuration key for requiring exact author signatures.
	ConfigIdentityDetectorExactSignatures = "IdentityDetector.ExactSignatures"
)

// Name returns the name of the analyzer.
func (d *IdentityDetector) Name() string {
	return "IdentityDetector"
}

// Flag returns the CLI flag for the analyzer.
func (d *IdentityDetector) Flag() string {
	return "identity-detector"
}

// Description returns a human-readable description of the analyzer.
func (d *IdentityDetector) Description() string {
	return "Determines the author of a commit."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (d *IdentityDetector) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name:        ConfigIdentityDetectorPeopleDictPath,
		Description: "Path to the file with developer -> name|email associations.",
		Flag:        "people-dict",
		Type:        pipeline.PathConfigurationOption,
		Default:     ""}, {
		Name: ConfigIdentityDetectorExactSignatures,
		//nolint:misspell // spelling is intentional.
		Description: "Disable separate name/email matching. This will lead to considerbly more " +
			"identities and should not be normally used.",
		Flag:    "exact-signatures",
		Type:    pipeline.BoolConfigurationOption,
		Default: false},
	}
}

// Configure sets up the analyzer with the provided facts.
func (d *IdentityDetector) Configure(facts map[string]any) error {
	if val, exists := facts[identity.FactIdentityDetectorPeopleDict].(map[string]int); exists {
		d.PeopleDict = val
	}

	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		d.ReversedPeopleDict = val
	}

	if val, exists := facts[ConfigIdentityDetectorExactSignatures].(bool); exists {
		d.ExactSignatures = val
	}

	if d.PeopleDict != nil && d.ReversedPeopleDict != nil {
		return nil
	}

	peopleDictPath, pathOK := facts[ConfigIdentityDetectorPeopleDictPath].(string)
	if pathOK && peopleDictPath != "" {
		return d.LoadPeopleDict(peopleDictPath)
	}

	// In explicit mode, we expect initialization to handle this if commits are available.
	// The original logic uses ConfigPipelineCommits.
	// Here we assume facts["commits"] or similar might be populated by the runner.
	// Let's rely on explicit LoadPeopleDict or Generate if needed.
	if commits, commitsOK := facts["commits"].([]*object.Commit); commitsOK {
		d.GeneratePeopleDict(commits)
	} else if pipelineCommits, pipelineOK := facts["Pipeline.Commits"].([]*object.Commit); pipelineOK {
		d.GeneratePeopleDict(pipelineCommits)
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (d *IdentityDetector) Initialize(_ *git.Repository) error {
	return nil
}

// Consume processes a single commit with the provided dependency results.
func (d *IdentityDetector) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit

	var authorID int

	var exists bool

	signature := commit.Author
	if !d.ExactSignatures {
		authorID, exists = d.PeopleDict[strings.ToLower(signature.Email)]
		if !exists {
			authorID, exists = d.PeopleDict[strings.ToLower(signature.Name)]
		}
	} else {
		authorID, exists = d.PeopleDict[strings.ToLower(signature.String())]
	}

	if !exists {
		authorID = identity.AuthorMissing
	}

	d.AuthorID = authorID

	return nil
}

// LoadPeopleDict loads the author identity mapping from a file.
func (d *IdentityDetector) LoadPeopleDict(path string) error {
	file, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("loadPeopleDict: %w", err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	dict := make(map[string]int)

	var reverseDict []string

	size := 0

	for scanner.Scan() {
		ids := strings.Split(scanner.Text(), "|")
		for _, id := range ids {
			dict[strings.ToLower(id)] = size
		}

		reverseDict = append(reverseDict, ids[0])
		size++
	}

	reverseDict = append(reverseDict, identity.AuthorMissingName)
	d.PeopleDict = dict
	d.ReversedPeopleDict = reverseDict

	return nil
}

// GeneratePeopleDict builds the author identity mapping.
func (d *IdentityDetector) GeneratePeopleDict(commits []*object.Commit) {
	if d.ExactSignatures {
		d.generateExactDict(commits)
	} else {
		d.generateLooseDict(commits)
	}
}

func (d *IdentityDetector) generateExactDict(commits []*object.Commit) {
	dict := map[string]int{}
	size := 0

	for _, commit := range commits {
		sig := strings.ToLower(commit.Author.String())
		if _, exists := dict[sig]; !exists {
			dict[sig] = size
			size++
		}
	}

	reverseDict := make([]string, size)

	for key, val := range dict {
		reverseDict[val] = key
	}

	d.PeopleDict = dict
	d.ReversedPeopleDict = reverseDict
}

func (d *IdentityDetector) generateLooseDict(commits []*object.Commit) {
	dict := map[string]int{}
	emails := map[int][]string{}
	names := map[int][]string{}
	size := 0

	for _, commit := range commits {
		email := strings.ToLower(commit.Author.Email)
		name := strings.ToLower(commit.Author.Name)

		size = registerLooseIdentity(dict, emails, names, email, name, size)
	}

	reverseDict := make([]string, size)

	for _, val := range dict {
		sort.Strings(names[val])
		sort.Strings(emails[val])
		reverseDict[val] = strings.Join(names[val], "|") + "|" + strings.Join(emails[val], "|")
	}

	d.PeopleDict = dict
	d.ReversedPeopleDict = reverseDict
}

func registerLooseIdentity(dict map[string]int, emails, names map[int][]string, email, name string, size int) int {
	id, exists := dict[email]
	if exists {
		if _, nameExists := dict[name]; !nameExists {
			dict[name] = id
			names[id] = append(names[id], name)
		}

		return size
	}

	id, exists = dict[name]
	if exists {
		dict[email] = id
		emails[id] = append(emails[id], email)

		return size
	}

	dict[email] = size
	dict[name] = size
	emails[size] = append(emails[size], email)
	names[size] = append(names[size], name)

	return size + 1
}

// Finalize completes the analysis and returns the result.
func (d *IdentityDetector) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (d *IdentityDetector) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *d
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (d *IdentityDetector) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (d *IdentityDetector) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
