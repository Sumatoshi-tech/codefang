package mapx

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// FRD: specs/frds/FRD-20260303-sort-and-limit.md.

func TestSortAndLimit(t *testing.T) {
	t.Parallel()

	descending := func(a, b int) bool { return a > b }

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := SortAndLimit[int](nil, descending, 5)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := SortAndLimit([]int{}, descending, 5)
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("limit_greater_than_length", func(t *testing.T) {
		t.Parallel()

		got := SortAndLimit([]int{3, 1, 2}, descending, 10)
		assert.Equal(t, []int{3, 2, 1}, got)
	})

	t.Run("limit_less_than_length", func(t *testing.T) {
		t.Parallel()

		got := SortAndLimit([]int{5, 1, 4, 2, 3}, descending, 3)
		assert.Equal(t, []int{5, 4, 3}, got)
	})

	t.Run("preserves_original", func(t *testing.T) {
		t.Parallel()

		original := []int{3, 1, 2}
		_ = SortAndLimit(original, descending, 2)
		assert.Equal(t, []int{3, 1, 2}, original)
	})

	t.Run("limit_equal_to_length", func(t *testing.T) {
		t.Parallel()

		got := SortAndLimit([]int{2, 1, 3}, descending, 3)
		assert.Equal(t, []int{3, 2, 1}, got)
	})

	t.Run("limit_zero_returns_all", func(t *testing.T) {
		t.Parallel()

		// limit=0 means "no limit" — returns all items sorted.
		// Used by AllIssues() in report_section.go files.
		got := SortAndLimit([]int{3, 1, 2}, descending, 0)
		assert.Equal(t, []int{3, 2, 1}, got)
	})
}

// FRD: specs/frds/FRD-20260303-build-lookup-set.md.

func TestBuildLookupSet(t *testing.T) {
	t.Parallel()

	t.Run("nil_returns_nil", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet[int](nil)
		assert.Nil(t, got)
	})

	t.Run("empty_returns_empty", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet([]int{})
		assert.NotNil(t, got)
		assert.Empty(t, got)
	})

	t.Run("no_duplicates", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet([]int{1, 2, 3})
		assert.Len(t, got, 3)
		assert.Contains(t, got, 1)
		assert.Contains(t, got, 2)
		assert.Contains(t, got, 3)
	})

	t.Run("with_duplicates", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet([]int{1, 2, 1, 3, 2})
		assert.Len(t, got, 3)
		assert.Contains(t, got, 1)
		assert.Contains(t, got, 2)
		assert.Contains(t, got, 3)
	})

	t.Run("single_element", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet([]int{42})
		assert.Len(t, got, 1)
		assert.Contains(t, got, 42)
	})

	t.Run("string_type", func(t *testing.T) {
		t.Parallel()

		got := BuildLookupSet([]string{"alpha", "beta", "alpha"})
		assert.Len(t, got, 2)
		assert.Contains(t, got, "alpha")
		assert.Contains(t, got, "beta")
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
