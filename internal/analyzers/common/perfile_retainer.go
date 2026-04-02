// FRD: specs/frds/FRD-20260327-perfile-retainer.md.

package common

import (
	"maps"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// PerFileRetainer stores per-file report snapshots during static analysis aggregation.
// When enabled, each call to Retain stores a shallow clone of the report keyed by source file path.
// When disabled (default), Retain is a no-op and PerFileResults returns nil.
type PerFileRetainer struct {
	enabled bool
	reports map[string]analyze.Report
}

// SetPerFileMode enables or disables per-file report retention.
func (r *PerFileRetainer) SetPerFileMode(enabled bool) {
	r.enabled = enabled

	if enabled && r.reports == nil {
		r.reports = make(map[string]analyze.Report)
	}
}

// Retain extracts the source file path from the report and stores a shallow clone.
// No-op when per-file mode is disabled or the report has no source file path.
func (r *PerFileRetainer) Retain(report analyze.Report) {
	if !r.enabled || report == nil {
		return
	}

	filePath := extractSourceFile(report)
	if filePath == "" {
		return
	}

	r.reports[filePath] = cloneReport(report)
}

// PerFileResults returns the retained per-file reports keyed by file path.
// Returns nil when per-file mode is disabled or no files were retained.
func (r *PerFileRetainer) PerFileResults() map[string]analyze.Report {
	if !r.enabled || len(r.reports) == 0 {
		return nil
	}

	return r.reports
}

// extractSourceFile finds the source file path from report values.
// Checks top-level SourceFileKey first, then collection-level sources.
func extractSourceFile(report analyze.Report) string {
	if sf, ok := report[analyze.SourceFileKey].(string); ok && sf != "" {
		return sf
	}

	return extractSourceFileFromCollections(report)
}

// extractSourceFileFromCollections checks TypedCollection.SourceFile and legacy _source_file items.
func extractSourceFileFromCollections(report analyze.Report) string {
	for _, val := range report {
		if sf := sourceFileFromValue(val); sf != "" {
			return sf
		}
	}

	return ""
}

// sourceFileFromValue extracts a source file path from a single report value.
func sourceFileFromValue(val any) string {
	switch typed := val.(type) {
	case analyze.TypedCollection:
		return typed.SourceFile
	case []map[string]any:
		for _, item := range typed {
			if sf, ok := item[analyze.SourceFileKey].(string); ok && sf != "" {
				return sf
			}
		}
	}

	return ""
}

// cloneReport creates a shallow clone of a report map.
func cloneReport(report analyze.Report) analyze.Report {
	return maps.Clone(report)
}
