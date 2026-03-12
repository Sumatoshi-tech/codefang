package sentiment

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed sentiment data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	timeSeries, tsErr := readTimeSeriesIfPresent(reader, kinds)
	if tsErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindTimeSeries, tsErr)
	}

	trend, trendErr := readTrendIfPresent(reader, kinds)
	if trendErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindTrend, trendErr)
	}

	agg, aggErr := readAggregateIfPresent(reader, kinds)
	if aggErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindAggregate, aggErr)
	}

	metrics := &ComputedMetrics{
		TimeSeries: timeSeries,
		Trend:      trend,
		Aggregate:  agg,
	}

	return buildStoreSections(metrics)
}

// readTimeSeriesIfPresent reads all time_series records, returning nil if absent.
func readTimeSeriesIfPresent(reader analyze.ReportReader, kinds []string) ([]TimeSeriesData, error) {
	return analyze.ReadRecordsIfPresent[TimeSeriesData](reader, kinds, KindTimeSeries)
}

// readTrendIfPresent reads the trend record, returning zero value if absent.
func readTrendIfPresent(reader analyze.ReportReader, kinds []string) (TrendData, error) {
	return analyze.ReadRecordIfPresent[TrendData](reader, kinds, KindTrend)
}

// readAggregateIfPresent reads the aggregate record, returning zero value if absent.
func readAggregateIfPresent(reader analyze.ReportReader, kinds []string) (AggregateData, error) {
	return analyze.ReadRecordIfPresent[AggregateData](reader, kinds, KindAggregate)
}

// buildStoreSections constructs the sentiment plot sections from pre-computed data.
func buildStoreSections(metrics *ComputedMetrics) ([]plotpage.Section, error) {
	if len(metrics.TimeSeries) == 0 {
		return nil, nil
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
