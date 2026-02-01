package burndown

import (
	"errors"
	"fmt"
	"io"
	"sort"
	"strconv"
	"time"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

const (
	reportKeyEndTime     = "EndTime"
	reportKeyProjectName = "ProjectName"
)

const (
	hoursPerDay            = 24
	daysPerMonth           = 30
	daysPerYear            = 365
	monthsPerYear          = 12
	minYearsForAggregation = 2
	// interpolationFactor adds points between samples for smooth curves (Hercules-style).
	interpolationFactor = 5
)

// generatePlot creates an interactive HTML stacked area chart from the burndown analysis report.
func (b *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := b.GenerateChart(report)
	if err != nil {
		return fmt.Errorf("generate chart: %w", err)
	}

	if r, ok := chart.(interface{ Render(io.Writer) error }); ok {
		err = r.Render(writer)
		if err != nil {
			return fmt.Errorf("render chart: %w", err)
		}

		return nil
	}

	return errors.New("chart does not support Render") //nolint:err113 // dynamic error
}

// burndownChartParams holds extracted parameters for chart generation.
type burndownChartParams struct {
	globalHistory DenseHistory
	sampling      int
	granularity   int
	tickSize      time.Duration
	endTime       time.Time
	projectName   string
}

// extractChartParams extracts and validates parameters from the report.
func extractChartParams(report analyze.Report) (*burndownChartParams, error) {
	globalHistory, ok := report["GlobalHistory"].(DenseHistory)
	if !ok {
		return nil, errors.New("expected DenseHistory for GlobalHistory") //nolint:err113 // descriptive error
	}

	sampling, ok := report["Sampling"].(int)
	if !ok {
		return nil, errors.New("expected int for Sampling") //nolint:err113 // descriptive error
	}

	granularity, ok := report["Granularity"].(int)
	if !ok {
		return nil, errors.New("expected int for Granularity") //nolint:err113 // descriptive error
	}

	tickSize, ok := report["TickSize"].(time.Duration)
	if !ok {
		tickSize = hoursPerDay * time.Hour
	}

	var endTime time.Time
	if et, hasET := report[reportKeyEndTime].(time.Time); hasET && !et.IsZero() {
		endTime = et
	}

	projectName := "project"
	if pn, hasPN := report[reportKeyProjectName].(string); hasPN && pn != "" {
		projectName = pn
	}

	return &burndownChartParams{
		globalHistory: globalHistory,
		sampling:      sampling,
		granularity:   granularity,
		tickSize:      tickSize,
		endTime:       endTime,
		projectName:   projectName,
	}, nil
}

// GenerateChart creates the chart object from the report.
func (b *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	params, err := extractChartParams(report)
	if err != nil {
		return nil, err
	}

	if len(params.globalHistory) == 0 {
		return createBurndownEmptyChart(), nil
	}

	xLabels := buildXLabelsInterpolated(
		params.globalHistory, params.sampling, params.tickSize, interpolationFactor)
	line := createBurndownLineChart(xLabels, params)
	addBurndownSeries(line, params, interpolationFactor)

	return line, nil
}

// buildXLabelsInterpolated builds X-axis labels, optionally with factor-1 extra points between samples.
func buildXLabelsInterpolated(
	globalHistory DenseHistory, sampling int, tickSize time.Duration, factor int,
) []string {
	n := len(globalHistory)
	points := max((n-1)*factor+1, 1)
	xLabels := make([]string, points)
	for i := range points {
		subIdx := float64(i) / float64(factor)
		ticks := int(subIdx * float64(sampling))
		days := (time.Duration(ticks) * tickSize).Hours() / hoursPerDay
		xLabels[i] = strconv.Itoa(int(days)) + "d"
	}
	return xLabels
}

// interpolateSeries linearly interpolates values between samples for smoother curves.
func interpolateSeries(values []float64, factor int) []opts.LineData {
	if factor <= 1 || len(values) < 2 {
		out := make([]opts.LineData, len(values))
		for i, v := range values {
			const roundHalf = 0.5
			out[i] = opts.LineData{Value: int64(v + roundHalf)}
		}
		return out
	}
	n := len(values)
	points := (n-1)*factor + 1
	out := make([]opts.LineData, points)
	for i := range points {
		subIdx := float64(i) / float64(factor)
		var val float64
		if n == 1 || subIdx >= float64(n-1) {
			val = values[n-1]
		} else {
			lo := int(subIdx)
			frac := subIdx - float64(lo)
			val = values[lo]*(1-frac) + values[lo+1]*frac
		}
		if val < 0 {
			val = 0
		}
		const roundHalf = 0.5
		out[i] = opts.LineData{Value: int64(val + roundHalf)}
	}
	return out
}

// burndownColorPalette matches the reference: reddish-brown, steel blue, lavender, gray, gold, green, pink, etc.
var burndownColorPalette = []string{ //nolint:gochecknoglobals // chart color palette
	"#8B4513", "#2f4554", "#9370DB", "#808080", "#DAA520",
	"#90EE90", "#FFB6C1", "#c23531", "#37a2da", "#6B8E23",
	"#4B0082", "#ffdb5c", "#749f83", "#fb7293", "#e5323e",
	"#32c5e9", "#9fe6b8", "#ff9f7f", "#67e0e3", "#e062ae",
}

func computeMaxLines(globalHistory DenseHistory) int64 {
	var maxLines int64

	for _, sample := range globalHistory {
		var sum int64

		for _, v := range sample {
			if v > 0 {
				sum += v
			}
		}

		if sum > maxLines {
			maxLines = sum
		}
	}

	return maxLines
}

func createBurndownLineChart(xLabels []string, params *burndownChartParams) *charts.Line {
	const (
		fullZoomPct     = 100
		blackBackground = "#000000"
		whiteGrid       = "#ffffff"
	)

	maxLines := computeMaxLines(params.globalHistory)
	title := fmt.Sprintf("%s x %d (granularity %d, sampling %d)",
		params.projectName, maxLines, params.granularity, params.sampling)

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(opts.Initialization{
			BackgroundColor: blackBackground,
			Theme:           "dark",
		}),
		charts.WithColorsOpts(burndownColorPalette),
		charts.WithTitleOpts(opts.Title{
			Title:      title,
			Left:       "center",
			TitleStyle: &opts.TextStyle{Color: whiteGrid},
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithLegendOpts(opts.Legend{
			Show: opts.Bool(true),
			Type: "scroll",
			Top:  "5%",
			Left: "5%",
			TextStyle: &opts.TextStyle{
				Color: whiteGrid,
			},
		}),
		charts.WithGridOpts(opts.Grid{
			Top:          "20%",
			Bottom:       "15%",
			Left:         "10%",
			Right:        "5%",
			ContainLabel: opts.Bool(true),
		}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Time (days)",
			AxisLabel: &opts.AxisLabel{
				Color: whiteGrid,
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: whiteGrid}},
			AxisTick: &opts.AxisTick{LineStyle: &opts.LineStyle{Color: whiteGrid}},
			SplitLine: &opts.SplitLine{
				Show:      opts.Bool(true),
				LineStyle: &opts.LineStyle{Color: "rgba(255,255,255,0.2)", Type: "solid"},
			},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name: "Lines of Code",
			AxisLabel: &opts.AxisLabel{
				Color: whiteGrid,
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: whiteGrid}},
			SplitLine: &opts.SplitLine{
				Show:      opts.Bool(true),
				LineStyle: &opts.LineStyle{Color: "rgba(255,255,255,0.2)", Type: "solid"},
			},
		}),
	)
	line.SetXAxis(xLabels)

	return line
}

