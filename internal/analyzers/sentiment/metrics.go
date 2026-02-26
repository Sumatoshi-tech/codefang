package sentiment

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// RegisterTimeSeriesExtractor registers the sentiment analyzer's time series
// extractor with the anomaly package for cross-analyzer anomaly detection.
func RegisterTimeSeriesExtractor() {
	anomaly.RegisterTimeSeriesExtractor("sentiment", extractTimeSeries)
}

func extractTimeSeries(report analyze.Report) (ticks []int, dimensions map[string][]float64) {
	data, err := ParseReportData(report)
	if err != nil || len(data.EmotionsByTick) == 0 {
		return nil, nil
	}

	ticks = sortedEmotionTicks(data.EmotionsByTick)
	dimensions = map[string][]float64{
		"sentiment": make([]float64, len(ticks)),
	}

	for i, tick := range ticks {
		dimensions["sentiment"][i] = float64(data.EmotionsByTick[tick])
	}

	return ticks, dimensions
}

func sortedEmotionTicks(m map[int]float32) []int {
	ticks := make([]int, 0, len(m))

	for tick := range m {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	return ticks
}

// AggregateCommitsToTicks groups per-commit comment data into per-tick
// comments and emotions by merging all commits belonging to each tick.
func AggregateCommitsToTicks(
	commentsByCommit map[string][]string,
	commitsByTick map[int][]gitlib.Hash,
) (commentsByTick map[int][]string, emotionsByTick map[int]float32) {
	if len(commentsByCommit) == 0 || len(commitsByTick) == 0 {
		return nil, nil
	}

	cbt := make(map[int][]string, len(commitsByTick))
	ebt := make(map[int]float32, len(commitsByTick))

	for tick, hashes := range commitsByTick {
		for _, hash := range hashes {
			comments, ok := commentsByCommit[hash.String()]
			if !ok {
				continue
			}

			cbt[tick] = append(cbt[tick], comments...)
		}

		ebt[tick] = ComputeSentiment(cbt[tick])
	}

	return cbt, ebt
}

// --- Input Data Types ---.

// ReportData is the parsed input data for sentiment metrics computation.
type ReportData struct {
	EmotionsByTick map[int]float32
	CommentsByTick map[int][]string
	CommitsByTick  map[int][]gitlib.Hash
}

// ParseReportData extracts ReportData from an analyzer report.
// Expects canonical format: comments_by_commit and commits_by_tick.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["commits_by_tick"].(map[int][]gitlib.Hash); ok {
		data.CommitsByTick = v
	}

	commentsByCommit, hasCommit := report["comments_by_commit"].(map[string][]string)

	if hasCommit && len(commentsByCommit) > 0 && len(data.CommitsByTick) > 0 {
		data.CommentsByTick, data.EmotionsByTick = AggregateCommitsToTicks(
			commentsByCommit, data.CommitsByTick,
		)
	}

	if data.EmotionsByTick == nil {
		data.EmotionsByTick = make(map[int]float32)
	}

	if data.CommentsByTick == nil {
		data.CommentsByTick = make(map[int][]string)
	}

	return data, nil
}

// --- Output Data Types ---.

// TimeSeriesData contains sentiment data for a time period.
type TimeSeriesData struct {
	Tick           int     `json:"tick"           yaml:"tick"`
	Sentiment      float32 `json:"sentiment"      yaml:"sentiment"`
	CommentCount   int     `json:"comment_count"  yaml:"comment_count"`
	CommitCount    int     `json:"commit_count"   yaml:"commit_count"`
	Classification string  `json:"classification" yaml:"classification"`
}

// TrendData contains trend information.
type TrendData struct {
	StartTick      int     `json:"start_tick"      yaml:"start_tick"`
	EndTick        int     `json:"end_tick"        yaml:"end_tick"`
	StartSentiment float32 `json:"start_sentiment" yaml:"start_sentiment"`
	EndSentiment   float32 `json:"end_sentiment"   yaml:"end_sentiment"`
	TrendDirection string  `json:"trend_direction" yaml:"trend_direction"`
	ChangePercent  float64 `json:"change_percent"  yaml:"change_percent"`
}

