package burndown

import (
	"encoding/json"
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
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	hoursPerDay            = 24
	daysPerMonth           = 30
	monthsPerYear          = 12
	minYearsForAggregation = 2
	interpolationFactor    = 5
	chartHeight            = "600px"
	areaOpacity            = 0.5
	roundingOffset         = 0.5
	minInterpolationLen    = 2
)

// ErrInvalidReport indicates the report doesn't contain expected data.
var ErrInvalidReport = errors.New("invalid burndown report: expected DenseHistory")

func (b *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := b.GenerateSections(report)
	if err != nil {
		return fmt.Errorf("generate sections: %w", err)
	}

	params := extractParams(report)
	title := "Code Burndown History"
	desc := "Visualizes code survival over time"
	if params != nil {
		title = fmt.Sprintf("Burndown: %s", params.projectName)
		desc = fmt.Sprintf("Granularity %d, sampling %d", params.granularity, params.sampling)
	}

	page := plotpage.NewPage(title, desc)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (b *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := b.buildChart(report)
	if err != nil {
		return nil, fmt.Errorf("generate chart: %w", err)
	}

	return []plotpage.Section{
		{
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
					"Look for: Rapid decrease in recent layers indicates instability",
				},
			},
		},
	}, nil
}

type burndownParams struct {
	globalHistory DenseHistory
	sampling      int
	granularity   int
	tickSize      time.Duration
	endTime       time.Time
	projectName   string
}

func extractParams(report analyze.Report) *burndownParams {
	globalHistory, histOK := report["GlobalHistory"].(DenseHistory)
	if !histOK {
		// Fallback: after binary encode -> JSON decode, "GlobalHistory" becomes
		// "global_survival" ([]any of map[string]any with band_breakdown).
		globalHistory = extractDenseHistoryFromBinary(report, "global_survival")
		if globalHistory == nil {
			return nil
		}
	}

	sampling, granularity := extractSamplingGranularity(report)

	return &burndownParams{
		globalHistory: globalHistory,
		sampling:      sampling,
		granularity:   granularity,
		tickSize:      extractTickSize(report),
		endTime:       extractEndTime(report),
		projectName:   extractProjectName(report),
	}
}

// extractSamplingGranularity extracts sampling and granularity from the report,
// falling back to aggregate inference after binary round-trip.
func extractSamplingGranularity(report analyze.Report) (sampling, granularity int) {
	sampling = extractInt(report, "Sampling", 0)
	granularity = extractInt(report, "Granularity", 0)

	// After binary round-trip, Sampling/Granularity are lost. Infer from aggregate.
	if sampling == 0 || granularity == 0 {
		sampling, granularity = inferFromAggregate(report, sampling, granularity)
	}

	return sampling, granularity
}

// inferFromAggregate attempts to infer sampling and granularity from the "aggregate" key.
func inferFromAggregate(report analyze.Report, sampling, granularity int) (outSampling, outGranularity int) {
	agg, aggOK := report["aggregate"].(map[string]any)
	if !aggOK {
		return sampling, granularity
	}

	numSamples := extractIntFromMap(agg, "num_samples", 0)
	numBands := extractIntFromMap(agg, "num_bands", 0)

	if sampling == 0 && numSamples > 1 && numBands > 0 {
		sampling = 1
	}

	if granularity == 0 && numBands > 0 {
		granularity = 1
	}

	return sampling, granularity
}

// extractTickSize returns the tick size from the report, defaulting to 24 hours.
func extractTickSize(report analyze.Report) time.Duration {
	tickSize, tsOK := report["TickSize"].(time.Duration)
	if !tsOK {
		return hoursPerDay * time.Hour
	}

	return tickSize
}

// extractEndTime returns the end time from the report, defaulting to zero time.
func extractEndTime(report analyze.Report) time.Time {
	endTime, etOK := report["EndTime"].(time.Time)
	if !etOK {
		return time.Time{}
	}

	return endTime
}

