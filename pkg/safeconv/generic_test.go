package safeconv

import (
	"math"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// MustConvert tests.

func TestMustConvert_UintToInt(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 42, MustConvert[uint, int](42))
	assert.Equal(t, 0, MustConvert[uint, int](0))
	assert.Equal(t, MaxInt, MustConvert[uint, int](uint(MaxInt)))
}

func TestMustConvert_UintToInt_Overflow(t *testing.T) {
	t.Parallel()

	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[uint, int](uint(MaxInt) + 1)
	})
}

func TestMustConvert_IntToUint(t *testing.T) {
	t.Parallel()

	assert.Equal(t, uint(42), MustConvert[int, uint](42))
	assert.Equal(t, uint(0), MustConvert[int, uint](0))
}

func TestMustConvert_IntToUint_Negative(t *testing.T) {
	t.Parallel()

	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[int, uint](-1)
	})
}

func TestMustConvert_IntToUint32(t *testing.T) {
	t.Parallel()

	assert.Equal(t, uint32(42), MustConvert[int, uint32](42))
	assert.Equal(t, uint32(0), MustConvert[int, uint32](0))
	assert.Equal(t, MaxUint32, MustConvert[int, uint32](int(MaxUint32)))
}

func TestMustConvert_IntToUint32_Overflow(t *testing.T) {
	t.Parallel()

	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[int, uint32](int(MaxUint32) + 1)
	})
}

func TestMustConvert_IntToUint32_Negative(t *testing.T) {
	t.Parallel()

	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[int, uint32](-1)
	})
}

func TestMustConvert_Int64ToInt8(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int8(42), MustConvert[int64, int8](42))
	assert.Equal(t, int8(math.MaxInt8), MustConvert[int64, int8](math.MaxInt8))
	assert.Equal(t, int8(math.MinInt8), MustConvert[int64, int8](math.MinInt8))
}

func TestMustConvert_Int64ToInt8_Overflow(t *testing.T) {
	t.Parallel()

	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[int64, int8](math.MaxInt8 + 1)
	})
	assert.PanicsWithValue(t, panicOverflow, func() {
		MustConvert[int64, int8](math.MinInt8 - 1)
	})
}

func TestMustConvert_SameType(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int(42), MustConvert[int, int](42))
	assert.Equal(t, uint(99), MustConvert[uint, uint](99))
}

// SafeConvert tests.

func TestSafeConvert_Uint64ToInt64(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    uint64
		expected int64
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "normal", input: 42, expected: 42},
		{name: "max_int64", input: uint64(math.MaxInt64), expected: math.MaxInt64},
		{name: "overflow_clamps", input: math.MaxUint64, expected: math.MaxInt64},
		{name: "just_above_max", input: uint64(math.MaxInt64) + 1, expected: math.MaxInt64},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeConvert[uint64, int64](tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSafeConvert_Uint64ToInt(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    uint64
		expected int
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "normal", input: 42, expected: 42},
		{name: "max_int", input: uint64(MaxInt), expected: MaxInt},
		{name: "overflow_clamps", input: math.MaxUint64, expected: MaxInt},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeConvert[uint64, int](tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSafeConvert_IntToUint(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    int
		expected uint
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "positive", input: 42, expected: 42},
		{name: "negative_clamps_to_zero", input: -1, expected: 0},
		{name: "min_int_clamps_to_zero", input: math.MinInt, expected: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeConvert[int, uint](tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSafeConvert_Int64ToInt8(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    int64
		expected int8
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "fits", input: 42, expected: 42},
		{name: "max_int8", input: math.MaxInt8, expected: math.MaxInt8},
		{name: "min_int8", input: math.MinInt8, expected: math.MinInt8},
		{name: "above_max_clamps", input: math.MaxInt8 + 1, expected: math.MaxInt8},
		{name: "below_min_clamps", input: math.MinInt8 - 1, expected: math.MinInt8},
		{name: "large_positive", input: math.MaxInt64, expected: math.MaxInt8},
		{name: "large_negative", input: math.MinInt64, expected: math.MinInt8},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeConvert[int64, int8](tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSafeConvert_SameType(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int(42), SafeConvert[int, int](42))
}

// Extract tests.

func TestExtract_DirectTypeMatch(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](42)
	require.True(t, ok)
	assert.Equal(t, 42, got)
}

func TestExtract_StringDirect(t *testing.T) {
	t.Parallel()

	got, ok := Extract[string]("hello")
	require.True(t, ok)
	assert.Equal(t, "hello", got)
}

func TestExtract_NumericCoercion_IntFromInt64(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](int64(99))
	require.True(t, ok)
	assert.Equal(t, 99, got)
}

func TestExtract_NumericCoercion_IntFromInt32(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](int32(100))
	require.True(t, ok)
	assert.Equal(t, 100, got)
}

func TestExtract_NumericCoercion_IntFromFloat64(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](float64(3.14))
	require.True(t, ok)
	assert.Equal(t, 3, got) // Truncation, same as Go conversion.
}

func TestExtract_NumericCoercion_Float64FromInt(t *testing.T) {
	t.Parallel()

	got, ok := Extract[float64](42)
	require.True(t, ok)
	assert.InDelta(t, 42.0, got, 0.001)
}

func TestExtract_NumericCoercion_Float64FromInt64(t *testing.T) {
	t.Parallel()

	got, ok := Extract[float64](int64(999))
	require.True(t, ok)
	assert.InDelta(t, 999.0, got, 0.001)
}

func TestExtract_UnsupportedType(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int]("not a number")
	assert.False(t, ok)
	assert.Equal(t, 0, got)
}

func TestExtract_Nil(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](nil)
	assert.False(t, ok)
	assert.Equal(t, 0, got)
}

func TestExtract_BoolToInt_Fails(t *testing.T) {
	t.Parallel()

	got, ok := Extract[int](true)
	assert.False(t, ok)
	assert.Equal(t, 0, got)
}

func TestExtract_NumericCoercion_UintFromInt(t *testing.T) {
	t.Parallel()

	got, ok := Extract[uint](42)
	require.True(t, ok)
	assert.Equal(t, uint(42), got)
}

func TestExtract_Float32FromFloat64(t *testing.T) {
	t.Parallel()

	got, ok := Extract[float32](float64(1.5))
	require.True(t, ok)
	assert.InDelta(t, float32(1.5), got, 0.001)
}
