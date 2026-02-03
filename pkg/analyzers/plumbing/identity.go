package plumbing

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
	// incrementalEmails and incrementalNames are used when building the dict incrementally
	// during Consume() when commits aren't available during Configure().
	incrementalEmails map[int][]string
	incrementalNames  map[int][]string
	incrementalSize   int
	dictFinalized     bool
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
	if commits, commitsOK := facts["commits"].([]*gitlib.Commit); commitsOK {
		d.GeneratePeopleDict(commits)
	} else if pipelineCommits, pipelineOK := facts["Pipeline.Commits"].([]*gitlib.Commit); pipelineOK {
		d.GeneratePeopleDict(pipelineCommits)
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (d *IdentityDetector) Initialize(_ *gitlib.Repository) error {
	// If PeopleDict is already set (from Configure), mark as finalized.
	if d.PeopleDict != nil {
		d.dictFinalized = true

		return nil
	}

	// Initialize for incremental building during Consume().
	d.PeopleDict = make(map[string]int)
	d.incrementalEmails = make(map[int][]string)
	d.incrementalNames = make(map[int][]string)
	d.incrementalSize = 0
	d.dictFinalized = false

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (d *IdentityDetector) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit
	signature := commit.Author()

	var (
		authorID int
		exists   bool
	)

	if d.ExactSignatures {
		authorID, exists = d.lookupExactSignature(signature)
	} else {
		authorID, exists = d.lookupLooseSignature(signature)
	}

	if !exists && d.dictFinalized {
		authorID = identity.AuthorMissing
	}

	d.AuthorID = authorID

	return nil
}

// lookupExactSignature finds or registers an author using exact signature matching.
func (d *IdentityDetector) lookupExactSignature(signature gitlib.Signature) (int, bool) {
	sigStr := strings.ToLower(fmt.Sprintf("%s <%s>", signature.Name, signature.Email))
	authorID, exists := d.PeopleDict[sigStr]

	if !exists && !d.dictFinalized {
		authorID = d.incrementalSize
		d.PeopleDict[sigStr] = authorID
		d.incrementalSize++
	}

	return authorID, exists
}

// lookupLooseSignature finds or registers an author using loose signature matching.
func (d *IdentityDetector) lookupLooseSignature(signature gitlib.Signature) (int, bool) {
	email := strings.ToLower(signature.Email)
	name := strings.ToLower(signature.Name)

	authorID, exists := d.PeopleDict[email]
	if !exists {
		authorID, exists = d.PeopleDict[name]
	}

	if !exists && !d.dictFinalized {
		d.incrementalSize = registerLooseIdentity(
			d.PeopleDict, d.incrementalEmails, d.incrementalNames,
			email, name, d.incrementalSize,
		)
		authorID = d.PeopleDict[email]
	}

	return authorID, exists
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
func (d *IdentityDetector) GeneratePeopleDict(commits []*gitlib.Commit) {
	if d.ExactSignatures {
		d.generateExactDict(commits)
	} else {
		d.generateLooseDict(commits)
	}
}

func (d *IdentityDetector) generateExactDict(commits []*gitlib.Commit) {
	dict := map[string]int{}
	size := 0

	for _, commit := range commits {
		author := commit.Author()

		sig := strings.ToLower(fmt.Sprintf("%s <%s>", author.Name, author.Email))
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

func (d *IdentityDetector) generateLooseDict(commits []*gitlib.Commit) {
	dict := map[string]int{}
	emails := map[int][]string{}
	names := map[int][]string{}
	size := 0

	for _, commit := range commits {
		author := commit.Author()
		email := strings.ToLower(author.Email)
		name := strings.ToLower(author.Name)

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
	// Build ReversedPeopleDict from incrementally collected data if needed.
	if !d.dictFinalized && d.incrementalSize > 0 {
		d.ReversedPeopleDict = make([]string, d.incrementalSize)

		if d.ExactSignatures {
			// For exact signatures, reverse the dict directly.
			for key, val := range d.PeopleDict {
				d.ReversedPeopleDict[val] = key
			}
		} else {
			// For loose matching, build readable names from emails and names.
			for id := range d.incrementalSize {
				sort.Strings(d.incrementalNames[id])
				sort.Strings(d.incrementalEmails[id])
				d.ReversedPeopleDict[id] = strings.Join(d.incrementalNames[id], "|") +
					"|" + strings.Join(d.incrementalEmails[id], "|")
			}
		}

		d.dictFinalized = true
	}

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
func (d *IdentityDetector) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

// GetAuthorID returns the author ID of the last processed commit.
func (d *IdentityDetector) GetAuthorID() int {
	return d.AuthorID
}
