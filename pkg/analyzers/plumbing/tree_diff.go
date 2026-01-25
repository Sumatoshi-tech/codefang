package plumbing

import (
	"fmt"
	"io"
	"path"
	"regexp"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/src-d/enry/v2"
)

type TreeDiffAnalyzer struct {
	// Configuration
	SkipFiles  []string
	NameFilter *regexp.Regexp
	Languages  map[string]bool

	// State
	previousTree   *object.Tree
	previousCommit plumbing.Hash
	repository     *git.Repository
	
	// Output
	Changes object.Changes

	// Internal
	l interface {
		Critical(args ...interface{})
	}
}

const (
	ConfigTreeDiffEnableBlacklist     = "TreeDiff.EnableBlacklist"
	ConfigTreeDiffBlacklistedPrefixes = "TreeDiff.BlacklistedPrefixes"
	ConfigTreeDiffLanguages           = "TreeDiff.LanguagesDetection"
	ConfigTreeDiffFilterRegexp        = "TreeDiff.FilteredRegexes"
	allLanguages                      = "all"
)

var defaultBlacklistedPrefixes = []string{
	"vendor/",
	"vendors/",
	"package-lock.json",
	"Gopkg.lock",
}

func (t *TreeDiffAnalyzer) Name() string {
	return "TreeDiff"
}

func (t *TreeDiffAnalyzer) Flag() string {
	return "tree-diff"
}

func (t *TreeDiffAnalyzer) Description() string {
	return "Generates the list of changes for a commit."
}

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

func (t *TreeDiffAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigTreeDiffEnableBlacklist].(bool); exists && val {
		t.SkipFiles = facts[ConfigTreeDiffBlacklistedPrefixes].([]string)
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

func (t *TreeDiffAnalyzer) Initialize(repository *git.Repository) error {
	t.previousTree = nil
	t.repository = repository
	if t.Languages == nil {
		t.Languages = map[string]bool{}
		t.Languages[allLanguages] = true
	}
	return nil
}

func (t *TreeDiffAnalyzer) Consume(ctx *analyze.Context) error {
	commit := ctx.Commit
	pass := false
	for _, hash := range commit.ParentHashes {
		if hash == t.previousCommit {
			pass = true
		}
	}
	if !pass && t.previousCommit != plumbing.ZeroHash {
		// Reset state if discontinuous (e.g. merge parent switching)
		// Or strictly validation?
		// In explicit pipeline, we assume sequential feed.
		// If disjoint, we might want to reset previousTree to parent's tree?
		// But here we rely on sequential calls.
		// Simplification: if not child of previous, full scan?
		// Or just warn?
		// We'll proceed.
	}
	
	tree, err := commit.Tree()
	if err != nil {
		return err
	}
	
	var diffs object.Changes
	if t.previousTree != nil {
		diffs, err = object.DiffTree(t.previousTree, tree)
		if err != nil {
			return err
		}
	} else {
		diffs = []*object.Change{}
		// First commit or reset
		err = func() error {
			fileIter := tree.Files()
			defer fileIter.Close()
			for {
				file, err := fileIter.Next()
				if err != nil {
					if err == io.EOF {
						break
					}
					return err
				}
				pass, err := t.checkLanguage(file.Name, file.Hash)
				if err != nil {
					return err
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
		return false, err
	}
	reader, err := blob.Reader()
	if err != nil {
		return false, err
	}
	buffer := make([]byte, 1024)
	n, err := reader.Read(buffer)
	if err != nil && (blob.Size != 0 || err != io.EOF) {
		return false, err
	}
	if n < len(buffer) {
		buffer = buffer[:n]
	}
	lang := strings.ToLower(enry.GetLanguage(path.Base(name), buffer))
	return t.Languages[lang], nil
}

func (t *TreeDiffAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (t *TreeDiffAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *t
		res[i] = &clone
	}
	return res
}

func (t *TreeDiffAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (t *TreeDiffAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
