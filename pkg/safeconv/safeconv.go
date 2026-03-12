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
// Prefer MustConvert[uint, int] for new code.
func MustUintToInt(v uint) int { return MustConvert[uint, int](v) }

// MustIntToUint converts int to uint, panics if negative.
// Prefer MustConvert[int, uint] for new code.
func MustIntToUint(v int) uint { return MustConvert[int, uint](v) }

// MustIntToUint32 converts int to uint32, panics on bounds violation.
// Prefer MustConvert[int, uint32] for new code.
func MustIntToUint32(v int) uint32 { return MustConvert[int, uint32](v) }

// SafeInt64 converts uint64 to int64, clamping on overflow.
// Prefer SafeConvert[uint64, int64] for new code.
func SafeInt64(v uint64) int64 { return SafeConvert[uint64, int64](v) }

// SafeInt converts uint64 to int, clamping on overflow.
// Prefer SafeConvert[uint64, int] for new code.
func SafeInt(v uint64) int { return SafeConvert[uint64, int](v) }

// ToInt extracts an int from an any value via numeric coercion.
// Returns (0, false) for non-numeric types.
func ToInt(value any) (int, bool) { return Extract[int](value) }

// ToFloat64 extracts a float64 from an any value via numeric coercion.
// Returns (0, false) for non-numeric types.
func ToFloat64(value any) (float64, bool) { return Extract[float64](value) }
