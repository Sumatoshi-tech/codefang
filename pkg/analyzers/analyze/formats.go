package analyze

import (
	"errors"
	"fmt"
	"slices"
	"strings"
)

const (
	// FormatBinAlias is a short CLI alias for binary output.
	FormatBinAlias = "bin"

	// FormatText is the human-readable output format for CLI display.
	FormatText = "text"

	// FormatCompact is the single-line-per-analyzer static analysis output format.
	FormatCompact = "compact"

	// FormatTimeSeries is the unified time-series output format that merges
	// all history analyzer data into a single JSON array keyed by commit.
	FormatTimeSeries = "timeseries"

	// FormatNDJSON is the streaming output format that writes one JSON line
	// per TC as commits are processed. No aggregator, no buffering.
	FormatNDJSON = "ndjson"
)

var (
	// ErrUnsupportedFormat indicates the requested output format is not supported.
	ErrUnsupportedFormat = errors.New("unsupported format")
)

// NormalizeFormat canonicalizes a user-provided output format string.
func NormalizeFormat(format string) string {
	normalized := strings.ToLower(strings.TrimSpace(format))
	if normalized == FormatBinAlias {
		return FormatBinary
	}

	return normalized
}

// UniversalFormats returns the canonical output formats supported by all analyzers.
func UniversalFormats() []string {
	return []string{FormatJSON, FormatYAML, FormatPlot, FormatBinary, FormatTimeSeries, FormatNDJSON, FormatText}
}

// ValidateFormat checks whether a format is in the provided support list.
func ValidateFormat(format string, supported []string) (string, error) {
	normalized := NormalizeFormat(format)
	for _, candidate := range supported {
		if normalized == NormalizeFormat(candidate) {
			return normalized, nil
		}
	}

	return "", fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
}

// ValidateUniversalFormat checks whether a format belongs to the universal contract.
func ValidateUniversalFormat(format string) (string, error) {
	normalized := NormalizeFormat(format)
	if slices.Contains(UniversalFormats(), normalized) {
		return normalized, nil
	}

	return "", fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
}
