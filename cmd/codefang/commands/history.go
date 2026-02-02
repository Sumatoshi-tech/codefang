package commands

import (
	"errors"
	"fmt"
	"log"
	"os"
	"regexp"
	"runtime/pprof"
	"strings"
	"time"

	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// Sentinel errors for the history command.
var (
	ErrNoAnalyzersSelected = errors.New(
		"no analyzers selected. Use -a flag, e.g.: -a burndown,couples\n" +
			"Available: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
	)
	ErrUnknownAnalyzer   = errors.New("unknown analyzer")
	ErrRepositoryLoad    = errors.New("failed to load repository")
	ErrInvalidTimeFormat = errors.New("cannot parse time")
)

// HistoryCommand holds the configuration for the history command.
type HistoryCommand struct {
	format      string
	analyzers   []string
	head        bool
	firstParent bool
	cpuprofile  string
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
  imports       Import/dependency analysis
  sentiment     Commit message sentiment analysis
  shotness      Code hotspot detection
  typos         Potential typo detection in identifiers`,
		RunE: hc.run,
	}

	cobraCmd.Flags().StringVarP(&hc.format, "format", "f", "yaml", "Output format (yaml, json, plot; plot is HTML for devs only)")
	cobraCmd.Flags().StringSliceVarP(&hc.analyzers, "analyzers", "a", nil, "Analyzers to run (comma-separated)")
	cobraCmd.Flags().BoolVar(&hc.head, "head", false, "Analyze only HEAD commit")
	cobraCmd.Flags().BoolVar(&hc.firstParent, "first-parent", false, "Follow only first parent of merge commits")
	cobraCmd.Flags().Int("limit", 0, "Limit number of commits to analyze (0 = no limit)")
	cobraCmd.Flags().String("since", "", "Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)")
	cobraCmd.Flags().StringVar(&hc.cpuprofile, "cpuprofile", "", "Write CPU profile to file")

	registerAnalyzerFlags(cobraCmd)

	return cobraCmd
}

// registerAnalyzerFlags registers dynamic flags from all analyzers.
func registerAnalyzerFlags(cobraCmd *cobra.Command) {
	dummyPipeline := newAnalyzerPipeline(nil)

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(dummyPipeline.core)+len(dummyPipeline.leaves))
	allAnalyzers = append(allAnalyzers, dummyPipeline.core...)

	for _, leaf := range dummyPipeline.leaves {
		allAnalyzers = append(allAnalyzers, leaf)
	}

	registeredFlags := make(map[string]bool)

	for _, analyzer := range allAnalyzers {
		for _, opt := range analyzer.ListConfigurationOptions() {
			if registeredFlags[opt.Flag] {
				continue
			}

			registeredFlags[opt.Flag] = true
			registerConfigFlag(cobraCmd, opt)
		}
	}
}

// registerConfigFlag registers a single configuration option as a cobra flag.
func registerConfigFlag(cobraCmd *cobra.Command, opt pipeline.ConfigurationOption) {
	switch opt.Type {
	case pipeline.BoolConfigurationOption:
		if v, ok := opt.Default.(bool); ok {
			cobraCmd.Flags().Bool(opt.Flag, v, opt.Description)
		}
	case pipeline.IntConfigurationOption:
		if v, ok := opt.Default.(int); ok {
			cobraCmd.Flags().Int(opt.Flag, v, opt.Description)
		}
	case pipeline.StringConfigurationOption:
		if v, ok := opt.Default.(string); ok {
			cobraCmd.Flags().String(opt.Flag, v, opt.Description)
		}
	case pipeline.StringsConfigurationOption:
		if v, ok := opt.Default.([]string); ok {
			cobraCmd.Flags().StringSlice(opt.Flag, v, opt.Description)
		}
	case pipeline.PathConfigurationOption:
		if v, ok := opt.Default.(string); ok {
			cobraCmd.Flags().String(opt.Flag, v, opt.Description)
		}
	case pipeline.FloatConfigurationOption:
		if v, ok := opt.Default.(float64); ok {
			cobraCmd.Flags().Float64(opt.Flag, v, opt.Description)
		}
	}
}

// run executes the history analysis pipeline.
func (hc *HistoryCommand) run(cmd *cobra.Command, args []string) error {
	stopProfiler, err := hc.maybeStartCPUProfile()
	if err != nil {
		return err
	}

	defer stopProfiler()

	if len(hc.analyzers) == 0 {
		return ErrNoAnalyzersSelected
	}

	uri, err := resolveRepoURI(args)
	if err != nil {
		return err
	}

	if hc.isFastDevsMode() {
		return hc.runFastDevsAnalyzer(cmd, uri)
	}

	repository := loadRepository(uri)
	if repository == nil {
		return fmt.Errorf("%w: %s", ErrRepositoryLoad, uri)
	}
	defer repository.Free()

	hc.ensureBurndownFirstParent()

	commits, err := hc.loadCommits(repository, cmd)
	if err != nil {
		return err
	}

	facts := hc.buildFacts(cmd)

	return hc.runPipeline(repository, uri, commits, hc.format, facts)
}

func (hc *HistoryCommand) maybeStartCPUProfile() (func(), error) {
	if hc.cpuprofile == "" {
		return func() {}, nil
	}

	profileFile, err := os.Create(hc.cpuprofile)
	if err != nil {
		return nil, fmt.Errorf("could not create CPU profile: %w", err)
	}

	err = pprof.StartCPUProfile(profileFile)
	if err != nil {
		profileFile.Close()

		return nil, fmt.Errorf("could not start CPU profile: %w", err)
	}

	stopAndClose := func() {
		pprof.StopCPUProfile()

		_ = profileFile.Close()
	}

	return stopAndClose, nil
}

func (hc *HistoryCommand) ensureBurndownFirstParent() {
	if hc.isBurndownOnly() && !hc.firstParent {
		hc.firstParent = true
	}
}

func (hc *HistoryCommand) buildFacts(cmd *cobra.Command) map[string]any {
	facts := map[string]any{}
	dummyPipeline := newAnalyzerPipeline(nil)

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(dummyPipeline.core)+len(dummyPipeline.leaves))
	allAnalyzers = append(allAnalyzers, dummyPipeline.core...)

	for _, leaf := range dummyPipeline.leaves {
		allAnalyzers = append(allAnalyzers, leaf)
	}

	for _, analyzer := range allAnalyzers {
		for _, opt := range analyzer.ListConfigurationOptions() {
			loadFlagValue(cmd, opt, facts)
		}
	}

	return facts
}

// isFastDevsMode returns true if only the fast devs analyzer is selected.
func (hc *HistoryCommand) isFastDevsMode() bool {
	return len(hc.analyzers) == 1 && hc.analyzers[0] == "devs"
}

// isBurndownOnly returns true if only the burndown analyzer is selected.
func (hc *HistoryCommand) isBurndownOnly() bool {
	return len(hc.analyzers) == 1 && hc.analyzers[0] == "burndown"
}

// resolveRepoURI resolves the repository URI from command args.
func resolveRepoURI(args []string) (string, error) {
	uri := "."
	if len(args) > 0 {
		uri = args[0]
	}

	// Expand ~ to home directory.
	if strings.HasPrefix(uri, "~") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", fmt.Errorf("failed to get home directory: %w", err)
		}

		uri = strings.Replace(uri, "~", home, 1)
	}

	return uri, nil
}

// runFastDevsAnalyzer runs the fast devs analyzer.
func (hc *HistoryCommand) runFastDevsAnalyzer(cmd *cobra.Command, uri string) error {
	fa := devs.NewFastAnalyzer()

	sinceStr, err := cmd.Flags().GetString("since")
	if err != nil {
		return fmt.Errorf("failed to get since flag: %w", err)
	}

	limit, err := cmd.Flags().GetInt("limit")
	if err != nil {
		return fmt.Errorf("failed to get limit flag: %w", err)
	}

	report, err := fa.Analyze(uri, sinceStr, limit)
	if err != nil {
		return fmt.Errorf("fast devs analysis failed: %w", err)
	}

	if hc.format != FormatJSON && hc.format != analyze.FormatPlot {
		printHeader()
		fmt.Fprintln(os.Stdout, "Devs:")
	}

	return fa.Serialize(report, hc.format, os.Stdout)
}

func loadRepository(uri string) *gitlib.Repository {
	if strings.Contains(uri, "://") || regexp.MustCompile(`^[A-Za-z]\w*@[A-Za-z0-9][\w.]*:`).MatchString(uri) {
		return nil // Remote repos not supported in codefang yet.
	}

	if uri[len(uri)-1] == os.PathSeparator {
		uri = uri[:len(uri)-1]
	}

	repository, err := gitlib.OpenRepository(uri)
	if err != nil {
		log.Fatalf("failed to open %s: %v", uri, err)
	}

	return repository
}

func (hc *HistoryCommand) loadCommits(repository *gitlib.Repository, cmd *cobra.Command) ([]*gitlib.Commit, error) {
	if hc.head {
		return hc.loadHeadCommit(repository)
	}

	return hc.loadHistoryCommits(repository, cmd)
}

// loadHeadCommit loads just the HEAD commit.
func (hc *HistoryCommand) loadHeadCommit(repository *gitlib.Repository) ([]*gitlib.Commit, error) {
	headHash, err := repository.Head()
	if err != nil {
		return nil, fmt.Errorf("failed to get HEAD: %w", err)
	}

	commit, err := repository.LookupCommit(headHash)
	if err != nil {
		return nil, fmt.Errorf("failed to get commit: %w", err)
	}

	return []*gitlib.Commit{commit}, nil
}

// loadHistoryCommits loads commits from the log based on command flags.
func (hc *HistoryCommand) loadHistoryCommits(repository *gitlib.Repository, cmd *cobra.Command) ([]*gitlib.Commit, error) {
	logOpts, err := hc.buildLogOptions(cmd)
	if err != nil {
		return nil, err
	}

	iter, err := repository.Log(logOpts)
	if err != nil {
		return nil, fmt.Errorf("failed to list commits: %w", err)
	}
	defer iter.Close()

	limit, err := cmd.Flags().GetInt("limit")
	if err != nil {
		return nil, fmt.Errorf("failed to get limit flag: %w", err)
	}

	commits := hc.collectCommits(iter, limit)
	reverseCommits(commits)

	return commits, nil
}

// buildLogOptions constructs LogOptions from command flags.
func (hc *HistoryCommand) buildLogOptions(cmd *cobra.Command) (*gitlib.LogOptions, error) {
	logOpts := &gitlib.LogOptions{
		FirstParent: hc.firstParent,
	}

	sinceStr, err := cmd.Flags().GetString("since")
	if err != nil {
		return nil, fmt.Errorf("failed to get since flag: %w", err)
	}

	if sinceStr != "" {
		sinceTime, parseErr := parseTime(sinceStr)
		if parseErr != nil {
			return nil, fmt.Errorf("invalid time format for --since: %w", parseErr)
		}

		logOpts.Since = &sinceTime
	}

	return logOpts, nil
}

// collectCommits iterates through commits and collects them up to the limit.
func (hc *HistoryCommand) collectCommits(iter *gitlib.CommitIter, limit int) []*gitlib.Commit {
	var commits []*gitlib.Commit

	count := 0

	for {
		commit, err := iter.Next()
		if err != nil {
			break // EOF or error.
		}

		if limit > 0 && count >= limit {
			commit.Free()

			break
		}

		// With FirstParent, gitlib.Log uses SimplifyFirstParent so we get the
		// first-parent chain; previous commit is always the parent. No need to skip merges.
		commits = append(commits, commit)
		count++
	}

	return commits
}

// reverseCommits reverses the order of commits (to oldest first).
func reverseCommits(commits []*gitlib.Commit) {
	for i, j := 0, len(commits)-1; i < j; i, j = i+1, j-1 {
		commits[i], commits[j] = commits[j], commits[i]
	}
}

func parseTime(s string) (time.Time, error) {
	// Try parsing as duration (e.g., "24h") relative to now.
	d, durationErr := time.ParseDuration(s)
	if durationErr == nil {
		return time.Now().Add(-d), nil
	}

	// Try RFC3339.
	parsedTime, rfc3339Err := time.Parse(time.RFC3339, s)
	if rfc3339Err == nil {
		return parsedTime, nil
	}

	// Try YYYY-MM-DD.
	parsedTime, dateOnlyErr := time.Parse(time.DateOnly, s)
	if dateOnlyErr == nil {
		return parsedTime, nil
	}

	return time.Time{}, fmt.Errorf("%w: %s", ErrInvalidTimeFormat, s)
}

func (hc *HistoryCommand) runPipeline(
	repository *gitlib.Repository, repoPath string, commits []*gitlib.Commit, format string,
	facts map[string]any,
) error {
	pl := newAnalyzerPipeline(repository)

	configureErr := configureAnalyzers(pl.core, facts)
	if configureErr != nil {
		return configureErr
	}

	selectedLeaves, selectErr := hc.selectLeaves(pl.leaves, facts)
	if selectErr != nil {
		return selectErr
	}

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pl.core)+len(selectedLeaves))
	allAnalyzers = append(allAnalyzers, pl.core...)
	allAnalyzers = append(allAnalyzers, selectedLeaves...)

	runner := framework.NewRunner(repository, repoPath, allAnalyzers...)

	results, runErr := runner.Run(commits)
	if runErr != nil {
		return fmt.Errorf("pipeline execution failed: %w", runErr)
	}

	return outputResults(selectedLeaves, results, format)
}

// analyzerPipeline holds the core and leaf analyzers for the pipeline.
type analyzerPipeline struct {
	core   []analyze.HistoryAnalyzer
	leaves map[string]analyze.HistoryAnalyzer
}

// newAnalyzerPipeline creates and configures all analyzers with their dependencies.
func newAnalyzerPipeline(repository *gitlib.Repository) *analyzerPipeline {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: repository}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: repository}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	return &analyzerPipeline{
		core: []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges,
		},
		leaves: map[string]analyze.HistoryAnalyzer{
			"burndown": &burndown.HistoryAnalyzer{
				BlobCache: blobCache, Ticks: ticks, Identity: identity, FileDiff: fileDiff, TreeDiff: treeDiff,
			},
			"couples": &couples.HistoryAnalyzer{Identity: identity, TreeDiff: treeDiff},
			"devs": &devs.HistoryAnalyzer{
				Identity: identity, TreeDiff: treeDiff, Ticks: ticks, Languages: langDetect, LineStats: lineStats,
			},
			"file-history": &filehistory.Analyzer{Identity: identity, TreeDiff: treeDiff, LineStats: lineStats},
			"imports": &imports.HistoryAnalyzer{
				TreeDiff: treeDiff, BlobCache: blobCache, Identity: identity, Ticks: ticks,
			},
			"sentiment": &sentiment.HistoryAnalyzer{UAST: uastChanges, Ticks: ticks},
			"shotness":  &shotness.HistoryAnalyzer{FileDiff: fileDiff, UAST: uastChanges},
			"typos": &typos.HistoryAnalyzer{
				UAST: uastChanges, BlobCache: blobCache, FileDiff: fileDiff,
			},
		},
	}
}

// configureAnalyzers configures all analyzers with the given facts.
func configureAnalyzers(analyzers []analyze.HistoryAnalyzer, facts map[string]any) error {
	for _, analyzer := range analyzers {
		configErr := analyzer.Configure(facts)
		if configErr != nil {
			return fmt.Errorf("failed to configure %s: %w", analyzer.Name(), configErr)
		}
	}

	return nil
}

// selectLeaves selects and configures the requested leaf analyzers.
func (hc *HistoryCommand) selectLeaves(
	leaves map[string]analyze.HistoryAnalyzer, facts map[string]any,
) ([]analyze.HistoryAnalyzer, error) {
	var selected []analyze.HistoryAnalyzer

	for _, name := range hc.analyzers {
		leaf, found := leaves[name]
		if !found {
			return nil, fmt.Errorf(
				"%w: %s\nAvailable: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
				ErrUnknownAnalyzer, name,
			)
		}

		configErr := leaf.Configure(facts)
		if configErr != nil {
			return nil, fmt.Errorf("failed to configure %s: %w", name, configErr)
		}

		selected = append(selected, leaf)
	}

	return selected, nil
}

// PlotGenerator interface for analyzers that can generate plots.
type PlotGenerator interface {
	GenerateChart(report analyze.Report) (components.Charter, error)
}

// outputResults outputs the results for all selected leaves.
func outputResults(leaves []analyze.HistoryAnalyzer, results map[analyze.HistoryAnalyzer]analyze.Report, format string) error {
	rawOutput := format == FormatJSON || format == analyze.FormatPlot
	if !rawOutput {
		printHeader()
	}

	if format == analyze.FormatPlot && len(leaves) > 1 {
		return outputCombinedPlot(leaves, results)
	}

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		if !rawOutput {
			fmt.Fprintf(os.Stdout, "%s:\n", leaf.Name())
		}

		serializeErr := leaf.Serialize(res, format, os.Stdout)
		if serializeErr != nil {
			return fmt.Errorf("serialization error for %s: %w", leaf.Name(), serializeErr)
		}
	}

	return nil
}

func outputCombinedPlot(leaves []analyze.HistoryAnalyzer, results map[analyze.HistoryAnalyzer]analyze.Report) error {
	page := components.NewPage()
	page.PageTitle = "Codefang Analysis Report"

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		if plotter, ok := leaf.(PlotGenerator); ok {
			chart, err := plotter.GenerateChart(res)
			if err != nil {
				return fmt.Errorf("failed to generate chart for %s: %w", leaf.Name(), err)
			}

			page.AddCharts(chart)
		}
	}

	err := page.Render(os.Stdout)
	if err != nil {
		return fmt.Errorf("render page: %w", err)
	}

	return nil
}

// printHeader prints the codefang version header.
func printHeader() {
	fmt.Fprintln(os.Stdout, "codefang (v2):")
	fmt.Fprintf(os.Stdout, "  version: %d\n", version.Binary)
	fmt.Fprintln(os.Stdout, "  hash:", version.BinaryGitHash)
}

// loadFlagValue loads a configuration option's value from command flags into facts.
func loadFlagValue(cmd *cobra.Command, opt pipeline.ConfigurationOption, facts map[string]any) {
	switch opt.Type {
	case pipeline.BoolConfigurationOption:
		val, flagErr := cmd.Flags().GetBool(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	case pipeline.IntConfigurationOption:
		val, flagErr := cmd.Flags().GetInt(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	case pipeline.StringConfigurationOption:
		val, flagErr := cmd.Flags().GetString(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	case pipeline.StringsConfigurationOption:
		val, flagErr := cmd.Flags().GetStringSlice(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	case pipeline.PathConfigurationOption:
		val, flagErr := cmd.Flags().GetString(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	case pipeline.FloatConfigurationOption:
		val, flagErr := cmd.Flags().GetFloat64(opt.Flag)
		if flagErr == nil {
			facts[opt.Name] = val
		}
	}
}
