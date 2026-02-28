// Package commands implements CLI command handlers for codefang.
package commands

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"log/slog"
	"os"
	"os/signal"
	"runtime"
	"slices"
	"syscall"
	"time"

	"github.com/dustin/go-humanize"
	"github.com/spf13/cobra"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/clones"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/quality"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/internal/budget"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	cfgpkg "github.com/Sumatoshi-tech/codefang/internal/config"
	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/internal/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

type staticExecutor func(
	path string,
	analyzerIDs []string,
	format string,
	verbose bool,
	noColor bool,
	writer io.Writer,
) error

type historyExecutor func(
	ctx context.Context, path string, analyzerIDs []string, format string,
	silent bool, opts HistoryRunOptions, writer io.Writer,
) error

type registryProvider func() (*analyze.Registry, error)

type observabilityInitFunc func(cfg observability.Config) (observability.Providers, error)

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

	DebugTrace bool
	NDJSON     bool

	ConfigFile string
}

var (
	// ErrNoAnalyzersSelected is returned when no analyzer IDs match the selection.
	ErrNoAnalyzersSelected = errors.New(
		"no analyzers selected. Use -a flag, e.g.: -a burndown,couples\n" +
			"Available: anomaly, burndown, couples, devs, file-history, imports, quality, sentiment, shotness, typos",
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

	debugTrace bool

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

	ndjson bool

	configFile      string
	listAnalyzers   bool
	diagnosticsAddr string

	staticExec        staticExecutor
	historyExec       historyExecutor
	registryFn        registryProvider
	observabilityInit observabilityInitFunc
}

// NewRunCommand creates the unified run command.
func NewRunCommand() *cobra.Command {
	anomaly.RegisterPlotSections()
	burndown.RegisterPlotSections()
	clones.RegisterPlotSections()
	cohesion.RegisterPlotSections()
	comments.RegisterPlotSections()
	complexity.RegisterPlotSections()
	couples.RegisterPlotSections()
	filehistory.RegisterPlotSections()
	halstead.RegisterPlotSections()
	imports.RegisterPlotSections()
	quality.RegisterPlotSections()
	sentiment.RegisterPlotSections()
	shotness.RegisterPlotSections()
	typos.RegisterPlotSections()

	quality.RegisterTimeSeriesExtractor()
	sentiment.RegisterTimeSeriesExtractor()
	renderer.RegisterPlotRenderer()

	return newRunCommandWithDeps(runStaticAnalyzers, runHistoryAnalyzers, defaultRegistry, observability.Init)
}

func newRunCommandWithDeps(
	staticExec staticExecutor,
	historyExec historyExecutor,
	registryFn registryProvider,
	otelInit observabilityInitFunc,
) *cobra.Command {
	rc := &RunCommand{
		format:            analyze.FormatJSON,
		staticExec:        staticExec,
		historyExec:       historyExec,
		registryFn:        registryFn,
		observabilityInit: otelInit,
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
	cmd.Flags().StringVar(&rc.format, "format", analyze.FormatJSON,
		"Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact")
	cmd.Flags().BoolVar(&rc.ndjson, "ndjson", false, "With --format timeseries: emit one JSON line per commit (NDJSON)")
	cmd.Flags().StringVar(&rc.inputPath, "input", "", "Input report path for cross-format conversion")
	cmd.Flags().StringVar(&rc.inputFormat, "input-format", analyze.InputFormatAuto, "Input format: auto, json, bin")
	cmd.Flags().IntVar(&rc.gogc, "gogc", 0, "GC percent for history pipeline (0 = auto, >0 = exact)")
	cmd.Flags().StringVar(&rc.ballastSize, "ballast-size", "0", "Optional GC ballast size for history pipeline (0 = disabled)")
	cmd.Flags().BoolVarP(&rc.verbose, "verbose", "v", false, "Show full static report details")
	cmd.Flags().BoolVar(&rc.silent, "silent", false, "Disable progress output")
	cmd.Flags().BoolVar(&rc.noColor, "no-color", false, "Disable colored static output")
	cmd.Flags().StringVarP(&rc.path, "path", "p", ".", "Folder/repository path to analyze")

	cmd.Flags().BoolVar(&rc.debugTrace, "debug-trace", false, "Enable 100% trace sampling for debugging")

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

	cmd.Flags().StringVar(&rc.configFile, "config", "", "Configuration file path (default: .codefang.yaml in CWD or $HOME)")
	cmd.Flags().BoolVar(&rc.listAnalyzers, "list-analyzers", false, "List all available analyzer IDs and exit")
	cmd.Flags().StringVar(
		&rc.diagnosticsAddr, "diagnostics-addr", "",
		"Start diagnostics HTTP server (health/metrics) at this address (e.g., :6060)",
	)

	registerAnalyzerFlags(cmd)

	return cmd
}

//nolint:funlen // tracing + diagnostics setup require extra statements.
func (rc *RunCommand) run(cmd *cobra.Command, args []string) (runResult error) {
	providers, err := rc.initObservability()
	if err != nil {
		return fmt.Errorf("init observability: %w", err)
	}

	ctx, stop := signal.NotifyContext(cmd.Context(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	defer func() {
		shutdownErr := providers.Shutdown(ctx)
		if shutdownErr != nil && providers.Logger != nil {
			providers.Logger.Warn("observability shutdown failed", "error", shutdownErr)
		}
	}()

	if providers.Tracer != nil {
		var rootSpan trace.Span

		ctx, rootSpan = providers.Tracer.Start(ctx, "codefang.run")

		start := time.Now()

		defer func() {
			var m runtime.MemStats
			runtime.ReadMemStats(&m)

			rootSpan.SetAttributes(
				attribute.Bool("error", runResult != nil),
				attribute.String("codefang.duration_class", durationClass(time.Since(start))),
				attribute.String("codefang.format", rc.format),
				attribute.Int64("codefang.memory_sys_mb", int64(m.Sys/bytesPerMiB)),
			)
			rootSpan.End()
		}()
	}

	path := rc.resolvePath(args)
	silent := rc.isSilent(cmd)
	progressWriter := cmd.ErrOrStderr()

	if rc.diagnosticsAddr != "" {
		diagServer, diagErr := observability.NewDiagnosticsServer(rc.diagnosticsAddr, providers.Meter)
		if diagErr != nil {
			return fmt.Errorf("start diagnostics server: %w", diagErr)
		}

		defer diagServer.Close()

		rc.progressf(silent, progressWriter, "diagnostics server listening on %s", diagServer.Addr())
	}

	rc.progressf(silent, progressWriter, "starting run path=%s", path)

	registry, err := rc.registryFn()
	if err != nil {
		return err
	}

	if rc.listAnalyzers {
		rc.printAnalyzerList(cmd.OutOrStdout(), registry)

		return nil
	}

	ids, err := registry.SelectedIDs(rc.analyzerIDs)
	if err != nil {
		return err
	}

	// Enrich root span with run parameters after resolution.
	if span := trace.SpanFromContext(ctx); span.IsRecording() {
		span.SetAttributes(
			attribute.String("codefang.path", path),
			attribute.Int("codefang.analyzers", len(ids)),
			attribute.Int("codefang.limit", rc.limit),
		)
	}

	rc.progressf(silent, progressWriter, "selected analyzers: total=%d", len(ids))

	if rc.inputPath != "" {
		return rc.runInputConversion(cmd.OutOrStdout(), registry, ids, silent, progressWriter)
	}

	return rc.runDirect(ctx, path, ids, registry, silent, progressWriter, cmd.OutOrStdout(), cmd)
}

func (rc *RunCommand) initObservability() (observability.Providers, error) {
	cfg := observability.DefaultConfig()
	cfg.ServiceVersion = version.Version
	cfg.OTLPEndpoint = os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT")
	cfg.OTLPHeaders = observability.ParseOTLPHeaders(os.Getenv("OTEL_EXPORTER_OTLP_HEADERS"))
	cfg.OTLPInsecure = os.Getenv("OTEL_EXPORTER_OTLP_INSECURE") == "true"
	cfg.Mode = observability.ModeCLI
	cfg.DebugTrace = rc.debugTrace

	return rc.observabilityInit(cfg)
}

// bytesPerMiB is used to convert bytes to mebibytes.
const bytesPerMiB = 1024 * 1024

// Duration class thresholds for tail-sampling support.
const (
	durationClassFastLimit   = 10 * time.Second
	durationClassNormalLimit = 60 * time.Second
)

// Duration class label values.
const (
	durationClassFast   = "fast"
	durationClassNormal = "normal"
	durationClassSlow   = "slow"
)

// durationClass returns a coarse duration label for tail-sampling filters.
func durationClass(d time.Duration) string {
	switch {
	case d < durationClassFastLimit:
		return durationClassFast
	case d < durationClassNormalLimit:
		return durationClassNormal
	default:
		return durationClassSlow
	}
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
	ctx context.Context,
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

		return rc.renderCombinedDirect(ctx, path, staticIDs, historyIDs, registry, staticFormat, silent, progressWriter, writer, cmd)
	}

	err = rc.runStaticPhase(path, staticIDs, staticFormat, silent, progressWriter, writer)
	if err != nil {
		return err
	}

	return rc.runHistoryPhase(ctx, path, historyIDs, historyFormat, silent, progressWriter, writer, cmd)
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
	ctx context.Context,
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

	err := rc.historyExec(ctx, path, historyIDs, historyFormat, silent, opts, writer)
	if err != nil {
		return err
	}

	rc.progressf(silent, progressWriter, "history phase finished in %s", time.Since(startedAt).Round(time.Millisecond))

	return nil
}

func (rc *RunCommand) renderCombinedDirect(
	ctx context.Context,
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

	err = rc.historyExec(ctx, path, historyIDs, analyze.FormatBinary, silent, opts, &raw)
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

	// Apply --ndjson modifier to timeseries format.
	renderFormat := outputFormat
	if rc.ndjson && renderFormat == analyze.FormatTimeSeries {
		renderFormat = analyze.FormatTimeSeriesNDJSON
	}

	rc.progressf(silent, progressWriter, "combined output rendering started")

	err = analyze.WriteConvertedOutput(model, renderFormat, writer)
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
		DebugTrace:      rc.debugTrace,
		NDJSON:          rc.ndjson,
		ConfigFile:      rc.configFile,
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

func (rc *RunCommand) printAnalyzerList(writer io.Writer, registry *analyze.Registry) {
	staticIDs := registry.IDsByMode(analyze.ModeStatic)
	historyIDs := registry.IDsByMode(analyze.ModeHistory)

	fmt.Fprintf(writer, "Static analyzers (%d):\n", len(staticIDs))

	for _, id := range staticIDs {
		desc, _ := registry.Descriptor(id)
		fmt.Fprintf(writer, "  %-30s %s\n", id, desc.Description)
	}

	fmt.Fprintf(writer, "\nHistory analyzers (%d):\n", len(historyIDs))

	for _, id := range historyIDs {
		desc, _ := registry.Descriptor(id)
		fmt.Fprintf(writer, "  %-30s %s\n", id, desc.Description)
	}

	fmt.Fprintf(writer, "\nTotal: %d analyzers\n", len(registry.All()))
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

	return service.RunAndFormat(context.Background(), path, analyzerIDs, format, verbose, noColor, writer)
}

func runHistoryAnalyzers(
	ctx context.Context, path string, analyzerIDs []string, format string,
	silent bool, opts HistoryRunOptions, writer io.Writer,
) error {
	restoreLogger := suppressStandardLogger(silent)
	defer restoreLogger()

	stopProfiler, err := framework.MaybeStartCPUProfile(opts.CPUProfile)
	if err != nil {
		return err
	}

	defer stopProfiler()
	defer framework.MaybeWriteHeapProfile(opts.HeapProfile, nil)

	configureLibgit2MemoryLimits(opts.MemoryBudget)

	result, err := initHistoryPipeline(ctx, path, analyzerIDs, format, opts)
	if err != nil {
		return err
	}
	defer result.repository.Free()

	if result.commitIter != nil {
		defer result.commitIter.Close()
	}

	// Apply --ndjson modifier: timeseries → timeseries+ndjson.
	pipelineFormat := result.format
	if opts.NDJSON && pipelineFormat == analyze.FormatTimeSeries {
		pipelineFormat = analyze.FormatTimeSeriesNDJSON
	}

	return executeHistoryPipeline(
		ctx, result.pipeline, path, result.selectedLeaves,
		result.commits, result.commitIter, result.commitCount,
		result.analyzerKeys, pipelineFormat, opts, result.repository, writer,
	)
}

// initResult holds the outputs of the init phase.
type initResult struct {
	pipeline       *historyPipeline
	repository     *gitlib.Repository
	commits        []*gitlib.Commit   // Used only for HeadOnly mode.
	commitIter     *gitlib.CommitIter // Iterator for streaming mode.
	commitCount    int                // Total commits for streaming mode.
	selectedLeaves []analyze.HistoryAnalyzer
	analyzerKeys   []string
	format         string
}

// initHistoryPipeline performs the initialization phase: builds the pipeline,
// resolves analyzers, loads the repository and commits. Emits a codefang.init span.
func initHistoryPipeline(
	ctx context.Context, path string, analyzerIDs []string, format string, opts HistoryRunOptions,
) (initResult, error) {
	tr := otel.Tracer("codefang")
	_, initSpan := tr.Start(ctx, "codefang.init")

	defer initSpan.End()

	pl := buildPipeline(nil)

	analyzerKeys, err := analyze.HistoryKeysByID(pl.Leaves, analyzerIDs)
	if err != nil {
		return initResult{}, err
	}

	if len(analyzerKeys) == 0 {
		return initResult{}, ErrNoAnalyzersSelected
	}

	normalizedFormat, err := analyze.ValidateUniversalFormat(format)
	if err != nil {
		return initResult{}, err
	}

	repository, err := gitlib.LoadRepository(path)
	if err != nil {
		return initResult{}, fmt.Errorf("%w: %s", ErrRepositoryLoad, path)
	}

	pl = buildPipeline(repository)

	if slices.Contains(analyzerKeys, "burndown") && !opts.FirstParent {
		opts.FirstParent = true
	}

	// HeadOnly mode: load a single commit, no iterator needed.
	if opts.Head {
		return initHeadOnly(ctx, repository, pl, analyzerKeys, normalizedFormat, opts.ConfigFile, initSpan)
	}

	// Streaming mode: count commits and create a reverse iterator.
	return initStreamingIterator(repository, pl, analyzerKeys, normalizedFormat, opts, initSpan)
}

// initHeadOnly loads only the HEAD commit and returns an initResult for head-only analysis.
func initHeadOnly(
	ctx context.Context,
	repository *gitlib.Repository,
	pl *historyPipeline,
	analyzerKeys []string,
	normalizedFormat string,
	configFile string,
	initSpan trace.Span,
) (initResult, error) {
	commits, loadErr := gitlib.LoadCommits(ctx, repository, gitlib.CommitLoadOptions{
		HeadOnly: true,
	})
	if loadErr != nil {
		repository.Free()

		return initResult{}, loadErr
	}

	selectedLeaves, configErr := configureAndSelect(pl, analyzerKeys, configFile)
	if configErr != nil {
		repository.Free()

		return initResult{}, configErr
	}

	initSpan.SetAttributes(
		attribute.Int("init.commits", len(commits)),
		attribute.Int("init.analyzers", len(analyzerKeys)),
	)

	return initResult{
		pipeline:       pl,
		repository:     repository,
		commits:        commits,
		selectedLeaves: selectedLeaves,
		analyzerKeys:   analyzerKeys,
		format:         normalizedFormat,
	}, nil
}

// initStreamingIterator counts commits and creates a reverse iterator for streaming analysis.
func initStreamingIterator(
	repository *gitlib.Repository,
	pl *historyPipeline,
	analyzerKeys []string,
	normalizedFormat string,
	opts HistoryRunOptions,
	initSpan trace.Span,
) (initResult, error) {
	logOpts := &gitlib.LogOptions{
		FirstParent: opts.FirstParent,
	}

	if opts.Since != "" {
		sinceTime, parseErr := gitlib.ParseTime(opts.Since)
		if parseErr != nil {
			repository.Free()

			return initResult{}, fmt.Errorf("invalid time format for --since: %w", parseErr)
		}

		logOpts.Since = &sinceTime
	}

	commitCount, err := repository.CommitCount(logOpts)
	if err != nil {
		repository.Free()

		return initResult{}, fmt.Errorf("failed to count commits: %w", err)
	}

	if opts.Limit > 0 && opts.Limit < commitCount {
		commitCount = opts.Limit
	}

	// Reverse is implicitly handled by the backend Log() implementation
	// for --first-parent.
	logOpts.Reverse = true

	iter, err := repository.Log(logOpts)
	if err != nil {
		repository.Free()

		return initResult{}, fmt.Errorf("failed to create commit iterator: %w", err)
	}

	selectedLeaves, configErr := configureAndSelect(pl, analyzerKeys, opts.ConfigFile)
	if configErr != nil {
		iter.Close()
		repository.Free()

		return initResult{}, configErr
	}

	initSpan.SetAttributes(
		attribute.Int("init.commits", commitCount),
		attribute.Int("init.analyzers", len(analyzerKeys)),
		attribute.Bool("init.iterator_mode", true),
	)

	return initResult{
		pipeline:       pl,
		repository:     repository,
		commitIter:     iter,
		commitCount:    commitCount,
		selectedLeaves: selectedLeaves,
		analyzerKeys:   analyzerKeys,
		format:         normalizedFormat,
	}, nil
}

// configureAndSelect configures core analyzers with facts and selects leaf analyzers.
// When configFile is non-empty, it loads analyzer settings from the given config file
// and applies them to facts before configuring analyzers.
func configureAndSelect(pl *historyPipeline, analyzerKeys []string, configFile string) ([]analyze.HistoryAnalyzer, error) {
	facts := buildFacts(pl)

	// Apply file-based configuration if provided.
	cfg, cfgErr := cfgpkg.LoadConfig(configFile)
	if cfgErr != nil {
		return nil, fmt.Errorf("load config: %w", cfgErr)
	}

	cfg.ApplyToFacts(facts)

	// Configure core (plumbing) analyzers first so they can publish facts
	// (e.g. TicksSinceStart publishes FactCommitsByTick) that leaves depend on.
	err := configureAnalyzers(pl.Core, facts)
	if err != nil {
		return nil, err
	}

	selectedLeaves, err := selectLeaves(pl.Leaves, analyzerKeys, facts)
	if err != nil {
		return nil, err
	}

	return selectedLeaves, nil
}

func executeHistoryPipeline(
	ctx context.Context,
	pl *historyPipeline,
	path string,
	selectedLeaves []analyze.HistoryAnalyzer,
	commits []*gitlib.Commit,
	commitIter *gitlib.CommitIter,
	commitCount int,
	analyzerKeys []string,
	normalizedFormat string,
	opts HistoryRunOptions,
	repository *gitlib.Repository,
	writer io.Writer,
) error {
	// Core analyzers are already configured in initHistoryPipeline (before leaf
	// selection) so that plumbing facts are available when leaves Configure().
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

	coordConfig.FirstParent = opts.FirstParent

	if !needsUAST(selectedLeaves) {
		coordConfig.UASTPipelineWorkers = 0
	}

	runner := framework.NewRunnerWithConfig(repository, path, coordConfig, allAnalyzers...)
	runner.CoreCount = len(pl.Core)

	red, analysisMetrics, metricsErr := createRunMetrics()
	if metricsErr != nil {
		return metricsErr
	}

	done := red.TrackInflight(ctx, "cli.run")
	runStart := time.Now()

	streamConfig := buildStreamingConfig(path, analyzerKeys, memBudget, opts, analysisMetrics, normalizedFormat, writer, selectedLeaves)

	var results map[analyze.HistoryAnalyzer]analyze.Report

	if commitIter != nil {
		results, err = framework.RunStreamingFromIterator(ctx, runner, commitIter, commitCount, allAnalyzers, streamConfig)
	} else {
		results, err = framework.RunStreaming(ctx, runner, commits, allAnalyzers, streamConfig)
	}

	recordRunCompletion(ctx, red, done, runStart, err)

	if err != nil {
		return fmt.Errorf("pipeline execution failed: %w", err)
	}

	// In NDJSON and streaming timeseries NDJSON modes, output was already written.
	if normalizedFormat == analyze.FormatNDJSON || normalizedFormat == analyze.FormatTimeSeriesNDJSON {
		return nil
	}

	enrichAnomalyReport(selectedLeaves, results)

	return renderReport(ctx, selectedLeaves, results, normalizedFormat, writer)
}

// buildStreamingConfig creates a StreamingConfig, wiring a TCSink when NDJSON format is requested,
// or a TimeSeriesChunkFlusher when streaming timeseries NDJSON is requested.
func buildStreamingConfig(
	path string, analyzerKeys []string, memBudget int64,
	opts HistoryRunOptions, analysisMetrics *observability.AnalysisMetrics,
	normalizedFormat string, writer io.Writer,
	selectedLeaves []analyze.HistoryAnalyzer,
) framework.StreamingConfig {
	cfg := framework.StreamingConfig{
		MemBudget:       memBudget,
		Logger:          slog.Default(),
		Checkpoint:      buildCheckpointParams(opts),
		RepoPath:        path,
		AnalyzerNames:   analyzerKeys,
		DebugTrace:      opts.DebugTrace,
		AnalysisMetrics: analysisMetrics,
	}

	// NDJSON mode: write one JSON line per TC directly to writer, bypass aggregators.
	if normalizedFormat == analyze.FormatNDJSON {
		sink := analyze.NewStreamingSink(writer)
		cfg.TCSink = sink.WriteTC
	}

	// Streaming timeseries NDJSON: drain per-commit data after each chunk,
	// write NDJSON lines, spill cumulative aggregator state to disk, and
	// skip final report generation. Spilling frees O(files²) coupling
	// matrices and O(files×ticks) burndown histories that would otherwise
	// grow unbounded across chunks.
	if normalizedFormat == analyze.FormatTimeSeriesNDJSON {
		flusher := analyze.NewTimeSeriesChunkFlusher(writer, selectedLeaves)
		cfg.OnChunkComplete = func(runner *framework.Runner) error {
			meta := runner.DrainCommitMeta()
			aggs := runner.LeafAggregators()

			_, err := flusher.Flush(aggs, meta)
			if err != nil {
				return err
			}

			// Discard cumulative state from both aggregators and leaf analyzers.
			// Since SkipFinalize is true, no final report will be generated.
			// Aggregator state: coupling matrices, burndown histories, etc.
			// Leaf analyzer state: shotness node coupling maps (O(N²)), etc.
			runner.DiscardAggregatorState()
			runner.DiscardLeafAnalyzerState()

			return nil
		}
		cfg.SkipFinalize = true
	}

	return cfg
}

// renderReport writes analysis results in the requested format, wrapped in a tracing span.
func renderReport(
	ctx context.Context,
	selectedLeaves []analyze.HistoryAnalyzer,
	results map[analyze.HistoryAnalyzer]analyze.Report,
	normalizedFormat string,
	writer io.Writer,
) error {
	tr := otel.Tracer("codefang")
	_, reportSpan := tr.Start(ctx, "codefang.report",
		trace.WithAttributes(
			attribute.String("report.format", normalizedFormat),
			attribute.Int("report.analyzers", len(selectedLeaves)),
		))

	reportErr := analyze.OutputHistoryResults(selectedLeaves, results, normalizedFormat, writer)

	reportSpan.End()

	return reportErr
}

// createRunMetrics creates RED and analysis metric instruments using the global meter.
func createRunMetrics() (*observability.REDMetrics, *observability.AnalysisMetrics, error) {
	meter := otel.Meter("codefang")

	red, err := observability.NewREDMetrics(meter)
	if err != nil {
		return nil, nil, fmt.Errorf("create RED metrics: %w", err)
	}

	analysis, err := observability.NewAnalysisMetrics(meter)
	if err != nil {
		return nil, nil, fmt.Errorf("create analysis metrics: %w", err)
	}

	return red, analysis, nil
}

// recordRunCompletion records RED metrics for a completed (or failed) CLI run
// and decrements the in-flight gauge.
func recordRunCompletion(ctx context.Context, red *observability.REDMetrics, done func(), start time.Time, runErr error) {
	duration := time.Since(start)

	done()

	status := "ok"
	if runErr != nil {
		status = "error"
	}

	red.RecordRequest(ctx, "cli.run", status, duration)
}

// enrichAnomalyReport runs cross-analyzer anomaly detection on time series
// from other analyzers and injects results into the anomaly report.
func enrichAnomalyReport(
	leaves []analyze.HistoryAnalyzer,
	results map[analyze.HistoryAnalyzer]analyze.Report,
) {
	var anomalyAnalyzer *anomaly.Analyzer

	var anomalyReport analyze.Report

	for _, leaf := range leaves {
		if a, ok := leaf.(*anomaly.Analyzer); ok {
			anomalyAnalyzer = a
			anomalyReport = results[leaf]

			break
		}
	}

	if anomalyAnalyzer == nil || anomalyReport == nil {
		return
	}

	otherReports := make(map[string]analyze.Report)

	for _, leaf := range leaves {
		if leaf == anomalyAnalyzer {
			continue
		}

		if rep := results[leaf]; rep != nil {
			otherReports[leaf.Flag()] = rep
		}
	}

	if len(otherReports) == 0 {
		return
	}

	anomaly.EnrichFromReports(
		anomalyReport,
		otherReports,
		anomalyAnalyzer.WindowSize,
		float64(anomalyAnalyzer.Threshold),
	)
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
				"%w: %s\nAvailable: anomaly, burndown, couples, devs, file-history, imports, quality, sentiment, shotness, typos",
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

// configureLibgit2MemoryLimits sets libgit2 global mwindow and object cache
// limits proportional to the memory budget. Must be called before opening
// any repository handles. When budgetStr is empty, uses auto-detected budget.
func configureLibgit2MemoryLimits(budgetStr string) {
	var budgetBytes int64

	if budgetStr != "" {
		parsed, err := humanize.ParseBytes(budgetStr)
		if err == nil {
			budgetBytes = framework.SafeInt64(parsed)
		}
	}

	if budgetBytes == 0 {
		budgetBytes = framework.DefaultMemoryBudget()
	}

	if budgetBytes <= 0 {
		return
	}

	limits := budget.NativeLimitsForBudget(budgetBytes)

	err := gitlib.ConfigureMemoryLimits(limits.MwindowMappedLimit, limits.CacheMaxSize, limits.MallocArenaMax)
	if err != nil {
		slog.Default().Warn("failed to configure libgit2 memory limits", "error", err)

		return
	}

	slog.Default().Info("native memory limits configured",
		"budget_mib", budgetBytes/budget.MiB,
		"mwindow_limit_mib", limits.MwindowMappedLimit/budget.MiB,
		"cache_limit_mib", limits.CacheMaxSize/budget.MiB,
		"malloc_arena_max", limits.MallocArenaMax)
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

func buildPipeline(repository *gitlib.Repository) *historyPipeline { //nolint:funlen // Expected length for pipeline initialization.
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
			"anomaly": func() *anomaly.Analyzer {
				a := anomaly.NewAnalyzer()
				a.TreeDiff = treeDiff
				a.Ticks = ticks
				a.LineStats = lineStats
				a.Languages = langDetect
				a.Identity = identity

				return a
			}(),
			"burndown": func() *burndown.HistoryAnalyzer {
				a := burndown.NewHistoryAnalyzer()
				a.BlobCache = blobCache
				a.Ticks = ticks
				a.Identity = identity
				a.FileDiff = fileDiff
				a.TreeDiff = treeDiff

				return a
			}(),
			"couples": func() *couples.HistoryAnalyzer {
				a := couples.NewHistoryAnalyzer()
				a.Identity = identity
				a.TreeDiff = treeDiff

				return a
			}(),
			"devs": func() *devs.Analyzer {
				a := devs.NewAnalyzer()
				a.Identity = identity
				a.TreeDiff = treeDiff
				a.Ticks = ticks
				a.Languages = langDetect
				a.LineStats = lineStats

				return a
			}(),
			"file-history": func() *filehistory.HistoryAnalyzer {
				a := filehistory.NewAnalyzer()
				a.Identity = identity
				a.TreeDiff = treeDiff
				a.LineStats = lineStats

				return a
			}(),
			"imports": func() *imports.HistoryAnalyzer {
				a := imports.NewHistoryAnalyzer()
				a.UAST = uastChanges
				a.Identity = identity
				a.Ticks = ticks

				return a
			}(),
			"quality": func() *quality.Analyzer {
				a := quality.NewAnalyzer()
				a.UAST = uastChanges
				a.Ticks = ticks

				return a
			}(),
			"sentiment": func() *sentiment.Analyzer {
				a := sentiment.NewAnalyzer()
				a.UAST = uastChanges
				a.Ticks = ticks

				return a
			}(),
			"shotness": func() *shotness.Analyzer {
				a := shotness.NewAnalyzer()
				a.FileDiff = fileDiff
				a.UAST = uastChanges

				return a
			}(),
			"typos": func() *typos.Analyzer {
				a := typos.NewAnalyzer()
				a.UAST = uastChanges
				a.BlobCache = blobCache
				a.FileDiff = fileDiff

				return a
			}(),
		},
	}
}

func defaultHistoryLeaves() []analyze.HistoryAnalyzer {
	leaves := buildPipeline(nil).Leaves

	return []analyze.HistoryAnalyzer{
		leaves["anomaly"],
		leaves["burndown"],
		leaves["couples"],
		leaves["devs"],
		leaves["file-history"],
		leaves["imports"],
		leaves["quality"],
		leaves["sentiment"],
		leaves["shotness"],
		leaves["typos"],
	}
}

func defaultStaticAnalyzers() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		clones.NewAnalyzer(),
		complexity.NewAnalyzer(),
		comments.NewAnalyzer(),
		halstead.NewAnalyzer(),
		cohesion.NewAnalyzer(),
		imports.NewAnalyzer(),
	}
}
