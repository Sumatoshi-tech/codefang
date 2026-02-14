// Package commands implements CLI command handlers for codefang.
package commands

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"slices"
	"time"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/halstead"
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
)

type staticExecutor func(
	path string,
	analyzerIDs []string,
	format string,
	verbose bool,
	noColor bool,
	writer io.Writer,
) error

type historyExecutor func(path string, analyzerIDs []string, format string, silent bool, opts HistoryRunOptions, writer io.Writer) error

type registryProvider func() (*analyze.Registry, error)

// HistoryRunOptions holds all history pipeline runtime options.
type HistoryRunOptions struct {
	GCPercent   int
	BallastSize string

	CPUProfile  string
	HeapProfile string

	Limit       int
	FirstParent bool
	Head        bool
	Since       string

	Workers         int
	BufferSize      int
	CommitBatchSize int
	BlobCacheSize   string
	DiffCacheSize   int
	BlobArenaSize   string
	MemoryBudget    string

	Checkpoint      *bool
	CheckpointDir   string
	Resume          *bool
	ClearCheckpoint bool
}

var (
	// ErrNoAnalyzersSelected is returned when no analyzer IDs match the selection.
	ErrNoAnalyzersSelected = errors.New(
		"no analyzers selected. Use -a flag, e.g.: -a burndown,couples\n" +
			"Available: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
	)
	// ErrUnknownAnalyzer indicates a requested analyzer ID is not in the registry.
	ErrUnknownAnalyzer = errors.New("unknown analyzer")
	// ErrRepositoryLoad indicates a failure to open or load the git repository.
	ErrRepositoryLoad = errors.New("failed to load repository")
)

// RunCommand holds configuration and dependencies for the unified run command.
type RunCommand struct {
	format      string
	analyzerIDs []string
	inputPath   string
	inputFormat string
	gogc        int
	ballastSize string
	verbose     bool
	silent      bool
	noColor     bool
	path        string

	cpuprofile  string
	heapprofile string

	limit       int
	firstParent bool
	head        bool
	since       string

	workers         int
	bufferSize      int
	commitBatchSize int
	blobCacheSize   string
	diffCacheSize   int
	blobArenaSize   string
	memoryBudget    string

	checkpointDir   string
	clearCheckpoint bool

	staticExec  staticExecutor
	historyExec historyExecutor
	registryFn  registryProvider
}

// NewRunCommand creates the unified run command.
func NewRunCommand() *cobra.Command {
	burndown.RegisterPlotSections()
	cohesion.RegisterPlotSections()
	comments.RegisterPlotSections()
	complexity.RegisterPlotSections()
	couples.RegisterPlotSections()
	filehistory.RegisterPlotSections()
	halstead.RegisterPlotSections()
	imports.RegisterPlotSections()
	sentiment.RegisterPlotSections()
	shotness.RegisterPlotSections()
	typos.RegisterPlotSections()
	renderer.RegisterPlotRenderer()

	return newRunCommandWithDeps(runStaticAnalyzers, runHistoryAnalyzers, defaultRegistry)
}