// extractProjectName returns the project name from the report, defaulting to "project".
func extractProjectName(report analyze.Report) string {
	projectName, pnOK := report["ProjectName"].(string)
	if !pnOK || projectName == "" {
		return "project"
	}

	return projectName
}

// extractDenseHistoryFromBinary converts binary-decoded JSON survival data back
// to DenseHistory. After binary encode -> JSON decode round-trip, survival data
// is []any where each element is map[string]any with "band_breakdown" ([]any of float64).
func extractDenseHistoryFromBinary(report analyze.Report, key string) DenseHistory {
	raw, rawOK := report[key]
	if !rawOK {
		return nil
	}

	if raw == nil {
		return DenseHistory{}
	}

	list, listOK := raw.([]any)
	if !listOK {
		return nil
	}

	if len(list) == 0 {
		return DenseHistory{}
	}

	result := make(DenseHistory, len(list))

	for i, item := range list {
		m, mOK := item.(map[string]any)
		if !mOK {
			continue
		}

		bands, bandsOK := m["band_breakdown"].([]any)
		if !bandsOK {
			continue
		}

		row := make([]int64, len(bands))

		for j, v := range bands {
			row[j] = toInt64(v)
		}

		result[i] = row
	}

	return result
}

// extractInt extracts an int from the report, handling float64 from JSON decode.
func extractInt(report analyze.Report, key string, fallback int) int {
	if intVal, intOK := report[key].(int); intOK {
		return intVal
	}

	if floatVal, floatOK := report[key].(float64); floatOK {
		return int(floatVal)
	}

	return fallback
}

// extractIntFromMap extracts an int from a generic map, handling float64 from JSON decode.
func extractIntFromMap(m map[string]any, key string, fallback int) int {
	if floatVal, floatOK := m[key].(float64); floatOK {
		return int(floatVal)
	}

	if intVal, intOK := m[key].(int); intOK {
		return intVal
	}

	return fallback
}

// toInt64 converts a numeric value (int, float64, etc.) to int64.
func toInt64(v any) int64 {
	switch num := v.(type) {
	case float64:
		return int64(num)
	case int64:
		return num
	case int:
		return int64(num)
	case json.Number:
		i, err := num.Int64()
		if err != nil {
			return 0
		}

		return i
	default:
		return 0
	}
}

// GenerateChart implements PlotGenerator interface.
func (b *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return b.buildChart(report)
}

// buildChart creates a burndown line chart from the report.
func (b *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.Line, error) {
	params := extractParams(report)
	if params == nil {
		return nil, ErrInvalidReport
	}
	if len(params.globalHistory) == 0 {
		return createEmptyBurndown(), nil
	}

	co := plotpage.DefaultChartOpts()
	xLabels := buildXLabels(params)
	line := createLineChart(xLabels, params, co)
	addSeries(line, params)

	return line, nil
}

func buildXLabels(params *burndownParams) []string {
	n := len(params.globalHistory)
	points := max((n-1)*interpolationFactor+1, 1)
	labels := make([]string, points)
	for i := range points {
		subIdx := float64(i) / float64(interpolationFactor)
		ticks := int(subIdx * float64(params.sampling))
		days := (time.Duration(ticks) * params.tickSize).Hours() / hoursPerDay
		labels[i] = strconv.Itoa(int(days)) + "d"
	}

	return labels
}

func getColorPalette() []string {
	return []string{
		"#8B4513", "#2f4554", "#9370DB", "#808080", "#DAA520",
		"#90EE90", "#FFB6C1", "#c23531", "#37a2da", "#6B8E23",
		"#4B0082", "#ffdb5c", "#749f83", "#fb7293", "#e5323e",
	}
}

