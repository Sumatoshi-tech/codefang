// Package reportutil provides type-safe accessors for analyze.Report fields.
package reportutil

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Formatting constants.
const (
	PercentMultiplier = 100
)

// GetFloat64 returns a float64 value from the report, handling int conversion.
func GetFloat64(report analyze.Report, key string) float64 {
	if v, ok := report[key]; ok {
		switch val := v.(type) {
		case float64:
			return val
		case int:
			return float64(val)
		}
	}

	return 0
}

// GetInt returns an int value from the report, handling float64 conversion.
func GetInt(report analyze.Report, key string) int {
	if v, ok := report[key]; ok {
		switch val := v.(type) {
		case int:
			return val
		case float64:
			return int(val)
		}
	}

	return 0
}

// GetString returns a string value from the report.
func GetString(report analyze.Report, key string) string {
	if v, ok := report[key]; ok {
		if s, isStr := v.(string); isStr {
			return s
		}
	}

	return ""
}

// GetFunctions returns the []map[string]any for the given key.
func GetFunctions(report analyze.Report, key string) []map[string]any {
	if v, ok := report[key]; ok {
		if fns, isFns := v.([]map[string]any); isFns {
			return fns
		}
	}

	return nil
}

// GetStringSlice returns a []string value from the report.
func GetStringSlice(report analyze.Report, key string) []string {
	if v, ok := report[key]; ok {
		if s, isSlice := v.([]string); isSlice {
			return s
		}
	}

	return nil
}

// GetStringIntMap returns a map[string]int value from the report.
func GetStringIntMap(report analyze.Report, key string) map[string]int {
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

// MapInt returns an int from a map[string]any, handling float64 conversion.
func MapInt(m map[string]any, key string) int {
	if v, ok := m[key]; ok {
		switch val := v.(type) {
		case int:
			return val
		case float64:
			return int(val)
		}
	}

	return 0
}

// MapFloat64 returns a float64 from a map[string]any, handling int conversion.
func MapFloat64(m map[string]any, key string) float64 {
	if v, ok := m[key]; ok {
		switch val := v.(type) {
		case float64:
			return val
		case int:
			return float64(val)
		}
	}

	return 0
}

// FormatInt formats an int as a string.
func FormatInt(v int) string {
	return fmt.Sprintf("%d", v) //nolint:perfsprint // fmt.Sprintf is clearer than string concat.
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
