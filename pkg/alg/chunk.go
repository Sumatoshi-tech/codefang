// Package alg provides generic algorithm utilities.
package alg

// Range represents a half-open interval [Start, End).
type Range struct {
	Start int // Inclusive index.
	End   int // Exclusive index.
}

// Chunk splits the range [0, total) into chunks of the given size.
// The last chunk may be smaller than size. Returns nil when total or size is non-positive.
func Chunk(total, size int) []Range {
	if total <= 0 || size <= 0 {
		return nil
	}

	n := (total + size - 1) / size
	chunks := make([]Range, 0, n)

	for start := 0; start < total; start += size {
		chunks = append(chunks, Range{Start: start, End: min(start+size, total)})
	}

	return chunks
}
