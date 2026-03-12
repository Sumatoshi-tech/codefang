package common

// FilterByInterface returns a new slice containing only those items from the
// input where cast returns (value, true). Preserves input order.
func FilterByInterface[T any, U any](items []T, cast func(T) (U, bool)) []U {
	var result []U

	for _, item := range items {
		if u, ok := cast(item); ok {
			result = append(result, u)
		}
	}

	return result
}
