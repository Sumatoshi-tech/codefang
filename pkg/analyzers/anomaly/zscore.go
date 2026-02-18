// Package anomaly provides temporal anomaly detection over commit history.
// It uses Z-score analysis with a sliding window to detect sudden quality
// degradation in per-tick metrics (files changed, lines added/removed, churn).
package anomaly

import "math"

// zScoreMaxSentinel is the Z-score returned when the standard deviation of the
// window is zero but the current value differs from the mean. This signals a
// definite anomaly against a perfectly stable baseline.
const zScoreMaxSentinel = 100.0

// ComputeZScores computes the Z-score for each value using a trailing sliding
// window of the given size. For index i, the window is values[max(0, i-window):i].
// The Z-score measures how many standard deviations the current value is from
// the window mean. When the standard deviation is zero and the value equals
// the mean, the Z-score is 0. When the standard deviation is zero and the
// value differs from the mean, the Z-score is +/- zScoreMaxSentinel.
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

		mean, stddev := MeanStdDev(windowSlice)

		if stddev == 0 {
			diff := values[i] - mean
			if diff == 0 {
				scores[i] = 0
			} else {
				scores[i] = math.Copysign(zScoreMaxSentinel, diff)
			}

			continue
		}

		scores[i] = (values[i] - mean) / stddev
	}

	return scores
}

// DetectAnomalies returns the indices where the absolute Z-score exceeds
// the given threshold.
func DetectAnomalies(scores []float64, threshold float64) []int {
	var anomalies []int

	for i, score := range scores {
		if math.Abs(score) > threshold {
			anomalies = append(anomalies, i)
		}
	}

	return anomalies
}

// MeanStdDev computes the population mean and standard deviation of the
// given values. Returns (0, 0) for empty input.
func MeanStdDev(values []float64) (mean, stddev float64) {
	count := len(values)
	if count == 0 {
		return 0, 0
	}

	var sum float64

	for _, v := range values {
		sum += v
	}

	mean = sum / float64(count)

	var sumSq float64

	for _, v := range values {
		diff := v - mean
		sumSq += diff * diff
	}

	stddev = math.Sqrt(sumSq / float64(count))

	return mean, stddev
}
