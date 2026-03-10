package alg

import (
	"errors"
	"io"
)

// Iterator is a pull-based sequence of T values.
// Next returns (value, nil) for each item and (zero, [io.EOF]) when exhausted.
// Close releases any resources held by the iterator.
type Iterator[T any] interface {
	Next() (T, error)
	Close()
}

// CollectN drains up to limit items from iter into a slice.
// A limit of 0 means unlimited — all items are collected.
// Returns (nil, nil) when the iterator is already exhausted.
// Non-EOF errors are returned immediately with a nil slice.
func CollectN[T any](iter Iterator[T], limit int) ([]T, error) {
	var result []T

	for i := 0; limit == 0 || i < limit; i++ {
		item, err := iter.Next()
		if errors.Is(err, io.EOF) {
			break
		}

		if err != nil {
			return nil, err
		}

		result = append(result, item)
	}

	return result, nil
}
