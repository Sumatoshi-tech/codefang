package stats

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestClamp(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		val, lo, hi float64
		expected    float64
	}{
		{name: "within_range", val: 5.0, lo: 0.0, hi: 10.0, expected: 5.0},
		{name: "below_min", val: -1.0, lo: 0.0, hi: 10.0, expected: 0.0},
		{name: "above_max", val: 15.0, lo: 0.0, hi: 10.0, expected: 10.0},
		{name: "at_min", val: 0.0, lo: 0.0, hi: 10.0, expected: 0.0},
		{name: "at_max", val: 10.0, lo: 0.0, hi: 10.0, expected: 10.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := Clamp(tt.val, tt.lo, tt.hi)
			assert.InDelta(t, tt.expected, got, 0.0001)
		})
	}
}

func TestClampInt(t *testing.T) {
	t.Parallel()

	got := Clamp(15, 0, 10)
	assert.Equal(t, 10, got)
}

func TestMin(t *testing.T) {
	t.Parallel()

	t.Run("empty_returns_zero", func(t *testing.T) {
		t.Parallel()

		got := Min([]float64{})
		assert.InDelta(t, 0, got, 0.0001)
	})

	t.Run("single_element", func(t *testing.T) {
		t.Parallel()

		got := Min([]float64{7.0})
		assert.InDelta(t, 7.0, got, 0.0001)
	})

	t.Run("multiple_elements", func(t *testing.T) {
		t.Parallel()

		got := Min([]float64{3.0, 1.0, 4.0, 1.5, 9.0})
		assert.InDelta(t, 1.0, got, 0.0001)
	})
}

func TestMax(t *testing.T) {
	t.Parallel()

	t.Run("empty_returns_zero", func(t *testing.T) {
		t.Parallel()

		got := Max([]float64{})
		assert.InDelta(t, 0, got, 0.0001)
	})

	t.Run("multiple_elements", func(t *testing.T) {
		t.Parallel()

		got := Max([]float64{3.0, 1.0, 9.0, 4.0})
		assert.InDelta(t, 9.0, got, 0.0001)
	})
}

func TestSum(t *testing.T) {
	t.Parallel()

	t.Run("empty_returns_zero", func(t *testing.T) {
		t.Parallel()

		got := Sum([]float64{})
		assert.InDelta(t, 0, got, 0.0001)
	})

	t.Run("multiple_elements", func(t *testing.T) {
		t.Parallel()

		got := Sum([]float64{1.0, 2.0, 3.0})
		assert.InDelta(t, 6.0, got, 0.0001)
	})

	t.Run("int_elements", func(t *testing.T) {
		t.Parallel()

		got := Sum([]int{1, 2, 3, 4})
		assert.Equal(t, 10, got)
	})
}

func TestMinInt(t *testing.T) {
	t.Parallel()

	got := Min([]int{3, 1, 4, 1, 5})
	assert.Equal(t, 1, got)
}

func TestMaxInt(t *testing.T) {
	t.Parallel()

	got := Max([]int{3, 1, 4, 1, 5})
	assert.Equal(t, 5, got)
}

func TestPercentile(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    []float64
		p        float64
		expected float64
	}{
		{name: "empty_returns_zero", input: nil, p: PercentileMedian, expected: 0},
		{name: "single_element", input: []float64{7.0}, p: PercentileMedian, expected: 7.0},
		{name: "median_odd", input: []float64{3.0, 1.0, 2.0}, p: PercentileMedian, expected: 2.0},
		{name: "median_even", input: []float64{1.0, 2.0, 3.0, 4.0}, p: PercentileMedian, expected: 2.5},
		{name: "p95_of_100", input: makeSequence(100), p: PercentileP95, expected: 95.05},
		{name: "p0_is_min", input: []float64{5.0, 1.0, 9.0}, p: 0, expected: 1.0},
		{name: "p100_is_max", input: []float64{5.0, 1.0, 9.0}, p: 1.0, expected: 9.0},
		{name: "unsorted_input", input: []float64{9.0, 1.0, 5.0, 3.0, 7.0}, p: PercentileMedian, expected: 5.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := Percentile(tt.input, tt.p)
			assert.InDelta(t, tt.expected, got, 0.1)
		})
	}
}

func TestMedian(t *testing.T) {
	t.Parallel()

	got := Median([]float64{3.0, 1.0, 2.0})
	assert.InDelta(t, 2.0, got, 0.0001)
}

// makeSequence returns [1.0, 2.0, ..., n].
func makeSequence(n int) []float64 {
	result := make([]float64, n)

	for i := range result {
		result[i] = float64(i + 1)
	}

	return result
}

func TestMeanStdDev(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		input      []float64
		wantMean   float64
		wantStdDev float64
	}{
		{name: "empty_returns_zeros", input: nil, wantMean: 0, wantStdDev: 0},
		{name: "single_element_zero_stddev", input: []float64{5.0}, wantMean: 5.0, wantStdDev: 0},
		{name: "uniform_values_zero_stddev", input: []float64{3.0, 3.0, 3.0}, wantMean: 3.0, wantStdDev: 0},
		{name: "known_population_stddev", input: []float64{2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0}, wantMean: 5.0, wantStdDev: 2.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			mean, stddev := MeanStdDev(tt.input)
			assert.InDelta(t, tt.wantMean, mean, 0.0001)
			assert.InDelta(t, tt.wantStdDev, stddev, 0.0001)
		})
	}
}

func TestMean(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    []float64
		expected float64
	}{
		{name: "empty_returns_zero", input: nil, expected: 0},
		{name: "single_element", input: []float64{5.0}, expected: 5.0},
		{name: "two_elements", input: []float64{2.0, 4.0}, expected: 3.0},
		{name: "known_mean", input: []float64{1.0, 2.0, 3.0, 4.0, 5.0}, expected: 3.0},
		{name: "negative_values", input: []float64{-2.0, -4.0}, expected: -3.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := Mean(tt.input)
			assert.InDelta(t, tt.expected, got, 0.0001)
		})
	}
}
