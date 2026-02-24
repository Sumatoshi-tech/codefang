package anomaly

import (
	"fmt"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	emptyChartHeight = "400px"
	lineWidth        = 2
	scatterSize      = 12
)

// GenerateSections returns the sections for combined reports.
func (h *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := h.buildChart(report)
	if err != nil {
		return nil, err
	}

	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	statSection := buildStatsSection(input)

	sections := []plotpage.Section{
		{
			Title:    "Net Churn Over Time with Anomalies",
			Subtitle: "Lines added minus lines removed per tick; anomalous ticks highlighted.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Blue line shows net code churn (lines added - lines removed) per time tick",
					"Red scatter points mark ticks flagged as anomalous (Z-score > threshold)",
					"Anomalies indicate sudden deviations from the rolling average",
					"Investigate anomaly ticks for large refactors, bulk imports, or regressions",
					"Adjust --anomaly-threshold to tune sensitivity (lower = more sensitive)",
					"Adjust --anomaly-window to change the rolling baseline period",
				},
			},
		},
		statSection,
	}

	if extSection, ok := buildExternalAnomalySection(input); ok {
		sections = append(sections, extSection)
	}

	return sections, nil
}

// GenerateChart implements PlotGenerator interface.
func (h *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return h.buildChart(report)
}

func (h *Analyzer) buildChart(report analyze.Report) (*charts.Line, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, fmt.Errorf("parse report: %w", err)
	}

	ticks := sortedTickKeys(input.TickMetrics)
	if len(ticks) == 0 {
		return createEmptyChart(), nil
	}

	labels, churnData, anomalyData := buildChartData(ticks, input)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	line := createChurnChart(labels, churnData, anomalyData, co, palette)

	return line, nil
}

func buildChartData(ticks []int, input *ReportData) (labels []string, churnData, anomalyData []opts.LineData) {
	anomalySet := make(map[int]bool, len(input.Anomalies))
	for _, a := range input.Anomalies {
		anomalySet[a.Tick] = true
	}

	labels = make([]string, len(ticks))
	churnData = make([]opts.LineData, len(ticks))
	anomalyData = make([]opts.LineData, len(ticks))

	for i, tick := range ticks {
		labels[i] = strconv.Itoa(tick)
		tm := input.TickMetrics[tick]
		churnData[i] = opts.LineData{Value: tm.NetChurn}

		if anomalySet[tick] {
			anomalyData[i] = opts.LineData{
				Value:  tm.NetChurn,
				Symbol: "circle",
			}
		} else {
			anomalyData[i] = opts.LineData{Value: "-"}
		}
	}

	return labels, churnData, anomalyData
}

func createChurnChart(
	labels []string,
	churnData, anomalyData []opts.LineData,
	co *plotpage.ChartOpts,
	palette plotpage.ChartPalette,
) *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis("Net Churn (lines)")),
		charts.WithGridOpts(co.Grid()),
	)
	line.SetXAxis(labels)
	line.AddSeries("Net Churn", churnData,
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(true)}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}),
		charts.WithLineStyleOpts(opts.LineStyle{Width: lineWidth}),
	)
	line.AddSeries("Anomalies", anomalyData,
		charts.WithLineChartOpts(opts.LineChart{
			Step: "",
		}),
		charts.WithItemStyleOpts(opts.ItemStyle{
			Color: palette.Semantic.Bad,
		}),
		charts.WithLineStyleOpts(opts.LineStyle{Width: 0, Opacity: opts.Float(0)}),
	)

	return line
}

func buildStatsSection(input *ReportData) plotpage.Section {
	aggregate := computeAggregate(input)

	anomalyRateStr := fmt.Sprintf("%.1f%%", aggregate.AnomalyRate)
	totalAnomaliesStr := strconv.Itoa(aggregate.TotalAnomalies)
	totalTicksStr := strconv.Itoa(aggregate.TotalTicks)

	var highestZStr string

	if len(input.Anomalies) > 0 {
		sorted := make([]Record, len(input.Anomalies))
		copy(sorted, input.Anomalies)

		sort.Slice(sorted, func(i, j int) bool {
			return sorted[i].MaxAbsZScore > sorted[j].MaxAbsZScore
		})

		highestZStr = fmt.Sprintf("%.1f", sorted[0].MaxAbsZScore)
	} else {
		highestZStr = "N/A"
	}

	trendColor := plotpage.BadgeSuccess
	if aggregate.AnomalyRate > anomalyRateWarningThreshold {
		trendColor = plotpage.BadgeWarning
	}

	if aggregate.AnomalyRate > anomalyRateErrorThreshold {
		trendColor = plotpage.BadgeError
	}

	avgLangDiversity := fmt.Sprintf("%.1f", aggregate.LangDiversityMean)
	avgAuthorCount := fmt.Sprintf("%.1f", aggregate.AuthorCountMean)

	grid := plotpage.NewGrid(
		maxStatsColumns,
		plotpage.NewStat("Total Ticks", totalTicksStr),
		plotpage.NewStat("Anomalies Detected", totalAnomaliesStr),
		plotpage.NewStat("Anomaly Rate", anomalyRateStr).WithTrend(
			anomalyRateStr, trendColor,
		),
		plotpage.NewStat("Highest Z-Score", highestZStr),
		plotpage.NewStat("Avg Language Diversity", avgLangDiversity),
		plotpage.NewStat("Avg Author Count", avgAuthorCount),
	)

	return plotpage.Section{
		Title:    "Anomaly Detection Summary",
		Subtitle: "Aggregate statistics from temporal anomaly analysis.",
		Chart:    grid,
	}
}

// Threshold constants for anomaly rate badge coloring.
const (
	anomalyRateWarningThreshold = 10.0
	anomalyRateErrorThreshold   = 25.0
	maxStatsColumns             = 4
)

func buildExternalAnomalySection(input *ReportData) (plotpage.Section, bool) {
	if len(input.ExternalSummaries) == 0 {
		return plotpage.Section{}, false
	}

	var stats []plotpage.Renderable

	for _, summary := range input.ExternalSummaries {
		label := summary.Source + " / " + summary.Dimension
		value := strconv.Itoa(summary.Anomalies)

		stat := plotpage.NewStat(label, value)

		if summary.Anomalies > 0 {
			zStr := fmt.Sprintf("peak Z=%.1f", summary.HighestZ)
			stat = stat.WithTrend(zStr, plotpage.BadgeWarning)
		}

		stats = append(stats, stat)
	}

	grid := plotpage.NewGrid(maxStatsColumns, stats...)

	return plotpage.Section{
		Title:    "Cross-Analyzer Anomaly Detection",
		Subtitle: "Anomalies detected on time series from other history analyzers.",
		Chart:    grid,
	}, true
}

func createEmptyChart() *charts.Line {
	co := plotpage.DefaultChartOpts()
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Temporal Anomaly Detection", "No data")),
	)

	return line
}

// RegisterPlotSections registers the anomaly plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/anomaly", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}
