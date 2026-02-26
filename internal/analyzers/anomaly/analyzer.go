package anomaly

import (
	"context"
	"maps"
	"sort"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// Configuration keys.
const (
	ConfigAnomalyThreshold  = "TemporalAnomaly.Threshold"
	ConfigAnomalyWindowSize = "TemporalAnomaly.WindowSize"
)

// Default configuration values.
const (
	DefaultAnomalyThreshold  = float32(2.0)
	DefaultAnomalyWindowSize = 20

	// MinWindowSize is the minimum valid sliding window size.
	MinWindowSize = 2
	// MinThreshold is the minimum valid Z-score threshold.
	MinThreshold = float32(0.1)
)

// TickMetrics holds the raw metrics collected for a single tick.
type TickMetrics struct {
	FilesChanged int
	LinesAdded   int
	LinesRemoved int
	NetChurn     int
	Files        []string
	Languages    map[string]int   // language name → file count for this tick.
	AuthorIDs    map[int]struct{} // unique author IDs seen in this tick.
}

// CommitAnomalyData holds raw metrics for a single commit.
type CommitAnomalyData struct {
	FilesChanged int            `json:"files_changed"`
	LinesAdded   int            `json:"lines_added"`
	LinesRemoved int            `json:"lines_removed"`
	NetChurn     int            `json:"net_churn"`
	Files        []string       `json:"files,omitempty"`
	Languages    map[string]int `json:"languages,omitempty"`
	AuthorID     int            `json:"author_id"`
}

// Analyzer detects temporal anomalies in commit history using Z-score
// analysis over a sliding window of per-tick metrics.
// Per-commit results are emitted as TCs; accumulated state lives in
// the Aggregator, not in the analyzer.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	TreeDiff  *plumbing.TreeDiffAnalyzer
	Ticks     *plumbing.TicksSinceStart
	LineStats *plumbing.LinesStatsCalculator
	Languages *plumbing.LanguagesDetectionAnalyzer
	Identity  *plumbing.IdentityDetector

	// Configuration (read-only after Configure).
	Threshold  float32
	WindowSize int

	commitsByTick map[int][]gitlib.Hash
}

// NewAnalyzer creates a new anomaly analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{}
	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/anomaly",
			Description: "Detects sudden quality degradation in commit history using Z-score anomaly detection.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ConfigOptions: []pipeline.ConfigurationOption{
			{
				Name:        ConfigAnomalyThreshold,
				Description: "Z-score threshold for anomaly detection (standard deviations).",
				Flag:        "anomaly-threshold",
				Type:        pipeline.FloatConfigurationOption,
				Default:     DefaultAnomalyThreshold,
			},
			{
				Name:        ConfigAnomalyWindowSize,
				Description: "Sliding window size in ticks for computing rolling statistics.",
				Flag:        "anomaly-window",
				Type:        pipeline.IntConfigurationOption,
				Default:     DefaultAnomalyWindowSize,
			},
		},
		ComputeMetricsFn: computeMetricsSafe,
		AggregatorFn: func(opts analyze.AggregatorOptions) analyze.Aggregator {
			return newAggregator(opts, a.Threshold, a.WindowSize)
		},
	}

	a.TicksToReportFn = func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
		return ticksToReport(ctx, ticks, a.Threshold, a.WindowSize, a.commitsByTick)
	}

	return a
}

// Name returns the analyzer name.
func (h *Analyzer) Name() string {
	return "TemporalAnomaly"
}

// CPUHeavy returns false because the anomaly analyzer does not perform
// expensive UAST processing per commit.
func (h *Analyzer) CPUHeavy() bool { return false }

func computeMetricsSafe(report analyze.Report) (*ComputedMetrics, error) {
	if len(report) == 0 {
		return &ComputedMetrics{}, nil
	}

	return ComputeAllMetrics(report)
}

// Configure applies configuration from the provided facts map.
func (h *Analyzer) Configure(facts map[string]any) error {
	if val, ok := facts[ConfigAnomalyThreshold].(float32); ok {
		h.Threshold = val
	}

	if val, ok := facts[ConfigAnomalyWindowSize].(int); ok {
		h.WindowSize = val
	}

	if val, ok := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); ok {
		h.commitsByTick = val
	}

	h.validate()

	return nil
}

