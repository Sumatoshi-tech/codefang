package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestClassifier_EmptyThresholds(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{}, "default")

	assert.Equal(t, "default", c.Classify(0))
	assert.Equal(t, "default", c.Classify(100))
}

func TestClassifier_SingleThreshold_AboveLimit(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{{Limit: 10, Label: "high"}}, "low")

	assert.Equal(t, "high", c.Classify(15))
}

func TestClassifier_SingleThreshold_ExactMatch(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{{Limit: 10, Label: "high"}}, "low")

	assert.Equal(t, "high", c.Classify(10))
}

func TestClassifier_SingleThreshold_BelowLimit(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{{Limit: 10, Label: "high"}}, "low")

	assert.Equal(t, "low", c.Classify(9))
}

func TestClassifier_MultipleThresholds_HighestMatch(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[float64]{
		{Limit: 1.0, Label: "Type-1"},
		{Limit: 0.8, Label: "Type-2"},
		{Limit: 0.5, Label: "Type-3"},
	}, "Type-4")

	assert.Equal(t, "Type-1", c.Classify(1.0))
}

func TestClassifier_MultipleThresholds_MiddleMatch(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[float64]{
		{Limit: 1.0, Label: "Type-1"},
		{Limit: 0.8, Label: "Type-2"},
		{Limit: 0.5, Label: "Type-3"},
	}, "Type-4")

	assert.Equal(t, "Type-2", c.Classify(0.9))
}

func TestClassifier_MultipleThresholds_LowestMatch(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[float64]{
		{Limit: 1.0, Label: "Type-1"},
		{Limit: 0.8, Label: "Type-2"},
		{Limit: 0.5, Label: "Type-3"},
	}, "Type-4")

	assert.Equal(t, "Type-3", c.Classify(0.5))
}

func TestClassifier_MultipleThresholds_BelowAll(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[float64]{
		{Limit: 1.0, Label: "Type-1"},
		{Limit: 0.8, Label: "Type-2"},
		{Limit: 0.5, Label: "Type-3"},
	}, "Type-4")

	assert.Equal(t, "Type-4", c.Classify(0.3))
}

func TestClassifier_UnsortedInput(t *testing.T) {
	t.Parallel()

	// Thresholds provided in ascending order — constructor should sort descending.
	c := NewClassifier([]Threshold[int]{
		{Limit: 5, Label: "low"},
		{Limit: 20, Label: "high"},
		{Limit: 10, Label: "medium"},
	}, "minimal")

	assert.Equal(t, "high", c.Classify(25))
	assert.Equal(t, "medium", c.Classify(15))
	assert.Equal(t, "low", c.Classify(7))
	assert.Equal(t, "minimal", c.Classify(3))
}

func TestClassifier_IntegerThresholds(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{
		{Limit: 20, Label: "HIGH"},
		{Limit: 10, Label: "MEDIUM"},
	}, "LOW")

	assert.Equal(t, "HIGH", c.Classify(20))
	assert.Equal(t, "MEDIUM", c.Classify(10))
	assert.Equal(t, "LOW", c.Classify(9))
}

func TestClassifier_NegativeValues(t *testing.T) {
	t.Parallel()

	c := NewClassifier([]Threshold[int]{
		{Limit: 0, Label: "non-negative"},
		{Limit: -10, Label: "mild-negative"},
	}, "deep-negative")

	assert.Equal(t, "non-negative", c.Classify(5))
	assert.Equal(t, "non-negative", c.Classify(0))
	assert.Equal(t, "mild-negative", c.Classify(-5))
	assert.Equal(t, "deep-negative", c.Classify(-20))
}

func TestClassifier_DoesNotMutateInput(t *testing.T) {
	t.Parallel()

	thresholds := []Threshold[int]{
		{Limit: 5, Label: "low"},
		{Limit: 20, Label: "high"},
		{Limit: 10, Label: "medium"},
	}

	_ = NewClassifier(thresholds, "default")

	// Original slice should be unchanged.
	assert.Equal(t, 5, thresholds[0].Limit)
	assert.Equal(t, 20, thresholds[1].Limit)
	assert.Equal(t, 10, thresholds[2].Limit)
}
