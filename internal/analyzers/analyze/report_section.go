package analyze

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"

// Metric represents a key-value metric for display.
type Metric struct {
	Label string // Display label (e.g., "Total Functions").
	Value string // Pre-formatted value (e.g., "156").
}

// DistributionItem represents a category in a distribution chart.
type DistributionItem struct {
	Label   string  // Category label (e.g., "Simple (1-5)").
	Percent float64 // Percentage as 0-1.
	Count   int     // Absolute count.
}

// Severity constants for Issue classification.
const (
	SeverityGood = "good"
	SeverityFair = "fair"
	SeverityPoor = "poor"
	SeverityInfo = "info"
)

// Issue represents a problem or item to highlight.
type Issue struct {
	Name     string // Item name (e.g., function name).
	Location string // File location (e.g., "pkg/foo/bar.go:42").
	Value    string // Metric value (e.g., "12").
	Severity string // "good", "fair", "poor", or "info".
}

// ScoreInfoOnly indicates a section has no score (info only).
const ScoreInfoOnly = -1.0

// ScoreLabelInfo is the label shown for info-only sections.
const ScoreLabelInfo = "Info"

// ReportSection provides a standardized structure for analyzer reports.
// Analyzers implement this to enable unified rendering.
type ReportSection interface {
	// SectionTitle returns the display title (e.g., "COMPLEXITY").
	SectionTitle() string

	// Score returns a 0-1 score, or ScoreInfoOnly for info-only sections.
	Score() float64

	// ScoreLabel returns formatted score (e.g., "8/10" or "Info").
	ScoreLabel() string

	// StatusMessage returns a summary message (e.g., "Good - reasonable complexity").
	StatusMessage() string

	// KeyMetrics returns ordered key metrics for display.
	KeyMetrics() []Metric

	// Distribution returns distribution data for bar charts.
	Distribution() []DistributionItem

	// TopIssues returns the top N issues/items to highlight.
	TopIssues(n int) []Issue

	// AllIssues returns all issues for verbose mode.
	AllIssues() []Issue
}

// ReportSectionProvider can create a ReportSection from report data.
// Analyzers implement this to enable executive summary generation.
type ReportSectionProvider interface {
	CreateReportSection(report Report) ReportSection
}

// BaseReportSection provides default implementations for ReportSection.
// Analyzers can embed this and override specific methods.
type BaseReportSection struct {
	Title      string
	Message    string
	ScoreValue float64
}

// SectionTitle returns the display title.
func (b *BaseReportSection) SectionTitle() string {
	return b.Title
}

// Score returns the score value.
func (b *BaseReportSection) Score() float64 {
	return b.ScoreValue
}

// ScoreLabel returns formatted score or "Info" for info-only sections.
func (b *BaseReportSection) ScoreLabel() string {
	if b.ScoreValue < 0 {
		return ScoreLabelInfo
	}

	return terminal.FormatScore(b.ScoreValue)
}

// StatusMessage returns the summary message.
func (b *BaseReportSection) StatusMessage() string {
	return b.Message
}

// KeyMetrics returns nil by default. Override to provide metrics.
func (b *BaseReportSection) KeyMetrics() []Metric {
	return nil
}

// Distribution returns nil by default. Override to provide distribution data.
func (b *BaseReportSection) Distribution() []DistributionItem {
	return nil
}

// TopIssues returns nil by default. Override to provide top issues.
func (b *BaseReportSection) TopIssues(_ int) []Issue {
	return nil
}

// AllIssues returns nil by default. Override to provide all issues.
func (b *BaseReportSection) AllIssues() []Issue {
	return nil
}