func newRunCommandWithDeps(
	staticExec staticExecutor,
	historyExec historyExecutor,
	registryFn registryProvider,
) *cobra.Command {
	rc := &RunCommand{
		format:      analyze.FormatJSON,
		staticExec:  staticExec,
		historyExec: historyExec,
		registryFn:  registryFn,
	}

	cmd := &cobra.Command{
		Use:   "run [path]",
		Short: "Run static and history analyzers",
		Long:  "Run selected static and history analyzers.",
		Args:  cobra.MaximumNArgs(1),
		RunE:  rc.run,
	}

	cmd.Flags().StringSliceVarP(&rc.analyzerIDs, "analyzers", "a", nil,
		"Analyzer IDs or glob patterns (example: static/complexity,history/*,*)")
	cmd.Flags().StringVar(&rc.format, "format", analyze.FormatJSON, "Output format: json, text, compact, yaml, plot, bin")
	cmd.Flags().StringVar(&rc.inputPath, "input", "", "Input report path for cross-format conversion")
	cmd.Flags().StringVar(&rc.inputFormat, "input-format", analyze.InputFormatAuto, "Input format: auto, json, bin")
	cmd.Flags().IntVar(&rc.gogc, "gogc", 0, "GC percent for history pipeline (0 = auto, >0 = exact)")
	cmd.Flags().StringVar(&rc.ballastSize, "ballast-size", "0", "Optional GC ballast size for history pipeline (0 = disabled)")
	cmd.Flags().BoolVarP(&rc.verbose, "verbose", "v", false, "Show full static report details")
	cmd.Flags().BoolVar(&rc.silent, "silent", false, "Disable progress output")
	cmd.Flags().BoolVar(&rc.noColor, "no-color", false, "Disable colored static output")
	cmd.Flags().StringVarP(&rc.path, "path", "p", ".", "Folder/repository path to analyze")

	cmd.Flags().StringVar(&rc.cpuprofile, "cpuprofile", "", "Write CPU profile to file")
	cmd.Flags().StringVar(&rc.heapprofile, "heapprofile", "", "Write heap profile to file")

	cmd.Flags().IntVar(&rc.limit, "limit", 0, "Limit number of commits to analyze (0 = no limit)")
	cmd.Flags().BoolVar(&rc.firstParent, "first-parent", false, "Follow only first parent of merge commits")
	cmd.Flags().BoolVar(&rc.head, "head", false, "Analyze only HEAD commit")
	cmd.Flags().StringVar(&rc.since, "since", "", "Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)")

	cmd.Flags().IntVar(&rc.workers, "workers", 0, "Number of parallel workers (0 = use CPU count)")
	cmd.Flags().IntVar(&rc.bufferSize, "buffer-size", 0, "Size of internal pipeline channels (0 = workers*2)")
	cmd.Flags().IntVar(&rc.commitBatchSize, "commit-batch-size", 0, "Commits per processing batch (0 = default 100)")
	cmd.Flags().StringVar(&rc.blobCacheSize, "blob-cache-size", "", "Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)")
	cmd.Flags().IntVar(&rc.diffCacheSize, "diff-cache-size", 0, "Max diff cache entries (0 = default 10000)")
	cmd.Flags().StringVar(&rc.blobArenaSize, "blob-arena-size", "", "Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)")
	cmd.Flags().StringVar(&rc.memoryBudget, "memory-budget", "", "Memory budget for auto-tuning (e.g., '512MB', '2GB')")

	cmd.Flags().Bool("checkpoint", true, "Enable checkpointing for crash recovery")
	cmd.Flags().StringVar(&rc.checkpointDir, "checkpoint-dir", "", "Checkpoint directory (default: ~/.codefang/checkpoints)")
	cmd.Flags().Bool("resume", true, "Resume from checkpoint if available")
	cmd.Flags().BoolVar(&rc.clearCheckpoint, "clear-checkpoint", false, "Clear existing checkpoint before run")

	registerAnalyzerFlags(cmd)

	return cmd
}

func (rc *RunCommand) run(cmd *cobra.Command, args []string) error {
	path := rc.resolvePath(args)
	silent := rc.isSilent(cmd)
	progressWriter := cmd.ErrOrStderr()

	rc.progressf(silent, progressWriter, "starting run path=%s", path)

	registry, err := rc.registryFn()
	if err != nil {
		return err
	}

	ids, err := registry.SelectedIDs(rc.analyzerIDs)
	if err != nil {
		return err
	}

	rc.progressf(silent, progressWriter, "selected analyzers: total=%d", len(ids))

	if rc.inputPath != "" {
		err = rc.runInputConversion(cmd.OutOrStdout(), registry, ids, silent, progressWriter)
	} else {
		err = rc.runDirect(path, ids, registry, silent, progressWriter, cmd.OutOrStdout(), cmd)
	}

	if err != nil {
		return err
	}

	rc.progressf(silent, progressWriter, "run completed")

	return nil
}

func (rc *RunCommand) resolvePath(args []string) string {
	if len(args) > 0 {
		return args[0]
	}

	return rc.path
}