func (h *Analyzer) validate() {
	if h.Threshold < MinThreshold {
		h.Threshold = DefaultAnomalyThreshold
	}

	if h.WindowSize < MinWindowSize {
		h.WindowSize = DefaultAnomalyWindowSize
	}
}

// Initialize prepares the analyzer for processing commits.
func (h *Analyzer) Initialize(_ *gitlib.Repository) error {
	h.validate()

	return nil
}

// Consume processes a single commit and returns a TC with per-commit metrics.
// The analyzer does not retain any per-commit state; all output is in the TC.
func (h *Analyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	changes := h.TreeDiff.Changes

	if ac == nil || ac.Commit == nil {
		return analyze.TC{}, nil
	}

	cm := &CommitAnomalyData{
		FilesChanged: len(changes),
		Languages:    make(map[string]int),
	}

	for _, change := range changes {
		cm.Files = append(cm.Files, change.To.Name)
	}

	h.accumulateLineStats(cm)
	h.accumulateLanguagesAndAuthors(cm)

	cm.NetChurn = cm.LinesAdded - cm.LinesRemoved

	return analyze.TC{
		Data:       cm,
		CommitHash: ac.Commit.Hash(),
	}, nil
}

func (h *Analyzer) accumulateLineStats(cm *CommitAnomalyData) {
	if h.LineStats == nil || h.LineStats.LineStats == nil {
		return
	}

	for _, stats := range h.LineStats.LineStats {
		cm.LinesAdded += stats.Added
		cm.LinesRemoved += stats.Removed
	}

	cm.NetChurn = cm.LinesAdded - cm.LinesRemoved
}

func (h *Analyzer) accumulateLanguagesAndAuthors(cm *CommitAnomalyData) {
	if h.Languages != nil {
		for _, lang := range h.Languages.Languages() {
			if lang != "" {
				cm.Languages[lang]++
			}
		}
	}

	if h.Identity != nil {
		cm.AuthorID = h.Identity.AuthorID
	}
}

func buildRecords(
	ticks []int,
	tickMetrics map[int]*TickMetrics,
	churnScores, filesScores, addedScores, removedScores,
	langDiversityScores, authorCountScores []float64,
	threshold float64,
) []Record {
	var anomalies []Record

	for i, tick := range ticks {
		scores := ZScoreSet{
			NetChurn:          churnScores[i],
			FilesChanged:      filesScores[i],
			LinesAdded:        addedScores[i],
			LinesRemoved:      removedScores[i],
			LanguageDiversity: langDiversityScores[i],
			AuthorCount:       authorCountScores[i],
		}

		maxAbs := scores.MaxAbs()
		if maxAbs <= threshold {
			continue
		}

		tm := tickMetrics[tick]

		anomalies = append(anomalies, Record{
			Tick:         tick,
			ZScores:      scores,
			MaxAbsZScore: maxAbs,
			Metrics: RawMetrics{
				FilesChanged:      tm.FilesChanged,
				LinesAdded:        tm.LinesAdded,
				LinesRemoved:      tm.LinesRemoved,
				NetChurn:          tm.NetChurn,
				LanguageDiversity: len(tm.Languages),
				AuthorCount:       len(tm.AuthorIDs),
			},
			Files: tm.Files,
		})
	}

	return anomalies
}

// SnapshotPlumbing captures the current plumbing output state.
func (h *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	snap := plumbing.Snapshot{
		Changes:   h.TreeDiff.Changes,
		Tick:      h.Ticks.Tick,
		LineStats: h.LineStats.LineStats,
	}

	if h.Languages != nil {
		snap.Languages = h.Languages.Languages()
	}

	if h.Identity != nil {
		snap.AuthorID = h.Identity.AuthorID
	}

	return snap
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.TreeDiff.Changes = ss.Changes
	h.Ticks.Tick = ss.Tick
	h.LineStats.LineStats = ss.LineStats

	if h.Languages != nil {
		h.Languages.SetLanguages(ss.Languages)
	}

	if h.Identity != nil {
		h.Identity.AuthorID = ss.AuthorID
	}
}

