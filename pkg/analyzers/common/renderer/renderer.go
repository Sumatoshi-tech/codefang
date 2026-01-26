// Package renderer provides section rendering for analyzer reports.
package renderer

import (
	"fmt"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

const (
	linesValue          = 3
	magic2              = 2
	magic2_1            = 2
	makeArg3            = 3
	separatorWidthValue = 2
)

// SectionRenderer renders ReportSection to formatted terminal output.
type SectionRenderer struct {
	config  terminal.Config
	verbose bool
}

// Compact mode constants.
const (
	CompactBarWidth   = 10
	CompactTitleWidth = 12
)

// NewSectionRenderer creates a renderer with the given configuration.
func NewSectionRenderer(width int, verbose, noColor bool) *SectionRenderer {
	return &SectionRenderer{
		config: terminal.Config{
			Width:   width,
			NoColor: noColor,
		},
		verbose: verbose,
	}
}

// ColorForSeverity maps an issue severity string to a terminal color.
func ColorForSeverity(severity string) terminal.Color {
	switch severity {
	case analyze.SeverityGood:
		return terminal.ColorGreen
	case analyze.SeverityFair:
		return terminal.ColorYellow
	case analyze.SeverityPoor:
		return terminal.ColorRed
	default:
		return terminal.ColorBlue
	}
}

// RenderCompact produces single-line output for narrow terminals.
// Format: "Title        [████████░░] 8/10  Message".
func (r *SectionRenderer) RenderCompact(section analyze.ReportSection) string {
	title := terminal.PadRight(section.SectionTitle(), CompactTitleWidth)
	scoreBar := terminal.FormatScoreBar(section.Score(), CompactBarWidth)
	scoreColor := terminal.ColorForScore(section.Score())
	scoreBar = r.config.Colorize(scoreBar, scoreColor)
	message := section.StatusMessage()

	return fmt.Sprintf("%s %s  %s", title, scoreBar, message)
}

// Render layout constants.
const (
	IndentWidth          = 2
	SummaryPrefix        = "Summary: "
	MetricsLabel         = "Key Metrics"
	MetricsPerRow        = 2
	MetricLabelWidth     = 20
	MetricValueWidth     = 12
	DistributionLabel    = "Distribution"
	DistributionBarWidth = 40
	DistLabelWidth       = 18
	IssuesLabel          = "Top Issues"
	AllIssuesLabel       = "All Issues"
	DefaultTopIssues     = 5
	IssueNameWidth       = 25
	IssueLocationWidth   = 35
)

// Render produces formatted output for a ReportSection.
func (r *SectionRenderer) Render(section analyze.ReportSection) string {
	var parts []string

	// Header with title and score.
	title := r.config.Colorize(section.SectionTitle(), terminal.ColorBlue)
	scoreText := "Score: " + section.ScoreLabel()
	scoreColor := terminal.ColorForScore(section.Score())
	scoreText = r.config.Colorize(scoreText, scoreColor)
	header := terminal.DrawHeader(title, scoreText, r.config.Width)
	parts = append(parts, header)

	// Summary line.
	indent := strings.Repeat(" ", IndentWidth)
	summary := fmt.Sprintf("\n%s%s%s", indent, SummaryPrefix, section.StatusMessage())
	parts = append(parts, summary)

	// Key Metrics section.
	metrics := section.KeyMetrics()
	if len(metrics) > 0 {
		parts = append(parts, r.renderMetrics(metrics, indent))
	}

	// Distribution section.
	distribution := section.Distribution()
	if len(distribution) > 0 {
		parts = append(parts, r.renderDistribution(distribution, indent))
	}

	// Issues section.
	var issues []analyze.Issue

	var issuesLabel string

	if r.verbose {
		issues = section.AllIssues()
		issuesLabel = AllIssuesLabel
	} else {
		issues = section.TopIssues(DefaultTopIssues)
		issuesLabel = IssuesLabel
	}

	if len(issues) > 0 {
		parts = append(parts, r.renderIssues(issues, issuesLabel, indent))
	}

	return strings.Join(parts, "\n")
}

// renderMetrics renders the key metrics section in 2-column layout.
func (r *SectionRenderer) renderMetrics(metrics []analyze.Metric, indent string) string {
	var lines []string

	// Section header.
	lines = append(lines, "")
	metricsHeader := r.config.Colorize(MetricsLabel, terminal.ColorGray)
	lines = append(lines, fmt.Sprintf("%s%s", indent, metricsHeader))
	separatorWidth := r.config.Width - (IndentWidth * separatorWidthValue)
	lines = append(lines, fmt.Sprintf("%s%s", indent, terminal.DrawSeparator(separatorWidth)))

	// Metrics in 2-column layout.
	for i := 0; i < len(metrics); i += MetricsPerRow {
		var row strings.Builder

		for j := 0; j < MetricsPerRow && i+j < len(metrics); j++ {
			m := metrics[i+j]
			label := terminal.PadRight(m.Label, MetricLabelWidth)
			value := terminal.PadRight(m.Value, MetricValueWidth)

			row.WriteString(label + value)
		}

		lines = append(lines, fmt.Sprintf("%s%s", indent, row.String()))
	}

	return strings.Join(lines, "\n")
}

// renderDistribution renders the distribution section with percent bars.
func (r *SectionRenderer) renderDistribution(items []analyze.DistributionItem, indent string) string {
	// 3 header lines + 1 line per item.
	lines := make([]string, 0, linesValue+len(items))
	// Section header.
	lines = append(lines, "")
	distHeader := r.config.Colorize(DistributionLabel, terminal.ColorGray)
	lines = append(lines, fmt.Sprintf("%s%s", indent, distHeader))
	separatorWidth := r.config.Width - (IndentWidth * magic2)
	lines = append(lines, fmt.Sprintf("%s%s", indent, terminal.DrawSeparator(separatorWidth)))

	// Distribution bars.
	for _, item := range items {
		bar := terminal.DrawPercentBar(item.Label, item.Percent, item.Count, DistLabelWidth, DistributionBarWidth)
		lines = append(lines, fmt.Sprintf("%s%s", indent, bar))
	}

	return strings.Join(lines, "\n")
}

// renderIssues renders the issues section with the given label.
func (r *SectionRenderer) renderIssues(issues []analyze.Issue, label, indent string) string {
	// 3 header lines + 1 line per issue.
	lines := make([]string, 0, makeArg3+len(issues))
	// Section header.
	lines = append(lines, "")
	issuesHeader := r.config.Colorize(label, terminal.ColorGray)
	lines = append(lines, fmt.Sprintf("%s%s", indent, issuesHeader))
	separatorWidth := r.config.Width - (IndentWidth * magic2_1)
	lines = append(lines, fmt.Sprintf("%s%s", indent, terminal.DrawSeparator(separatorWidth)))

	// Issues list.
	for _, issue := range issues {
		name := terminal.TruncateWithEllipsis(issue.Name, IssueNameWidth)
		name = terminal.PadRight(name, IssueNameWidth)
		location := terminal.TruncateWithEllipsis(issue.Location, IssueLocationWidth)
		location = terminal.PadRight(location, IssueLocationWidth)
		valueColor := ColorForSeverity(issue.Severity)
		coloredValue := r.config.Colorize(issue.Value, valueColor)
		lines = append(lines, fmt.Sprintf("%s%s %s %s", indent, name, location, coloredValue))
	}

	return strings.Join(lines, "\n")
}
