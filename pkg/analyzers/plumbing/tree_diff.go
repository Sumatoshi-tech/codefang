package plumbing

import (
	"errors"
	"fmt"
	"io"
	"path"
	"regexp"
	"strings"

	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	bufferValue = 1024
)

// TreeDiffAnalyzer computes tree-level diffs between commits.
type TreeDiffAnalyzer struct {
	l              interface{ Critical(args ...any) } //nolint:unused // used via dependency injection.
	NameFilter     *regexp.Regexp
	Languages      map[string]bool
	previousTree   *object.Tree
	repository     *git.Repository
	SkipFiles      []string
	Changes        object.Changes
	previousCommit plumbing.Hash
}

const (
	// ConfigTreeDiffEnableBlacklist is the configuration key for enabling path blacklisting.
	ConfigTreeDiffEnableBlacklist = "TreeDiff.EnableBlacklist"
	// ConfigTreeDiffBlacklistedPrefixes is the configuration key for path prefixes to exclude from diffs.
	ConfigTreeDiffBlacklistedPrefixes = "TreeDiff.BlacklistedPrefixes"
	// ConfigTreeDiffLanguages is the configuration key for filtering by programming language.
	ConfigTreeDiffLanguages = "TreeDiff.LanguagesDetection"
	// ConfigTreeDiffFilterRegexp is the configuration key for the file path filter regular expression.
	ConfigTreeDiffFilterRegexp = "TreeDiff.FilteredRegexes"
	allLanguages               = "all"
)

var defaultBlacklistedPrefixes = []string{ //nolint:gochecknoglobals // global is needed for registration.
	"vendor/",
	"vendors/",
	"package-lock.json",
	"Gopkg.lock",
}

// Name returns the name of the analyzer.
func (t *TreeDiffAnalyzer) Name() string {
	return "TreeDiff"
}

// Flag returns the CLI flag for the analyzer.
func (t *TreeDiffAnalyzer) Flag() string {
	return "tree-diff"
}

// Description returns a human-readable description of the analyzer.
func (t *TreeDiffAnalyzer) Description() string {
	return "Generates the list of changes for a commit."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (t *TreeDiffAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name: ConfigTreeDiffEnableBlacklist,
		Description: "Skip blacklisted directories and vendored files (according to " +
			"src-d/enry.IsVendor).",
		Flag:    "skip-blacklist",
		Type:    pipeline.BoolConfigurationOption,
		Default: false}, {

		Name: ConfigTreeDiffBlacklistedPrefixes,
		Description: "List of blacklisted path prefixes (e.g. directories or specific files). " +
			"Values are in the UNIX format (\"path/to/x\"). Values should *not* start with \"/\". " +
			"Separated with commas \",\".",
		Flag:    "blacklisted-prefixes",
		Type:    pipeline.StringsConfigurationOption,
		Default: defaultBlacklistedPrefixes}, {

		Name: ConfigTreeDiffLanguages,
		Description: fmt.Sprintf(
			"List of programming languages to analyze. Separated by comma \",\". "+
				"The names are the keys in https://github.com/github/linguist/blob/master/lib/linguist/languages.yml "+
				"\"%s\" is the special name which disables this filter and lets all the files through.",
			allLanguages),
		Flag:    "languages",
		Type:    pipeline.StringsConfigurationOption,
		Default: []string{allLanguages}}, {

		Name:        ConfigTreeDiffFilterRegexp,
		Description: "Whitelist regexp to determine which files to analyze.",
		Flag:        "whitelist",
		Type:        pipeline.StringConfigurationOption,
		Default:     ""},
	}
}

