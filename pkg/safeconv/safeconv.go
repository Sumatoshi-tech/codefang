// Package safeconv provides safe type conversion functions.
// Must* variants panic on overflow, Safe* variants clamp, To* variants extract typed values.
package safeconv

import "math"

// MaxInt is the maximum value for int type (platform-dependent).
const MaxInt = int(^uint(0) >> 1)

// MaxInt64 is the maximum value for int64 type.
const MaxInt64 = int64(math.MaxInt64)

// MaxUint32 is the maximum value for uint32 type.
const MaxUint32 = uint32(math.MaxUint32)

// MustUintToInt converts uint to int, panics on overflow.
// Use only when overflow is logically impossible.
func MustUintToInt(v uint) int {
	if v > uint(MaxInt) {
		panic("safeconv: uint to int overflow")
	}

	return int(v)
}

// MustIntToUint converts int to uint, panics if negative.
// Use only when negative values are logically impossible.
func MustIntToUint(v int) uint {
	if v < 0 {
		panic("safeconv: negative int to uint conversion")
	}

	return uint(v)
}

// MustIntToUint32 converts int to uint32, panics on bounds violation.
// Use only when bounds violations are logically impossible.
func MustIntToUint32(v int) uint32 {
	if v < 0 || v > int(MaxUint32) {
		panic("safeconv: int to uint32 out of bounds")
	}

	return uint32(v)
}

// SafeInt64 converts uint64 to int64, clamping to [MaxInt64] on overflow.
func SafeInt64(v uint64) int64 {
	if v > uint64(MaxInt64) {
		return MaxInt64
	}

	return int64(v)
}

// SafeInt converts uint64 to int, clamping to [MaxInt] on overflow.
func SafeInt(v uint64) int {
	if v > uint64(MaxInt) {
		return MaxInt
	}

	return int(v)
}

// ToInt extracts an int from an any value via type switch.
// Supports int, int32, int64, and float64 source types.
// Returns (0, false) for unsupported types.
func ToInt(value any) (int, bool) {
	switch typedVal := value.(type) {
	case int:
		return typedVal, true
	case int32:
		return int(typedVal), true
	case int64:
		return int(typedVal), true
	case float64:
		return int(typedVal), true
	default:
		return 0, false
	}
}

// ToFloat64 extracts a float64 from an any value via type switch.
// Supports float64, int, int32, and int64 source types.
// Returns (0, false) for unsupported types.
func ToFloat64(value any) (float64, bool) {
	switch typedVal := value.(type) {
	case float64:
		return typedVal, true
	case int:
		return float64(typedVal), true
	case int32:
		return float64(typedVal), true
	case int64:
		return float64(typedVal), true
	default:
		return 0, false
	}
}
