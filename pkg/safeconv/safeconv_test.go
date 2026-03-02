package safeconv

// FRD: specs/frds/FRD-20260302-safeconv-expansion.md.

import (
	"math"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestMustUintToInt(t *testing.T) {
	t.Parallel()

	t.Run("normal_value", func(t *testing.T) {
		t.Parallel()

		got := MustUintToInt(42)
		assert.Equal(t, 42, got)
	})

	t.Run("zero", func(t *testing.T) {
		t.Parallel()

		got := MustUintToInt(0)
		assert.Equal(t, 0, got)
	})

	t.Run("max_int", func(t *testing.T) {
		t.Parallel()

		got := MustUintToInt(uint(MaxInt))
		assert.Equal(t, MaxInt, got)
	})

	t.Run("overflow_panics", func(t *testing.T) {
		t.Parallel()

		assert.PanicsWithValue(t, "safeconv: uint to int overflow", func() {
			MustUintToInt(uint(MaxInt) + 1)
		})
	})
}

func TestMustIntToUint(t *testing.T) {
	t.Parallel()

	t.Run("normal_value", func(t *testing.T) {
		t.Parallel()

		got := MustIntToUint(42)
		assert.Equal(t, uint(42), got)
	})

	t.Run("zero", func(t *testing.T) {
		t.Parallel()

		got := MustIntToUint(0)
		assert.Equal(t, uint(0), got)
	})

	t.Run("negative_panics", func(t *testing.T) {
		t.Parallel()

		assert.PanicsWithValue(t, "safeconv: negative int to uint conversion", func() {
			MustIntToUint(-1)
		})
	})
}

func TestMustIntToUint32(t *testing.T) {
	t.Parallel()

	t.Run("normal_value", func(t *testing.T) {
		t.Parallel()

		got := MustIntToUint32(42)
		assert.Equal(t, uint32(42), got)
	})

	t.Run("zero", func(t *testing.T) {
		t.Parallel()

		got := MustIntToUint32(0)
		assert.Equal(t, uint32(0), got)
	})

	t.Run("max_uint32", func(t *testing.T) {
		t.Parallel()

		got := MustIntToUint32(int(MaxUint32))
		assert.Equal(t, MaxUint32, got)
	})

	t.Run("negative_panics", func(t *testing.T) {
		t.Parallel()

		assert.PanicsWithValue(t, "safeconv: int to uint32 out of bounds", func() {
			MustIntToUint32(-1)
		})
	})

	t.Run("overflow_panics", func(t *testing.T) {
		t.Parallel()

		assert.PanicsWithValue(t, "safeconv: int to uint32 out of bounds", func() {
			MustIntToUint32(int(MaxUint32) + 1)
		})
	})
}

func TestToInt(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    any
		expected int
		ok       bool
	}{
		{name: "int", input: int(42), expected: 42, ok: true},
		{name: "int32", input: int32(100), expected: 100, ok: true},
		{name: "int64", input: int64(999), expected: 999, ok: true},
		{name: "float64", input: float64(3.14), expected: 3, ok: true},
		{name: "zero_int", input: int(0), expected: 0, ok: true},
		{name: "negative_float", input: float64(-2.9), expected: -2, ok: true},
		{name: "string_unsupported", input: "42", expected: 0, ok: false},
		{name: "nil_unsupported", input: nil, expected: 0, ok: false},
		{name: "bool_unsupported", input: true, expected: 0, ok: false},
		{name: "uint_unsupported", input: uint(10), expected: 0, ok: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := ToInt(tt.input)
			assert.Equal(t, tt.ok, ok, "ok mismatch")
			assert.Equal(t, tt.expected, got, "value mismatch")
		})
	}
}

func TestToFloat64(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    any
		expected float64
		ok       bool
	}{
		{name: "float64", input: float64(3.14), expected: 3.14, ok: true},
		{name: "int", input: int(42), expected: 42.0, ok: true},
		{name: "int32", input: int32(100), expected: 100.0, ok: true},
		{name: "int64", input: int64(999), expected: 999.0, ok: true},
		{name: "zero_float", input: float64(0), expected: 0.0, ok: true},
		{name: "negative_int", input: int(-5), expected: -5.0, ok: true},
		{name: "string_unsupported", input: "3.14", expected: 0, ok: false},
		{name: "nil_unsupported", input: nil, expected: 0, ok: false},
		{name: "bool_unsupported", input: true, expected: 0, ok: false},
		{name: "uint_unsupported", input: uint(10), expected: 0, ok: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := ToFloat64(tt.input)
			assert.Equal(t, tt.ok, ok, "ok mismatch")
			assert.InDelta(t, tt.expected, got, 0.001, "value mismatch")
		})
	}
}

func TestSafeInt(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    uint64
		expected int
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "normal_value", input: 42, expected: 42},
		{name: "max_int", input: uint64(MaxInt), expected: MaxInt},
		{name: "overflow_clamps", input: math.MaxUint64, expected: MaxInt},
		{name: "just_above_max_int", input: uint64(MaxInt) + 1, expected: MaxInt},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeInt(tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSafeInt64(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    uint64
		expected int64
	}{
		{name: "zero", input: 0, expected: 0},
		{name: "normal_value", input: 42, expected: 42},
		{name: "max_int64", input: uint64(math.MaxInt64), expected: math.MaxInt64},
		{name: "overflow_clamps", input: math.MaxUint64, expected: math.MaxInt64},
		{name: "just_above_max_int64", input: uint64(math.MaxInt64) + 1, expected: math.MaxInt64},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := SafeInt64(tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}
