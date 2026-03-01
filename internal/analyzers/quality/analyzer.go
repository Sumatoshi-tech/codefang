// Package quality tracks code quality metrics (complexity, Halstead, comments,
// cohesion) across commit history by running static analyzers on per-commit
// UAST-parsed changed files.
package quality

import (
	"context"
	"maps"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TickQuality holds per-file quality metric values for a single tick.
// Values are appended per-file during Consume; statistics are computed at output time.
type TickQuality struct {
	// Per-file complexity values.
	Complexities    []float64 // Cyclomatic complexity per file.
	Cognitives      []float64 // Cognitive complexity per file.
	MaxComplexities []int     // Max single-function complexity per file.
	Functions       []int     // Function count per file.

	// Per-file Halstead values.
	HalsteadVolumes []float64
	HalsteadEfforts []float64
	DeliveredBugs   []float64

	// Per-file comment/doc values.
	CommentScores []float64
	DocCoverages  []float64

	// Per-file cohesion values.
	CohesionScores []float64
}

// merge incorporates values from another TickQuality into this one.
func (tq *TickQuality) merge(other *TickQuality) {
	if other == nil {
		return
	}

	tq.Complexities = append(tq.Complexities, other.Complexities...)
	tq.Cognitives = append(tq.Cognitives, other.Cognitives...)
	tq.MaxComplexities = append(tq.MaxComplexities, other.MaxComplexities...)
	tq.Functions = append(tq.Functions, other.Functions...)

	tq.HalsteadVolumes = append(tq.HalsteadVolumes, other.HalsteadVolumes...)
	tq.HalsteadEfforts = append(tq.HalsteadEfforts, other.HalsteadEfforts...)
	tq.DeliveredBugs = append(tq.DeliveredBugs, other.DeliveredBugs...)

	tq.CommentScores = append(tq.CommentScores, other.CommentScores...)
	tq.DocCoverages = append(tq.DocCoverages, other.DocCoverages...)

	tq.CohesionScores = append(tq.CohesionScores, other.CohesionScores...)
}

// TickData is the per-tick aggregated payload for the quality analyzer.
// It holds per-commit quality for the canonical report format.
type TickData struct {
	// CommitQuality maps commit hash (hex) to per-commit TickQuality.
	CommitQuality map[string]*TickQuality
}

// tickAccumulator holds per-commit quality during aggregation.
type tickAccumulator struct {
	commitQuality map[string]*TickQuality
}

// Analyzer tracks code quality metrics across commit history by running
// static analyzers on UAST-parsed changed files per commit.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	UAST  *plumbing.UASTChangesAnalyzer
	Ticks *plumbing.TicksSinceStart

	commitsByTick map[int][]gitlib.Hash

	// Static analyzers (stateless, created in Initialize).
	complexityAnalyzer *complexity.Analyzer
	halsteadAnalyzer   *halstead.Analyzer
	commentsAnalyzer   *comments.Analyzer
	cohesionAnalyzer   *cohesion.Analyzer
}

// NewAnalyzer creates a new quality Analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{
		complexityAnalyzer: complexity.NewAnalyzer(),
		halsteadAnalyzer:   halstead.NewAnalyzer(),
		commentsAnalyzer:   comments.NewAnalyzer(),
		cohesionAnalyzer:   cohesion.NewAnalyzer(),
	}

	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/quality",
			Description: "Tracks complexity, Halstead, comment quality, and cohesion metrics over commit history.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ComputeMetricsFn: func(report analyze.Report) (*ComputedMetrics, error) {
			if len(report) == 0 {
				return &ComputedMetrics{}, nil
			}

			return ComputeAllMetrics(report)
		},
		AggregatorFn: func(opts analyze.AggregatorOptions) analyze.Aggregator {
			agg := analyze.NewGenericAggregator[*tickAccumulator, *TickData](opts, extractTC, mergeState, sizeState, buildTick)
			agg.DrainCommitDataFn = drainQualityCommitData

			return agg
		},
	}

	a.TicksToReportFn = func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
		return ticksToReport(ctx, ticks, a.commitsByTick)
	}

	return a
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (a *Analyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return ticksToReport(ctx, ticks, a.commitsByTick), nil
}