// Configure sets up the analyzer with the provided facts.
func (t *TreeDiffAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTreeDiffEnableBlacklist].(bool); exists && val {
		skipFiles, ok := facts[ConfigTreeDiffBlacklistedPrefixes].([]string)
		if !ok {
			return errors.New("expected []string for SkipFiles") //nolint:err113 // descriptive error for type assertion failure.
		}

		t.SkipFiles = skipFiles
	}

	if val, exists := facts[ConfigTreeDiffLanguages].([]string); exists {
		t.Languages = map[string]bool{}
		for _, lang := range val {
			t.Languages[strings.ToLower(strings.TrimSpace(lang))] = true
		}
	} else if t.Languages == nil {
		t.Languages = map[string]bool{}
		t.Languages[allLanguages] = true
	}

	if val, exists := facts[ConfigTreeDiffFilterRegexp].(string); exists {
		t.NameFilter = regexp.MustCompile(val)
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (t *TreeDiffAnalyzer) Initialize(repository *git.Repository) error {
	t.previousTree = nil

	t.repository = repository
	if t.Languages == nil {
		t.Languages = map[string]bool{}
		t.Languages[allLanguages] = true
	}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (t *TreeDiffAnalyzer) Consume(ctx *analyze.Context) error { //nolint:gocognit // complex function.
	commit := ctx.Commit

	// If not child of previous commit, we proceed anyway (sequential assumption).
	// Discontinuous feeds (e.g. merge parent switching) are tolerated.

	tree, err := commit.Tree()
	if err != nil {
		return fmt.Errorf("consume: %w", err)
	}

	var diffs object.Changes
	if t.previousTree != nil { //nolint:nestif // nested logic is clear in context.
		diffs, err = object.DiffTree(t.previousTree, tree)
		if err != nil {
			return fmt.Errorf("consume: %w", err)
		}
	} else {
		diffs = []*object.Change{}
		// First commit or reset.
		err = func() error {
			fileIter := tree.Files()
			defer fileIter.Close()

			for {
				file, iterErr := fileIter.Next()
				if iterErr != nil {
					if iterErr == io.EOF { //nolint:errorlint // error comparison is intentional.
						break
					}

					return fmt.Errorf("consume: %w", iterErr)
				}

				pass, langErr := t.checkLanguage(file.Name, file.Hash)
				if langErr != nil {
					return langErr
				}

				if !pass {
					continue
				}

				diffs = append(diffs, &object.Change{
					To: object.ChangeEntry{Name: file.Name, Tree: tree, TreeEntry: object.TreeEntry{
						Name: file.Name, Mode: file.Mode, Hash: file.Hash}}})
			}

			return nil
		}()
		if err != nil {
			return err
		}
	}

	t.previousTree = tree
	t.previousCommit = commit.Hash
	t.Changes = t.filterDiffs(diffs)

	return nil
}

//nolint:gocognit // complex function.
func (t *TreeDiffAnalyzer) filterDiffs(diffs object.Changes) object.Changes {
	filteredDiffs := make(object.Changes, 0, len(diffs))

OUTER:
	for _, change := range diffs {
		if len(t.SkipFiles) > 0 && (enry.IsVendor(change.To.Name) || enry.IsVendor(change.From.Name)) {
			continue
		}

		for _, dir := range t.SkipFiles {
			if strings.HasPrefix(change.To.Name, dir) || strings.HasPrefix(change.From.Name, dir) {
				continue OUTER
			}
		}

		if t.NameFilter != nil {
			matchedTo := t.NameFilter.MatchString(change.To.Name)
			matchedFrom := t.NameFilter.MatchString(change.From.Name)

			if !matchedTo && !matchedFrom {
				continue
			}
		}

		var changeEntry object.ChangeEntry
		if change.To.Tree == nil {
			changeEntry = change.From
		} else {
			changeEntry = change.To
		}

		//nolint:errcheck // error return value is intentionally ignored.
		if pass, _ := t.checkLanguage(changeEntry.Name, changeEntry.TreeEntry.Hash); !pass {
			continue
		}

		filteredDiffs = append(filteredDiffs, change)
	}

	return filteredDiffs
}

func (t *TreeDiffAnalyzer) checkLanguage(name string, blobHash plumbing.Hash) (bool, error) {
	if t.Languages[allLanguages] {
		return true, nil
	}

	blob, err := t.repository.BlobObject(blobHash)
	if err != nil {
		return false, fmt.Errorf("checkLanguage: %w", err)
	}

	reader, err := blob.Reader()
	if err != nil {
		return false, fmt.Errorf("checkLanguage: %w", err)
	}

	buffer := make([]byte, bufferValue)

	n, err := reader.Read(buffer)
	if err != nil && (blob.Size != 0 || err != io.EOF) { //nolint:errorlint // error comparison is intentional.
		return false, fmt.Errorf("checkLanguage: %w", err)
	}

	if n < len(buffer) {
		buffer = buffer[:n]
	}

	lang := strings.ToLower(enry.GetLanguage(path.Base(name), buffer))

	return t.Languages[lang], nil
}

// Finalize completes the analysis and returns the result.
func (t *TreeDiffAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (t *TreeDiffAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *t
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *TreeDiffAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (t *TreeDiffAnalyzer) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
