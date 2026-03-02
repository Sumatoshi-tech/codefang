package burndown

import (
	"fmt"
	"slices"
	"strconv"
	"time"

	"github.com/go-echarts/go-echarts/v2/charts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed burndown data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	chartData, chartErr := readChartDataIfPresent(reader, kinds)
	if chartErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindChartData, chartErr)
	}

	metrics, metricsErr := readMetricsIfPresent(reader, kinds)
	if metricsErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindMetrics, metricsErr)
	}

	return buildStoreSections(chartData, metrics), nil
}

// readChartDataIfPresent reads the chart_data record, returning an empty value if absent.
func readChartDataIfPresent(reader analyze.ReportReader, kinds []string) (*ChartData, error) {
	if !slices.Contains(kinds, KindChartData) {
		return &ChartData{}, nil
	}

	var data ChartData

	iterErr := reader.Iter(KindChartData, func(raw []byte) error {
		return analyze.GobDecode(raw, &data)
	})
	if iterErr != nil {
		return nil, iterErr
	}

	return &data, nil
}

// readMetricsIfPresent reads the metrics record, returning an empty value if absent.
func readMetricsIfPresent(reader analyze.ReportReader, kinds []string) (*ComputedMetrics, error) {
	if !slices.Contains(kinds, KindMetrics) {
		return &ComputedMetrics{}, nil
	}

	var metrics ComputedMetrics

	iterErr := reader.Iter(KindMetrics, func(raw []byte) error {
		return analyze.GobDecode(raw, &metrics)
	})
	if iterErr != nil {
		return nil, iterErr
	}

	return &metrics, nil
}

// buildStoreSections constructs the burndown plot sections from pre-computed data.
func buildStoreSections(chartData *ChartData, metrics *ComputedMetrics) []plotpage.Section {
	var result []plotpage.Section

	// Section 1: Summary stats from pre-computed metrics.
	if metrics != nil {
		result = append(result, buildStoreSummarySection(metrics))
	}

	// Section 2: Burndown chart from pre-computed dense history.
	if chartData != nil && len(chartData.GlobalHistory) > 0 {
		chart := buildChartFromStoreData(chartData)

		result = append(result, plotpage.Section{
			Title:    "Code Burndown Chart",
			Subtitle: "Shows how code written at different times survives over the project's lifetime.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Stacked layers = code written in different time periods",
					"Bottom layers = oldest code still surviving",
					"Narrowing layers = code being deleted or rewritten",
					"Flat layers = stable code that rarely changes",
					"Rapid decrease in recent layers indicates instability",
				},
			},
		})
	}

	return result
}

// buildStoreSummarySection builds the summary section from pre-computed metrics.
func buildStoreSummarySection(metrics *ComputedMetrics) plotpage.Section {
	agg := metrics.Aggregate
	survivalPct := fmt.Sprintf("%.1f%%", agg.OverallSurvivalRate*percentMultiplier)
	survivalColor := survivalBadgeColor(agg.OverallSurvivalRate)

	stats := []plotpage.Renderable{
		plotpage.NewStat("Current Lines", formatInt64(agg.TotalCurrentLines)),
		plotpage.NewStat("Peak Lines", formatInt64(agg.TotalPeakLines)),
		plotpage.NewStat("Survival Rate", survivalPct).WithTrend(survivalPct, survivalColor),
		plotpage.NewStat("Analysis Period", fmt.Sprintf("%d days", agg.AnalysisPeriodDays)),
	}

	if agg.TrackedDevelopers > 0 {
		stats = append(stats, plotpage.NewStat("Developers", strconv.Itoa(agg.TrackedDevelopers)))
	}

	if agg.TrackedFiles > 0 {
		stats = append(stats, plotpage.NewStat("Tracked Files", strconv.Itoa(agg.TrackedFiles)))
	}

	return plotpage.Section{
		Title:    "Burndown Summary",
		Subtitle: "Aggregate statistics from code burndown analysis.",
		Chart:    plotpage.NewGrid(plotMaxStatsColumns, stats...),
	}
}

// buildChartFromStoreData builds the burndown chart directly from store data.
func buildChartFromStoreData(data *ChartData) *charts.Line {
	params := &burndownParams{
		globalHistory: data.GlobalHistory,
		sampling:      data.Sampling,
		granularity:   data.Granularity,
		tickSize:      time.Duration(data.TickSize),
		endTime:       time.Unix(0, data.EndTime),
	}

	co := plotpage.DefaultChartOpts()
	xLabels := buildXLabels(params)
	line := createLineChart(xLabels, params, co)
	addSeries(line, params)

	return line
}
