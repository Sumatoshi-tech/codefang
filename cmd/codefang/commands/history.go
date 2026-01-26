package commands

import (
	"errors"
	"fmt"
	"log"
	"os"
	"regexp"
	"strings"

	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// Sentinel errors for the history command.
var (
	ErrNoAnalyzersSelected = errors.New(
		"no analyzers selected. Use -a flag, e.g.: -a burndown,couples\n" +
			"Available: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
	)
	ErrUnknownAnalyzer = errors.New("unknown analyzer")
)

// HistoryCommand holds the configuration for the history command.
type HistoryCommand struct {
	format      string
	analyzers   []string
	head        bool
	firstParent bool
}

// NewHistoryCommand creates and configures the history command.
func NewHistoryCommand() *cobra.Command {
	hc := &HistoryCommand{}

	cobraCmd := &cobra.Command{
		Use:   "history [repository]",
		Short: "Analyze git repository history",
		Long: `Analyze git repository history with various metrics.

Available analyzers:
  burndown      Line burndown stats over time
  couples       File/developer coupling matrix
  devs          Developer statistics (commits, lines added/removed)
  file-history  File change history with contributors
  imports       Import usage per developer over time
  sentiment     Comment sentiment classification
  shotness      Structural hotspots (fine-grained couples)
  typos         Typo-fix pairs extracted from diffs

Examples:
  codefang history -a burndown .
  codefang history -a burndown,couples,devs .
  codefang history -a burndown --head .
  codefang history -a devs -f json .`,
		Args: cobra.ExactArgs(1),
		RunE: hc.Run,
	}

	cobraCmd.Flags().StringSliceVarP(&hc.analyzers, "analyzers", "a", []string{},
		"Analyzers to run (comma-separated)")
	cobraCmd.Flags().StringVarP(&hc.format, "format", "f", "yaml", "Output format: yaml or json")
	cobraCmd.Flags().BoolVar(&hc.head, "head", false, "Analyze only the latest commit")
	cobraCmd.Flags().BoolVar(&hc.firstParent, "first-parent", false, "Follow only the first parent")

	return cobraCmd
}

// Run executes the history command.
func (hc *HistoryCommand) Run(_ *cobra.Command, args []string) error {
	if len(hc.analyzers) == 0 {
		return ErrNoAnalyzersSelected
	}

	uri := args[0]
	repository := loadRepository(uri)

	commits, err := hc.loadCommits(repository)
	if err != nil {
		return err
	}

	return hc.runPipeline(repository, commits)
}

func loadRepository(uri string) *git.Repository {
	var repository *git.Repository

	var err error

	if strings.Contains(uri, "://") || regexp.MustCompile(`^[A-Za-z]\w*@[A-Za-z0-9][\w.]*:`).MatchString(uri) {
		return nil // Remote repos not supported in codefang yet.
	}

	if uri[len(uri)-1] == os.PathSeparator {
		uri = uri[:len(uri)-1]
	}

	repository, err = git.PlainOpen(uri)
	if err != nil {
		log.Fatalf("failed to open %s: %v", uri, err)
	}

	return repository
}

func (hc *HistoryCommand) loadCommits(repository *git.Repository) ([]*object.Commit, error) {
	var commits []*object.Commit

	if hc.head {
		ref, err := repository.Head()
		if err != nil {
			return nil, fmt.Errorf("failed to get HEAD: %w", err)
		}

		commit, err := repository.CommitObject(ref.Hash())
		if err != nil {
			return nil, fmt.Errorf("failed to get commit: %w", err)
		}

		return []*object.Commit{commit}, nil
	}

	iter, err := repository.Log(&git.LogOptions{})
	if err != nil {
		return nil, fmt.Errorf("failed to list commits: %w", err)
	}

	err = iter.ForEach(func(commit *object.Commit) error {
		if hc.firstParent && commit.NumParents() > 1 {
			return nil
		}

		commits = append(commits, commit)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to iterate commits: %w", err)
	}

	// Reverse to oldest first.
	for i, j := 0, len(commits)-1; i < j; i, j = i+1, j-1 {
		commits[i], commits[j] = commits[j], commits[i]
	}

	return commits, nil
}

//nolint:funlen // pipeline setup requires many struct initializations
func (hc *HistoryCommand) runPipeline(repository *git.Repository, commits []*object.Commit) error {
	// Instantiate Plumbing.
	treeDiff := &plumbing.TreeDiffAnalyzer{}
	blobCache := &plumbing.BlobCacheAnalyzer{
		TreeDiff: treeDiff,
	}
	fileDiff := &plumbing.FileDiffAnalyzer{
		TreeDiff: treeDiff, BlobCache: blobCache,
	}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	langs := &plumbing.LanguagesDetectionAnalyzer{
		TreeDiff: treeDiff, BlobCache: blobCache,
	}
	lineStats := &plumbing.LinesStatsCalculator{
		TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff,
	}
	uastChanges := &plumbing.UASTChangesAnalyzer{
		FileDiff: fileDiff, BlobCache: blobCache,
	}

	// Instantiate Leaves.
	leaves := map[string]analyze.HistoryAnalyzer{
		"burndown": &burndown.BurndownHistoryAnalyzer{
			TreeDiff: treeDiff, FileDiff: fileDiff, BlobCache: blobCache,
			Identity: identity, Ticks: ticks,
		},
		"couples": &couples.CouplesHistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff,
		},
		"devs": &devs.DevsHistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff, Ticks: ticks,
			Languages: langs, LineStats: lineStats,
		},
		"file-history": &file_history.FileHistoryAnalyzer{
			Identity: identity, TreeDiff: treeDiff, LineStats: lineStats,
		},
		"imports": &imports.ImportsHistoryAnalyzer{
			TreeDiff: treeDiff, BlobCache: blobCache, Identity: identity, Ticks: ticks,
		},
		"sentiment": &sentiment.SentimentHistoryAnalyzer{
			UASTChanges: uastChanges, Ticks: ticks,
		},
		"shotness": &shotness.ShotnessHistoryAnalyzer{
			FileDiff: fileDiff, UASTChanges: uastChanges,
		},
		"typos": &typos.TyposHistoryAnalyzer{
			UASTChanges: uastChanges, FileDiff: fileDiff, BlobCache: blobCache,
		},
	}

	// Plumbing analyzers.
	plumbingAnalyzers := []analyze.HistoryAnalyzer{
		treeDiff, blobCache, fileDiff, identity, ticks, langs, lineStats, uastChanges,
	}

	var activeAnalyzers []analyze.HistoryAnalyzer

	var selectedLeaves []analyze.HistoryAnalyzer

	facts := map[string]any{}

	// Configure plumbing.
	for _, currentAnalyzer := range plumbingAnalyzers {
		configErr := currentAnalyzer.Configure(facts)
		if configErr != nil {
			return fmt.Errorf("failed to configure %s: %w", currentAnalyzer.Name(), configErr)
		}

		activeAnalyzers = append(activeAnalyzers, currentAnalyzer)
	}

	// Select and configure leaves.
	for _, name := range hc.analyzers {
		leaf, found := leaves[name]
		if !found {
			return fmt.Errorf("%w: %s\nAvailable: burndown, couples, devs, "+
				"file-history, imports, sentiment, shotness, typos", ErrUnknownAnalyzer, name)
		}

		configErr := leaf.Configure(facts)
		if configErr != nil {
			return fmt.Errorf("failed to configure %s: %w", name, configErr)
		}

		activeAnalyzers = append(activeAnalyzers, leaf)
		selectedLeaves = append(selectedLeaves, leaf)
	}

	// Run.
	runner := framework.NewRunner(repository, activeAnalyzers...)

	results, err := runner.Run(commits)
	if err != nil {
		return fmt.Errorf("pipeline execution failed: %w", err)
	}

	// Output.
	fmt.Println("codefang (v2):")                 //nolint:forbidigo // CLI user output
	fmt.Printf("  version: %d\n", version.Binary) //nolint:forbidigo // CLI user output
	fmt.Println("  hash:", version.BinaryGitHash) //nolint:forbidigo // CLI user output

	for _, leaf := range selectedLeaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		fmt.Printf("%s:\n", leaf.Name()) //nolint:forbidigo // CLI user output

		serializeErr := leaf.Serialize(res, false, os.Stdout)
		if serializeErr != nil {
			return fmt.Errorf("serialization error for %s: %w", leaf.Name(), serializeErr)
		}
	}

	return nil
}
