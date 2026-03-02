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
	if v, ok := report[key]; ok {
		if s, isStr := v.(string); isStr {
			return s
		}
	}

	return ""
}

// GetFunctions returns the []map[string]any for the given key.
func GetFunctions(report map[string]any, key string) []map[string]any {
	if v, ok := report[key]; ok {
		if fns, isFns := v.([]map[string]any); isFns {
			return fns
		}
	}

	return nil
}

// GetStringSlice returns a []string value from the report.
func GetStringSlice(report map[string]any, key string) []string {
	if v, ok := report[key]; ok {
		if s, isSlice := v.([]string); isSlice {
			return s
		}
	}

	return nil
}

// GetStringIntMap returns a map[string]int value from the report.
func GetStringIntMap(report map[string]any, key string) map[string]int {
	if v, ok := report[key]; ok {
		if m, isMap := v.(map[string]int); isMap {
			return m
		}
	}

	return nil
}

// MapString returns a string from a map[string]any.
func MapString(m map[string]any, key string) string {
	if v, ok := m[key]; ok {
		if s, isStr := v.(string); isStr {
			return s
		}
	}

	return ""
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
