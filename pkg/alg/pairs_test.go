package alg_test

// FRD: specs/frds/FRD-20260302-chunk-pairs.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/alg"
)

func TestForEachPair_ZeroElements(t *testing.T) {
	t.Parallel()

	count := 0

	alg.ForEachPair(0, func(_, _ int) { count++ })
	assert.Equal(t, 0, count)
}

func TestForEachPair_OneElement(t *testing.T) {
	t.Parallel()

	count := 0

	alg.ForEachPair(1, func(_, _ int) { count++ })
	assert.Equal(t, 0, count)
}

func TestForEachPair_TwoElements(t *testing.T) {
	t.Parallel()

	var pairs [][2]int

	alg.ForEachPair(2, func(i, j int) {
		pairs = append(pairs, [2]int{i, j})
	})

	expected := [][2]int{{0, 1}}
	assert.Equal(t, expected, pairs)
}

func TestForEachPair_ThreeElements(t *testing.T) {
	t.Parallel()

	var pairs [][2]int

	alg.ForEachPair(3, func(i, j int) {
		pairs = append(pairs, [2]int{i, j})
	})

	expected := [][2]int{{0, 1}, {0, 2}, {1, 2}}
	assert.Equal(t, expected, pairs)
}

func TestForEachPair_FiveElements_Count(t *testing.T) {
	t.Parallel()

	const n = 5

	const expectedPairs = n * (n - 1) / 2

	count := 0

	alg.ForEachPair(n, func(_, _ int) { count++ })
	assert.Equal(t, expectedPairs, count)
}

func TestForEachPair_OrderingInvariant(t *testing.T) {
	t.Parallel()

	const n = 4

	alg.ForEachPair(n, func(i, j int) {
		assert.Less(t, i, j, "i must be less than j")
		assert.GreaterOrEqual(t, i, 0, "i must be non-negative")
		assert.Less(t, j, n, "j must be less than n")
	})
}

func TestForEachPair_NegativeN(t *testing.T) {
	t.Parallel()

	count := 0

	alg.ForEachPair(-1, func(_, _ int) { count++ })
	assert.Equal(t, 0, count)
}
