package anomaly

import (
	"math"
	"sort"
)

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