func (rc *RunCommand) runInputConversion(
	writer io.Writer,
	registry *analyze.Registry,
	ids []string,
	silent bool,
	progressWriter io.Writer,
) error {
	rc.progressf(silent, progressWriter, "conversion mode: input=%s input_format=%s output_format=%s",
		rc.inputPath, rc.inputFormat, rc.format)

	outputFormat, err := analyze.ValidateUniversalFormat(rc.format)
	if err != nil {
		return err
	}

	inputFormat, err := analyze.ResolveInputFormat(rc.inputPath, rc.inputFormat)
	if err != nil {
		return err
	}

	inputBytes, err := os.ReadFile(rc.inputPath)
	if err != nil {
		return fmt.Errorf("read input %s: %w", rc.inputPath, err)
	}

	orderedIDs, err := analyze.OrderedRunIDs(registry, ids)
	if err != nil {
		return err
	}

	model, err := analyze.DecodeInputModel(inputBytes, inputFormat, orderedIDs, registry)
	if err != nil {
		return err
	}

	return analyze.WriteConvertedOutput(model, outputFormat, writer)
}

func (rc *RunCommand) runDirect(
	path string,
	ids []string,
	registry *analyze.Registry,
	silent bool,
	progressWriter io.Writer,
	writer io.Writer,
	cmd *cobra.Command,
) error {
	staticIDs, historyIDs, err := registry.Split(ids)
	if err != nil {
		return err
	}

	staticFormat, historyFormat, err := analyze.ResolveFormats(rc.format, len(staticIDs) > 0, len(historyIDs) > 0)
	if err != nil {
		return err
	}

	resolvedOutputFormat := staticFormat
	if resolvedOutputFormat == "" {
		resolvedOutputFormat = historyFormat
	}

	rc.progressf(silent, progressWriter, "resolved analyzers: static=%d history=%d output_format=%s",
		len(staticIDs), len(historyIDs), resolvedOutputFormat)

	if len(staticIDs) > 0 && len(historyIDs) > 0 {
		rc.progressf(silent, progressWriter, "mixed run detected: rendering combined output")

		return rc.renderCombinedDirect(path, staticIDs, historyIDs, registry, staticFormat, silent, progressWriter, writer, cmd)
	}

	err = rc.runStaticPhase(path, staticIDs, staticFormat, silent, progressWriter, writer)
	if err != nil {
		return err
	}

	return rc.runHistoryPhase(path, historyIDs, historyFormat, silent, progressWriter, writer, cmd)
}

func (rc *RunCommand) runStaticPhase(
	path string,
	staticIDs []string,
	staticFormat string,
	silent bool,
	progressWriter io.Writer,
	writer io.Writer,
) error {
	if len(staticIDs) == 0 {
		return nil
	}

	startedAt := time.Now()

	rc.progressf(silent, progressWriter, "static phase started (%d analyzers)", len(staticIDs))

	err := rc.staticExec(path, staticIDs, staticFormat, rc.verbose, rc.noColor, writer)
	if err != nil {
		return err
	}

	rc.progressf(silent, progressWriter, "static phase finished in %s", time.Since(startedAt).Round(time.Millisecond))

	return nil
}

func (rc *RunCommand) runHistoryPhase(
	path string,
	historyIDs []string,
	historyFormat string,
	silent bool,
	progressWriter io.Writer,
	writer io.Writer,
	cmd *cobra.Command,
) error {
	if len(historyIDs) == 0 {
		return nil
	}

	startedAt := time.Now()

	rc.progressf(silent, progressWriter, "history phase started (%d analyzers)", len(historyIDs))

	opts := rc.buildHistoryRunOptions(cmd)

	err := rc.historyExec(path, historyIDs, historyFormat, silent, opts, writer)
	if err != nil {
		return err
	}

	rc.progressf(silent, progressWriter, "history phase finished in %s", time.Since(startedAt).Round(time.Millisecond))

	return nil
}

