// Package anomaly provides temporal anomaly detection over commit history.
// It uses Z-score analysis with a sliding window to detect sudden quality
// degradation in per-tick metrics (files changed, lines added/removed, churn).
package anomaly

import (
	"math"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/stats"
)

// ComputeZScores computes the Z-score for each value using a trailing sliding
// window of the given size. For index i, the window is values[max(0, i-window):i].
// The Z-score measures how many standard deviations the current value is from
// the window mean. When the standard deviation is zero and the value equals
// the mean, the Z-score is 0. When the standard deviation is zero and the
// value differs from the mean, the Z-score is +/- [stats.ZScoreMaxSentinel].
func ComputeZScores(values []float64, window int) []float64 {
	count := len(values)
	if count == 0 {
		return nil
	}

	if window < 1 {
		window = 1
	}

	scores := make([]float64, count)

	for i := range count {
		start := max(0, i-window)

		windowSlice := values[start:i]
		if len(windowSlice) == 0 {
			scores[i] = 0

			continue
		}

		mean, stddev := stats.MeanStdDev(windowSlice)

		if stddev == 0 {
			diff := values[i] - mean
			if diff == 0 {
				scores[i] = 0
			} else {
				scores[i] = math.Copysign(stats.ZScoreMaxSentinel, diff)
			}

			continue
		}

		scores[i] = (values[i] - mean) / stddev
	}

	return scores
}

// MeanStdDev delegates to [stats.MeanStdDev].
func MeanStdDev(values []float64) (mean, stddev float64) {
	return stats.MeanStdDev(values)
}
