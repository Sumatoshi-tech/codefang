package common

import "github.com/Sumatoshi-tech/codefang/pkg/safeconv"

// ToFloat64 safely converts an any value to float64.
// Supports float64, int, int32, and int64 source types.
// Delegates to [safeconv.ToFloat64].
func ToFloat64(value any) (float64, bool) {
	return safeconv.ToFloat64(value)
}

// ToInt safely converts an any value to int.
// Supports int, int32, int64, and float64 source types.
// Delegates to [safeconv.ToInt].
func ToInt(value any) (int, bool) {
	return safeconv.ToInt(value)
}