// ReleaseSnapshot releases resources owned by the snapshot.
// The anomaly analyzer does not hold UAST trees, so this is a no-op.
func (h *Analyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// ExtractCommitTimeSeries extracts per-commit anomaly metrics from a finalized report.
// Implements [analyze.CommitTimeSeriesProvider].
func (h *Analyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitMetrics, ok := report["commit_metrics"].(map[string]*CommitAnomalyData)
	if !ok || len(commitMetrics) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitMetrics))

	for hash, cm := range commitMetrics {
		result[hash] = cm
	}

	return result
}

// NewAggregator creates an anomaly Aggregator configured with the given options.
func (h *Analyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return h.AggregatorFn(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (h *Analyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return h.TicksToReportFn(ctx, ticks), nil
}

// --- Generic Aggregator Delegates ---.

type tickAccumulator struct {
	commitMetrics map[string]*CommitAnomalyData
	startTime     time.Time
	endTime       time.Time
}

// TickData is the per-tick aggregated payload for the anomaly analyzer.
// It holds per-commit metrics for the canonical report format.
type TickData struct {
	// CommitMetrics maps commit hash (hex) to per-commit CommitAnomalyData.
	CommitMetrics map[string]*CommitAnomalyData
}

func newAggregator(opts analyze.AggregatorOptions, _ float32, _ int) analyze.Aggregator {
	return analyze.NewGenericAggregator[*tickAccumulator, *TickData](
		opts,
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
}

func extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	cm, isCM := tc.Data.(*CommitAnomalyData)
	if !isCM || cm == nil || tc.CommitHash.IsZero() {
		return nil
	}

	acc, ok := byTick[tc.Tick]
	if !ok {
		acc = &tickAccumulator{
			commitMetrics: make(map[string]*CommitAnomalyData),
			startTime:     tc.Timestamp,
			endTime:       tc.Timestamp,
		}
		byTick[tc.Tick] = acc
	}

	acc.commitMetrics[tc.CommitHash.String()] = cm
	updateTimeRange(acc, tc.Timestamp)

	return nil
}

func updateTimeRange(acc *tickAccumulator, ts time.Time) {
	if ts.IsZero() {
		return
	}

	if ts.Before(acc.startTime) || acc.startTime.IsZero() {
		acc.startTime = ts
	}

	if ts.After(acc.endTime) {
		acc.endTime = ts
	}
}

func mergeState(existing, incoming *tickAccumulator) *tickAccumulator {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	if incoming.commitMetrics != nil {
		if existing.commitMetrics == nil {
			existing.commitMetrics = make(map[string]*CommitAnomalyData)
		}

		maps.Copy(existing.commitMetrics, incoming.commitMetrics)
	}

	if !incoming.startTime.IsZero() && (incoming.startTime.Before(existing.startTime) || existing.startTime.IsZero()) {
		existing.startTime = incoming.startTime
	}

	if !incoming.endTime.IsZero() && incoming.endTime.After(existing.endTime) {
		existing.endTime = incoming.endTime
	}

	return existing
}

func sizeState(state *tickAccumulator) int64 {
	if state == nil || state.commitMetrics == nil {
		return 0
	}

	const (
		stateOverhead  = 128
		hashEntryBytes = 50
		bytesPerFile   = 64
		bytesPerLang   = 32
	)

	var size int64

	size += stateOverhead

	for _, cm := range state.commitMetrics {
		if cm == nil {
			continue
		}

		size += hashEntryBytes
		size += int64(len(cm.Files)) * bytesPerFile
		size += int64(len(cm.Languages)) * bytesPerLang
	}

	return size
}

func buildTick(tick int, state *tickAccumulator) (analyze.TICK, error) {
	if state == nil || state.commitMetrics == nil {
		return analyze.TICK{Tick: tick, Data: &TickData{CommitMetrics: make(map[string]*CommitAnomalyData)}}, nil
	}

	return analyze.TICK{
		Tick:      tick,
		StartTime: state.startTime,
		EndTime:   state.endTime,
		Data:      &TickData{CommitMetrics: state.commitMetrics},
	}, nil
}

func ticksToReport(
	_ context.Context,
	ticks []analyze.TICK,
	threshold float32,
	window int,
	commitsByTick map[int][]gitlib.Hash,
) analyze.Report {
	commitMetrics := buildCommitMetricsFromTicks(ticks)
	ct := commitsByTick

	if ct == nil {
		ct = buildCommitsByTickFromTicks(ticks)
	}

	tickMetrics := AggregateCommitsToTicks(commitMetrics, ct)
	anomalies := detectAnomaliesFromTicks(tickMetrics, threshold, window)

	return analyze.Report{
		"commit_metrics":  commitMetrics,
		"commits_by_tick": ct,
		"anomalies":       anomalies,
		"threshold":       threshold,
		"window_size":     window,
	}
}

func buildCommitMetricsFromTicks(ticks []analyze.TICK) map[string]*CommitAnomalyData {
	commitMetrics := make(map[string]*CommitAnomalyData)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommitMetrics == nil {
			continue
		}

		maps.Copy(commitMetrics, td.CommitMetrics)
	}

	return commitMetrics
}

func buildCommitsByTickFromTicks(ticks []analyze.TICK) map[int][]gitlib.Hash {
	ct := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommitMetrics == nil {
			continue
		}

		hashes := make([]gitlib.Hash, 0, len(td.CommitMetrics))

		for h := range td.CommitMetrics {
			hashes = append(hashes, gitlib.NewHash(h))
		}

		ct[tick.Tick] = append(ct[tick.Tick], hashes...)
	}

	return ct
}

