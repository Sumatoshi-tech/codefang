package filehistory

import (
	"fmt"
	"io"
	"strings"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

const (
	textIndent        = "  "
	textMaxFiles      = 10
	textBarWidth      = 30
	textCategoryWidth = 16
)

// generateText writes a human-readable file history summary to the writer.
func generateText(report analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	cfg := terminal.NewConfig()
	width := cfg.Width
	agg := computed.Aggregate

	// Header.
	header := terminal.DrawHeader(
		"File History",
		fmt.Sprintf("%d files", agg.TotalFiles),
		width,
	)
	fmt.Fprintln(writer, header)
	fmt.Fprintln(writer)

	// Summary section.
	writeFileSummary(writer, cfg, agg)

	// Composition section.
	if len(computed.Composition.Breakdown) > 0 {
		fmt.Fprintln(writer)
		writeComposition(writer, cfg, computed.Composition)
	}

	// Top churned files.
	if len(computed.FileChurn) > 0 {
		fmt.Fprintln(writer)
		writeTopFiles(writer, cfg, computed.FileChurn)
	}

	fmt.Fprintln(writer)

	return nil
}

func writeFileSummary(writer io.Writer, cfg terminal.Config, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	fmt.Fprintf(writer, "%s%-26s %d\n", textIndent, "Total Files", agg.TotalFiles)
	fmt.Fprintf(writer, "%s%-26s %d\n", textIndent, "Total Commits", agg.TotalCommits)
	fmt.Fprintf(writer, "%s%-26s %d\n", textIndent, "Total Contributors", agg.TotalContributors)
	fmt.Fprintf(writer, "%s%-26s %.1f\n", textIndent, "Avg Commits/File", agg.AvgCommitsPerFile)
	fmt.Fprintf(writer, "%s%-26s %d\n", textIndent, "High Churn Files", agg.HighChurnFiles)
}

func writeComposition(writer io.Writer, cfg terminal.Config, comp CompositionData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("File Composition", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	for _, cat := range AllCategories {
		count := comp.Breakdown[string(cat)]
		pct := comp.Percentages[string(cat)]

		if count == 0 {
			continue
		}

		bar := buildBar(pct, textBarWidth)
		fmt.Fprintf(writer, "%s%-*s %5d (%5.1f%%) %s\n",
			textIndent, textCategoryWidth, string(cat), count, pct, bar)
	}
}

func buildBar(pct float64, maxWidth int) string {
	filled := int(pct / percentMultiplier * float64(maxWidth))
	if filled < 0 {
		filled = 0
	}

	if filled > maxWidth {
		filled = maxWidth
	}

	return strings.Repeat("|", filled)
}

func writeTopFiles(writer io.Writer, cfg terminal.Config, churn []FileChurnData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Most Modified Files", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	limit := min(len(churn), textMaxFiles)

	for i := range limit {
		f := churn[i]
		fmt.Fprintf(writer, "%s%4d commits  %s\n", textIndent, f.CommitCount, f.Path)
	}

	if len(churn) > limit {
		fmt.Fprintf(writer, "%s... and %d more files\n", textIndent, len(churn)-limit)
	}
}