func computeMaxLines(history DenseHistory) int64 {
	var maxLines int64
	for _, sample := range history {
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

func createLineChart(xLabels []string, params *burndownParams, co *plotpage.ChartOpts) *charts.Line {
	maxLines := computeMaxLines(params.globalHistory)
	title := fmt.Sprintf("%s x %d (granularity %d, sampling %d)",
		params.projectName, maxLines, params.granularity, params.sampling)

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", chartHeight)),
		charts.WithColorsOpts(getColorPalette()),
		charts.WithTitleOpts(co.Title(title, "")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(opts.Legend{
			Show: opts.Bool(true), Type: "scroll", Top: "5%", Left: "5%",
			TextStyle: &opts.TextStyle{Color: co.TextMutedColor()},
		}),
		charts.WithGridOpts(opts.Grid{
			Top: "20%", Bottom: "15%",
			Left: "10%", Right: "5%",
			ContainLabel: opts.Bool(true),
		}),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (days)")),
		charts.WithYAxisOpts(co.YAxis("Lines of Code")),
	)
	line.SetXAxis(xLabels)

	return line
}

func addSeries(line *charts.Line, params *burndownParams) {
	if agg := aggregateByYear(params); agg != nil {
		addYearSeries(line, agg)
		return
	}
	addBandSeries(line, params)
}

func addYearSeries(line *charts.Line, agg *yearAgg) {
	for i, year := range agg.years {
		data := interpolate(agg.data[i])
		line.AddSeries(strconv.Itoa(year), data,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacity)}),
		)
	}
}

func addBandSeries(line *charts.Line, params *burndownParams) {
	numBands := len(params.globalHistory[0])
	for rev := numBands - 1; rev >= 0; rev-- {
		raw := extractBandValues(params.globalHistory, rev)
		data := interpolate(raw)
		label := bandLabel(rev, params)
		line.AddSeries(label, data,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacity)}),
		)
	}
}

func extractBandValues(history DenseHistory, bandIdx int) []float64 {
	raw := make([]float64, len(history))
	for i, sample := range history {
		if bandIdx < len(sample) && sample[bandIdx] > 0 {
			raw[i] = float64(sample[bandIdx])
		}
	}

	return raw
}

func interpolate(values []float64) []opts.LineData {
	if len(values) < minInterpolationLen {
		return interpolateShort(values)
	}

	return interpolateFull(values)
}

func interpolateShort(values []float64) []opts.LineData {
	out := make([]opts.LineData, len(values))
	for i, v := range values {
		out[i] = opts.LineData{Value: int64(v + roundingOffset)}
	}

	return out
}

func interpolateFull(values []float64) []opts.LineData {
	n := len(values)
	points := (n-1)*interpolationFactor + 1
	out := make([]opts.LineData, points)

	for i := range points {
		val := interpolatePoint(values, i, n)
		out[i] = opts.LineData{Value: int64(val + roundingOffset)}
	}

	return out
}

func interpolatePoint(values []float64, idx, n int) float64 {
	subIdx := float64(idx) / float64(interpolationFactor)
	if subIdx >= float64(n-1) {
		return values[n-1]
	}
	lo := int(subIdx)
	frac := subIdx - float64(lo)
	val := values[lo]*(1-frac) + values[lo+1]*frac
	if val < 0 {
		return 0
	}

	return val
}

type yearAgg struct {
	years []int
	data  [][]float64
}

func aggregateByYear(params *burndownParams) *yearAgg {
	if !canAggregateByYear(params) {
		return nil
	}

	numBands := len(params.globalHistory[0])
	numSamples := len(params.globalHistory)
	startTime := computeStartTime(params, numSamples)

	bandWeights, yearSet := computeBandWeights(params, numBands, startTime)
	years := sortedYears(yearSet)
	if len(years) < minYearsForAggregation {
		return nil
	}

	data := computeYearData(params, bandWeights, years, numSamples)

	return &yearAgg{years, data}
}

func canAggregateByYear(params *burndownParams) bool {
	return !params.endTime.IsZero() &&
		len(params.globalHistory) > 0 &&
		len(params.globalHistory[0]) > 0
}

