// Package reportutil provides type-safe accessors for map[string]any fields.
package reportutil

import (
	"fmt"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
)

// Formatting constants.
const (
	PercentMultiplier = 100
)

// GetAs extracts a value of type T from a report map via direct type assertion.
// Returns (zero, false) if the key is absent or the value is not of type T.
// For numeric types requiring cross-type coercion use [GetFloat64] or [GetInt].
func GetAs[T any](report map[string]any, key string) (T, bool) {
	v, ok := report[key]
	if !ok {
		var zero T

		return zero, false
	}

	t, ok := v.(T)

	return t, ok
}

// GetFloat64 returns a float64 value from the report, handling type conversion.
// Delegates to [safeconv.ToFloat64] for consistent type handling.
func GetFloat64(report map[string]any, key string) float64 {
	v, exists := report[key]
	if !exists {
		return 0
	}

	f, valid := safeconv.ToFloat64(v)
	if !valid {
		return 0
	}

	return f
}

// GetInt returns an int value from the report, handling type conversion.
// Delegates to [safeconv.ToInt] for consistent type handling.
func GetInt(report map[string]any, key string) int {
	v, exists := report[key]
	if !exists {
		return 0
	}

	i, valid := safeconv.ToInt(v)
	if !valid {
		return 0
	}

	return i
}

// GetString returns a string value from the report.
func GetString(report map[string]any, key string) string {
	s, _ := GetAs[string](report, key)

	return s
}

// mapSlicer is satisfied by analyze.TypedCollection without importing analyze.
type mapSlicer interface {
	MapSlice() []map[string]any
}

// GetFunctions returns the []map[string]any for the given key.
// Handles both direct []map[string]any and TypedCollection values.
func GetFunctions(report map[string]any, key string) []map[string]any {
	val, exists := report[key]
	if !exists {
		return nil
	}

	if fns, ok := val.([]map[string]any); ok {
		return fns
	}

	if tc, ok := val.(mapSlicer); ok {
		return tc.MapSlice()
	}

	return nil
}

// GetStringSlice returns a []string value from the report.
func GetStringSlice(report map[string]any, key string) []string {
	s, _ := GetAs[[]string](report, key)

	return s
}

// GetStringIntMap returns a map[string]int value from the report.
func GetStringIntMap(report map[string]any, key string) map[string]int {
	m, _ := GetAs[map[string]int](report, key)

	return m
}

// MapString returns a string from a map[string]any.
func MapString(m map[string]any, key string) string {
	s, _ := GetAs[string](m, key)

	return s
}

// FormatInt formats an int as a string.
func FormatInt(v int) string {
	return strconv.Itoa(v)
}

// FormatFloat formats a float64 with 1 decimal place.
func FormatFloat(v float64) string {
	return fmt.Sprintf("%.1f", v)
}

// FormatPercent formats a float64 (0-1) as a percentage string.
func FormatPercent(v float64) string {
	return fmt.Sprintf("%.1f%%", v*PercentMultiplier)
}

// Pct calculates percentage as float64 (0-1).
func Pct(count, total int) float64 {
	if total == 0 {
		return 0
	}

	return float64(count) / float64(total)
}
