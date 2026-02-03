package halstead

import (
	"encoding/json"
	"fmt"
	"io"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

// Configuration constants for Halstead report formatting.
const (
	// MaxItemsValue is the maximum number of items displayed in Halstead report summaries.
	MaxItemsValue           = 10
	bugsThresholdMedium     = 0.5
	bugsThresholdMin        = 0.1
	difficultyThresholdHigh = 5
	effortThresholdHigh     = 1000
	magic10000              = 10000
	magic15                 = 15
)

// ReportFormatter handles formatting of Halstead analysis reports.
type ReportFormatter struct {
	reporter *common.Reporter
}

// NewReportFormatter creates a new report formatter.
func NewReportFormatter() *ReportFormatter {
	config := common.ReportConfig{
		Format:         "text",
		IncludeDetails: true,
		SortBy:         "volume",
		SortOrder:      "desc",
		MaxItems:       MaxItemsValue,
		MetricKeys:     []string{"volume", "difficulty", "effort", "time_to_program", "delivered_bugs"},
	}

	return &ReportFormatter{
		reporter: common.NewReporter(config),
	}
}

// FormatReport formats the analysis report for display.
func (rf *ReportFormatter) FormatReport(report analyze.Report, w io.Writer) error {
	formatted, err := rf.reporter.GenerateReport(report)
	if err != nil {
		return err
	}

	_, err = fmt.Fprint(w, formatted)
	if err != nil {
		return fmt.Errorf("formatreport: %w", err)
	}

	return nil
}

// FormatReportJSON formats the analysis report as JSON.
func (rf *ReportFormatter) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	jsonData, err := json.MarshalIndent(metrics, "", "  ")
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	_, err = fmt.Fprint(w, string(jsonData))
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	return nil
}

// FormatReportYAML formats the analysis report as YAML.
func (rf *ReportFormatter) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	return nil
}

// GetHalsteadMessage returns a message based on the Halstead metrics.
func (rf *ReportFormatter) GetHalsteadMessage(volume, difficulty, effort float64) string {
	if volume <= 100 && difficulty <= 5 && effort <= 1000 {
		return "Excellent complexity - code is simple and maintainable"
	}

	if volume <= 1000 && difficulty <= 15 && effort <= 10000 {
		return "Good complexity - code is reasonably complex"
	}

	if volume <= 5000 && difficulty <= 30 && effort <= 50000 {
		return "Fair complexity - consider simplifying some functions"
	}

	return "High complexity - code should be refactored for better maintainability"
}

// GetVolumeAssessment returns an assessment with emoji for volume.
func (rf *ReportFormatter) GetVolumeAssessment(volume float64) string {
	if volume <= volumeThresholdHigh {
		return "🟢 Low"
	}

	if volume <= magic1000 {
		return "🟡 Medium"
	}

	return "🔴 High"
}

// GetDifficultyAssessment returns an assessment with emoji for difficulty.
func (rf *ReportFormatter) GetDifficultyAssessment(difficulty float64) string {
	if difficulty <= difficultyThresholdHigh {
		return "🟢 Simple"
	}

	if difficulty <= magic15 {
		return "🟡 Moderate"
	}

	return "🔴 Complex"
}

// GetEffortAssessment returns an assessment with emoji for effort.
func (rf *ReportFormatter) GetEffortAssessment(effort float64) string {
	if effort <= effortThresholdHigh {
		return "🟢 Low"
	}

	if effort <= magic10000 {
		return "🟡 Medium"
	}

	return "🔴 High"
}

// GetBugAssessment returns an assessment with emoji for delivered bugs.
func (rf *ReportFormatter) GetBugAssessment(bugs float64) string {
	if bugs <= bugsThresholdMin {
		return "🟢 Low Risk"
	}

	if bugs <= bugsThresholdMedium {
		return "🟡 Medium Risk"
	}

	return "🔴 High Risk"
}
