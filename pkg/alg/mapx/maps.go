// Package mapx provides generic map operations: clone, merge, and sorted-key extraction.
package mapx

import (
	"cmp"
	stdmaps "maps"
	"slices"
)

// Numeric is the constraint for types that support the += operator.
type Numeric interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
		~float32 | ~float64
}

// Clone returns a shallow copy of m.
// Returns nil for a nil map.
func Clone[K comparable, V any](m map[K]V) map[K]V {
	if m == nil {
		return nil
	}

	clone := make(map[K]V, len(m))
	stdmaps.Copy(clone, m)

	return clone
}

// CloneFunc returns a deep copy of m, applying cloneV to each value.
// Returns nil for a nil map.
func CloneFunc[K comparable, V any](m map[K]V, cloneV func(V) V) map[K]V {
	if m == nil {
		return nil
	}

	clone := make(map[K]V, len(m))

	for k, v := range m {
		clone[k] = cloneV(v)
	}

	return clone
}

// CloneNested returns a deep copy of a two-level nested map.
// Outer and inner maps are independently allocated.
// Returns nil for a nil map. Nil inner maps are preserved as nil.
func CloneNested[K1, K2 comparable, V any](m map[K1]map[K2]V) map[K1]map[K2]V {
	if m == nil {
		return nil
	}

	clone := make(map[K1]map[K2]V, len(m))

	for k1, inner := range m {
		if inner == nil {
			clone[k1] = nil

			continue
		}

		cp := make(map[K2]V, len(inner))
		stdmaps.Copy(cp, inner)
		clone[k1] = cp
	}

	return clone
}

// MergeAdditive additively merges src into dst: dst[k] += src[k] for every key in src.
// If dst is nil, this is a no-op.
func MergeAdditive[K comparable, V Numeric](dst, src map[K]V) {
	if dst == nil {
		return
	}

	for k, v := range src {
		dst[k] += v
	}
}

// SortedKeys returns the keys of m in sorted order.
// Returns nil for a nil map.
func SortedKeys[K cmp.Ordered, V any](m map[K]V) []K {
	if m == nil {
		return nil
	}

	keys := make([]K, 0, len(m))

	for k := range m {
		keys = append(keys, k)
	}

	slices.Sort(keys)

	return keys
}