// Configure applies configuration from the provided facts map.
func (a *Analyzer) Configure(facts map[string]any) error {
	if val, ok := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); ok {
		a.commitsByTick = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (a *Analyzer) Initialize(_ *gitlib.Repository) error {
	return nil
}

// CPUHeavy returns true because quality analysis performs UAST processing per commit.
func (a *Analyzer) CPUHeavy() bool { return true }

// Consume processes a single commit, running static analyzers on each changed
// file's UAST. Returns a TC with the per-commit *TickQuality as payload.
func (a *Analyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	changes := a.UAST.Changes(ctx)
	cq := &TickQuality{}

	for change := range changes {
		if change.After == nil {
			continue
		}

		a.analyzeNode(change.After, cq)
	}

	tc := analyze.TC{Data: cq}

	if ac != nil && ac.Commit != nil {
		tc.CommitHash = ac.Commit.Hash()
	}

	return tc, nil
}

func (a *Analyzer) analyzeNode(root *node.Node, tq *TickQuality) {
	a.analyzeComplexity(root, tq)
	a.analyzeHalstead(root, tq)
	a.analyzeComments(root, tq)
	a.analyzeCohesion(root, tq)
}

func (a *Analyzer) analyzeComplexity(root *node.Node, tq *TickQuality) {
	report, err := a.complexityAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.Complexities = append(tq.Complexities, float64(extractInt(report, "total_complexity")))
	tq.Cognitives = append(tq.Cognitives, float64(extractInt(report, "cognitive_complexity")))
	tq.MaxComplexities = append(tq.MaxComplexities, extractInt(report, "max_complexity"))
	tq.Functions = append(tq.Functions, extractInt(report, "total_functions"))
}

func (a *Analyzer) analyzeHalstead(root *node.Node, tq *TickQuality) {
	report, err := a.halsteadAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.HalsteadVolumes = append(tq.HalsteadVolumes, extractFloat(report, "volume"))
	tq.HalsteadEfforts = append(tq.HalsteadEfforts, extractFloat(report, "effort"))
	tq.DeliveredBugs = append(tq.DeliveredBugs, extractFloat(report, "delivered_bugs"))
}

func (a *Analyzer) analyzeComments(root *node.Node, tq *TickQuality) {
	report, err := a.commentsAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.CommentScores = append(tq.CommentScores, extractFloat(report, "overall_score"))
	tq.DocCoverages = append(tq.DocCoverages, extractFloat(report, "documentation_coverage"))
}

func (a *Analyzer) analyzeCohesion(root *node.Node, tq *TickQuality) {
	report, err := a.cohesionAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.CohesionScores = append(tq.CohesionScores, extractFloat(report, "cohesion_score"))
}

// NewAggregator creates a quality Aggregator configured with the given options.
func (a *Analyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return a.AggregatorFn(opts)
}

// Fork creates independent copies of the analyzer for parallel processing.
func (a *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &Analyzer{
			BaseHistoryAnalyzer: a.BaseHistoryAnalyzer,
			UAST:                &plumbing.UASTChangesAnalyzer{},
			Ticks:               &plumbing.TicksSinceStart{},
			commitsByTick:       a.commitsByTick, // shared read-only.
			complexityAnalyzer:  complexity.NewAnalyzer(),
			halsteadAnalyzer:    halstead.NewAnalyzer(),
			commentsAnalyzer:    comments.NewAnalyzer(),
			cohesionAnalyzer:    cohesion.NewAnalyzer(),
		}
		res[i] = clone
	}

	return res
}

