package commands

import (
	"errors"
	"fmt"
	"log"
	"os"
	"regexp"
	"runtime/pprof"
	"slices"
	"strings"
	"time"

	"github.com/dustin/go-humanize"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/budget"
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// Maximum integer values for safe conversion from uint64.
const (
	maxInt   = int(^uint(0) >> 1)
	maxInt64 = int64(^uint64(0) >> 1)
)

// Sentinel errors for the history command.
var (
	ErrNoAnalyzersSelected = errors.New(
		"no analyzers selected. Use -a flag, e.g.: -a burndown,couples\n" +
			"Available: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
	)
	ErrUnknownAnalyzer      = errors.New("unknown analyzer")
	ErrRepositoryLoad       = errors.New("failed to load repository")
	ErrInvalidTimeFormat    = errors.New("cannot parse time")
	ErrInvalidSizeFormat    = errors.New("invalid size format")
	ErrInvalidStreamingMode = errors.New("invalid streaming mode")
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

	// Resource knob flags for tuning pipeline performance.
	cobraCmd.Flags().Int("workers", 0, "Number of parallel workers (0 = use CPU count)")
	cobraCmd.Flags().Int("buffer-size", 0, "Size of internal pipeline channels (0 = workers×2)")
	cobraCmd.Flags().Int("commit-batch-size", 0, "Commits per processing batch (0 = default 100)")
	cobraCmd.Flags().String("blob-cache-size", "", "Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)")
	cobraCmd.Flags().Int("diff-cache-size", 0, "Max diff cache entries (0 = default 10000)")
	cobraCmd.Flags().String("blob-arena-size", "", "Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)")
	cobraCmd.Flags().String("memory-budget", "", "Memory budget for auto-tuning (e.g., '512MB', '2GB')")
	cobraCmd.Flags().String("streaming-mode", "auto", "Streaming mode for large repos (auto, on, off)")

	// Checkpoint flags.
	cobraCmd.Flags().Bool("checkpoint", true, "Enable checkpointing for crash recovery")
	cobraCmd.Flags().String("checkpoint-dir", "", "Checkpoint directory (default: ~/.codefang/checkpoints)")
	cobraCmd.Flags().Bool("resume", true, "Resume from checkpoint if available")
	cobraCmd.Flags().Bool("clear-checkpoint", false, "Clear existing checkpoint before run")

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

	coordConfig, memBudget, err := buildCoordinatorConfigWithBudget(cmd)
	if err != nil {
		return err
	}

	streamMode, err := getStreamingMode(cmd)
	if err != nil {
		return err
	}

	cpConfig, err := getCheckpointConfig(cmd)
	if err != nil {
		return err
	}

	return hc.runPipeline(repository, uri, commits, hc.format, facts, coordConfig, memBudget, streamMode, cpConfig)
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
	if hc.hasBurndown() && !hc.firstParent {
		hc.firstParent = true
	}
}

// hasBurndown returns true if burndown is one of the selected analyzers.
func (hc *HistoryCommand) hasBurndown() bool {
	return slices.Contains(hc.analyzers, "burndown")
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

// checkpointConfig holds checkpoint-related configuration.
type checkpointConfig struct {
	enabled   bool
	dir       string
	resume    bool
	clearPrev bool
}

// getCheckpointConfig parses checkpoint-related flags.
func getCheckpointConfig(cmd *cobra.Command) (checkpointConfig, error) {
	cfg := checkpointConfig{}

	var err error

	cfg.enabled, err = cmd.Flags().GetBool("checkpoint")
	if err != nil {
		return cfg, fmt.Errorf("failed to get checkpoint flag: %w", err)
	}

	cfg.dir, err = cmd.Flags().GetString("checkpoint-dir")
	if err != nil {
		return cfg, fmt.Errorf("failed to get checkpoint-dir flag: %w", err)
	}

	if cfg.dir == "" {
		cfg.dir = checkpoint.DefaultDir()
	}

	cfg.resume, err = cmd.Flags().GetBool("resume")
	if err != nil {
		return cfg, fmt.Errorf("failed to get resume flag: %w", err)
	}

	cfg.clearPrev, err = cmd.Flags().GetBool("clear-checkpoint")
	if err != nil {
		return cfg, fmt.Errorf("failed to get clear-checkpoint flag: %w", err)
	}

	return cfg, nil
}

// getStreamingMode parses the --streaming-mode flag.
func getStreamingMode(cmd *cobra.Command) (streaming.Mode, error) {
	modeStr, err := cmd.Flags().GetString("streaming-mode")
	if err != nil {
		return streaming.ModeAuto, fmt.Errorf("failed to get streaming-mode flag: %w", err)
	}

	mode, err := streaming.ParseMode(modeStr)
	if err != nil {
		return streaming.ModeAuto, fmt.Errorf("%w: %s (use auto, on, or off)", ErrInvalidStreamingMode, modeStr)
	}

	return mode, nil
}

// buildCoordinatorConfigWithBudget builds CoordinatorConfig and returns the memory budget if set.
// Zero/empty values use defaults from framework.DefaultCoordinatorConfig().
// If --memory-budget is specified, it takes precedence and auto-tunes knobs.
func buildCoordinatorConfigWithBudget(cmd *cobra.Command) (framework.CoordinatorConfig, int64, error) {
	// Check if memory budget is specified first.
	memBudgetStr, err := cmd.Flags().GetString("memory-budget")
	if err != nil {
		return framework.CoordinatorConfig{}, 0, fmt.Errorf("failed to get memory-budget flag: %w", err)
	}

	if memBudgetStr != "" {
		cfg, budgetErr := buildConfigFromBudget(memBudgetStr)
		if budgetErr != nil {
			return framework.CoordinatorConfig{}, 0, budgetErr
		}

		// Parse succeeded in buildConfigFromBudget, so this won't error.
		budgetBytes, parseErr := humanize.ParseBytes(memBudgetStr)
		if parseErr != nil {
			return framework.CoordinatorConfig{}, 0, fmt.Errorf("failed to parse budget: %w", parseErr)
		}

		return cfg, safeInt64(budgetBytes), nil
	}

	// No budget specified, use manual knobs or defaults.
	config := framework.DefaultCoordinatorConfig()

	err = applyIntFlags(cmd, &config)
	if err != nil {
		return config, 0, err
	}

	err = applySizeFlags(cmd, &config)
	if err != nil {
		return config, 0, err
	}

	return config, 0, nil
}

// buildCoordinatorConfig builds CoordinatorConfig from CLI flags (legacy wrapper).
func buildCoordinatorConfig(cmd *cobra.Command) (framework.CoordinatorConfig, error) {
	cfg, _, err := buildCoordinatorConfigWithBudget(cmd)

	return cfg, err
}

// buildConfigFromBudget creates a CoordinatorConfig from a memory budget string.
func buildConfigFromBudget(budgetStr string) (framework.CoordinatorConfig, error) {
	budgetBytes, err := humanize.ParseBytes(budgetStr)
	if err != nil {
		return framework.CoordinatorConfig{}, fmt.Errorf("%w for --memory-budget: %s", ErrInvalidSizeFormat, budgetStr)
	}

	cfg, err := budget.SolveForBudget(safeInt64(budgetBytes))
	if err != nil {
		return framework.CoordinatorConfig{}, fmt.Errorf("memory budget error: %w", err)
	}

	return cfg, nil
}

// applyIntFlags applies integer flags to the config.
func applyIntFlags(cmd *cobra.Command, config *framework.CoordinatorConfig) error {
	workers, err := cmd.Flags().GetInt("workers")
	if err != nil {
		return fmt.Errorf("failed to get workers flag: %w", err)
	}

	if workers > 0 {
		config.Workers = workers
	}

	bufferSize, err := cmd.Flags().GetInt("buffer-size")
	if err != nil {
		return fmt.Errorf("failed to get buffer-size flag: %w", err)
	}

	if bufferSize > 0 {
		config.BufferSize = bufferSize
	}

	commitBatchSize, err := cmd.Flags().GetInt("commit-batch-size")
	if err != nil {
		return fmt.Errorf("failed to get commit-batch-size flag: %w", err)
	}

	if commitBatchSize > 0 {
		config.CommitBatchSize = commitBatchSize
	}

	diffCacheSize, err := cmd.Flags().GetInt("diff-cache-size")
	if err != nil {
		return fmt.Errorf("failed to get diff-cache-size flag: %w", err)
	}

	if diffCacheSize > 0 {
		config.DiffCacheSize = diffCacheSize
	}

	return nil
}

// applySizeFlags applies human-readable size flags to the config.
func applySizeFlags(cmd *cobra.Command, config *framework.CoordinatorConfig) error {
	blobCacheSizeStr, err := cmd.Flags().GetString("blob-cache-size")
	if err != nil {
		return fmt.Errorf("failed to get blob-cache-size flag: %w", err)
	}

	if blobCacheSizeStr != "" {
		size, parseErr := humanize.ParseBytes(blobCacheSizeStr)
		if parseErr != nil {
			return fmt.Errorf("%w for --blob-cache-size: %s", ErrInvalidSizeFormat, blobCacheSizeStr)
		}

		config.BlobCacheSize = safeInt64(size)
	}

	blobArenaSizeStr, err := cmd.Flags().GetString("blob-arena-size")
	if err != nil {
		return fmt.Errorf("failed to get blob-arena-size flag: %w", err)
	}

	if blobArenaSizeStr != "" {
		size, parseErr := humanize.ParseBytes(blobArenaSizeStr)
		if parseErr != nil {
			return fmt.Errorf("%w for --blob-arena-size: %s", ErrInvalidSizeFormat, blobArenaSizeStr)
		}

		config.BlobArenaSize = safeInt(size)
	}

	return nil
}

// safeInt64 converts uint64 to int64, clamping to maxInt64 to prevent overflow.
func safeInt64(v uint64) int64 {
	if v > uint64(maxInt64) {
		return maxInt64
	}

	return int64(v)
}

// safeInt converts uint64 to int, clamping to maxInt to prevent overflow.
func safeInt(v uint64) int {
	if v > uint64(maxInt) {
		return maxInt
	}

	return int(v)
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
	facts map[string]any, coordConfig framework.CoordinatorConfig, memBudget int64, streamMode streaming.Mode,
	cpConfig checkpointConfig,
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

	// Determine if streaming mode should be used.
	useStreaming := shouldUseStreaming(streamMode, len(commits), memBudget)

	runner := framework.NewRunnerWithConfig(repository, repoPath, coordConfig, allAnalyzers...)

	// Setup checkpoint manager if enabled and streaming.
	var cpManager *checkpoint.Manager

	analyzerNames := hc.analyzers

	if cpConfig.enabled && useStreaming {
		repoHash := checkpoint.RepoHash(repoPath)
		cpManager = checkpoint.NewManager(cpConfig.dir, repoHash)

		if cpConfig.clearPrev {
			clearErr := cpManager.Clear()
			if clearErr != nil {
				log.Printf("warning: failed to clear checkpoint: %v", clearErr)
			}
		}
	}

	var results map[analyze.HistoryAnalyzer]analyze.Report

	var runErr error

	if useStreaming {
		results, runErr = runWithStreaming(runner, commits, memBudget, allAnalyzers, cpManager, repoPath, analyzerNames, cpConfig.resume)
	} else {
		results, runErr = runner.Run(commits)
	}

	if runErr != nil {
		return fmt.Errorf("pipeline execution failed: %w", runErr)
	}

	// Clear checkpoint on successful completion.
	if cpManager != nil {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			log.Printf("warning: failed to clear checkpoint after completion: %v", clearErr)
		}
	}

	return outputResults(selectedLeaves, results, format)
}

// shouldUseStreaming determines if streaming mode should be used based on mode, commit count, and budget.
func shouldUseStreaming(mode streaming.Mode, commitCount int, memBudget int64) bool {
	switch mode {
	case streaming.ModeOn:
		return true
	case streaming.ModeOff:
		return false
	case streaming.ModeAuto:
		detector := streaming.Detector{
			CommitCount:  commitCount,
			MemoryBudget: memBudget,
		}

		return detector.ShouldStream()
	}

	return false
}

// runWithStreaming executes the pipeline in streaming chunks.
func runWithStreaming(
	runner *framework.Runner,
	commits []*gitlib.Commit,
	memBudget int64,
	analyzers []analyze.HistoryAnalyzer,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	resumeEnabled bool,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	chunks := planChunks(len(commits), memBudget)
	hibernatables := collectHibernatables(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	log.Printf("streaming: processing %d commits in %d chunks", len(commits), len(chunks))

	startChunk := 0

	// Try to resume from checkpoint if enabled.
	if cpManager != nil && resumeEnabled && cpManager.Exists() {
		resumedChunk, err := tryResumeFromCheckpoint(cpManager, checkpointables, repoPath, analyzerNames)
		if err != nil {
			log.Printf("checkpoint: resume failed, starting fresh: %v", err)
		} else if resumedChunk > 0 {
			startChunk = resumedChunk
			log.Printf("checkpoint: resuming from chunk %d", startChunk+1)
		}
	}

	// Initialize once at the start (or after resume).
	if startChunk == 0 {
		err := runner.Initialize()
		if err != nil {
			return nil, fmt.Errorf("initialization failed: %w", err)
		}
	}

	err := processChunksWithCheckpoint(
		runner, commits, chunks, hibernatables, checkpointables,
		cpManager, repoPath, analyzerNames, startChunk,
	)
	if err != nil {
		return nil, err
	}

	// Finalize once at the end.
	return runner.Finalize()
}

// planChunks creates chunk boundaries for streaming execution.
func planChunks(commitCount int, memBudget int64) []streaming.ChunkBounds {
	planner := streaming.Planner{
		TotalCommits: commitCount,
		MemoryBudget: memBudget,
	}

	return planner.Plan()
}

// collectHibernatables extracts analyzers that support hibernation.
func collectHibernatables(analyzers []analyze.HistoryAnalyzer) []streaming.Hibernatable {
	var hibernatables []streaming.Hibernatable

	for _, a := range analyzers {
		if h, ok := a.(streaming.Hibernatable); ok {
			hibernatables = append(hibernatables, h)
		}
	}

	return hibernatables
}

// collectCheckpointables extracts analyzers that support checkpointing.
func collectCheckpointables(analyzers []analyze.HistoryAnalyzer) []checkpoint.Checkpointable {
	var checkpointables []checkpoint.Checkpointable

	for _, a := range analyzers {
		if c, ok := a.(checkpoint.Checkpointable); ok {
			checkpointables = append(checkpointables, c)
		}
	}

	return checkpointables
}

// tryResumeFromCheckpoint attempts to restore state from a checkpoint.
// Returns the chunk index to resume from, or 0 if no valid checkpoint.
func tryResumeFromCheckpoint(
	cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable,
	repoPath string,
	analyzerNames []string,
) (int, error) {
	// Validate checkpoint matches current run.
	validateErr := cpManager.Validate(repoPath, analyzerNames)
	if validateErr != nil {
		return 0, fmt.Errorf("checkpoint validation failed: %w", validateErr)
	}

	// Load state into analyzers.
	state, err := cpManager.Load(checkpointables)
	if err != nil {
		return 0, fmt.Errorf("checkpoint load failed: %w", err)
	}

	// Return the next chunk to process.
	return state.CurrentChunk + 1, nil
}

// processChunksWithCheckpoint executes chunks with optional checkpointing.
func processChunksWithCheckpoint(
	runner *framework.Runner,
	commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
) error {
	for i := startChunk; i < len(chunks); i++ {
		chunk := chunks[i]
		log.Printf("streaming: processing chunk %d/%d (commits %d-%d)",
			i+1, len(chunks), chunk.Start, chunk.End)

		if i > startChunk {
			hibErr := hibernateAndBoot(hibernatables)
			if hibErr != nil {
				return hibErr
			}
		}

		chunkCommits := commits[chunk.Start:chunk.End]

		err := runner.ProcessChunk(chunkCommits, chunk.Start)
		if err != nil {
			return fmt.Errorf("chunk %d failed: %w", i+1, err)
		}

		// Save checkpoint after each chunk (except the last).
		if cpManager != nil && i < len(chunks)-1 {
			lastCommit := chunkCommits[len(chunkCommits)-1]
			state := checkpoint.StreamingState{
				TotalCommits:     len(commits),
				ProcessedCommits: chunk.End,
				CurrentChunk:     i,
				TotalChunks:      len(chunks),
				LastCommitHash:   lastCommit.Hash().String(),
			}

			saveErr := cpManager.Save(checkpointables, state, repoPath, analyzerNames)
			if saveErr != nil {
				log.Printf("warning: failed to save checkpoint: %v", saveErr)
			} else {
				log.Printf("checkpoint: saved after chunk %d", i+1)
			}
		}
	}

	return nil
}

// hibernateAndBoot hibernates and then boots all hibernatable analyzers.
func hibernateAndBoot(hibernatables []streaming.Hibernatable) error {
	for _, h := range hibernatables {
		err := h.Hibernate()
		if err != nil {
			return fmt.Errorf("hibernation failed: %w", err)
		}
	}

	for _, h := range hibernatables {
		err := h.Boot()
		if err != nil {
			return fmt.Errorf("boot failed: %w", err)
		}
	}

	return nil
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

// SectionGenerator interface for analyzers that can generate page sections.
type SectionGenerator interface {
	GenerateSections(report analyze.Report) ([]plotpage.Section, error)
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
	page := buildCombinedPage(leaves)

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		err := addLeafToPage(page, leaf, res)
		if err != nil {
			return err
		}
	}

	err := page.Render(os.Stdout)
	if err != nil {
		return fmt.Errorf("render page: %w", err)
	}

	return nil
}

func buildCombinedPage(leaves []analyze.HistoryAnalyzer) *plotpage.Page {
	names := make([]string, 0, len(leaves))
	for _, leaf := range leaves {
		names = append(names, leaf.Name())
	}

	return plotpage.NewPage(
		"Combined Analysis Report",
		fmt.Sprintf("Analysis results for: %s", strings.Join(names, ", ")),
	)
}

func addLeafToPage(page *plotpage.Page, leaf analyze.HistoryAnalyzer, res analyze.Report) error {
	if sectionGen, ok := leaf.(SectionGenerator); ok {
		return addSectionsToPage(page, sectionGen, leaf.Name(), res)
	}

	if plotter, ok := leaf.(PlotGenerator); ok {
		return addChartToPage(page, plotter, leaf.Name(), res)
	}

	return nil
}

func addSectionsToPage(page *plotpage.Page, gen SectionGenerator, name string, res analyze.Report) error {
	sections, err := gen.GenerateSections(res)
	if err != nil {
		return fmt.Errorf("failed to generate sections for %s: %w", name, err)
	}

	page.Add(sections...)

	return nil
}

func addChartToPage(page *plotpage.Page, plotter PlotGenerator, name string, res analyze.Report) error {
	chart, err := plotter.GenerateChart(res)
	if err != nil {
		return fmt.Errorf("failed to generate chart for %s: %w", name, err)
	}

	if renderable, ok := chart.(plotpage.Renderable); ok {
		page.Add(plotpage.Section{
			Title:    name,
			Subtitle: fmt.Sprintf("Results from %s analyzer", name),
			Chart:    plotpage.WrapChart(renderable),
		})
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
