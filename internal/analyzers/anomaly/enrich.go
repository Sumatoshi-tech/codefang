package anomaly

import (
	"math"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// EnrichFromReports runs Z-score anomaly detection on external analyzer time
// series and injects the results into the anomaly report. It iterates over
// registered TimeSeriesExtractors, calls ComputeZScores on each dimension,
// and stores ExternalAnomaly/ExternalSummary records under well-known keys.
func EnrichFromReports(
	anomalyReport analyze.Report,
	otherReports map[string]analyze.Report,
	windowSize int,
	threshold float64,
) {
	var allAnomalies []ExternalAnomaly

	var allSummaries []ExternalSummary

	for source, extractor := range snapshotExtractors() {
		report, ok := otherReports[source]
		if !ok {
			continue
		}

		ticks, dimensions := extractor(report)
		if len(ticks) == 0 || len(dimensions) == 0 {
			continue
		}

		anomalies, summaries := detectExternalAnomalies(source, ticks, dimensions, windowSize, threshold)
		allAnomalies = append(allAnomalies, anomalies...)
		allSummaries = append(allSummaries, summaries...)
	}

	// Sort anomalies by absolute Z-score descending.
	sort.Slice(allAnomalies, func(i, j int) bool {
		return math.Abs(allAnomalies[i].ZScore) > math.Abs(allAnomalies[j].ZScore)
	})

	// Sort summaries by source then dimension for stable output.
	sort.Slice(allSummaries, func(i, j int) bool {
		if allSummaries[i].Source != allSummaries[j].Source {
			return allSummaries[i].Source < allSummaries[j].Source
		}

		return allSummaries[i].Dimension < allSummaries[j].Dimension
	})

	anomalyReport["external_anomalies"] = allAnomalies
	anomalyReport["external_summaries"] = allSummaries
}

func detectExternalAnomalies(
	source string,
	ticks []int,
	dimensions map[string][]float64,
	windowSize int,
	threshold float64,
) ([]ExternalAnomaly, []ExternalSummary) {
	var anomalies []ExternalAnomaly

	var summaries []ExternalSummary

	// Process dimensions in sorted order for deterministic output.
	dimNames := make([]string, 0, len(dimensions))

	for name := range dimensions {
		dimNames = append(dimNames, name)
	}

	sort.Strings(dimNames)

	for _, dimName := range dimNames {
		values := dimensions[dimName]
		if len(values) != len(ticks) {
			continue
		}

		scores := ComputeZScores(values, windowSize)
		mean, stddev := MeanStdDev(values)

		var highestZ float64

		var anomalyCount int

		for i, score := range scores {
			absScore := math.Abs(score)

			if absScore > threshold {
				anomalies = append(anomalies, ExternalAnomaly{
					Source:    source,
					Dimension: dimName,
					Tick:      ticks[i],
					ZScore:    score,
					RawValue:  values[i],
				})
				anomalyCount++
			}

			if absScore > highestZ {
				highestZ = absScore
			}
		}

		summaries = append(summaries, ExternalSummary{
			Source:    source,
			Dimension: dimName,
			Mean:      mean,
			StdDev:    stddev,
			Anomalies: anomalyCount,
			HighestZ:  highestZ,
		})
	}

	return anomalies, summaries
}