func (rc *RunCommand) renderCombinedDirect(
	path string,
	staticIDs []string,
	historyIDs []string,
	registry *analyze.Registry,
	outputFormat string,
	silent bool,
	progressWriter io.Writer,
	writer io.Writer,
	cmd *cobra.Command,
) error {
	var raw bytes.Buffer

	startedAt := time.Now()

	rc.progressf(silent, progressWriter, "combined static phase started")

	err := rc.staticExec(path, staticIDs, analyze.FormatBinary, rc.verbose, rc.noColor, &raw)
	if err != nil {
		return fmt.Errorf("render combined static phase: %w", err)
	}

	rc.progressf(silent, progressWriter, "combined static phase finished in %s", time.Since(startedAt).Round(time.Millisecond))

	startedAt = time.Now()

	rc.progressf(silent, progressWriter, "combined history phase started")

	opts := rc.buildHistoryRunOptions(cmd)

	err = rc.historyExec(path, historyIDs, analyze.FormatBinary, silent, opts, &raw)
	if err != nil {
		return fmt.Errorf("render combined history phase: %w", err)
	}

	rc.progressf(silent, progressWriter, "combined history phase finished in %s", time.Since(startedAt).Round(time.Millisecond))

	orderedIDs := make([]string, 0, len(staticIDs)+len(historyIDs))
	orderedIDs = append(orderedIDs, staticIDs...)
	orderedIDs = append(orderedIDs, historyIDs...)

	model, err := analyze.DecodeBinaryInputModel(raw.Bytes(), orderedIDs, registry)
	if err != nil {
		return fmt.Errorf("decode combined payload: %w", err)
	}

	rc.progressf(silent, progressWriter, "combined payload decoded")

	startedAt = time.Now()

	rc.progressf(silent, progressWriter, "combined output rendering started")

	err = analyze.WriteConvertedOutput(model, outputFormat, writer)
	if err != nil {
		return fmt.Errorf("render combined output: %w", err)
	}

	rc.progressf(silent, progressWriter, "combined output rendering finished in %s", time.Since(startedAt).Round(time.Millisecond))

	return nil
}

func (rc *RunCommand) buildHistoryRunOptions(cmd *cobra.Command) HistoryRunOptions {
	opts := HistoryRunOptions{
		GCPercent:       rc.gogc,
		BallastSize:     rc.ballastSize,
		CPUProfile:      rc.cpuprofile,
		HeapProfile:     rc.heapprofile,
		Limit:           rc.limit,
		FirstParent:     rc.firstParent,
		Head:            rc.head,
		Since:           rc.since,
		Workers:         rc.workers,
		BufferSize:      rc.bufferSize,
		CommitBatchSize: rc.commitBatchSize,
		BlobCacheSize:   rc.blobCacheSize,
		DiffCacheSize:   rc.diffCacheSize,
		BlobArenaSize:   rc.blobArenaSize,
		MemoryBudget:    rc.memoryBudget,
		CheckpointDir:   rc.checkpointDir,
		ClearCheckpoint: rc.clearCheckpoint,
	}

	if cmd.Flags().Changed("checkpoint") {
		v, err := cmd.Flags().GetBool("checkpoint")
		if err != nil {
			return opts // flag is registered; GetBool should not fail.
		}

		opts.Checkpoint = &v
	}

	if cmd.Flags().Changed("resume") {
		v, err := cmd.Flags().GetBool("resume")
		if err != nil {
			return opts // flag is registered; GetBool should not fail.
		}

		opts.Resume = &v
	}

	return opts
}

func defaultRegistry() (*analyze.Registry, error) {
	return analyze.NewRegistry(defaultStaticAnalyzers(), defaultHistoryLeaves())
}

func runStaticAnalyzers(
	path string,
	analyzerIDs []string,
	format string,
	verbose bool,
	noColor bool,
	writer io.Writer,
) error {
	service := analyze.NewStaticService(defaultStaticAnalyzers())
	service.Renderer = renderer.NewDefaultStaticRenderer()

	return service.RunAndFormat(path, analyzerIDs, format, verbose, noColor, writer)
}

