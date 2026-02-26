package devs

import (
	"fmt"
	"io"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
)

const (
	maxDevs = 20
)

// GenerateChart creates a stacked bar chart showing developer activity over time.
func GenerateChart(report analyze.Report) (components.Charter, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, fmt.Errorf("devs plot compute metrics: %w", err)
	}

	if len(metrics.Activity) == 0 {
		return createEmptyBar(), nil
	}

	topDevs := getTopDevIDs(metrics.Developers, maxDevs)
	xLabels := make([]string, len(metrics.Activity))

	for i, ad := range metrics.Activity {
		xLabels[i] = strconv.Itoa(ad.Tick)
	}

	nameByID := make(map[int]string)
	for _, dev := range metrics.Developers {
		nameByID[dev.ID] = dev.Name
	}

	series := make([]plotpage.BarSeries, 0, len(topDevs)+1)

	series = append(series, buildTopDevBarSeries(metrics.Activity, topDevs, nameByID)...)

	if len(metrics.Developers) > maxDevs {
		series = append(series, buildOthersBarSeries(metrics.Activity, topDevs))
	}

	bar := plotpage.BuildBarChart(plotpage.DefaultChartOpts(), xLabels, series, "Commits")

	return bar, nil
}

func buildTopDevBarSeries(activity []ActivityData, topDevs []int, nameByID map[int]string) []plotpage.BarSeries {
	series := make([]plotpage.BarSeries, 0, len(topDevs))

	for _, devID := range topDevs {
		data := make([]plotpage.SeriesData, len(activity))
		for i, ad := range activity {
			data[i] = ad.ByDeveloper[devID]
		}

		name := nameByID[devID]
		if name == "" {
			name = fmt.Sprintf("dev_%d", devID)
			if devID == identity.AuthorMissing {
				name = identity.AuthorMissingName
			}
		}

		series = append(series, plotpage.BarSeries{
			Name:  name,
			Data:  data,
			Stack: "total",
		})
	}

	return series
}

func buildOthersBarSeries(activity []ActivityData, topDevs []int) plotpage.BarSeries {
	othersData := make([]plotpage.SeriesData, len(activity))

	// create a fast lookup for topDevs.
	topDevsSet := make(map[int]bool, len(topDevs))
	for _, id := range topDevs {
		topDevsSet[id] = true
	}

	for i, ad := range activity {
		total := 0

		for devID, commits := range ad.ByDeveloper {
			if !topDevsSet[devID] {
				total += commits
			}
		}

		othersData[i] = total
	}

	return plotpage.BarSeries{
		Name:  "Others",
		Data:  othersData,
		Stack: "total",
	}
}

// GenerateChart creates a chart for the history analyzer.
func (a *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return GenerateChart(report)
}

// GenerateSections returns the dashboard sections for combined reports.
func (a *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	return GenerateSections(report)
}

// GenerateDashboardForAnalyzer creates the full dashboard for this analyzer.
func (a *Analyzer) GenerateDashboardForAnalyzer(report analyze.Report, writer io.Writer) error {
	return GenerateDashboard(report, writer)
}

// RegisterDevPlotSections registers the plot section renderer for the devs analyzer.
func RegisterDevPlotSections() {
	analyze.RegisterPlotSections("history/devs", GenerateSections)
}

func createEmptyBar() *charts.Bar {
	bar := plotpage.BuildBarChart(plotpage.DefaultChartOpts(), []string{}, nil, "Commits")
	bar.SetGlobalOptions(
		charts.WithTitleOpts(plotpage.DefaultChartOpts().Title("Developer Activity", "No data")),
	)

	return bar
}
