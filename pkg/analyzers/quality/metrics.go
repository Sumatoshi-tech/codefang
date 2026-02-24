package quality

import (
	"math"
	"slices"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// RegisterTimeSeriesExtractor registers the quality analyzer's time series
// extractor with the anomaly package for cross-analyzer anomaly detection.
func RegisterTimeSeriesExtractor() {
	anomaly.RegisterTimeSeriesExtractor("quality", extractTimeSeries)
}

func extractTimeSeries(report analyze.Report) (ticks []int, dimensions map[string][]float64) {
	data, err := ParseReportData(report)
	if err != nil || len(data.TickQuality) == 0 {
		return nil, nil
	}

	ticks = sortedTickKeys(data.TickQuality)
	dimensions = map[string][]float64{
		"complexity_median":   make([]float64, len(ticks)),
		"complexity_p95":      make([]float64, len(ticks)),
		"halstead_vol_median": make([]float64, len(ticks)),
		"delivered_bugs_sum":  make([]float64, len(ticks)),
		"comment_score_min":   make([]float64, len(ticks)),
		"cohesion_min":        make([]float64, len(ticks)),
	}

	for i, tick := range ticks {
		stats := computeTickStats(data.TickQuality[tick])
		dimensions["complexity_median"][i] = stats.ComplexityMedian
		dimensions["complexity_p95"][i] = stats.ComplexityP95
		dimensions["halstead_vol_median"][i] = stats.HalsteadVolMedian
		dimensions["delivered_bugs_sum"][i] = stats.DeliveredBugsSum
		dimensions["comment_score_min"][i] = stats.CommentScoreMin
		dimensions["cohesion_min"][i] = stats.CohesionMin
	}

	return ticks, dimensions
}

// AggregateCommitsToTicks groups per-commit TickQuality data into per-tick
// TickQuality by merging all commits that belong to each tick.
func AggregateCommitsToTicks(
	commitQuality map[string]*TickQuality,
	commitsByTick map[int][]gitlib.Hash,
) map[int]*TickQuality {
	if len(commitQuality) == 0 || len(commitsByTick) == 0 {
		return nil
	}

	result := make(map[int]*TickQuality, len(commitsByTick))

	for tick, hashes := range commitsByTick {
		var merged *TickQuality

		for _, hash := range hashes {
			cq, ok := commitQuality[hash.String()]
			if !ok {
				continue
			}

			if merged == nil {
				merged = &TickQuality{}
			}

			merged.merge(cq)
		}

		if merged != nil {
			result[tick] = merged
		}
	}

	return result
}

// --- Data Types ---.

// TickQuality holds per-file quality metric values for a single tick.
// Values are appended per-file during Consume; statistics are computed at output time.

// filesAnalyzed returns the number of files analyzed in this tick.
func (tq *TickQuality) filesAnalyzed() int {
	return len(tq.Complexities)
}

// TickStats holds computed statistics for a single tick.
type TickStats struct {
	// Complexity.
	ComplexityMean   float64 `json:"complexity_mean"   yaml:"complexity_mean"`
	ComplexityMedian float64 `json:"complexity_median" yaml:"complexity_median"`
	ComplexityP95    float64 `json:"complexity_p95"    yaml:"complexity_p95"`
	ComplexityMax    float64 `json:"complexity_max"    yaml:"complexity_max"`

	// Halstead.
	HalsteadVolMean   float64 `json:"halstead_vol_mean"   yaml:"halstead_vol_mean"`
	HalsteadVolMedian float64 `json:"halstead_vol_median" yaml:"halstead_vol_median"`
	HalsteadVolP95    float64 `json:"halstead_vol_p95"    yaml:"halstead_vol_p95"`
	HalsteadVolSum    float64 `json:"halstead_vol_sum"    yaml:"halstead_vol_sum"`

	// Delivered Bugs.
	DeliveredBugsSum float64 `json:"delivered_bugs_sum" yaml:"delivered_bugs_sum"`

	// Comments.
	CommentScoreMean float64 `json:"comment_score_mean" yaml:"comment_score_mean"`
	CommentScoreMin  float64 `json:"comment_score_min"  yaml:"comment_score_min"`
	DocCoverageMean  float64 `json:"doc_coverage_mean"  yaml:"doc_coverage_mean"`

	// Cohesion.
	CohesionMean float64 `json:"cohesion_mean" yaml:"cohesion_mean"`
	CohesionMin  float64 `json:"cohesion_min"  yaml:"cohesion_min"`

	// Bookkeeping.
	FilesAnalyzed  int `json:"files_analyzed"  yaml:"files_analyzed"`
	TotalFunctions int `json:"total_functions" yaml:"total_functions"`
	MaxComplexity  int `json:"max_complexity"  yaml:"max_complexity"`
}

func computeTickStats(tq *TickQuality) TickStats {
	n := tq.filesAnalyzed()
	if n == 0 {
		return TickStats{}
	}

	return TickStats{
		// Complexity.
		ComplexityMean:   meanFloat(tq.Complexities),
		ComplexityMedian: medianFloat(tq.Complexities),
		ComplexityP95:    p95Float(tq.Complexities),
		ComplexityMax:    maxFloat(tq.Complexities),

		// Halstead.
		HalsteadVolMean:   meanFloat(tq.HalsteadVolumes),
		HalsteadVolMedian: medianFloat(tq.HalsteadVolumes),
		HalsteadVolP95:    p95Float(tq.HalsteadVolumes),
		HalsteadVolSum:    sumFloat(tq.HalsteadVolumes),

		// Delivered Bugs.
		DeliveredBugsSum: sumFloat(tq.DeliveredBugs),

		// Comments.
		CommentScoreMean: meanFloat(tq.CommentScores),
		CommentScoreMin:  minFloat(tq.CommentScores),
		DocCoverageMean:  meanFloat(tq.DocCoverages),

		// Cohesion.
		CohesionMean: meanFloat(tq.CohesionScores),
		CohesionMin:  minFloat(tq.CohesionScores),

		// Bookkeeping.
		FilesAnalyzed:  n,
		TotalFunctions: sumInt(tq.Functions),
		MaxComplexity:  maxInt(tq.MaxComplexities),
	}
}

// TimeSeriesEntry holds per-tick quality data for the time series output.
type TimeSeriesEntry struct {
	Tick  int       `json:"tick"  yaml:"tick"`
	Stats TickStats `json:"stats" yaml:"stats"`
}

// AggregateData contains overall summary statistics.
type AggregateData struct {
	TotalTicks            int     `json:"total_ticks"              yaml:"total_ticks"`
	TotalFilesAnalyzed    int     `json:"total_files_analyzed"     yaml:"total_files_analyzed"`
	ComplexityMedianMean  float64 `json:"complexity_median_mean"   yaml:"complexity_median_mean"`
	ComplexityP95Mean     float64 `json:"complexity_p95_mean"      yaml:"complexity_p95_mean"`
	HalsteadVolMedianMean float64 `json:"halstead_vol_median_mean" yaml:"halstead_vol_median_mean"`
	TotalDeliveredBugs    float64 `json:"total_delivered_bugs"     yaml:"total_delivered_bugs"`
	CommentScoreMeanMean  float64 `json:"comment_score_mean_mean"  yaml:"comment_score_mean_mean"`
	MinCommentScore       float64 `json:"min_comment_score"        yaml:"min_comment_score"`
	CohesionMeanMean      float64 `json:"cohesion_mean_mean"       yaml:"cohesion_mean_mean"`
	MinCohesion           float64 `json:"min_cohesion"             yaml:"min_cohesion"`
}

// --- Report Parsing ---.

// ReportData is the parsed input data for quality metrics computation.
type ReportData struct {
	TickQuality map[int]*TickQuality
}

// ParseReportData extracts ReportData from an analyzer report.
// Expects canonical format: commit_quality and commits_by_tick.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	commitQuality, hasCommit := report["commit_quality"].(map[string]*TickQuality)
	commitsByTick, hasTicks := report["commits_by_tick"].(map[int][]gitlib.Hash)

	if hasCommit && hasTicks && len(commitQuality) > 0 {
		data.TickQuality = AggregateCommitsToTicks(commitQuality, commitsByTick)
	}

	if data.TickQuality == nil {
		data.TickQuality = make(map[int]*TickQuality)
	}

	return data, nil
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the quality analyzer.
type ComputedMetrics struct {
	TimeSeries []TimeSeriesEntry `json:"time_series" yaml:"time_series"`
	Aggregate  AggregateData     `json:"aggregate"   yaml:"aggregate"`
}

// ComputeAllMetrics runs all quality metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	ticks := sortedTickKeys(input.TickQuality)
	timeSeries := make([]TimeSeriesEntry, len(ticks))

	// Collect per-tick stats for aggregate computation.
	complexityMedians := make([]float64, len(ticks))
	complexityP95s := make([]float64, len(ticks))
	halsteadMedians := make([]float64, len(ticks))
	commentMeans := make([]float64, len(ticks))
	cohesionMeans := make([]float64, len(ticks))

	var totalFiles int

	var totalBugs float64

	globalMinComment := math.Inf(1)
	globalMinCohesion := math.Inf(1)

	for i, tick := range ticks {
		stats := computeTickStats(input.TickQuality[tick])
		timeSeries[i] = TimeSeriesEntry{Tick: tick, Stats: stats}

		complexityMedians[i] = stats.ComplexityMedian
		complexityP95s[i] = stats.ComplexityP95
		halsteadMedians[i] = stats.HalsteadVolMedian
		commentMeans[i] = stats.CommentScoreMean
		cohesionMeans[i] = stats.CohesionMean

		totalFiles += stats.FilesAnalyzed
		totalBugs += stats.DeliveredBugsSum

		if stats.CommentScoreMin < globalMinComment && stats.FilesAnalyzed > 0 {
			globalMinComment = stats.CommentScoreMin
		}

		if stats.CohesionMin < globalMinCohesion && stats.FilesAnalyzed > 0 {
			globalMinCohesion = stats.CohesionMin
		}
	}

	if math.IsInf(globalMinComment, 1) {
		globalMinComment = 0
	}

	if math.IsInf(globalMinCohesion, 1) {
		globalMinCohesion = 0
	}

	complexityMedianMean, _ := meanStdDev(complexityMedians)
	complexityP95Mean, _ := meanStdDev(complexityP95s)
	halsteadMedianMean, _ := meanStdDev(halsteadMedians)
	commentMeanMean, _ := meanStdDev(commentMeans)
	cohesionMeanMean, _ := meanStdDev(cohesionMeans)

	return &ComputedMetrics{
		TimeSeries: timeSeries,
		Aggregate: AggregateData{
			TotalTicks:            len(ticks),
			TotalFilesAnalyzed:    totalFiles,
			ComplexityMedianMean:  complexityMedianMean,
			ComplexityP95Mean:     complexityP95Mean,
			HalsteadVolMedianMean: halsteadMedianMean,
			TotalDeliveredBugs:    totalBugs,
			CommentScoreMeanMean:  commentMeanMean,
			MinCommentScore:       globalMinComment,
			CohesionMeanMean:      cohesionMeanMean,
			MinCohesion:           globalMinCohesion,
		},
	}, nil
}

