package common

import (
	"cmp"
	"slices"
)

// Threshold defines a single classification boundary.
// Values >= Limit are assigned the given Label.
type Threshold[T cmp.Ordered] struct {
	Limit T
	Label string
}

// Classifier maps ordered values to string labels using descending thresholds.
// It is safe for concurrent use after construction.
type Classifier[T cmp.Ordered] struct {
	thresholds   []Threshold[T]
	defaultLabel string
}

// NewClassifier creates a classifier from the given thresholds and default label.
// Thresholds are copied and sorted in descending order by Limit.
// The input slice is not modified.
func NewClassifier[T cmp.Ordered](thresholds []Threshold[T], defaultLabel string) Classifier[T] {
	sorted := make([]Threshold[T], len(thresholds))
	copy(sorted, thresholds)

	slices.SortFunc(sorted, func(a, b Threshold[T]) int {
		return cmp.Compare(b.Limit, a.Limit)
	})

	return Classifier[T]{
		thresholds:   sorted,
		defaultLabel: defaultLabel,
	}
}

// Classify returns the label of the first threshold where value >= Limit.
// If no threshold matches, the default label is returned.
func (c Classifier[T]) Classify(value T) string {
	for _, t := range c.thresholds {
		if value >= t.Limit {
			return t.Label
		}
	}

	return c.defaultLabel
}
