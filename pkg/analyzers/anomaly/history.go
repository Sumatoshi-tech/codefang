package anomaly

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"sort"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
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

// HistoryAnalyzer detects temporal anomalies in commit history using Z-score
// analysis over a sliding window of per-tick metrics.
type HistoryAnalyzer struct {
	TreeDiff  *plumbing.TreeDiffAnalyzer
	Ticks     *plumbing.TicksSinceStart
	LineStats *plumbing.LinesStatsCalculator
	Languages *plumbing.LanguagesDetectionAnalyzer
	Identity  *plumbing.IdentityDetector

	// Configuration (read-only after Configure).
	Threshold  float32
	WindowSize int

	// Mutable per-commit state accumulated during Consume.
	commitMetrics map[string]*CommitAnomalyData // per-commit metrics keyed by hash hex.
	commitsByTick map[int][]gitlib.Hash
}

// Name returns the analyzer name.
func (h *HistoryAnalyzer) Name() string {
	return "TemporalAnomaly"
}

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string {
	return "anomaly"
}

// Descriptor returns stable analyzer metadata.
func (h *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          "history/anomaly",
		Description: "Detects sudden quality degradation in commit history using Z-score anomaly detection.",
		Mode:        analyze.ModeHistory,
	}
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
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
	}
}

// Configure applies configuration from the provided facts map.
func (h *HistoryAnalyzer) Configure(facts map[string]any) error {
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

func (h *HistoryAnalyzer) validate() {
	if h.Threshold < MinThreshold {
		h.Threshold = DefaultAnomalyThreshold
	}

	if h.WindowSize < MinWindowSize {
		h.WindowSize = DefaultAnomalyWindowSize
	}
}

// Initialize prepares the analyzer for processing commits.
func (h *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	h.commitMetrics = make(map[string]*CommitAnomalyData)
	h.validate()

	return nil
}

// Consume processes a single commit, accumulating per-commit metrics from
// plumbing analyzers.
func (h *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) error {
	changes := h.TreeDiff.Changes

	if ac == nil || ac.Commit == nil {
		return nil
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

	h.commitMetrics[ac.Commit.Hash().String()] = cm

	return nil
}

func (h *HistoryAnalyzer) accumulateLineStats(cm *CommitAnomalyData) {
	if h.LineStats == nil || h.LineStats.LineStats == nil {
		return
	}

	for _, stats := range h.LineStats.LineStats {
		cm.LinesAdded += stats.Added
		cm.LinesRemoved += stats.Removed
	}

	cm.NetChurn = cm.LinesAdded - cm.LinesRemoved
}

func (h *HistoryAnalyzer) accumulateLanguagesAndAuthors(cm *CommitAnomalyData) {
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

// Finalize computes anomaly detection and returns the analysis report.
func (h *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	tickMetrics := AggregateCommitsToTicks(h.commitMetrics, h.commitsByTick)
	if tickMetrics == nil {
		tickMetrics = make(map[int]*TickMetrics)
	}

	ticks := sortedTickKeys(tickMetrics)

	if len(ticks) == 0 {
		return analyze.Report{
			"anomalies":       []Record{},
			"commit_metrics":  h.commitMetrics,
			"commits_by_tick": h.commitsByTick,
			"threshold":       h.Threshold,
			"window_size":     h.WindowSize,
		}, nil
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

	churnScores := ComputeZScores(churnValues, h.WindowSize)
	filesScores := ComputeZScores(filesValues, h.WindowSize)
	addedScores := ComputeZScores(addedValues, h.WindowSize)
	removedScores := ComputeZScores(removedValues, h.WindowSize)
	langDiversityScores := ComputeZScores(langDiversityValues, h.WindowSize)
	authorCountScores := ComputeZScores(authorCountValues, h.WindowSize)

	threshold := float64(h.Threshold)
	anomalies := buildRecords(
		ticks, tickMetrics, churnScores, filesScores, addedScores, removedScores,
		langDiversityScores, authorCountScores, threshold,
	)

	// Sort anomalies by severity (highest absolute Z-score first).
	sort.Slice(anomalies, func(i, j int) bool {
		return anomalies[i].MaxAbsZScore > anomalies[j].MaxAbsZScore
	})

	return analyze.Report{
		"anomalies":       anomalies,
		"commit_metrics":  h.commitMetrics,
		"commits_by_tick": h.commitsByTick,
		"threshold":       h.Threshold,
		"window_size":     h.WindowSize,
	}, nil
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

// Fork creates independent copies of the analyzer for parallel processing.
func (h *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &HistoryAnalyzer{
			TreeDiff:      &plumbing.TreeDiffAnalyzer{},
			Ticks:         &plumbing.TicksSinceStart{},
			LineStats:     &plumbing.LinesStatsCalculator{},
			Languages:     &plumbing.LanguagesDetectionAnalyzer{},
			Identity:      &plumbing.IdentityDetector{},
			Threshold:     h.Threshold,
			WindowSize:    h.WindowSize,
			commitMetrics: make(map[string]*CommitAnomalyData),
			commitsByTick: h.commitsByTick, // shared read-only.
		}
		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (h *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		// Per-commit data: each fork processes distinct commits.
		maps.Copy(h.commitMetrics, other.commitMetrics)
	}
}

// SequentialOnly returns false because the anomaly analyzer can be parallelized.
func (h *HistoryAnalyzer) SequentialOnly() bool { return false }

// CPUHeavy returns false because the anomaly analyzer does not perform
// expensive UAST processing per commit.
func (h *HistoryAnalyzer) CPUHeavy() bool { return false }

// SnapshotPlumbing captures the current plumbing output state.
func (h *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
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
func (h *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
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
func (h *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Serialize writes the analysis result in the specified format.
func (h *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return h.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return h.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return h.generatePlot(result, writer)
	case analyze.FormatBinary:
		return h.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (h *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(computed)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (h *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(computed)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("yaml write: %w", err)
	}

	return nil
}

func (h *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(computed, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report in YAML.
func (h *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, analyze.FormatYAML, writer)
}