// formatBandLabel returns a compact age label (e.g. "1mo", "2mo" or "2020", "2021").
func formatBandLabel(bandIdx, granularity int, tickSize time.Duration, maxBandIdx int, endTime time.Time) string {
	upperTicks := (bandIdx + 1) * granularity
	ageDuration := time.Duration(upperTicks) * tickSize
	ageDays := ageDuration.Hours() / hoursPerDay
	ageMonths := max(int(ageDays)/daysPerMonth, 1)

	maxUpperTicks := (maxBandIdx + 1) * granularity
	maxAgeDays := (time.Duration(maxUpperTicks) * tickSize).Hours() / hoursPerDay
	maxAgeMonths := int(maxAgeDays) / daysPerMonth

	if maxAgeMonths >= monthsPerYear && !endTime.IsZero() {
		bandDate := endTime.Add(-ageDuration)
		return strconv.Itoa(bandDate.Year())
	}
	if maxAgeMonths >= monthsPerYear {
		ageYears := ageMonths / monthsPerYear
		if ageYears == 0 {
			return fmt.Sprintf("%dmo", ageMonths)
		}
		return fmt.Sprintf("%dy", ageYears)
	}
	return fmt.Sprintf("%dmo", ageMonths)
}

// yearAggregation holds bands aggregated by calendar year for "code from each time period" display.
type yearAggregation struct {
	years []int
	data  [][]float64
}