func detectAnomaliesFromTicks(
	tickMetrics map[int]*TickMetrics,
	threshold float32,
	window int,
) []Record {
	ticks := sortedTickKeys(tickMetrics)

	if len(ticks) == 0 {
		return []Record{}
	}

	churnValues := make([]float64, len(ticks))
	filesValues := make([]float64, len(ticks))
	addedValues := make([]float64, len(ticks))
	removedValues := make([]float64, len(ticks))
	langDiversityValues := make([]float64, len(ticks))
	authorCountValues := make([]float64, len(ticks))

	for i, tick := range ticks {
		tm := tickMetrics[tick]
		churnValues[i] = float64(tm.NetChurn)
		filesValues[i] = float64(tm.FilesChanged)
		addedValues[i] = float64(tm.LinesAdded)
		removedValues[i] = float64(tm.LinesRemoved)
		langDiversityValues[i] = float64(len(tm.Languages))
		authorCountValues[i] = float64(len(tm.AuthorIDs))
	}

	churnScores := ComputeZScores(churnValues, window)
	filesScores := ComputeZScores(filesValues, window)
	addedScores := ComputeZScores(addedValues, window)
	removedScores := ComputeZScores(removedValues, window)
	langDiversityScores := ComputeZScores(langDiversityValues, window)
	authorCountScores := ComputeZScores(authorCountValues, window)

	thresholdF := float64(threshold)
	anomalies := buildRecords(
		ticks, tickMetrics, churnScores, filesScores, addedScores, removedScores,
		langDiversityScores, authorCountScores, thresholdF,
	)

	sort.Slice(anomalies, func(i, j int) bool {
		return anomalies[i].MaxAbsZScore > anomalies[j].MaxAbsZScore
	})

	return anomalies
}

// Fork creates independent copies of the analyzer for parallel processing.
func (h *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &Analyzer{
			BaseHistoryAnalyzer: h.BaseHistoryAnalyzer,
			TreeDiff:            &plumbing.TreeDiffAnalyzer{},
			Ticks:               &plumbing.TicksSinceStart{},
			LineStats:           &plumbing.LinesStatsCalculator{},
			Languages:           &plumbing.LanguagesDetectionAnalyzer{},
			Identity:            &plumbing.IdentityDetector{},
			Threshold:           h.Threshold,
			WindowSize:          h.WindowSize,
			commitsByTick:       h.commitsByTick, // shared read-only.
		}
		res[i] = clone
	}

	return res
}

// Merge is a no-op. Per-commit results are emitted as TCs and
// collected by the framework, not accumulated inside the analyzer.
func (h *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {}
