package plumbing

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path"
	"regexp"
	"strings"

	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing/langpath"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pathfilter"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// TreeDiffAnalyzer computes tree-level diffs between commits.
type TreeDiffAnalyzer struct {
	NameFilter     *regexp.Regexp
	Languages      map[string]bool
	previousTree   *gitlib.Tree
	Repository     *gitlib.Repository
	SkipFiles      []string
	pathFilter     *pathfilter.Filter
	Changes        gitlib.Changes
	previousCommit gitlib.Hash
	// Pathspec holds pre-computed libgit2 pathspec globs derived from the
	// configured --languages set via langpath.Globs. Empty when no language
	// restriction applies.
	Pathspec []string
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

// ErrInvalidSkipFiles indicates a type assertion failure for SkipFiles configuration.
var ErrInvalidSkipFiles = errors.New("expected []string for SkipFiles")

// defaultBlacklistedPrefixes: path prefixes only (e.g. vendor/). No language-specific filenames.
var defaultBlacklistedPrefixes = []string{
	"vendor/",
	"vendors/",
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
	return t.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (t *TreeDiffAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeHistory,
		t.Name(),
		"Generates the list of changes for a commit.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (t *TreeDiffAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name: ConfigTreeDiffEnableBlacklist,
		Description: "Skip blacklisted directories, vendored files (enry.IsVendor), " +
			"and generated files (e.g., .pb.go, zz_generated*).",
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

// applyLanguageConfig normalises the user-supplied language tokens into
// the canonical Languages set and, when the set restricts by language,
// pre-computes the libgit2 pathspec globs via langpath.Globs.
//
// Aliases (e.g. "golang" → "Go", "js" → "JavaScript") are resolved via
// enry so that the Go-side filter keys match the canonical lowercase
// name returned by enry.GetLanguage for detected files.
func (t *TreeDiffAnalyzer) applyLanguageConfig(val []string) error {
	t.Languages = map[string]bool{}

	for _, lang := range val {
		token := strings.TrimSpace(lang)
		if strings.EqualFold(token, allLanguages) {
			t.Languages[allLanguages] = true

			continue
		}

		canonical, ok := enry.GetLanguageByAlias(token)
		if !ok {
			// langpath.Globs below will reject the same token with a
			// richer error; fall through so the caller sees that error.
			t.Languages[strings.ToLower(token)] = true

			continue
		}

		t.Languages[strings.ToLower(canonical)] = true
	}

	globs, wantsAll, err := langpath.Globs(val)
	if err != nil {
		return fmt.Errorf("tree-diff pathspec: %w", err)
	}

	if wantsAll {
		t.Pathspec = nil
	} else {
		t.Pathspec = globs
	}

	return nil
}

// Configure sets up the analyzer with the provided facts.
func (t *TreeDiffAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigTreeDiffEnableBlacklist].(bool); exists && val {
		skipFiles, ok := facts[ConfigTreeDiffBlacklistedPrefixes].([]string)
		if !ok {
			return ErrInvalidSkipFiles
		}

		t.SkipFiles = skipFiles
		t.pathFilter = pathfilter.New()
	}

	if val, exists := facts[ConfigTreeDiffLanguages].([]string); exists {
		err := t.applyLanguageConfig(val)
		if err != nil {
			return err
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
func (t *TreeDiffAnalyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	if ac != nil && ac.Changes != nil {
		t.Changes = t.filterChanges(ctx, ac.Changes)

		return analyze.TC{}, nil
	}

	return analyze.TC{}, t.computeTreeDiff(ctx, ac.Commit)
}

// computeTreeDiff performs traditional tree diff computation as a fallback.
func (t *TreeDiffAnalyzer) computeTreeDiff(ctx context.Context, commit analyze.CommitLike) error {
	tree, err := commit.Tree()
	if err != nil {
		return fmt.Errorf("consume: %w", err)
	}

	t.ensurePreviousTree(commit)

	changes, err := t.diffTrees(ctx, tree)
	if err != nil {
		return err
	}

	if t.previousTree != nil {
		t.previousTree.Free()
	}

	t.previousTree = tree
	t.previousCommit = commit.Hash()
	t.Changes = t.filterChanges(ctx, changes)

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
func (t *TreeDiffAnalyzer) diffTrees(ctx context.Context, tree *gitlib.Tree) (gitlib.Changes, error) {
	if t.previousTree != nil {
		changes, err := gitlib.TreeDiff(ctx, t.Repository, t.previousTree, tree)
		if err != nil {
			return nil, fmt.Errorf("consume: %w", err)
		}

		return changes, nil
	}

	return gitlib.InitialTreeChanges(ctx, t.Repository, tree)
}

func (t *TreeDiffAnalyzer) filterChanges(ctx context.Context, changes gitlib.Changes) gitlib.Changes {
	filtered := make(gitlib.Changes, 0, len(changes))

	for _, change := range changes {
		if t.shouldIncludeChange(ctx, change) {
			filtered = append(filtered, change)
		}
	}

	return filtered
}

func (t *TreeDiffAnalyzer) shouldIncludeChange(ctx context.Context, change *gitlib.Change) bool {
	name, hash := changeNameHash(change)

	// Check blacklist: user-specified prefixes + vendor/generated detection.
	if len(t.SkipFiles) > 0 && t.isBlacklisted(name) {
		return false
	}

	// Check whitelist regex.
	if t.NameFilter != nil && !t.NameFilter.MatchString(name) {
		return false
	}

	// Check language filter.
	if !t.Languages[allLanguages] {
		pass, err := t.checkLanguage(ctx, name, hash)
		if err != nil || !pass {
			return false
		}
	}

	return true
}

// changeNameHash returns the relevant file name and hash for a change entry.
func changeNameHash(change *gitlib.Change) (string, gitlib.Hash) {
	if change.Action == gitlib.Delete {
		return change.From.Name, change.From.Hash
	}

	return change.To.Name, change.To.Hash
}

// isBlacklisted checks whether a file path matches any blacklist prefix or is
// excluded by the path filter.
func (t *TreeDiffAnalyzer) isBlacklisted(name string) bool {
	for _, prefix := range t.SkipFiles {
		if strings.HasPrefix(name, prefix) {
			return true
		}
	}

	return t.pathFilter != nil && t.pathFilter.IsExcluded(name)
}

func (t *TreeDiffAnalyzer) checkLanguage(ctx context.Context, fileName string, hash gitlib.Hash) (bool, error) {
	if t.Languages[allLanguages] {
		return true, nil
	}

	lang := enry.GetLanguage(path.Base(fileName), nil)
	if lang == "" {
		// Try to detect from content.
		blob, err := t.Repository.LookupBlob(ctx, hash)
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

// WorkingStateSize returns 0 — plumbing analyzers are excluded from budget planning.
func (t *TreeDiffAnalyzer) WorkingStateSize() int64 { return 0 }

// AvgTCSize returns 0 — plumbing analyzers do not emit meaningful TC payloads.
func (t *TreeDiffAnalyzer) AvgTCSize() int64 { return 0 }

// NewAggregator returns nil — plumbing analyzers do not aggregate.
func (t *TreeDiffAnalyzer) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator { return nil }

// SerializeTICKs returns ErrNotImplemented — plumbing analyzers do not produce TICKs.
func (t *TreeDiffAnalyzer) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

// ReportFromTICKs returns ErrNotImplemented — plumbing analyzers do not produce reports.
func (t *TreeDiffAnalyzer) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}

// InjectPreparedData sets pre-computed changes from parallel preparation.
func (t *TreeDiffAnalyzer) InjectPreparedData(
	changes []*gitlib.Change,
	_ map[gitlib.Hash]*gitlib.CachedBlob,
	_ any,
) {
	t.Changes = changes
}
