package mapx

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestCloneSlice(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := CloneSlice[int](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := CloneSlice([]int{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("shallow_copy_independence", func(t *testing.T) {
		t.Parallel()

		src := []int{1, 2, 3}
		got := CloneSlice(src)
		assert.Equal(t, src, got)

		// Mutation independence.
		got[0] = 99

		assert.Equal(t, 1, src[0])
	})

	t.Run("string_slice", func(t *testing.T) {
		t.Parallel()

		src := []string{"a", "b", "c"}
		got := CloneSlice(src)
		assert.Equal(t, src, got)
	})
}

func TestUnique(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := Unique[int](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := Unique([]int{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("no_duplicates_unchanged", func(t *testing.T) {
		t.Parallel()

		got := Unique([]int{1, 2, 3})
		assert.Equal(t, []int{1, 2, 3}, got)
	})

	t.Run("removes_duplicates_preserves_order", func(t *testing.T) {
		t.Parallel()

		got := Unique([]int{3, 1, 2, 1, 3, 4, 2})
		assert.Equal(t, []int{3, 1, 2, 4}, got)
	})

	t.Run("all_same", func(t *testing.T) {
		t.Parallel()

		got := Unique([]string{"a", "a", "a"})
		assert.Equal(t, []string{"a"}, got)
	})

	t.Run("single_element", func(t *testing.T) {
		t.Parallel()

		got := Unique([]int{42})
		assert.Equal(t, []int{42}, got)
	})
}
