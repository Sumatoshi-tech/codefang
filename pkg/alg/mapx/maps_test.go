package mapx

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestClone(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := Clone[string, int](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := Clone(map[string]int{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("shallow_copy", func(t *testing.T) {
		t.Parallel()

		src := map[string]int{"a": 1, "b": 2}
		got := Clone(src)
		assert.Equal(t, src, got)

		// Mutation independence.
		got["c"] = 3

		assert.NotContains(t, src, "c")
	})
}

func TestCloneFunc(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := CloneFunc[string, []int](nil, nil)
		assert.Nil(t, got)
	})

	t.Run("deep_copy_with_custom_cloner", func(t *testing.T) {
		t.Parallel()

		src := map[string][]int{
			"x": {1, 2, 3},
			"y": {4, 5},
		}

		got := CloneFunc(src, func(v []int) []int {
			cp := make([]int, len(v))
			copy(cp, v)

			return cp
		})

		assert.Equal(t, src, got)

		// Inner slice mutation independence.
		got["x"][0] = 99

		assert.Equal(t, 1, src["x"][0])
	})
}

func TestCloneNested(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := CloneNested[string, int, bool](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := CloneNested(map[string]map[int]bool{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("deep_independence", func(t *testing.T) {
		t.Parallel()

		src := map[int]map[int]int64{
			1: {10: 100, 20: 200},
			2: {30: 300},
		}

		got := CloneNested(src)
		assert.Equal(t, src, got)

		// Inner map mutation independence.
		got[1][10] = 999

		assert.Equal(t, int64(100), src[1][10])

		// New key in clone does not appear in source.
		got[1][99] = 1

		assert.NotContains(t, src[1], 99)
	})

	t.Run("nil_inner_maps_preserved", func(t *testing.T) {
		t.Parallel()

		src := map[string]map[string]int{
			"a": nil,
			"b": {"x": 1},
		}

		got := CloneNested(src)
		assert.Nil(t, got["a"])
		assert.Equal(t, map[string]int{"x": 1}, got["b"])
	})
}

func TestMergeAdditive(t *testing.T) {
	t.Parallel()

	t.Run("nil_src_no_op", func(t *testing.T) {
		t.Parallel()

		dst := map[string]int{"a": 1}
		MergeAdditive(dst, nil)
		assert.Equal(t, map[string]int{"a": 1}, dst)
	})

	t.Run("nil_dst_no_panic", func(t *testing.T) {
		t.Parallel()

		assert.NotPanics(t, func() {
			MergeAdditive(nil, map[string]int{"a": 1})
		})
	})

	t.Run("additive_int", func(t *testing.T) {
		t.Parallel()

		dst := map[string]int{"a": 1, "b": 2}
		src := map[string]int{"b": 3, "c": 4}
		MergeAdditive(dst, src)

		assert.Equal(t, 1, dst["a"])
		assert.Equal(t, 5, dst["b"])
		assert.Equal(t, 4, dst["c"])
	})

	t.Run("additive_int64", func(t *testing.T) {
		t.Parallel()

		dst := map[int]int64{1: 100}
		src := map[int]int64{1: 50, 2: 200}
		MergeAdditive(dst, src)

		assert.Equal(t, int64(150), dst[1])
		assert.Equal(t, int64(200), dst[2])
	})

	t.Run("additive_float64", func(t *testing.T) {
		t.Parallel()

		dst := map[string]float64{"x": 1.5}
		src := map[string]float64{"x": 2.5, "y": 3.0}
		MergeAdditive(dst, src)

		assert.InDelta(t, 4.0, dst["x"], 0.0001)
		assert.InDelta(t, 3.0, dst["y"], 0.0001)
	})
}

func TestSortedKeys(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := SortedKeys[int, any](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := SortedKeys(map[int]string{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("int_keys_sorted", func(t *testing.T) {
		t.Parallel()

		m := map[int]string{3: "c", 1: "a", 2: "b"}
		got := SortedKeys(m)
		assert.Equal(t, []int{1, 2, 3}, got)
	})

	t.Run("string_keys_sorted", func(t *testing.T) {
		t.Parallel()

		m := map[string]int{"banana": 2, "apple": 1, "cherry": 3}
		got := SortedKeys(m)
		assert.Equal(t, []string{"apple", "banana", "cherry"}, got)
	})
}