func computeStartTime(params *burndownParams, numSamples int) time.Time {
	lastTick := (numSamples - 1) * params.sampling

	return params.endTime.Add(-time.Duration(lastTick) * params.tickSize)
}

type yearWeight struct {
	year   int
	weight float64
}

func computeBandWeights(
	params *burndownParams,
	numBands int,
	startTime time.Time,
) (bandWeights [][]yearWeight, yearSet map[int]bool) {
	bandWeights = make([][]yearWeight, numBands)
	yearSet = make(map[int]bool)

	for bandIdx := range numBands {
		weights := computeSingleBandWeights(params, bandIdx, startTime)
		bandWeights[bandIdx] = weights
		for _, w := range weights {
			yearSet[w.year] = true
		}
	}

	return bandWeights, yearSet
}

func computeSingleBandWeights(params *burndownParams, bandIdx int, startTime time.Time) []yearWeight {
	bandStart := startTime.Add(time.Duration(bandIdx*params.granularity) * params.tickSize)
	bandDur := time.Duration(params.granularity) * params.tickSize
	bandEnd := bandStart.Add(bandDur)

	var weights []yearWeight
	for year := bandStart.Year(); year <= bandEnd.Year(); year++ {
		if w := computeYearWeight(year, bandStart, bandEnd, bandDur, startTime.Location()); w > 0 {
			weights = append(weights, yearWeight{year, w})
		}
	}

	return weights
}

func computeYearWeight(year int, bandStart, bandEnd time.Time, bandDur time.Duration, loc *time.Location) float64 {
	yearStart := time.Date(year, 1, 1, 0, 0, 0, 0, loc)
	yearEnd := time.Date(year+1, 1, 1, 0, 0, 0, 0, loc)

	start, end := bandStart, bandEnd
	if yearStart.After(start) {
		start = yearStart
	}
	if yearEnd.Before(end) {
		end = yearEnd
	}

	if end.After(start) {
		return float64(end.Sub(start)) / float64(bandDur)
	}

	return 0
}

func sortedYears(yearSet map[int]bool) []int {
	years := make([]int, 0, len(yearSet))
	for y := range yearSet {
		years = append(years, y)
	}
	sort.Ints(years)

	return years
}

func computeYearData(params *burndownParams, bandWeights [][]yearWeight, years []int, numSamples int) [][]float64 {
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
			for _, w := range bandWeights[bandIdx] {
				data[yearIdx[w.year]][sampleIdx] += float64(val) * w.weight
			}
		}
	}

	return data
}

func bandLabel(bandIdx int, params *burndownParams) string {
	upperTicks := (bandIdx + 1) * params.granularity
	ageDur := time.Duration(upperTicks) * params.tickSize
	ageDays := ageDur.Hours() / hoursPerDay
	ageMonths := max(int(ageDays)/daysPerMonth, 1)

	maxBandIdx := len(params.globalHistory[0]) - 1
	maxDays := (time.Duration((maxBandIdx+1)*params.granularity) * params.tickSize).Hours() / hoursPerDay
	maxMonths := int(maxDays) / daysPerMonth

	if maxMonths >= monthsPerYear && !params.endTime.IsZero() {
		return strconv.Itoa(params.endTime.Add(-ageDur).Year())
	}
	if maxMonths >= monthsPerYear {
		if y := ageMonths / monthsPerYear; y > 0 {
			return fmt.Sprintf("%dy", y)
		}
	}

	return fmt.Sprintf("%dmo", ageMonths)
}

func init() { //nolint:gochecknoinits // registration pattern
	analyze.RegisterPlotSections("history/burndown", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&HistoryAnalyzer{}).GenerateSections(report)
	})
}

func createEmptyBurndown() *charts.Line {
	co := plotpage.DefaultChartOpts()
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", chartHeight)),
		charts.WithTitleOpts(co.Title("Burndown", "No data")),
	)
	line.SetXAxis([]string{})

	return line
}
