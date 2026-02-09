package imports

import (
	"fmt"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
)

const (
	importsPieRadius      = "60%"
	importsCategoryHeight = "420px"
)

func init() { //nolint:gochecknoinits // registration pattern
	analyze.RegisterPlotSections("static/imports", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generateStaticSections(report), nil
	})
}

// FormatReportPlot renders static imports analysis using the same plot framework as other analyzers.
func (a *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	page := plotpage.NewPage(
		"Static Import Analysis",
		"Import usage, categories, and dependency risks in the scanned source tree",
	)

	page.Add(a.generateStaticSections(report)...)

	return page.Render(w)
}

func (a *Analyzer) generateStaticSections(report analyze.Report) []plotpage.Section {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	return []plotpage.Section{
		{
			Title:    "Top Imports Usage",
			Subtitle: "Most frequently used imports across scanned files.",
			Chart:    buildStaticImportsBarChart(report, metrics),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars indicate the most reused imports.",
					"High concentration in few imports can signal architectural coupling.",
					"Review rarely used imports for cleanup opportunities.",
				},
			},
		},
		{
			Title:    "Import Categories",
			Subtitle: "Distribution across stdlib, external, and relative imports.",
			Chart:    buildImportCategoriesPie(metrics),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Higher external share often implies larger supply-chain surface.",
					"Relative imports can indicate local module coupling.",
					"Use category mix to guide dependency governance decisions.",
				},
			},
		},
		{
			Title:    "Dependency Risk Overview",
			Subtitle: "Potentially risky import patterns extracted from static metrics.",
			Chart:    buildDependencyRiskTable(metrics),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"MEDIUM risk often means deeply nested relative imports.",
					"LOW risk often indicates long package paths.",
					"Treat this table as triage input for refactoring.",
				},
			},
		},
	}
}

func buildStaticImportsBarChart(report analyze.Report, metrics *ComputedMetrics) *charts.Bar {
	importCounts := reportutil.GetStringIntMap(report, KeyImportCounts)
	if len(importCounts) == 0 {
		importCounts = make(map[string]int)
		for _, imp := range metrics.ImportList {
			importCounts[imp.Path]++
		}
	}

	if len(importCounts) == 0 {
		return createEmptyImportsChart()
	}

	counts := make(map[string]int64, len(importCounts))
	for name, count := range importCounts {
		counts[name] = int64(count)
	}

	labels, data := topImports(counts, topImportsLimit)
	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createImportsBarChart(labels, data, co, palette)
}

func buildImportCategoriesPie(metrics *ComputedMetrics) *charts.Pie {
	if len(metrics.Categories) == 0 {
		return createEmptyImportCategoriesPie()
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", importsCategoryHeight)),
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithTitleOpts(co.Title("Import Categories", "")),
	)

	data := make([]opts.PieData, 0, len(metrics.Categories))
	colorCount := len(palette.Primary)

	for idx, category := range metrics.Categories {
		color := "#5470c6"
		if colorCount > 0 {
			color = palette.Primary[idx%colorCount]
		}

		data = append(data, opts.PieData{
			Name:  category.Category,
			Value: category.Count,
			ItemStyle: &opts.ItemStyle{
				Color: color,
			},
		})
	}

	pie.AddSeries("Categories", data).
		SetSeriesOptions(
			charts.WithPieChartOpts(opts.PieChart{Radius: importsPieRadius}),
			charts.WithLabelOpts(opts.Label{
				Show:      opts.Bool(true),
				Formatter: "{b}: {c} ({d}%)",
				Color:     co.TextColor(),
			}),
		)

	return pie
}

func createEmptyImportCategoriesPie() *charts.Pie {
	co := plotpage.DefaultChartOpts()
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", importsCategoryHeight)),
		charts.WithTitleOpts(co.Title("Import Categories", "No data")),
	)

	return pie
}

const maxDependencyRiskRows = 30

func buildDependencyRiskTable(metrics *ComputedMetrics) *plotpage.Table {
	table := plotpage.NewTable([]string{"Import", "Risk", "Reason"})

	if len(metrics.Dependencies) == 0 {
		table.AddRow("No dependency risks detected", "INFO", "-")

		return table
	}

	deps := make([]ImportDependencyData, len(metrics.Dependencies))
	copy(deps, metrics.Dependencies)
	sort.Slice(deps, func(i, j int) bool {
		if deps[i].RiskLevel != deps[j].RiskLevel {
			return deps[i].RiskLevel > deps[j].RiskLevel
		}

		return deps[i].Path < deps[j].Path
	})

	limit := len(deps)
	if limit > maxDependencyRiskRows {
		limit = maxDependencyRiskRows
	}

	for _, dep := range deps[:limit] {
		table.AddRow(dep.Path, dep.RiskLevel, dep.Reason)
	}

	if len(deps) > maxDependencyRiskRows {
		table.AddRow(
			fmt.Sprintf("... and %d more", len(deps)-maxDependencyRiskRows),
			"INFO",
			fmt.Sprintf("Showing top %d of %d total risks", maxDependencyRiskRows, len(deps)),
		)
	}

	return table
}
