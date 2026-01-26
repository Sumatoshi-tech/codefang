// Package mathutil provides generic integer math helper functions.
package mathutil

// Min calculates the minimum of two 32-bit integers.
func Min(a, b int) int {
	if a < b {
		return a
	}

	return b
}

// Max calculates the maximum of two 32-bit integers.
func Max(a, b int) int {
	if a < b {
		return b
	}

	return a
}
