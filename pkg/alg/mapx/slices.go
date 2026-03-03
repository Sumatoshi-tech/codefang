package mapx

import "sort"

// CloneSlice returns a shallow copy of s.
// Returns nil for a nil slice.
func CloneSlice[T any](s []T) []T {
	if s == nil {
		return nil
	}

	clone := make([]T, len(s))
	copy(clone, s)

	return clone
}

// SortAndLimit copies items, sorts the copy using less, and returns at most limit elements.
// Returns nil for a nil slice. If limit <= 0, returns an empty slice.
func SortAndLimit[T any](items []T, less func(a, b T) bool, limit int) []T {
	if items == nil {
		return nil
	}

	sorted := make([]T, len(items))
	copy(sorted, items)

	sort.Slice(sorted, func(i, j int) bool {
		return less(sorted[i], sorted[j])
	})

	if limit > 0 && len(sorted) > limit {
		sorted = sorted[:limit]
	}

	return sorted
}

// BuildLookupSet converts a slice into a lookup set (map[T]struct{}).
// Duplicate items are silently deduplicated. Returns nil for a nil slice.
func BuildLookupSet[T comparable](items []T) map[T]struct{} {
	if items == nil {
		return nil
	}

	set := make(map[T]struct{}, len(items))

	for _, item := range items {
		set[item] = struct{}{}
	}

	return set
}

// Unique returns a new slice containing only the first occurrence of each element.
// Insertion order is preserved. Returns nil for a nil slice.
func Unique[T comparable](s []T) []T {
	if s == nil {
		return nil
	}

	seen := make(map[T]struct{}, len(s))
	result := make([]T, 0, len(s))

	for _, v := range s {
		if _, ok := seen[v]; ok {
			continue
		}

		seen[v] = struct{}{}
		result = append(result, v)
	}

	return result
}
