package anomaly

import (
	"math"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// --- Data Types ---.

// ZScoreSet holds per-metric Z-scores for a single tick.
type ZScoreSet struct {
	NetChurn          float64 `json:"net_churn"          yaml:"net_churn"`
	FilesChanged      float64 `json:"files_changed"      yaml:"files_changed"`
	LinesAdded        float64 `json:"lines_added"        yaml:"lines_added"`
	LinesRemoved      float64 `json:"lines_removed"      yaml:"lines_removed"`
	LanguageDiversity float64 `json:"language_diversity" yaml:"language_diversity"`
	AuthorCount       float64 `json:"author_count"       yaml:"author_count"`
}

// MaxAbs returns the maximum absolute Z-score across all metrics.
func (z ZScoreSet) MaxAbs() float64 {
	return max(
		math.Abs(z.NetChurn),
		math.Abs(z.FilesChanged),
		math.Abs(z.LinesAdded),
		math.Abs(z.LinesRemoved),
		math.Abs(z.LanguageDiversity),
		math.Abs(z.AuthorCount),
	)
}

// RawMetrics holds the raw metric values for a single tick.
type RawMetrics struct {
	FilesChanged      int `json:"files_changed"      yaml:"files_changed"`
	LinesAdded        int `json:"lines_added"        yaml:"lines_added"`
	LinesRemoved      int `json:"lines_removed"      yaml:"lines_removed"`
	NetChurn          int `json:"net_churn"          yaml:"net_churn"`
	LanguageDiversity int `json:"language_diversity" yaml:"language_diversity"`
	AuthorCount       int `json:"author_count"       yaml:"author_count"`
}

// Record describes a detected anomaly at a specific tick.
type Record struct {
	Tick         int        `json:"tick"            yaml:"tick"`
	ZScores      ZScoreSet  `json:"z_scores"        yaml:"z_scores"`
	MaxAbsZScore float64    `json:"max_abs_z_score" yaml:"max_abs_z_score"`
	Metrics      RawMetrics `json:"metrics"         yaml:"metrics"`
	Files        []string   `json:"files"           yaml:"files"`
}

// AggregateData contains summary statistics for the anomaly analysis.
type AggregateData struct {
	TotalTicks          int     `json:"total_ticks"           yaml:"total_ticks"`
	TotalAnomalies      int     `json:"total_anomalies"       yaml:"total_anomalies"`
	AnomalyRate         float64 `json:"anomaly_rate"          yaml:"anomaly_rate"`
	Threshold           float32 `json:"threshold"             yaml:"threshold"`
	WindowSize          int     `json:"window_size"           yaml:"window_size"`
	ChurnMean           float64 `json:"churn_mean"            yaml:"churn_mean"`
	ChurnStdDev         float64 `json:"churn_stddev"          yaml:"churn_stddev"`
	FilesMean           float64 `json:"files_mean"            yaml:"files_mean"`
	FilesStdDev         float64 `json:"files_stddev"          yaml:"files_stddev"`
	LangDiversityMean   float64 `json:"lang_diversity_mean"   yaml:"lang_diversity_mean"`
	LangDiversityStdDev float64 `json:"lang_diversity_stddev" yaml:"lang_diversity_stddev"`
	AuthorCountMean     float64 `json:"author_count_mean"     yaml:"author_count_mean"`
	AuthorCountStdDev   float64 `json:"author_count_stddev"   yaml:"author_count_stddev"`
}

// TimeSeriesEntry holds per-tick data for the time series output.
type TimeSeriesEntry struct {
	Tick              int        `json:"tick"               yaml:"tick"`
	Metrics           RawMetrics `json:"metrics"            yaml:"metrics"`
	IsAnomaly         bool       `json:"is_anomaly"         yaml:"is_anomaly"`
	ChurnZScore       float64    `json:"churn_z_score"      yaml:"churn_z_score"`
	LanguageDiversity int        `json:"language_diversity" yaml:"language_diversity"`
	AuthorCount       int        `json:"author_count"       yaml:"author_count"`
}

// --- External Anomaly Types ---.

// ExternalAnomaly describes an anomaly detected on an external analyzer's time series dimension.
type ExternalAnomaly struct {
	Source    string  `json:"source"    yaml:"source"`
	Dimension string  `json:"dimension" yaml:"dimension"`
	Tick      int     `json:"tick"      yaml:"tick"`
	ZScore    float64 `json:"z_score"   yaml:"z_score"`
	RawValue  float64 `json:"raw_value" yaml:"raw_value"`
}

// ExternalSummary summarizes anomaly detection results for one external dimension.
type ExternalSummary struct {
	Source    string  `json:"source"    yaml:"source"`
	Dimension string  `json:"dimension" yaml:"dimension"`
	Mean      float64 `json:"mean"      yaml:"mean"`
	StdDev    float64 `json:"stddev"    yaml:"stddev"`
	Anomalies int     `json:"anomalies" yaml:"anomalies"`
	HighestZ  float64 `json:"highest_z" yaml:"highest_z"`
}

// --- Metric Implementations ---.

// computeList extracts the anomaly list.
func computeList(input *ReportData) []Record {
	return input.Anomalies
}

// percentMultiplier converts fractions to percentages.
const percentMultiplier = 100

// computeAggregate calculates aggregate statistics.
func computeAggregate(input *ReportData) AggregateData {
	totalTicks := len(input.TickMetrics)
	totalAnomalies := len(input.Anomalies)

	var anomalyRate float64

	if totalTicks > 0 {
		anomalyRate = float64(totalAnomalies) / float64(totalTicks) * percentMultiplier
	}

	ticks := sortedTickKeys(input.TickMetrics)

	churnValues := make([]float64, len(ticks))
	filesValues := make([]float64, len(ticks))
	langDiversityValues := make([]float64, len(ticks))
	authorCountValues := make([]float64, len(ticks))

	for i, tick := range ticks {
		tm := input.TickMetrics[tick]
		churnValues[i] = float64(tm.NetChurn)
		filesValues[i] = float64(tm.FilesChanged)
		langDiversityValues[i] = float64(len(tm.Languages))
		authorCountValues[i] = float64(len(tm.AuthorIDs))
	}

	churnMean, churnStdDev := MeanStdDev(churnValues)
	filesMean, filesStdDev := MeanStdDev(filesValues)
	langDivMean, langDivStdDev := MeanStdDev(langDiversityValues)
	authorMean, authorStdDev := MeanStdDev(authorCountValues)

	return AggregateData{
		TotalTicks:          totalTicks,
		TotalAnomalies:      totalAnomalies,
		AnomalyRate:         anomalyRate,
		Threshold:           input.Threshold,
		WindowSize:          input.WindowSize,
		ChurnMean:           churnMean,
		ChurnStdDev:         churnStdDev,
		FilesMean:           filesMean,
		FilesStdDev:         filesStdDev,
		LangDiversityMean:   langDivMean,
		LangDiversityStdDev: langDivStdDev,
		AuthorCountMean:     authorMean,
		AuthorCountStdDev:   authorStdDev,
	}
}

// computeTimeSeries builds the annotated time series.
func computeTimeSeries(input *ReportData) []TimeSeriesEntry {
	ticks := sortedTickKeys(input.TickMetrics)

	// Build anomaly set for O(1) lookup.
	anomalySet := make(map[int]float64, len(input.Anomalies))
	for _, a := range input.Anomalies {
		anomalySet[a.Tick] = a.ZScores.NetChurn
	}

	// Build churn Z-scores.
	churnValues := make([]float64, len(ticks))
	for i, tick := range ticks {
		churnValues[i] = float64(input.TickMetrics[tick].NetChurn)
	}

	churnScores := ComputeZScores(churnValues, input.WindowSize)

	entries := make([]TimeSeriesEntry, len(ticks))

	for i, tick := range ticks {
		tm := input.TickMetrics[tick]
		_, isAnomaly := anomalySet[tick]

		churnZ := 0.0
		if i < len(churnScores) {
			churnZ = churnScores[i]
		}

		entries[i] = TimeSeriesEntry{
			Tick: tick,
			Metrics: RawMetrics{
				FilesChanged:      tm.FilesChanged,
				LinesAdded:        tm.LinesAdded,
				LinesRemoved:      tm.LinesRemoved,
				NetChurn:          tm.NetChurn,
				LanguageDiversity: len(tm.Languages),
				AuthorCount:       len(tm.AuthorIDs),
			},
			IsAnomaly:         isAnomaly,
			ChurnZScore:       churnZ,
			LanguageDiversity: len(tm.Languages),
			AuthorCount:       len(tm.AuthorIDs),
		}
	}

	return entries
}

// --- Report Parsing ---.

// ReportData is the parsed input data for anomaly metrics computation.
type ReportData struct {
	Anomalies         []Record
	TickMetrics       map[int]*TickMetrics
	Threshold         float32
	WindowSize        int
	ExternalAnomalies []ExternalAnomaly
	ExternalSummaries []ExternalSummary
}

// AggregateCommitsToTicks builds per-tick metrics from per-commit data grouped
// by the commits_by_tick mapping. This replaces the need for a separate
// per-tick accumulation path during Consume.
func AggregateCommitsToTicks(
	commitMetrics map[string]*CommitAnomalyData,
	commitsByTick map[int][]gitlib.Hash,
) map[int]*TickMetrics {
	if len(commitMetrics) == 0 || len(commitsByTick) == 0 {
		return nil
	}

	result := make(map[int]*TickMetrics, len(commitsByTick))

	for tick, hashes := range commitsByTick {
		tm := aggregateTickFromCommits(hashes, commitMetrics)
		if tm != nil {
			tm.NetChurn = tm.LinesAdded - tm.LinesRemoved
			result[tick] = tm
		}
	}

	return result
}

// aggregateTickFromCommits merges commit-level anomaly data for a single tick.
func aggregateTickFromCommits(hashes []gitlib.Hash, commitMetrics map[string]*CommitAnomalyData) *TickMetrics {
	var tm *TickMetrics

	for _, hash := range hashes {
		cm, ok := commitMetrics[hash.String()]
		if !ok {
			continue
		}

		if tm == nil {
			tm = &TickMetrics{
				Languages: make(map[string]int),
				AuthorIDs: make(map[int]struct{}),
			}
		}

		tm.FilesChanged += cm.FilesChanged
		tm.LinesAdded += cm.LinesAdded
		tm.LinesRemoved += cm.LinesRemoved
		tm.Files = append(tm.Files, cm.Files...)

		for lang, count := range cm.Languages {
			tm.Languages[lang] += count
		}

		tm.AuthorIDs[cm.AuthorID] = struct{}{}
	}

	return tm
}

// ParseReportData extracts ReportData from an analyzer report.
// Expects canonical format: commit_metrics and commits_by_tick.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["anomalies"].([]Record); ok {
		data.Anomalies = v
	}

	// Prefer per-commit aggregation (canonical path).
	commitMetrics, hasCommit := report["commit_metrics"].(map[string]*CommitAnomalyData)
	commitsByTick, hasTicks := report["commits_by_tick"].(map[int][]gitlib.Hash)

	if hasCommit && hasTicks && len(commitMetrics) > 0 {
		data.TickMetrics = AggregateCommitsToTicks(commitMetrics, commitsByTick)
	}

	if data.TickMetrics == nil {
		data.TickMetrics = make(map[int]*TickMetrics)
	}

	if v, ok := report["threshold"].(float32); ok {
		data.Threshold = v
	}

	if v, ok := report["window_size"].(int); ok {
		data.WindowSize = v
	}

	if v, ok := report["external_anomalies"].([]ExternalAnomaly); ok {
		data.ExternalAnomalies = v
	}

	if v, ok := report["external_summaries"].([]ExternalSummary); ok {
		data.ExternalSummaries = v
	}

	return data, nil
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the anomaly analyzer.
type ComputedMetrics struct {
	Anomalies         []Record          `json:"anomalies"                    yaml:"anomalies"`
	TimeSeries        []TimeSeriesEntry `json:"time_series"                  yaml:"time_series"`
	Aggregate         AggregateData     `json:"aggregate"                    yaml:"aggregate"`
	ExternalAnomalies []ExternalAnomaly `json:"external_anomalies,omitempty" yaml:"external_anomalies,omitempty"`
	ExternalSummaries []ExternalSummary `json:"external_summaries,omitempty" yaml:"external_summaries,omitempty"`
}

const analyzerNameAnomaly = "anomaly"

// AnalyzerName returns the name of the analyzer.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameAnomaly
}

// ToJSON returns the metrics in a format suitable for JSON marshaling.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in a format suitable for YAML marshaling.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all anomaly metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	return &ComputedMetrics{
		Anomalies:         computeList(input),
		TimeSeries:        computeTimeSeries(input),
		Aggregate:         computeAggregate(input),
		ExternalAnomalies: input.ExternalAnomalies,
		ExternalSummaries: input.ExternalSummaries,
	}, nil
}

// --- Helpers ---.

func sortedTickKeys(tickMetrics map[int]*TickMetrics) []int {
	ticks := make([]int, 0, len(tickMetrics))

	for tick := range tickMetrics {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	return ticks
}
