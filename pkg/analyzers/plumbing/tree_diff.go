package plumbing

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path"
	"regexp"
	"strings"

	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// TreeDiffAnalyzer computes tree-level diffs between commits.
type TreeDiffAnalyzer struct {
	l              interface{ Critical(args ...any) } //nolint:unused // used via dependency injection.
	NameFilter     *regexp.Regexp
	Languages      map[string]bool
	previousTree   *gitlib.Tree
	Repository     *gitlib.Repository
	SkipFiles      []string
	Changes        gitlib.Changes
	previousCommit gitlib.Hash
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
func (t *TreeDiffAnalyzer) Initialize(repository *gitlib.Repository) error {
	t.previousTree = nil
	t.Repository = repository

	if t.Languages == nil {
		t.Languages = map[string]bool{}
		t.Languages[allLanguages] = true
	}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (t *TreeDiffAnalyzer) Consume(ctx *analyze.Context) error {
	if ctx != nil && ctx.Changes != nil {
		t.Changes = t.filterChanges(ctx.Changes)

		return nil
	}

	return t.computeTreeDiff(ctx.Commit)
}

// computeTreeDiff performs traditional tree diff computation as a fallback.
func (t *TreeDiffAnalyzer) computeTreeDiff(commit analyze.CommitLike) error {
	tree, err := commit.Tree()
	if err != nil {
		return fmt.Errorf("consume: %w", err)
	}

	t.ensurePreviousTree(commit)

	changes, err := t.diffTrees(tree)
	if err != nil {
		return err
	}

	if t.previousTree != nil {
		t.previousTree.Free()
	}

	t.previousTree = tree
	t.previousCommit = commit.Hash()
	t.Changes = t.filterChanges(changes)

	return nil
}

// ensurePreviousTree fetches the parent tree if needed for parallel processing.
func (t *TreeDiffAnalyzer) ensurePreviousTree(commit analyze.CommitLike) {
	if t.previousTree != nil || commit.NumParents() == 0 {
		return
	}

	parent, err := commit.Parent(0)
	if err != nil || parent == nil {
		return
	}

	defer parent.Free()

	tree, treeErr := parent.Tree()
	if treeErr == nil {
		t.previousTree = tree
	}
}

// diffTrees computes the diff between previous tree and current tree.
func (t *TreeDiffAnalyzer) diffTrees(tree *gitlib.Tree) (gitlib.Changes, error) {
	if t.previousTree != nil {
		changes, err := gitlib.TreeDiff(t.Repository, t.previousTree, tree)
		if err != nil {
			return nil, fmt.Errorf("consume: %w", err)
		}

		return changes, nil
	}

	return gitlib.InitialTreeChanges(t.Repository, tree)
}

func (t *TreeDiffAnalyzer) filterChanges(changes gitlib.Changes) gitlib.Changes {
	filtered := make(gitlib.Changes, 0, len(changes))

	for _, change := range changes {
		if t.shouldIncludeChange(change) {
			filtered = append(filtered, change)
		}
	}

	return filtered
}

func (t *TreeDiffAnalyzer) shouldIncludeChange(change *gitlib.Change) bool {
	var name string

	var hash gitlib.Hash

	switch change.Action {
	case gitlib.Insert:
		name = change.To.Name
		hash = change.To.Hash
	case gitlib.Delete:
		name = change.From.Name
		hash = change.From.Hash
	case gitlib.Modify:
		name = change.To.Name
		hash = change.To.Hash
	}

	// Check blacklist.
	if len(t.SkipFiles) > 0 {
		for _, prefix := range t.SkipFiles {
			if strings.HasPrefix(name, prefix) {
				return false
			}
		}

		if enry.IsVendor(name) {
			return false
		}
	}

	// Check whitelist regex.
	if t.NameFilter != nil && !t.NameFilter.MatchString(name) {
		return false
	}

	// Check language filter.
	if !t.Languages[allLanguages] {
		pass, err := t.checkLanguage(name, hash)
		if err != nil || !pass {
			return false
		}
	}

	return true
}

func (t *TreeDiffAnalyzer) checkLanguage(fileName string, hash gitlib.Hash) (bool, error) {
	if t.Languages[allLanguages] {
		return true, nil
	}

	lang := enry.GetLanguage(path.Base(fileName), nil)
	if lang == "" {
		// Try to detect from content.
		blob, err := t.Repository.LookupBlob(hash)
		if err == nil {
			defer blob.Free()

			contents := blob.Contents()
			if len(contents) > 0 {
				lang = enry.GetLanguage(path.Base(fileName), contents)
			}
		}
	}

	if lang == "" {
		return false, nil
	}

	return t.Languages[strings.ToLower(lang)], nil
}

// Finalize completes the analysis and returns the result.
func (t *TreeDiffAnalyzer) Finalize() (analyze.Report, error) {
	// Clean up.
	if t.previousTree != nil {
		t.previousTree.Free()
		t.previousTree = nil
	}

	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (t *TreeDiffAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *t
		clone.previousTree = nil // Each fork starts fresh.
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (t *TreeDiffAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (t *TreeDiffAnalyzer) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

// InjectPreparedData sets pre-computed changes from parallel preparation.
func (t *TreeDiffAnalyzer) InjectPreparedData(
	changes []*gitlib.Change,
	_ map[gitlib.Hash]*gitlib.CachedBlob,
	_ any,
) {
	t.Changes = changes
}