// --- Statistical Helpers ---.

func meanStdDev(values []float64) (mean, stddev float64) {
	n := len(values)
	if n == 0 {
		return 0, 0
	}

	var sum float64

	for _, v := range values {
		sum += v
	}

	mean = sum / float64(n)

	var variance float64

	for _, v := range values {
		d := v - mean
		variance += d * d
	}

	stddev = math.Sqrt(variance / float64(n))

	return mean, stddev
}

func meanFloat(values []float64) float64 {
	if len(values) == 0 {
		return 0
	}

	var sum float64

	for _, v := range values {
		sum += v
	}

	return sum / float64(len(values))
}

// Percentile thresholds.
const (
	percentileMedian = 0.5
	percentileP95    = 0.95
)

func medianFloat(values []float64) float64 {
	return percentileFloat(values, percentileMedian)
}

func p95Float(values []float64) float64 {
	return percentileFloat(values, percentileP95)
}

func percentileFloat(values []float64, p float64) float64 {
	n := len(values)
	if n == 0 {
		return 0
	}

	sorted := make([]float64, n)
	copy(sorted, values)
	slices.Sort(sorted)

	idx := p * float64(n-1)
	lower := int(math.Floor(idx))
	upper := int(math.Ceil(idx))

	if lower == upper || upper >= n {
		return sorted[lower]
	}

	frac := idx - float64(lower)

	return sorted[lower]*(1-frac) + sorted[upper]*frac
}

func maxFloat(values []float64) float64 {
	if len(values) == 0 {
		return 0
	}

	m := values[0]

	for _, v := range values[1:] {
		if v > m {
			m = v
		}
	}

	return m
}

func minFloat(values []float64) float64 {
	if len(values) == 0 {
		return 0
	}

	m := values[0]

	for _, v := range values[1:] {
		if v < m {
			m = v
		}
	}

	return m
}

func sumFloat(values []float64) float64 {
	var s float64

	for _, v := range values {
		s += v
	}

	return s
}

func maxInt(values []int) int {
	if len(values) == 0 {
		return 0
	}

	m := values[0]

	for _, v := range values[1:] {
		if v > m {
			m = v
		}
	}

	return m
}

func sumInt(values []int) int {
	var s int

	for _, v := range values {
		s += v
	}

	return s
}

// sortedTickKeys returns a sorted slice of tick keys from the given map.
func sortedTickKeys(tickQuality map[int]*TickQuality) []int {
	keys := make([]int, 0, len(tickQuality))
	for k := range tickQuality {
		keys = append(keys, k)
	}

	sort.Ints(keys)

	return keys
}
