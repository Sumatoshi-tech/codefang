package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// thresholdLabelerFixture returns a standard 4-bucket labeler for tests.
func thresholdLabelerFixture() ThresholdLabeler {
	return ThresholdLabeler{
		{Limit: 0.8, Label: "Excellent"},
		{Limit: 0.6, Label: "Good"},
		{Limit: 0.4, Label: "Fair"},
		{Limit: 0.0, Label: "Poor"},
	}
}

func TestThresholdLabeler_Label_NilSlice(t *testing.T) {
	t.Parallel()

	var l ThresholdLabeler
	assert.Empty(t, l.Label(0.5))
}

func TestThresholdLabeler_Label_EmptySlice(t *testing.T) {
	t.Parallel()

	assert.Empty(t, ThresholdLabeler{}.Label(0.5))
}

func TestThresholdLabeler_Label_NoMatch(t *testing.T) {
	t.Parallel()

	// Score below all Limit values → no match → "".
	l := ThresholdLabeler{
		{Limit: 0.8, Label: "Excellent"},
		{Limit: 0.6, Label: "Good"},
	}
	assert.Empty(t, l.Label(0.5))
}

func TestThresholdLabeler_Label_FirstMatchWins(t *testing.T) {
	t.Parallel()

	l := thresholdLabelerFixture()

	tests := []struct {
		name  string
		score float64
		want  string
	}{
		{"at_top_threshold", 0.8, "Excellent"},
		{"above_top_threshold", 1.0, "Excellent"},
		{"between_first_and_second", 0.7, "Good"},
		{"at_second_threshold", 0.6, "Good"},
		{"at_third_threshold", 0.4, "Fair"},
		{"at_fallback_zero", 0.0, "Poor"},
		{"below_fallback", -0.1, ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.want, l.Label(tt.score))
		})
	}
}

func TestThresholdLabeler_Label_SingleThreshold(t *testing.T) {
	t.Parallel()

	l := ThresholdLabeler{{Limit: 0.5, Label: "Pass"}}

	assert.Equal(t, "Pass", l.Label(0.5))
	assert.Equal(t, "Pass", l.Label(1.0))
	assert.Empty(t, l.Label(0.49))
}

func TestThresholdLabeler_Label_LargeValues(t *testing.T) {
	t.Parallel()

	// Models halstead volume thresholds (large floats, higher = worse).
	l := ThresholdLabeler{
		{Limit: 5000, Label: "Very High"},
		{Limit: 1000, Label: "High"},
		{Limit: 100, Label: "Moderate"},
		{Limit: 0, Label: "Low"},
	}

	tests := []struct {
		name   string
		volume float64
		want   string
	}{
		{"above_very_high", 6000, "Very High"},
		{"at_very_high", 5000, "Very High"},
		{"just_below_very_high", 4999, "High"},
		{"at_high", 1000, "High"},
		{"at_moderate", 100, "Moderate"},
		{"below_moderate", 50, "Low"},
		{"zero", 0, "Low"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.want, l.Label(tt.volume))
		})
	}
}

func TestThresholdLabeler_Label_ExactBoundary(t *testing.T) {
	t.Parallel()

	// score >= Limit is inclusive; verify boundary semantics precisely.
	l := ThresholdLabeler{
		{Limit: 0.7, Label: "A"},
		{Limit: 0.4, Label: "B"},
		{Limit: 0.0, Label: "C"},
	}

	// Exactly at threshold: match the threshold's label.
	assert.Equal(t, "A", l.Label(0.7))
	assert.Equal(t, "B", l.Label(0.4))
	assert.Equal(t, "C", l.Label(0.0))

	// Just below threshold: falls to next bucket.
	assert.Equal(t, "B", l.Label(0.69))
	assert.Equal(t, "C", l.Label(0.39))
}
