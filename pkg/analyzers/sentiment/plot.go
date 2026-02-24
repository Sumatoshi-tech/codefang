package sentiment

import (
	"fmt"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	areaOpacity           = 0.3
	commentAxisIndex      = 1
	commentBarOpacity     = 0.4
	smoothCurve           = true
	positiveZoneLabel     = "Positive Zone"
	negativeZoneLabel     = "Negative Zone"
	sentimentSeriesLabel  = "Sentiment"
	commentCountLabel     = "Comments"
	trendLineLabel        = "Trend"
	sentimentAxisLabel    = "Sentiment Score"
	commentCountAxisLabel = "Comment Count"
	chartSectionTitle     = "Sentiment Analysis Over Time"
	chartSectionSubtitle  = "Sentiment score and comment volume per time interval. Green zone = positive, red zone = negative."
	distributionTitle     = "Sentiment Distribution"
	distributionSubtitle  = "Breakdown of positive, neutral, and negative time periods."
	sentimentLineWidth    = 2
	zoneLineWidth         = 1
	zoneOpacity           = 0.08
	distributionInner     = "40%"
	distributionOuter     = "70%"
	pieChartHeight        = "400px"
	initialSectionCap     = 2
)

// RegisterPlotSections registers the sentiment plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/sentiment", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}

// GenerateSections returns the sections for combined reports.
func (s *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, err
	}

	sections := make([]plotpage.Section, 0, initialSectionCap)

	mainChart := buildSentimentChart(metrics)

	sections = append(sections, plotpage.Section{
		Title:    chartSectionTitle,
		Subtitle: chartSectionSubtitle,
		Chart:    plotpage.WrapChart(mainChart),
		Hint:     buildMainChartHint(metrics),
	})

	if len(metrics.TimeSeries) > 0 {
		pieChart := buildDistributionChart(metrics)
		sections = append(sections, plotpage.Section{
			Title:    distributionTitle,
			Subtitle: distributionSubtitle,
			Chart:    plotpage.WrapChart(pieChart),
		})
	}

	return sections, nil
}

// GenerateChart implements PlotGenerator interface.
func (s *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, err
	}

	return buildSentimentChart(metrics), nil
}

func buildSentimentChart(metrics *ComputedMetrics) *charts.Line {
	if len(metrics.TimeSeries) == 0 {
		return plotpage.BuildLineChart(nil, nil, nil, sentimentAxisLabel)
	}

	cOpts := plotpage.DefaultChartOpts()

	line := initSentimentLine(cOpts)
	labels, seriesData := prepareChartData(metrics)

	line.SetXAxis(labels)
	addChartSeries(line, metrics, seriesData)

	return line
}

type chartSeriesData struct {
	sentiment    []opts.LineData
	comments     []opts.LineData
	positiveZone []opts.LineData
	negativeZone []opts.LineData
	trend        []opts.LineData
}

func initSentimentLine(cOpts *plotpage.ChartOpts) *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(cOpts.Init("100%", "500px")),
		charts.WithTooltipOpts(cOpts.Tooltip("axis")),
		charts.WithDataZoomOpts(cOpts.DataZoom()...),
		charts.WithLegendOpts(cOpts.Legend()),
		charts.WithXAxisOpts(cOpts.XAxis("")),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      sentimentAxisLabel,
			Min:       0,
			Max:       1,
			AxisLabel: &opts.AxisLabel{Color: cOpts.TextMutedColor()},
			AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: cOpts.AxisColor()}},
			SplitLine: &opts.SplitLine{
				Show:      opts.Bool(true),
				LineStyle: &opts.LineStyle{Color: cOpts.GridColor()},
			},
		}),
		charts.WithGridOpts(cOpts.Grid()),
	)

	line.ExtendYAxis(opts.YAxis{
		Name:      commentCountAxisLabel,
		AxisLabel: &opts.AxisLabel{Color: cOpts.TextMutedColor()},
		AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: cOpts.AxisColor()}},
		SplitLine: &opts.SplitLine{Show: opts.Bool(false)},
	})

	return line
}

func prepareChartData(metrics *ComputedMetrics) ([]string, chartSeriesData) {
	n := len(metrics.TimeSeries)
	labels := make([]string, n)
	data := chartSeriesData{
		sentiment:    make([]opts.LineData, n),
		comments:     make([]opts.LineData, n),
		positiveZone: make([]opts.LineData, n),
		negativeZone: make([]opts.LineData, n),
		trend:        make([]opts.LineData, n),
	}

	for i, ts := range metrics.TimeSeries {
		labels[i] = strconv.Itoa(ts.Tick)
		data.sentiment[i] = opts.LineData{Value: float64(ts.Sentiment)}
		data.comments[i] = opts.LineData{Value: ts.CommentCount}
		data.positiveZone[i] = opts.LineData{Value: SentimentPositiveThreshold}
		data.negativeZone[i] = opts.LineData{Value: SentimentNegativeThreshold}
	}

	if n > 1 {
		start := metrics.Trend.StartSentiment
		end := metrics.Trend.EndSentiment
		step := (end - start) / float32(n-1)

		for i := range n {
			data.trend[i] = opts.LineData{Value: float64(start + step*float32(i))}
		}
	} else {
		data.trend[0] = opts.LineData{Value: float64(metrics.Trend.StartSentiment)}
	}

	return labels, data
}