func runHistoryAnalyzers(path string, analyzerIDs []string, format string, silent bool, opts HistoryRunOptions, writer io.Writer) error {
	restoreLogger := suppressStandardLogger(silent)
	defer restoreLogger()

	stopProfiler, err := framework.MaybeStartCPUProfile(opts.CPUProfile)
	if err != nil {
		return err
	}

	defer stopProfiler()
	defer framework.MaybeWriteHeapProfile(opts.HeapProfile)

	pl := buildPipeline(nil)

	analyzerKeys, err := analyze.HistoryKeysByID(pl.Leaves, analyzerIDs)
	if err != nil {
		return err
	}

	if len(analyzerKeys) == 0 {
		return ErrNoAnalyzersSelected
	}

	normalizedFormat, err := analyze.ValidateUniversalFormat(format)
	if err != nil {
		return err
	}

	repository, err := gitlib.LoadRepository(path)
	if err != nil {
		return fmt.Errorf("%w: %s", ErrRepositoryLoad, path)
	}
	defer repository.Free()

	pl = buildPipeline(repository)

	if slices.Contains(analyzerKeys, "burndown") && !opts.FirstParent {
		opts.FirstParent = true
	}

	commits, err := gitlib.LoadCommits(repository, gitlib.CommitLoadOptions{
		Limit:       opts.Limit,
		FirstParent: opts.FirstParent,
		HeadOnly:    opts.Head,
		Since:       opts.Since,
	})
	if err != nil {
		return err
	}

	facts := buildFacts(pl)

	selectedLeaves, err := selectLeaves(pl.Leaves, analyzerKeys, facts)
	if err != nil {
		return err
	}

	return executeHistoryPipeline(pl, path, selectedLeaves, facts, commits, analyzerKeys, normalizedFormat, opts, repository, writer)
}

func executeHistoryPipeline(
	pl *historyPipeline,
	path string,
	selectedLeaves []analyze.HistoryAnalyzer,
	facts map[string]any,
	commits []*gitlib.Commit,
	analyzerKeys []string,
	normalizedFormat string,
	opts HistoryRunOptions,
	repository *gitlib.Repository,
	writer io.Writer,
) error {
	err := configureAnalyzers(pl.Core, facts)
	if err != nil {
		return err
	}

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pl.Core)+len(selectedLeaves))
	allAnalyzers = append(allAnalyzers, pl.Core...)
	allAnalyzers = append(allAnalyzers, selectedLeaves...)

	coordConfig, memBudget, err := framework.BuildConfigFromParams(framework.ConfigParams{
		Workers:         opts.Workers,
		BufferSize:      opts.BufferSize,
		CommitBatchSize: opts.CommitBatchSize,
		BlobCacheSize:   opts.BlobCacheSize,
		DiffCacheSize:   opts.DiffCacheSize,
		BlobArenaSize:   opts.BlobArenaSize,
		MemoryBudget:    opts.MemoryBudget,
		GCPercent:       opts.GCPercent,
		BallastSize:     opts.BallastSize,
	}, budget.SolveForBudget)
	if err != nil {
		return err
	}

	if !needsUAST(selectedLeaves) {
		coordConfig.UASTPipelineWorkers = 0
	}

	runner := framework.NewRunnerWithConfig(repository, path, coordConfig, allAnalyzers...)
	runner.CoreCount = len(pl.Core)

	results, err := framework.RunStreaming(runner, commits, allAnalyzers, framework.StreamingConfig{
		MemBudget:     memBudget,
		Checkpoint:    buildCheckpointParams(opts),
		RepoPath:      path,
		AnalyzerNames: analyzerKeys,
	})
	if err != nil {
		return fmt.Errorf("pipeline execution failed: %w", err)
	}

	return analyze.OutputHistoryResults(selectedLeaves, results, normalizedFormat, writer)
}

func selectLeaves(
	leaves map[string]analyze.HistoryAnalyzer,
	keys []string,
	facts map[string]any,
) ([]analyze.HistoryAnalyzer, error) {
	var selected []analyze.HistoryAnalyzer

	for _, name := range keys {
		leaf, found := leaves[name]
		if !found {
			return nil, fmt.Errorf(
				"%w: %s\nAvailable: burndown, couples, devs, file-history, imports, sentiment, shotness, typos",
				ErrUnknownAnalyzer, name,
			)
		}

		err := leaf.Configure(facts)
		if err != nil {
			return nil, fmt.Errorf("failed to configure %s: %w", name, err)
		}

		selected = append(selected, leaf)
	}

	return selected, nil
}

