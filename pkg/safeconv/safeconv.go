// Package safeconv provides safe integer type conversion functions that panic on overflow.
package safeconv

import "math"

// MaxInt is the maximum value for int type (platform-dependent).
const MaxInt = int(^uint(0) >> 1)

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