func addChartSeries(line *charts.Line, metrics *ComputedMetrics, data chartSeriesData) {
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	line.AddSeries(positiveZoneLabel, data.positiveZone,
		charts.WithLineStyleOpts(opts.LineStyle{Color: palette.Semantic.Good, Type: "dashed", Width: zoneLineWidth}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(zoneOpacity), Color: palette.Semantic.Good}),
		charts.WithLineChartOpts(opts.LineChart{Stack: "zone", Smooth: opts.Bool(false), ShowSymbol: opts.Bool(false)}),
	)

	line.AddSeries(negativeZoneLabel, data.negativeZone,
		charts.WithLineStyleOpts(opts.LineStyle{Color: palette.Semantic.Bad, Type: "dashed", Width: zoneLineWidth}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Bad}),
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(false), ShowSymbol: opts.Bool(false)}),
	)

	line.AddSeries(sentimentSeriesLabel, data.sentiment,
		charts.WithLineStyleOpts(opts.LineStyle{Color: palette.Primary[0], Width: sentimentLineWidth}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[0]}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacity)}),
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(smoothCurve)}),
	)

	if len(metrics.TimeSeries) > 1 {
		line.AddSeries(trendLineLabel, data.trend,
			charts.WithLineStyleOpts(opts.LineStyle{Color: palette.Secondary[1], Type: "dashed", Width: zoneLineWidth}),
			charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Secondary[1]}),
			charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(false), ShowSymbol: opts.Bool(false)}),
		)
	}

	line.AddSeries(commentCountLabel, data.comments,
		charts.WithLineStyleOpts(opts.LineStyle{Color: palette.Primary[2]}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[2], Opacity: opts.Float(commentBarOpacity)}),
		charts.WithLineChartOpts(opts.LineChart{YAxisIndex: commentAxisIndex}),
	)
}

func buildDistributionChart(metrics *ComputedMetrics) *charts.Pie {
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	cOpts := plotpage.DefaultChartOpts()

	pie := charts.NewPie()
	pie.SetGlobalOptions(
		charts.WithInitializationOpts(cOpts.Init("100%", pieChartHeight)),
		charts.WithTooltipOpts(cOpts.Tooltip("item")),
		charts.WithLegendOpts(cOpts.Legend()),
	)

	data := []opts.PieData{
		{
			Name:      fmt.Sprintf("Positive (%d)", metrics.Aggregate.PositiveTicks),
			Value:     metrics.Aggregate.PositiveTicks,
			ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Good},
		},
		{
			Name:      fmt.Sprintf("Neutral (%d)", metrics.Aggregate.NeutralTicks),
			Value:     metrics.Aggregate.NeutralTicks,
			ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Warning},
		},
		{
			Name:      fmt.Sprintf("Negative (%d)", metrics.Aggregate.NegativeTicks),
			Value:     metrics.Aggregate.NegativeTicks,
			ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Bad},
		},
	}

	pie.AddSeries(distributionTitle, data,
		charts.WithLabelOpts(opts.Label{
			Show:      opts.Bool(true),
			Formatter: "{b}: {d}%",
			Color:     cOpts.TextColor(),
		}),
		charts.WithPieChartOpts(opts.PieChart{
			Radius: [2]string{distributionInner, distributionOuter},
		}),
	)

	return pie
}

func buildMainChartHint(metrics *ComputedMetrics) plotpage.Hint {
	items := []string{
		"Green dashed line = positive threshold (0.6+), Red dashed line = negative threshold (0.4-)",
		"Solid line = actual sentiment score per tick, Dashed line = regression trend",
		"Secondary axis shows comment count per tick",
	}

	if metrics.Trend.TrendDirection != "" {
		items = append(items, fmt.Sprintf("Trend: %s (%.1f%% change)",
			metrics.Trend.TrendDirection, metrics.Trend.ChangePercent))
	}

	if len(metrics.LowSentimentPeriods) > 0 {
		items = append(items, fmt.Sprintf("Warning: %d low-sentiment period(s) detected",
			len(metrics.LowSentimentPeriods)))
	}

	items = append(items,
		"Sudden drops may indicate stressful periods or difficult bugs",
		"SE-domain terms (kill, abort, fatal) are adjusted to avoid false negatives",
	)

	return plotpage.Hint{
		Title: "How to interpret:",
		Items: items,
	}
}
