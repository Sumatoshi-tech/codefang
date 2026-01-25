package halstead

import (
	"encoding/json"
	"fmt"
	"io"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

// ReportFormatter handles formatting of Halstead analysis reports
type ReportFormatter struct {
	reporter *common.Reporter
}

// NewReportFormatter creates a new report formatter
func NewReportFormatter() *ReportFormatter {
	config := common.ReportConfig{
		Format:         "text",
		IncludeDetails: true,
		SortBy:         "volume",
		SortOrder:      "desc",
		MaxItems:       10,
		MetricKeys:     []string{"volume", "difficulty", "effort", "time_to_program", "delivered_bugs"},
	}

	return &ReportFormatter{
		reporter: common.NewReporter(config),
	}
}

// FormatReport formats the analysis report for display
func (rf *ReportFormatter) FormatReport(report analyze.Report, w io.Writer) error {
	formatted, err := rf.reporter.GenerateReport(report)
	if err != nil {
		return err
	}
	_, err = fmt.Fprint(w, formatted)
	return err
}

// FormatReportJSON formats the analysis report as JSON
func (rf *ReportFormatter) FormatReportJSON(report analyze.Report, w io.Writer) error {
	jsonData, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	_, err = fmt.Fprint(w, string(jsonData))
	return err
}

// GetHalsteadMessage returns a message based on the Halstead metrics
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

// GetVolumeAssessment returns an assessment with emoji for volume
func (rf *ReportFormatter) GetVolumeAssessment(volume float64) string {
	if volume <= 100 {
		return "🟢 Low"
	}
	if volume <= 1000 {
		return "🟡 Medium"
	}
	return "🔴 High"
}

// GetDifficultyAssessment returns an assessment with emoji for difficulty
func (rf *ReportFormatter) GetDifficultyAssessment(difficulty float64) string {
	if difficulty <= 5 {
		return "🟢 Simple"
	}
	if difficulty <= 15 {
		return "🟡 Moderate"
	}
	return "🔴 Complex"
}

// GetEffortAssessment returns an assessment with emoji for effort
func (rf *ReportFormatter) GetEffortAssessment(effort float64) string {
	if effort <= 1000 {
		return "🟢 Low"
	}
	if effort <= 10000 {
		return "🟡 Medium"
	}
	return "🔴 High"
}

// GetBugAssessment returns an assessment with emoji for delivered bugs
func (rf *ReportFormatter) GetBugAssessment(bugs float64) string {
	if bugs <= 0.1 {
		return "🟢 Low Risk"
	}
	if bugs <= 0.5 {
		return "🟡 Medium Risk"
	}
	return "🔴 High Risk"
}
