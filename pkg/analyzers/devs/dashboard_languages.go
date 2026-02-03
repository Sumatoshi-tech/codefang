package devs

import (
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

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
		charts.WithTitleOpts(co.Title("Language Expertise", "Radar chart showing developer expertise across languages")),
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
	maxValues := computeLanguageMaxValues(data)

	for i, lang := range data.TopLanguages {
		indicators[i] = &opts.Indicator{
			Name: lang,
			Max:  float32(maxValues[lang]),
		}
	}

	return indicators
}

func computeLanguageMaxValues(data *DashboardData) map[string]int {
	maxValues := make(map[string]int)

	for _, dev := range data.Metrics.Developers {
		for lang, stats := range dev.Languages {
			if stats.Added > maxValues[lang] {
				maxValues[lang] = stats.Added
			}
		}
	}

	return maxValues
}

type radarSeriesData struct {
	name   string
	values []float64
}

func buildRadarData(data *DashboardData) []radarSeriesData {
	count := min(topDevsForRadar, len(data.Metrics.Developers))
	result := make([]radarSeriesData, count)

	for i := range count {
		dev := data.Metrics.Developers[i]
		values := make([]float64, len(data.TopLanguages))

		for j, lang := range data.TopLanguages {
			if stats, ok := dev.Languages[lang]; ok {
				values[j] = float64(stats.Added)
			}
		}

		result[i] = radarSeriesData{name: dev.Name, values: values}
	}

	return result
}