// LowSentimentPeriodData identifies periods with negative sentiment.
type LowSentimentPeriodData struct {
	Tick      int      `json:"tick"       yaml:"tick"`
	Sentiment float32  `json:"sentiment"  yaml:"sentiment"`
	Comments  []string `json:"comments"   yaml:"comments"`
	RiskLevel string   `json:"risk_level" yaml:"risk_level"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalTicks       int     `json:"total_ticks"       yaml:"total_ticks"`
	TotalComments    int     `json:"total_comments"    yaml:"total_comments"`
	TotalCommits     int     `json:"total_commits"     yaml:"total_commits"`
	AverageSentiment float32 `json:"average_sentiment" yaml:"average_sentiment"`
	PositiveTicks    int     `json:"positive_ticks"    yaml:"positive_ticks"`
	NeutralTicks     int     `json:"neutral_ticks"     yaml:"neutral_ticks"`
	NegativeTicks    int     `json:"negative_ticks"    yaml:"negative_ticks"`
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the sentiment analyzer.
type ComputedMetrics struct {
	TimeSeries          []TimeSeriesData         `json:"time_series"           yaml:"time_series"`
	Trend               TrendData                `json:"trend"                 yaml:"trend"`
	LowSentimentPeriods []LowSentimentPeriodData `json:"low_sentiment_periods" yaml:"low_sentiment_periods"`
	Aggregate           AggregateData            `json:"aggregate"             yaml:"aggregate"`
}

const analyzerNameSentiment = "sentiment"

// AnalyzerName returns the name of the analyzer that produced these metrics.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameSentiment
}

// ToJSON returns the metrics in a format suitable for JSON marshaling.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in a format suitable for YAML marshaling.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all sentiment metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	return &ComputedMetrics{
		TimeSeries:          computeTimeSeries(input),
		Trend:               computeTrend(input),
		LowSentimentPeriods: computeLowSentimentPeriods(input),
		Aggregate:           computeAggregate(input),
	}, nil
}

// --- Metric Implementations ---.

// Sentiment thresholds and constants.
const (
	SentimentPositiveThreshold = 0.6
	SentimentNegativeThreshold = 0.4

	// Trend direction thresholds.
	trendThreshold = 0.1

	// Risk thresholds.
	lowSentimentRiskThreshold = 0.2

	// Percent multiplier for calculations.
	percentMultiplier = 100
)

func classifyTrendDirection(startSentiment, endSentiment float32) string {
	switch {
	case endSentiment > startSentiment+trendThreshold:
		return "improving"
	case endSentiment < startSentiment-trendThreshold:
		return "declining"
	default:
		return "stable"
	}
}

func computeTimeSeries(input *ReportData) []TimeSeriesData {
	ticks := make([]int, 0, len(input.EmotionsByTick))
	for tick := range input.EmotionsByTick {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	result := make([]TimeSeriesData, 0, len(ticks))

	for _, tick := range ticks {
		sentiment := input.EmotionsByTick[tick]

		commentCount := 0
		if comments, ok := input.CommentsByTick[tick]; ok {
			commentCount = len(comments)
		}

		commitCount := 0
		if commits, ok := input.CommitsByTick[tick]; ok {
			commitCount = len(commits)
		}

		classification := classifySentiment(sentiment)

		result = append(result, TimeSeriesData{
			Tick:           tick,
			Sentiment:      sentiment,
			CommentCount:   commentCount,
			CommitCount:    commitCount,
			Classification: classification,
		})
	}

	return result
}

func classifySentiment(sentiment float32) string {
	switch {
	case sentiment >= SentimentPositiveThreshold:
		return "positive"
	case sentiment <= SentimentNegativeThreshold:
		return "negative"
	default:
		return "neutral"
	}
}

func computeTrend(input *ReportData) TrendData {
	if len(input.EmotionsByTick) == 0 {
		return TrendData{}
	}

	ticks := make([]int, 0, len(input.EmotionsByTick))

	for tick := range input.EmotionsByTick {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	startTick := ticks[0]
	endTick := ticks[len(ticks)-1]

	regressionStart, regressionEnd := linearRegressionEndpoints(ticks, input.EmotionsByTick)

	changePercent := 0.0

	if regressionStart > 0 {
		changePercent = float64(regressionEnd-regressionStart) / float64(regressionStart) * percentMultiplier
	}

	direction := classifyTrendDirection(regressionStart, regressionEnd)

	return TrendData{
		StartTick:      startTick,
		EndTick:        endTick,
		StartSentiment: regressionStart,
		EndSentiment:   regressionEnd,
		TrendDirection: direction,
		ChangePercent:  changePercent,
	}
}

// linearRegressionEndpoints fits a least-squares line to the sentiment time series
// and returns the fitted values at the first and last tick.
// For a single data point, returns that value for both endpoints.
func linearRegressionEndpoints(ticks []int, emotions map[int]float32) (start, end float32) {
	n := float64(len(ticks))
	if n == 0 {
		return 0, 0
	}

	if n == 1 {
		v := emotions[ticks[0]]

		return v, v
	}

	var sumX, sumY, sumXY, sumX2 float64

	for _, t := range ticks {
		x := float64(t)
		y := float64(emotions[t])
		sumX += x
		sumY += y
		sumXY += x * y
		sumX2 += x * x
	}

	denom := n*sumX2 - sumX*sumX
	if denom == 0 {
		avg := float32(sumY / n)

		return avg, avg
	}

	slope := (n*sumXY - sumX*sumY) / denom
	intercept := (sumY - slope*sumX) / n

	startVal := float32(intercept + slope*float64(ticks[0]))
	endVal := float32(intercept + slope*float64(ticks[len(ticks)-1]))

	return startVal, endVal
}

func computeLowSentimentPeriods(input *ReportData) []LowSentimentPeriodData {
	var result []LowSentimentPeriodData

	for tick, sentiment := range input.EmotionsByTick {
		if sentiment > SentimentNegativeThreshold {
			continue
		}

		var riskLevel string
		if sentiment <= lowSentimentRiskThreshold {
			riskLevel = "HIGH"
		} else {
			riskLevel = "MEDIUM"
		}

		comments := input.CommentsByTick[tick]

		result = append(result, LowSentimentPeriodData{
			Tick:      tick,
			Sentiment: sentiment,
			Comments:  comments,
			RiskLevel: riskLevel,
		})
	}

	// Sort by sentiment ascending (worst first).
	sort.Slice(result, func(i, j int) bool {
		return result[i].Sentiment < result[j].Sentiment
	})

	return result
}

func computeAggregate(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalTicks: len(input.EmotionsByTick),
	}

	if agg.TotalTicks == 0 {
		return agg
	}

	var totalSentiment float32

	for tick, sentiment := range input.EmotionsByTick {
		totalSentiment += sentiment

		switch {
		case sentiment >= SentimentPositiveThreshold:
			agg.PositiveTicks++
		case sentiment <= SentimentNegativeThreshold:
			agg.NegativeTicks++
		default:
			agg.NeutralTicks++
		}

		if comments, ok := input.CommentsByTick[tick]; ok {
			agg.TotalComments += len(comments)
		}

		if commits, ok := input.CommitsByTick[tick]; ok {
			agg.TotalCommits += len(commits)
		}
	}

	agg.AverageSentiment = totalSentiment / float32(agg.TotalTicks)

	return agg
}
