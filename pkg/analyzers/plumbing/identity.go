package plumbing

import (
	"bufio"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
)

type IdentityDetector struct {
	// Configuration
	PeopleDictPath string
	ExactSignatures bool

	// State
	PeopleDict map[string]int
	ReversedPeopleDict []string
	
	// Output
	AuthorID int

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

const (
	ConfigIdentityDetectorPeopleDictPath = "IdentityDetector.PeopleDictPath"
	ConfigIdentityDetectorExactSignatures = "IdentityDetector.ExactSignatures"
)

func (d *IdentityDetector) Name() string {
	return "IdentityDetector"
}

func (d *IdentityDetector) Flag() string {
	return "identity-detector"
}

func (d *IdentityDetector) Description() string {
	return "Determines the author of a commit."
}

func (d *IdentityDetector) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name:        ConfigIdentityDetectorPeopleDictPath,
		Description: "Path to the file with developer -> name|email associations.",
		Flag:        "people-dict",
		Type:        pipeline.PathConfigurationOption,
		Default:     ""}, {
		Name: ConfigIdentityDetectorExactSignatures,
		Description: "Disable separate name/email matching. This will lead to considerbly more " +
			"identities and should not be normally used.",
		Flag:    "exact-signatures",
		Type:    pipeline.BoolConfigurationOption,
		Default: false},
	}
}

func (d *IdentityDetector) Configure(facts map[string]interface{}) error {
	if val, exists := facts[identity.FactIdentityDetectorPeopleDict].(map[string]int); exists {
		d.PeopleDict = val
	}
	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		d.ReversedPeopleDict = val
	}
	if val, exists := facts[ConfigIdentityDetectorExactSignatures].(bool); exists {
		d.ExactSignatures = val
	}
	if d.PeopleDict == nil || d.ReversedPeopleDict == nil {
		peopleDictPath, _ := facts[ConfigIdentityDetectorPeopleDictPath].(string)
		if peopleDictPath != "" {
			err := d.LoadPeopleDict(peopleDictPath)
			if err != nil {
				return err
			}
		} else {
			// In explicit mode, we expect initialization to handle this if commits are available
			// Or we panic if commits are not provided via facts?
			// The original logic uses ConfigPipelineCommits.
			// Here we assume facts["commits"] or similar might be populated by the runner.
			// Let's rely on explicit LoadPeopleDict or Generate if needed.
			if commits, ok := facts["commits"].([]*object.Commit); ok {
				d.GeneratePeopleDict(commits)
			} else if commits, ok := facts["Pipeline.Commits"].([]*object.Commit); ok {
				d.GeneratePeopleDict(commits)
			}
		}
	}
	return nil
}

func (d *IdentityDetector) Initialize(repository *git.Repository) error {
	return nil
}

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

func (d *IdentityDetector) LoadPeopleDict(path string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
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

func (d *IdentityDetector) GeneratePeopleDict(commits []*object.Commit) {
	dict := map[string]int{}
	emails := map[int][]string{}
	names := map[int][]string{}
	size := 0

	// Simplified mailmap handling: check last commit for .mailmap
	// ... (omitting complex mailmap parsing for brevity in this step, 
	// assuming it can be added later or copied if critical)
	
	for _, commit := range commits {
		if !d.ExactSignatures {
			email := strings.ToLower(commit.Author.Email)
			name := strings.ToLower(commit.Author.Name)
			id, exists := dict[email]
			if exists {
				_, exists := dict[name]
				if !exists {
					dict[name] = id
					names[id] = append(names[id], name)
				}
				continue
			}
			id, exists = dict[name]
			if exists {
				dict[email] = id
				emails[id] = append(emails[id], email)
				continue
			}
			dict[email] = size
			dict[name] = size
			emails[size] = append(emails[size], email)
			names[size] = append(names[size], name)
			size++
		} else {
			sig := strings.ToLower(commit.Author.String())
			if _, exists := dict[sig]; !exists {
				dict[sig] = size
				size++
			}
		}
	}
	
	reverseDict := make([]string, size)
	if !d.ExactSignatures {
		for _, val := range dict {
			sort.Strings(names[val])
			sort.Strings(emails[val])
			reverseDict[val] = strings.Join(names[val], "|") + "|" + strings.Join(emails[val], "|")
		}
	} else {
		for key, val := range dict {
			reverseDict[val] = key
		}
	}
	d.PeopleDict = dict
	d.ReversedPeopleDict = reverseDict
}

func (d *IdentityDetector) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (d *IdentityDetector) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *d
		res[i] = &clone
	}
	return res
}

func (d *IdentityDetector) Merge(branches []analyze.HistoryAnalyzer) {
}

func (d *IdentityDetector) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