func buildFacts(pl *historyPipeline) map[string]any {
	facts := map[string]any{}

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(pl.Core)+len(pl.Leaves))
	allAnalyzers = append(allAnalyzers, pl.Core...)

	for _, leaf := range pl.Leaves {
		allAnalyzers = append(allAnalyzers, leaf)
	}

	for _, a := range allAnalyzers {
		for _, opt := range a.ListConfigurationOptions() {
			if opt.Default != nil {
				facts[opt.Name] = opt.Default
			}
		}
	}

	return facts
}

func configureAnalyzers(analyzers []analyze.HistoryAnalyzer, facts map[string]any) error {
	for _, a := range analyzers {
		err := a.Configure(facts)
		if err != nil {
			return fmt.Errorf("failed to configure %s: %w", a.Name(), err)
		}
	}

	return nil
}

func buildCheckpointParams(opts HistoryRunOptions) framework.CheckpointParams {
	params := framework.CheckpointParams{
		Enabled:   true,
		Resume:    true,
		ClearPrev: opts.ClearCheckpoint,
		Dir:       opts.CheckpointDir,
	}

	if params.Dir == "" {
		params.Dir = checkpoint.DefaultDir()
	}

	if opts.Checkpoint != nil {
		params.Enabled = *opts.Checkpoint
	}

	if opts.Resume != nil {
		params.Resume = *opts.Resume
	}

	return params
}

func registerAnalyzerFlags(cobraCmd *cobra.Command) {
	dummyPipeline := buildPipeline(nil)

	allAnalyzers := make([]analyze.HistoryAnalyzer, 0, len(dummyPipeline.Core)+len(dummyPipeline.Leaves))
	allAnalyzers = append(allAnalyzers, dummyPipeline.Core...)

	for _, leaf := range dummyPipeline.Leaves {
		allAnalyzers = append(allAnalyzers, leaf)
	}

	registeredFlags := make(map[string]bool)

	for _, a := range allAnalyzers {
		for _, opt := range a.ListConfigurationOptions() {
			if registeredFlags[opt.Flag] {
				continue
			}

			registeredFlags[opt.Flag] = true
			registerConfigFlag(cobraCmd, opt)
		}
	}
}

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

type uastDependent interface {
	NeedsUAST() bool
}

func needsUAST(leaves []analyze.HistoryAnalyzer) bool {
	for _, leaf := range leaves {
		if ud, ok := leaf.(uastDependent); ok && ud.NeedsUAST() {
			return true
		}
	}

	return false
}

func (rc *RunCommand) isSilent(cmd *cobra.Command) bool {
	if rc.silent {
		return true
	}

	quiet, err := cmd.Flags().GetBool("quiet")
	if err != nil {
		return false
	}

	return quiet
}

func (rc *RunCommand) progressf(silent bool, writer io.Writer, format string, args ...any) {
	if silent {
		return
	}

	_, _ = fmt.Fprintf(writer, "progress: "+format+"\n", args...)
}

func suppressStandardLogger(silent bool) func() {
	if !silent {
		return func() {}
	}

	previousWriter := log.Writer()
	previousPrefix := log.Prefix()
	previousFlags := log.Flags()

	log.SetOutput(io.Discard)

	return func() {
		log.SetOutput(previousWriter)
		log.SetPrefix(previousPrefix)
		log.SetFlags(previousFlags)
	}
}

type historyPipeline struct {
	Core   []analyze.HistoryAnalyzer
	Leaves map[string]analyze.HistoryAnalyzer
}

func buildPipeline(repository *gitlib.Repository) *historyPipeline {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: repository}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: repository}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	return &historyPipeline{
		Core: []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges,
		},
		Leaves: map[string]analyze.HistoryAnalyzer{
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

func defaultHistoryLeaves() []analyze.HistoryAnalyzer {
	leaves := buildPipeline(nil).Leaves

	return []analyze.HistoryAnalyzer{
		leaves["burndown"],
		leaves["couples"],
		leaves["devs"],
		leaves["file-history"],
		leaves["imports"],
		leaves["sentiment"],
		leaves["shotness"],
		leaves["typos"],
	}
}

func defaultStaticAnalyzers() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		complexity.NewAnalyzer(),
		comments.NewAnalyzer(),
		halstead.NewAnalyzer(),
		cohesion.NewAnalyzer(),
		imports.NewAnalyzer(),
	}
}
