package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

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
		{name: "string", input: "3.14", expected: 0, ok: false},
		{name: "nil", input: nil, expected: 0, ok: false},
		{name: "bool", input: true, expected: 0, ok: false},
		{name: "uint", input: uint(10), expected: 0, ok: false},
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
		{name: "string", input: "42", expected: 0, ok: false},
		{name: "nil", input: nil, expected: 0, ok: false},
		{name: "bool", input: true, expected: 0, ok: false},
		{name: "uint", input: uint(10), expected: 0, ok: false},
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
