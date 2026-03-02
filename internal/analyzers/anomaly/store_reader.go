package anomaly

import (
	"fmt"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
)

// GenerateStoreSections reads pre-computed anomaly data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	timeSeries, tsErr := ReadTimeSeriesIfPresent(reader, kinds)
	if tsErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindTimeSeries, tsErr)
	}

	anomalies, anomErr := ReadAnomaliesIfPresent(reader, kinds)
	if anomErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindAnomalyRecord, anomErr)
	}

	agg, aggErr := ReadAggregateIfPresent(reader, kinds)
	if aggErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindAggregate, aggErr)
	}

	externalAnomalies, eaErr := ReadExternalAnomaliesIfPresent(reader, kinds)
	if eaErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindExternalAnomaly, eaErr)
	}

	externalSummaries, esErr := ReadExternalSummariesIfPresent(reader, kinds)
	if esErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindExternalSummary, esErr)
	}

	if len(timeSeries) == 0 {
		return nil, nil
	}

	return buildStoreSections(timeSeries, anomalies, agg, externalSummaries, externalAnomalies)
}

// ReadTimeSeriesIfPresent reads all time_series records, returning nil if absent.
func ReadTimeSeriesIfPresent(reader analyze.ReportReader, kinds []string) ([]TimeSeriesEntry, error) {
	return analyze.ReadRecordsIfPresent[TimeSeriesEntry](reader, kinds, KindTimeSeries)
}

// ReadAnomaliesIfPresent reads all anomaly_record records, returning nil if absent.
func ReadAnomaliesIfPresent(reader analyze.ReportReader, kinds []string) ([]Record, error) {
	return analyze.ReadRecordsIfPresent[Record](reader, kinds, KindAnomalyRecord)
}

// ReadAggregateIfPresent reads the single aggregate record, returning zero value if absent.
func ReadAggregateIfPresent(reader analyze.ReportReader, kinds []string) (AggregateData, error) {
	return analyze.ReadRecordIfPresent[AggregateData](reader, kinds, KindAggregate)
}

// ReadExternalAnomaliesIfPresent reads all external_anomaly records.
func ReadExternalAnomaliesIfPresent(reader analyze.ReportReader, kinds []string) ([]ExternalAnomaly, error) {
	return analyze.ReadRecordsIfPresent[ExternalAnomaly](reader, kinds, KindExternalAnomaly)
}

// ReadExternalSummariesIfPresent reads all external_summary records.
func ReadExternalSummariesIfPresent(reader analyze.ReportReader, kinds []string) ([]ExternalSummary, error) {
	return analyze.ReadRecordsIfPresent[ExternalSummary](reader, kinds, KindExternalSummary)
}

// buildStoreSections constructs anomaly plot sections from pre-computed data.
func buildStoreSections(
	timeSeries []TimeSeriesEntry,
	anomalies []Record,
	agg AggregateData,
	externalSummaries []ExternalSummary,
	externalAnomalies []ExternalAnomaly,
) ([]plotpage.Section, error) {
	// Build ReportData for chart construction.
	tickMetrics := buildTickMetricsFromTimeSeries(timeSeries)

	input := &ReportData{
		Anomalies:         anomalies,
		TickMetrics:       tickMetrics,
		Threshold:         agg.Threshold,
		WindowSize:        agg.WindowSize,
		ExternalAnomalies: externalAnomalies,
		ExternalSummaries: externalSummaries,
	}

	ticks := mapx.SortedKeys(tickMetrics)
	if len(ticks) == 0 {
		return nil, nil
	}

	labels, churnData, anomalyData := buildChartData(ticks, input)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	chart := createChurnChart(labels, churnData, anomalyData, co, palette)

	statSection := buildStatsSectionFromAggregate(agg, anomalies)

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

// buildTickMetricsFromTimeSeries reconstructs TickMetrics from stored TimeSeriesEntry records.
// This provides the minimal data needed for chart rendering (net churn values).
func buildTickMetricsFromTimeSeries(timeSeries []TimeSeriesEntry) map[int]*TickMetrics {
	result := make(map[int]*TickMetrics, len(timeSeries))

	for _, ts := range timeSeries {
		result[ts.Tick] = &TickMetrics{
			FilesChanged: ts.Metrics.FilesChanged,
			LinesAdded:   ts.Metrics.LinesAdded,
			LinesRemoved: ts.Metrics.LinesRemoved,
			NetChurn:     ts.Metrics.NetChurn,
		}
	}

	return result
}

// buildStatsSectionFromAggregate creates the stats section from pre-computed aggregate data.
func buildStatsSectionFromAggregate(agg AggregateData, anomalies []Record) plotpage.Section {
	anomalyRateStr := fmt.Sprintf("%.1f%%", agg.AnomalyRate)
	totalAnomaliesStr := strconv.Itoa(agg.TotalAnomalies)
	totalTicksStr := strconv.Itoa(agg.TotalTicks)

	highestZStr := "N/A"
	if len(anomalies) > 0 {
		highestZStr = fmt.Sprintf("%.1f", anomalies[0].MaxAbsZScore)
	}

	trendColor := plotpage.BadgeSuccess
	if agg.AnomalyRate > anomalyRateWarningThreshold {
		trendColor = plotpage.BadgeWarning
	}

	if agg.AnomalyRate > anomalyRateErrorThreshold {
		trendColor = plotpage.BadgeError
	}

	avgLangDiversity := fmt.Sprintf("%.1f", agg.LangDiversityMean)
	avgAuthorCount := fmt.Sprintf("%.1f", agg.AuthorCountMean)

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
