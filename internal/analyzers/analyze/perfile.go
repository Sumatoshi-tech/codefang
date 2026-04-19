package analyze

import "path/filepath"

// PerFileModeEnabled is implemented by aggregators that support per-file report retention.
// StaticService uses this to enable per-file mode and extract results after analysis.
type PerFileModeEnabled interface {
	SetPerFileMode(enabled bool)
	PerFileResults() map[string]Report
}

// PerFileResults returns per-file reports collected during the last AnalyzeFolder call.
// Returns nil when PerFile is false or no files were analyzed.
// Keyed by analyzer name → file path → per-file report.
func (svc *StaticService) PerFileResults() map[string]map[string]Report {
	return svc.perFileResults
}

// extractPerFileResults collects per-file reports from all aggregators that support it.
func extractPerFileResults(aggregators map[string]ResultAggregator) map[string]map[string]Report {
	result := make(map[string]map[string]Report, len(aggregators))

	for name, agg := range aggregators {
		pfm, ok := agg.(PerFileModeEnabled)
		if !ok {
			continue
		}

		fileReports := pfm.PerFileResults()
		if len(fileReports) > 0 {
			result[name] = fileReports
		}
	}

	if len(result) == 0 {
		return nil
	}

	return result
}

// enrichWithPerFileData takes the base JSON report and injects per-file data into each section.
// It uses the PerFileEnricher interface to avoid import cycles with the renderer package.
// Returns the enriched report (same reference if type assertion succeeds, original otherwise).
func (svc *StaticService) enrichWithPerFileData(report any, _ []ReportSection) any {
	enricher, ok := report.(PerFileEnricher)
	if !ok {
		return report
	}

	enricher.EnrichWithPerFileData(svc.PerFileResults(), svc.analysisRootPath, svc.allFormattable())

	return report
}

// PerFileEnricher is implemented by JSON report types that support per-file data injection.
// The renderer.JSONReport implements this to avoid import cycles.
type PerFileEnricher interface {
	EnrichWithPerFileData(
		perFileResults map[string]map[string]Report,
		rootPath string,
		analyzers []FormattableAnalyzer,
	)
}

// MakeRelativePath converts an absolute file path to be relative to rootPath.
// Returns the original path if it cannot be made relative.
func MakeRelativePath(filePath, rootPath string) string {
	if rootPath == "" {
		return filePath
	}

	rel, err := filepath.Rel(rootPath, filePath)
	if err != nil {
		return filePath
	}

	return rel
}
