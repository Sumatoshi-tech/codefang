package renderer

import (
	"fmt"
	"strings"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

// Summary constants.
const (
	MinSectionsForSummary = 2
	SummaryTitle          = "CODE ANALYSIS REPORT"
	SummaryOverallPrefix  = "Overall: "
	SummaryAnalyzerCol    = "Analyzer"
	SummaryScoreCol       = "Score"
	SummaryStatusCol      = "Status"
	SummaryAnalyzerWidth  = 16
	SummaryScoreWidth     = 7
	summaryFixedParts     = 4 // header, blank, column headers, separator.
)

// ExecutiveSummary holds data for the executive summary report.
type ExecutiveSummary struct {
	Sections []analyze.ReportSection
}

// NewExecutiveSummary creates an ExecutiveSummary from report sections.
func NewExecutiveSummary(sections []analyze.ReportSection) *ExecutiveSummary {
	if sections == nil {
		sections = []analyze.ReportSection{}
	}

	return &ExecutiveSummary{
		Sections: sections,
	}
}

// OverallScore returns the average score of all scored sections.
// Info-only sections (ScoreInfoOnly) are excluded from the average.
// Returns ScoreInfoOnly if no scored sections exist.
func (s *ExecutiveSummary) OverallScore() float64 {
	var total float64

	var count int

	for _, section := range s.Sections {
		score := section.Score()
		if score >= 0 {
			total += score
			count++
		}
	}

	if count == 0 {
		return analyze.ScoreInfoOnly
	}

	return total / float64(count)
}

// OverallScoreLabel returns the formatted overall score ("N/10" or "Info").
func (s *ExecutiveSummary) OverallScoreLabel() string {
	score := s.OverallScore()
	if score < 0 {
		return analyze.ScoreLabelInfo
	}

	return terminal.FormatScore(score)
}

// RenderSummary produces the executive summary output.
func (r *SectionRenderer) RenderSummary(summary *ExecutiveSummary) string {
	parts := make([]string, 0, summaryFixedParts+len(summary.Sections))

	// Header with title and overall score.
	title := r.config.Colorize(SummaryTitle, terminal.ColorBlue)
	overallScore := summary.OverallScore()

	overallLabel := summary.OverallScoreLabel()
	if overallScore >= 0 {
		overallLabel = r.config.Colorize(overallLabel, terminal.ColorForScore(overallScore))
	}

	rightText := SummaryOverallPrefix + overallLabel
	header := terminal.DrawHeader(title, rightText, r.config.Width)
	parts = append(parts, header)

	// Column headers.
	indent := strings.Repeat(" ", IndentWidth)
	headerRow := fmt.Sprintf("%s%s%s%s",
		indent,
		terminal.PadRight(SummaryAnalyzerCol, SummaryAnalyzerWidth),
		terminal.PadRight(SummaryScoreCol, SummaryScoreWidth),
		SummaryStatusCol,
	)
	headerRow = r.config.Colorize(headerRow, terminal.ColorGray)

	parts = append(parts, "", headerRow)

	// Separator.
	separatorWidth := r.config.Width - (IndentWidth * separatorWidthValue)
	parts = append(parts, fmt.Sprintf("%s%s", indent, terminal.DrawSeparator(separatorWidth)))

	// Analyzer rows.
	for _, section := range summary.Sections {
		name := terminal.PadRight(section.SectionTitle(), SummaryAnalyzerWidth)
		score := section.ScoreLabel()

		sectionScore := section.Score()
		if sectionScore >= 0 {
			score = r.config.Colorize(score, terminal.ColorForScore(sectionScore))
		}

		score = terminal.PadRight(score, SummaryScoreWidth)
		message := section.StatusMessage()
		parts = append(parts, fmt.Sprintf("%s%s%s%s", indent, name, score, message))
	}

	return strings.Join(parts, "\n")
}
