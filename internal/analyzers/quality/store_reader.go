package quality

import (
	"fmt"
	"slices"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed quality data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	timeSeries, tsErr := readTimeSeriesIfPresent(reader, kinds)
	if tsErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindTimeSeries, tsErr)
	}

	agg, aggErr := readAggregateIfPresent(reader, kinds)
	if aggErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindAggregate, aggErr)
	}

	if len(timeSeries) == 0 {
		return nil, nil
	}

	metrics := &ComputedMetrics{
		TimeSeries: timeSeries,
		Aggregate:  agg,
	}

	return buildStoreSections(metrics)
}

// readTimeSeriesIfPresent reads all time_series records, returning nil if absent.
func readTimeSeriesIfPresent(reader analyze.ReportReader, kinds []string) ([]TimeSeriesEntry, error) {
	if !slices.Contains(kinds, KindTimeSeries) {
		return nil, nil
	}

	var result []TimeSeriesEntry

	iterErr := reader.Iter(KindTimeSeries, func(raw []byte) error {
		var entry TimeSeriesEntry

		decErr := analyze.GobDecode(raw, &entry)
		if decErr != nil {
			return decErr
		}

		result = append(result, entry)

		return nil
	})

	return result, iterErr
}

// readAggregateIfPresent reads the single aggregate record, returning zero value if absent.
func readAggregateIfPresent(reader analyze.ReportReader, kinds []string) (AggregateData, error) {
	if !slices.Contains(kinds, KindAggregate) {
		return AggregateData{}, nil
	}

	var agg AggregateData

	iterErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &agg)
	})

	return agg, iterErr
}

// buildStoreSections constructs all quality plot sections from pre-computed metrics.
func buildStoreSections(metrics *ComputedMetrics) ([]plotpage.Section, error) {
	complexityChart := buildComplexityChart(metrics)
	halsteadChart := buildHalsteadChart(metrics)
	statSection := buildQualityStatsSection(metrics)

	return []plotpage.Section{
		{
			Title:    "Cyclomatic Complexity Over Time",
			Subtitle: "Median, mean, and P95 cyclomatic complexity per tick.",
			Chart:    plotpage.WrapChart(complexityChart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<b>Median</b> (solid) — robust central tendency, resistant to outliers",
					"<b>Mean</b> (dashed) — pulled up by complex outlier files; gap with median reveals skew",
					"<b>P95</b> (dotted) — the 95th percentile; shows worst-case complexity trend",
					"Rising median trend indicates overall code is becoming harder to maintain",
					"Large mean/median gap reveals heavy-tailed outliers (generated code, bulk imports)",
				},
			},
		},
		{
			Title:    "Halstead Volume Over Time",
			Subtitle: "Median, mean, and P95 Halstead volume per tick.",
			Chart:    plotpage.WrapChart(halsteadChart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Halstead volume measures code size and complexity in information-theoretic terms",
					"<b>Median</b> shows typical file complexity; <b>P95</b> shows outlier magnitude",
					"Large gap between mean and median indicates a few very large/complex files dominate",
				},
			},
		},
		statSection,
	}, nil
}
