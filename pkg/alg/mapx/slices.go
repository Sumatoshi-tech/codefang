package mapx

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
