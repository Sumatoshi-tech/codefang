package devs

import (
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

// radarIndicatorMax is the percentage ceiling for radar indicators.
const radarIndicatorMax = 100

type languagesContent struct {
	chart *charts.Radar
}

func createLanguagesTab(data *DashboardData) *languagesContent {
	return &languagesContent{chart: createLanguagesRadar(data)}
}

// Render renders the languages content to the writer.
func (lc *languagesContent) Render(w io.Writer) error {
	if lc.chart == nil {
		return plotpage.NewText("No language data available").Render(w)
	}

	return plotpage.WrapChart(lc.chart).Render(w)
}

func createLanguagesRadar(data *DashboardData) *charts.Radar {
	if len(data.TopLanguages) == 0 || len(data.Metrics.Developers) == 0 {
		return nil
	}

	indicators := buildRadarIndicators(data)
	radarData := buildRadarData(data)

	co := plotpage.DefaultChartOpts()

	radar := charts.NewRadar()
	radar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", radarHeight)),
		charts.WithTitleOpts(co.Title(
			"Language Expertise",
			"Relative expertise profile per developer (strongest language = 100%)",
		)),
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithRadarComponentOpts(co.RadarComponent(indicators, radarSplitNum)),
	)

	for _, rd := range radarData {
		radar.AddSeries(rd.name, []opts.RadarData{{Value: rd.values}},
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(radarAreaOpacity)}),
			charts.WithLineStyleOpts(opts.LineStyle{Width: lineWidth}),
		)
	}

	return radar
}

func buildRadarIndicators(data *DashboardData) []*opts.Indicator {
	indicators := make([]*opts.Indicator, len(data.TopLanguages))

	for i, lang := range data.TopLanguages {
		indicators[i] = &opts.Indicator{
			Name: lang,
			Max:  radarIndicatorMax,
		}
	}

	return indicators
}

type radarSeriesData struct {
	name   string
	values []float64
}

// topDevsByContribution returns the top N developers sorted by total contribution
// (Added+Removed) across the top languages shown on the radar. This ensures the
// radar shows the biggest code owners, not the most prolific committers (which may
// be bots).
func topDevsByContribution(data *DashboardData, n int) []DeveloperData {
	type devScore struct {
		dev          DeveloperData
		contribution int
	}

	scored := make([]devScore, len(data.Metrics.Developers))

	for i, dev := range data.Metrics.Developers {
		total := 0

		for _, lang := range data.TopLanguages {
			if stats, ok := dev.Languages[lang]; ok {
				total += stats.Added + stats.Removed
			}
		}

		scored[i] = devScore{dev: dev, contribution: total}
	}

	sort.Slice(scored, func(i, j int) bool {
		return scored[i].contribution > scored[j].contribution
	})

	count := min(n, len(scored))
	result := make([]DeveloperData, count)

	for i := range count {
		result[i] = scored[i].dev
	}

	return result
}

// devContribution returns the total contribution (Added+Removed) for a developer
// across the given languages.
func devContribution(dev DeveloperData, langs []string) map[string]int {
	result := make(map[string]int, len(langs))

	for _, lang := range langs {
		if stats, ok := dev.Languages[lang]; ok {
			result[lang] = stats.Added + stats.Removed
		}
	}

	return result
}

// buildRadarData computes per-developer relative expertise profiles.
// Each developer is normalized independently: their strongest language = 100%,
// and all other languages are relative to that. This produces visually distinct
// shapes that show expertise distribution regardless of project scale.
func buildRadarData(data *DashboardData) []radarSeriesData {
	topDevs := topDevsByContribution(data, topDevsForRadar)
	result := make([]radarSeriesData, len(topDevs))

	for i, dev := range topDevs {
		contribs := devContribution(dev, data.TopLanguages)

		// Find this developer's max contribution across top languages.
		maxContrib := 0

		for _, c := range contribs {
			if c > maxContrib {
				maxContrib = c
			}
		}

		values := make([]float64, len(data.TopLanguages))

		if maxContrib > 0 {
			for j, lang := range data.TopLanguages {
				values[j] = float64(contribs[lang]) / float64(maxContrib) * radarIndicatorMax
			}
		}

		result[i] = radarSeriesData{name: dev.Name, values: values}
	}

	return result
}