type yearWeight struct {
	year   int
	weight float64
}

func getBandWeights(bandIdx, granularity int, tickSize time.Duration, startTime time.Time) []yearWeight {
	bandStart := startTime.Add(time.Duration(bandIdx*granularity) * tickSize)
	bandDuration := time.Duration(granularity) * tickSize
	bandEnd := bandStart.Add(bandDuration)

	startYear := bandStart.Year()
	endYear := bandEnd.Year()

	var weights []yearWeight

	for year := startYear; year <= endYear; year++ {
		yearStart := time.Date(year, 1, 1, 0, 0, 0, 0, startTime.Location())
		yearEnd := time.Date(year+1, 1, 1, 0, 0, 0, 0, startTime.Location())

		start := bandStart
		if yearStart.After(start) {
			start = yearStart
		}

		end := bandEnd
		if yearEnd.Before(end) {
			end = yearEnd
		}

		if end.After(start) {
			overlap := end.Sub(start)
			weight := float64(overlap) / float64(bandDuration)
			weights = append(weights, yearWeight{year: year, weight: weight})
		}
	}
	return weights
}

// aggregateBandsByYear groups bands by calendar year with weighted distribution.
func aggregateBandsByYear(params *burndownChartParams) *yearAggregation { //nolint:gocognit // aggregation over bands and samples
	if params.endTime.IsZero() || len(params.globalHistory) == 0 {
		return nil
	}
	numBands := len(params.globalHistory[0])
	numSamples := len(params.globalHistory)
	if numBands == 0 || numSamples == 0 {
		return nil
	}
	lastTick := (numSamples - 1) * params.sampling
	startTime := params.endTime.Add(-time.Duration(lastTick) * params.tickSize)

	bandWeights := make([][]yearWeight, numBands)
	yearSet := make(map[int]bool)

	for b := range numBands {
		w := getBandWeights(b, params.granularity, params.tickSize, startTime)
		bandWeights[b] = w
		for _, yw := range w {
			yearSet[yw.year] = true
		}
	}

	years := make([]int, 0, len(yearSet))
	for y := range yearSet {
		years = append(years, y)
	}
	sort.Ints(years)

	if len(years) < minYearsForAggregation {
		return nil
	}

	yearIdx := make(map[int]int)
	for i, y := range years {
		yearIdx[y] = i
	}

	data := make([][]float64, len(years))
	for i := range years {
		data[i] = make([]float64, numSamples)
	}

	for sampleIdx, sample := range params.globalHistory {
		for bandIdx, val := range sample {
			if val <= 0 {
				continue
			}
			weights := bandWeights[bandIdx]
			for _, yw := range weights {
				idx := yearIdx[yw.year]
				data[idx][sampleIdx] += float64(val) * yw.weight
			}
		}
	}

	return &yearAggregation{years: years, data: data}
}

func addBurndownSeries(line *charts.Line, params *burndownChartParams, interpFactor int) {
	const opacity = 0.5

	agg := aggregateBandsByYear(params)
	if agg != nil {
		// One series per year, oldest at bottom: "how many codes lives from each time period".
		for i, year := range agg.years {
			data := interpolateSeries(agg.data[i], interpFactor)
			line.AddSeries(
				strconv.Itoa(year),
				data,
				charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
				charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(opacity)}),
			)
		}
		return
	}

	// Fallback: raw bands with month/year labels.
	numBands := len(params.globalHistory[0])
	maxBandIdx := numBands - 1

	for rev := numBands - 1; rev >= 0; rev-- {
		bandIdx := rev
		raw := make([]float64, len(params.globalHistory))
		for sampleIdx, sample := range params.globalHistory {
			val := int64(0)
			if bandIdx < len(sample) {
				val = sample[bandIdx]
			}
			if val < 0 {
				val = 0
			}
			raw[sampleIdx] = float64(val)
		}
		data := interpolateSeries(raw, interpFactor)
		label := formatBandLabel(bandIdx, params.granularity, params.tickSize, maxBandIdx, params.endTime)
		line.AddSeries(
			label,
			data,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(opacity)}),
		)
	}
}

func createBurndownEmptyChart() *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(opts.Initialization{
			BackgroundColor: "#000000",
			Theme:           "dark",
		}),
		charts.WithTitleOpts(opts.Title{
			Title:         "Code Burndown History",
			Subtitle:      "No data",
			TitleStyle:    &opts.TextStyle{Color: "#ffffff"},
			SubtitleStyle: &opts.TextStyle{Color: "#ffffff"},
		}),
	)
	line.SetXAxis([]string{})

	return line
}
