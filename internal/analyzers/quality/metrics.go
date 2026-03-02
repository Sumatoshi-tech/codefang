package quality

import (
	"math"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/stats"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Dimension names used by the quality time series extractor.
const (
	DimComplexityMedian  = "complexity_median"
	DimComplexityP95     = "complexity_p95"
	DimHalsteadVolMedian = "halstead_vol_median"
	DimDeliveredBugsSum  = "delivered_bugs_sum"
	DimCommentScoreMin   = "comment_score_min"
	DimCohesionMin       = "cohesion_min"
	dimensionCount       = 6
)

// RegisterStoreTimeSeriesExtractor registers the quality analyzer's store-based
// time series extractor with the anomaly package for cross-analyzer anomaly detection.
func RegisterStoreTimeSeriesExtractor() {
	anomaly.RegisterStoreTimeSeriesExtractor("quality", extractStoreTimeSeries)
}

func extractStoreTimeSeries(reader analyze.ReportReader) (ticks []int, dimensions map[string][]float64) {
	timeSeries, tsErr := readTimeSeriesIfPresent(reader, reader.Kinds())
	if tsErr != nil || len(timeSeries) == 0 {
		return nil, nil
	}

	ticks = make([]int, len(timeSeries))
	dimensions = makeDimensionSlices(len(timeSeries))

	for i, ts := range timeSeries {
		ticks[i] = ts.Tick
		fillDimensionsFromStats(dimensions, i, ts.Stats)
	}

	return ticks, dimensions
}

func makeDimensionSlices(n int) map[string][]float64 {
	return map[string][]float64{
		DimComplexityMedian:  make([]float64, n),
		DimComplexityP95:     make([]float64, n),
		DimHalsteadVolMedian: make([]float64, n),
		DimDeliveredBugsSum:  make([]float64, n),
		DimCommentScoreMin:   make([]float64, n),
		DimCohesionMin:       make([]float64, n),
	}
}

func fillDimensionsFromStats(dimensions map[string][]float64, i int, ts TickStats) {
	dimensions[DimComplexityMedian][i] = ts.ComplexityMedian
	dimensions[DimComplexityP95][i] = ts.ComplexityP95
	dimensions[DimHalsteadVolMedian][i] = ts.HalsteadVolMedian
	dimensions[DimDeliveredBugsSum][i] = ts.DeliveredBugsSum
	dimensions[DimCommentScoreMin][i] = ts.CommentScoreMin
	dimensions[DimCohesionMin][i] = ts.CohesionMin
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
		ComplexityMean:   stats.Mean(tq.Complexities),
		ComplexityMedian: stats.Median(tq.Complexities),
		ComplexityP95:    stats.Percentile(tq.Complexities, stats.PercentileP95),
		ComplexityMax:    stats.Max(tq.Complexities),

		// Halstead.
		HalsteadVolMean:   stats.Mean(tq.HalsteadVolumes),
		HalsteadVolMedian: stats.Median(tq.HalsteadVolumes),
		HalsteadVolP95:    stats.Percentile(tq.HalsteadVolumes, stats.PercentileP95),
		HalsteadVolSum:    stats.Sum(tq.HalsteadVolumes),

		// Delivered Bugs.
		DeliveredBugsSum: stats.Sum(tq.DeliveredBugs),

		// Comments.
		CommentScoreMean: stats.Mean(tq.CommentScores),
		CommentScoreMin:  stats.Min(tq.CommentScores),
		DocCoverageMean:  stats.Mean(tq.DocCoverages),

		// Cohesion.
		CohesionMean: stats.Mean(tq.CohesionScores),
		CohesionMin:  stats.Min(tq.CohesionScores),

		// Bookkeeping.
		FilesAnalyzed:  n,
		TotalFunctions: stats.Sum(tq.Functions),
		MaxComplexity:  stats.Max(tq.MaxComplexities),
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

	ticks := mapx.SortedKeys(input.TickQuality)
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
		ts := computeTickStats(input.TickQuality[tick])
		timeSeries[i] = TimeSeriesEntry{Tick: tick, Stats: ts}

		complexityMedians[i] = ts.ComplexityMedian
		complexityP95s[i] = ts.ComplexityP95
		halsteadMedians[i] = ts.HalsteadVolMedian
		commentMeans[i] = ts.CommentScoreMean
		cohesionMeans[i] = ts.CohesionMean

		totalFiles += ts.FilesAnalyzed
		totalBugs += ts.DeliveredBugsSum

		if ts.CommentScoreMin < globalMinComment && ts.FilesAnalyzed > 0 {
			globalMinComment = ts.CommentScoreMin
		}

		if ts.CohesionMin < globalMinCohesion && ts.FilesAnalyzed > 0 {
			globalMinCohesion = ts.CohesionMin
		}
	}

	if math.IsInf(globalMinComment, 1) {
		globalMinComment = 0
	}

	if math.IsInf(globalMinCohesion, 1) {
		globalMinCohesion = 0
	}

	complexityMedianMean := stats.Mean(complexityMedians)
	complexityP95Mean := stats.Mean(complexityP95s)
	halsteadMedianMean := stats.Mean(halsteadMedians)
	commentMeanMean := stats.Mean(commentMeans)
	cohesionMeanMean := stats.Mean(cohesionMeans)

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
