// Package stats provides core statistical functions for numerical analysis.
// All standard deviation calculations use population stddev (÷n, not ÷(n−1)).
package stats

import (
	"cmp"
	"math"
	"slices"
)

// Mean returns the arithmetic mean of values.
// Returns 0 for an empty slice.
func Mean(values []float64) float64 {
	if len(values) == 0 {
		return 0
	}

	var sum float64

	for _, v := range values {
		sum += v
	}

	return sum / float64(len(values))
}

// MeanStdDev returns the arithmetic mean and population standard deviation.
// Returns (0, 0) for an empty slice.
func MeanStdDev(values []float64) (mean, stddev float64) {
	count := len(values)
	if count == 0 {
		return 0, 0
	}

	mean = Mean(values)

	var sumSq float64

	for _, v := range values {
		diff := v - mean
		sumSq += diff * diff
	}

	return mean, math.Sqrt(sumSq / float64(count))
}

// Well-known percentile thresholds.
const (
	PercentileMedian = 0.5
	PercentileP95    = 0.95
)

// Percentile returns the p-th percentile of values using linear interpolation.
// p must be in [0, 1]. The input slice is not modified (a copy is sorted internally).
// Returns 0 for an empty slice.
func Percentile(values []float64, p float64) float64 {
	count := len(values)
	if count == 0 {
		return 0
	}

	sorted := make([]float64, count)
	copy(sorted, values)
	slices.Sort(sorted)

	idx := p * float64(count-1)
	lower := int(math.Floor(idx))
	upper := int(math.Ceil(idx))

	if lower == upper || upper >= count {
		return sorted[lower]
	}

	frac := idx - float64(lower)

	return sorted[lower]*(1-frac) + sorted[upper]*frac
}

// Median returns the 50th percentile of values.
// Returns 0 for an empty slice.
func Median(values []float64) float64 {
	return Percentile(values, PercentileMedian)
}

// ZScoreMaxSentinel is the cap for z-score when stddev is zero but value differs from mean.
const ZScoreMaxSentinel = 100.0

// Clamp restricts val to the range [lo, hi].
func Clamp[T cmp.Ordered](val, lo, hi T) T {
	return max(lo, min(val, hi))
}

// Min returns the smallest element in values.
// Returns the zero value of T for an empty slice.
func Min[T cmp.Ordered](values []T) T {
	if len(values) == 0 {
		var zero T

		return zero
	}

	result := values[0]

	for _, v := range values[1:] {
		if v < result {
			result = v
		}
	}

	return result
}

// Max returns the largest element in values.
// Returns the zero value of T for an empty slice.
func Max[T cmp.Ordered](values []T) T {
	if len(values) == 0 {
		var zero T

		return zero
	}

	result := values[0]

	for _, v := range values[1:] {
		if v > result {
			result = v
		}
	}

	return result
}

// Sum returns the sum of all elements in values.
// Returns the zero value of T for an empty slice.
func Sum[T cmp.Ordered](values []T) T {
	var result T

	for _, v := range values {
		result += v
	}

	return result
}
