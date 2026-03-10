package safeconv

import (
	"math"
	"reflect"
)

const panicOverflow = "safeconv: integer conversion overflow"

// Integer constrains types to built-in integer types.
type Integer interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~uintptr
}

// MustConvert converts v from From to To, panicking on overflow or sign loss.
func MustConvert[From, To Integer](v From) To {
	to := To(v)
	if From(to) != v || (v < 0) != (to < 0) {
		panic(panicOverflow)
	}

	return to
}

// SafeConvert converts v from From to To, clamping to the target type's
// range on overflow.
func SafeConvert[From, To Integer](v From) To {
	to := To(v)
	if From(to) == v && (v < 0) == (to < 0) {
		return to
	}

	if v < 0 {
		return minVal[To]()
	}

	return maxVal[To]()
}

// Extract type-asserts v (type any) to T. If the direct assertion fails,
// it attempts numeric coercion via reflect for numeric source and target types.
// Returns (zero, false) for nil, non-numeric coercion, or type mismatch.
func Extract[T any](v any) (T, bool) {
	if t, ok := v.(T); ok {
		return t, true
	}

	return numericCoerce[T](v)
}

// numericCoerce attempts reflect-based numeric conversion from v to T.
func numericCoerce[T any](v any) (T, bool) {
	var zero T

	targetType := reflect.TypeFor[T]()

	sourceVal := reflect.ValueOf(v)
	if !sourceVal.IsValid() {
		return zero, false
	}

	if !isNumericKind(sourceVal.Kind()) || !isNumericKind(targetType.Kind()) {
		return zero, false
	}

	if !sourceVal.CanConvert(targetType) {
		return zero, false
	}

	result, ok := sourceVal.Convert(targetType).Interface().(T)

	return result, ok
}

// isNumericKind returns true for integer and floating-point reflect kinds.
func isNumericKind(k reflect.Kind) bool {
	return k >= reflect.Int && k <= reflect.Float64
}

// maxVal returns the maximum value for integer type T without using unsafe.
func maxVal[T Integer]() T {
	if T(0)-1 < 0 {
		return signedMax[T]()
	}

	return ^T(0)
}

// minVal returns the minimum value for integer type T.
func minVal[T Integer]() T {
	if T(0)-1 < 0 {
		return ^signedMax[T]()
	}

	return 0
}

// signedMax returns the maximum value for a signed integer type T.
// It probes math constants via round-trip conversion to detect the bit width.
func signedMax[T Integer]() T {
	candidates := [4]int64{math.MaxInt64, math.MaxInt32, math.MaxInt16, math.MaxInt8}

	for _, c := range candidates {
		m := T(c)
		if m > 0 && int64(m) == c {
			return m
		}
	}

	return T(math.MaxInt8)
}