// Merge is a no-op. Per-commit results are emitted as TCs and
// collected by the framework, not accumulated inside the analyzer.
func (a *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {}

// NeedsUAST returns true to enable the UAST pipeline.
func (a *Analyzer) NeedsUAST() bool { return true }

// SnapshotPlumbing captures the current plumbing output state.
func (a *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: a.UAST.TransferChanges(),
		Tick:        a.Ticks.Tick,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (a *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	a.UAST.SetChanges(ss.UASTChanges)
	a.Ticks.Tick = ss.Tick
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (a *Analyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	plumbing.ReleaseSnapshotUAST(ss)
}

func extractInt(report map[string]any, key string) int {
	if val, ok := report[key].(int); ok {
		return val
	}

	if val, ok := report[key].(float64); ok {
		return int(val)
	}

	return 0
}

func extractFloat(report map[string]any, key string) float64 {
	if val, ok := report[key].(float64); ok {
		return val
	}

	if val, ok := report[key].(int); ok {
		return float64(val)
	}

	return 0.0
}

// --- Generic Aggregator Delegates ---.

func extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	if tc.Data == nil || tc.CommitHash.IsZero() {
		return nil
	}

	tq, ok := tc.Data.(*TickQuality)
	if !ok || tq == nil {
		return nil
	}

	acc, ok := byTick[tc.Tick]
	if !ok {
		acc = &tickAccumulator{
			commitQuality: make(map[string]*TickQuality),
		}
		byTick[tc.Tick] = acc
	}

	acc.commitQuality[tc.CommitHash.String()] = tq

	return nil
}

func mergeState(dst, src *tickAccumulator) *tickAccumulator {
	if dst == nil {
		dst = &tickAccumulator{commitQuality: make(map[string]*TickQuality)}
	}

	if src != nil && src.commitQuality != nil {
		maps.Copy(dst.commitQuality, src.commitQuality)
	}

	return dst
}

func sizeState(state *tickAccumulator) int64 {
	if state == nil || state.commitQuality == nil {
		return 0
	}

	const (
		bytesPerEntry  = 8
		structOverhead = 64
		hashEntryBytes = 50
	)

	var size int64

	size += structOverhead

	for _, q := range state.commitQuality {
		if q == nil {
			continue
		}

		size += hashEntryBytes
		size += int64(len(q.Complexities)) * bytesPerEntry
		size += int64(len(q.Cognitives)) * bytesPerEntry
		size += int64(len(q.MaxComplexities)) * bytesPerEntry
		size += int64(len(q.Functions)) * bytesPerEntry
		size += int64(len(q.HalsteadVolumes)) * bytesPerEntry
		size += int64(len(q.HalsteadEfforts)) * bytesPerEntry
		size += int64(len(q.DeliveredBugs)) * bytesPerEntry
		size += int64(len(q.CommentScores)) * bytesPerEntry
		size += int64(len(q.DocCoverages)) * bytesPerEntry
		size += int64(len(q.CohesionScores)) * bytesPerEntry
	}

	return size
}

func buildTick(tick int, state *tickAccumulator) (analyze.TICK, error) {
	if state == nil || state.commitQuality == nil {
		return analyze.TICK{Tick: tick, Data: &TickData{CommitQuality: make(map[string]*TickQuality)}}, nil
	}

	return analyze.TICK{
		Tick: tick,
		Data: &TickData{
			CommitQuality: state.commitQuality,
		},
	}, nil
}

func ticksToReport(_ context.Context, ticks []analyze.TICK, commitsByTick map[int][]gitlib.Hash) analyze.Report {
	commitQuality := buildCommitQualityFromTicks(ticks)
	ct := commitsByTick

	if ct == nil {
		ct = buildCommitsByTickFromTicks(ticks)
	}

	return analyze.Report{
		"commit_quality":  commitQuality,
		"commits_by_tick": ct,
	}
}

func buildCommitQualityFromTicks(ticks []analyze.TICK) map[string]*TickQuality {
	commitQuality := make(map[string]*TickQuality)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommitQuality == nil {
			continue
		}

		maps.Copy(commitQuality, td.CommitQuality)
	}

	return commitQuality
}

func buildCommitsByTickFromTicks(ticks []analyze.TICK) map[int][]gitlib.Hash {
	ct := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommitQuality == nil {
			continue
		}

		hashes := make([]gitlib.Hash, 0, len(td.CommitQuality))

		for h := range td.CommitQuality {
			hashes = append(hashes, gitlib.NewHash(h))
		}

		ct[tick.Tick] = append(ct[tick.Tick], hashes...)
	}

	return ct
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It converts per-commit TickQuality data into summary statistics for the
// unified timeseries output, covering complexity, halstead, comments, and cohesion.
func (a *Analyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitQuality, ok := report["commit_quality"].(map[string]*TickQuality)
	if !ok || len(commitQuality) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitQuality))

	for hash, tq := range commitQuality {
		stats := computeTickStats(tq)
		result[hash] = map[string]any{
			"complexity_median":      stats.ComplexityMedian,
			"cognitive_median":       medianFloat(tq.Cognitives),
			"max_complexity":         stats.MaxComplexity,
			"functions":              stats.TotalFunctions,
			"halstead_vol_median":    stats.HalsteadVolMedian,
			"halstead_effort_median": medianFloat(tq.HalsteadEfforts),
			"delivered_bugs_sum":     stats.DeliveredBugsSum,
			"comment_score_min":      stats.CommentScoreMin,
			"doc_coverage_mean":      stats.DocCoverageMean,
			"cohesion_min":           stats.CohesionMin,
			"files_analyzed":         stats.FilesAnalyzed,
		}
	}

	return result
}

func drainQualityCommitData(state *tickAccumulator) (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if state == nil || len(state.commitQuality) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(state.commitQuality))
	for hash, tq := range state.commitQuality {
		stats := computeTickStats(tq)
		result[hash] = map[string]any{
			"complexity_median":      stats.ComplexityMedian,
			"cognitive_median":       medianFloat(tq.Cognitives),
			"max_complexity":         stats.MaxComplexity,
			"functions":              stats.TotalFunctions,
			"halstead_vol_median":    stats.HalsteadVolMedian,
			"halstead_effort_median": medianFloat(tq.HalsteadEfforts),
			"delivered_bugs_sum":     stats.DeliveredBugsSum,
			"comment_score_min":      stats.CommentScoreMin,
			"doc_coverage_mean":      stats.DocCoverageMean,
			"cohesion_min":           stats.CohesionMin,
			"files_analyzed":         stats.FilesAnalyzed,
		}
	}

	state.commitQuality = make(map[string]*TickQuality)

	return result, nil
}
