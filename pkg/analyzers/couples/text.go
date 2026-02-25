package couples

import (
	"fmt"
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

// Text output constants.
const (
	textMaxFileCouples = 7
	textMaxDevCouples  = 7
	textMaxOwnership   = 7
	textNameWidth      = 25
	textIndent         = "  "
	textOwnershipWidth = textNameWidth*2 + 3 // file1 + separator + file2 width.
	textIndentBothSide = 2                   // indent on both sides.
	pctMultiplier      = 100
	singleContributor  = 1
)

// generateText writes a human-readable couples summary to the writer.
func (c *HistoryAnalyzer) generateText(report analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	cfg := terminal.NewConfig()
	width := cfg.Width
	agg := metrics.Aggregate

	// Header.
	header := terminal.DrawHeader(
		"Couples",
		fmt.Sprintf("%d files", agg.TotalFiles),
		width,
	)
	fmt.Fprintln(writer, header)
	fmt.Fprintln(writer)

	// Summary section.
	writeCouplesSummary(writer, cfg, agg)

	// Top file couples.
	if len(metrics.FileCoupling) > 0 {
		fmt.Fprintln(writer)
		writeFileCouples(writer, cfg, metrics.FileCoupling)
	}

	// Top developer couples.
	if len(metrics.DeveloperCoupling) > 0 {
		fmt.Fprintln(writer)
		writeDevCouples(writer, cfg, metrics.DeveloperCoupling)
	}

	// File ownership risk.
	if len(metrics.FileOwnership) > 0 {
		fmt.Fprintln(writer)
		writeOwnershipRisk(writer, cfg, metrics.FileOwnership)
	}

	fmt.Fprintln(writer)

	return nil
}

func writeCouplesSummary(writer io.Writer, cfg terminal.Config, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*textIndentBothSide))
	fmt.Fprintf(writer, "%s%-22s %-12s%-22s %s\n", textIndent,
		"Total Files", formatCouplesInt(agg.TotalFiles),
		"Total Developers", formatCouplesInt(agg.TotalDevelopers))
	fmt.Fprintf(writer, "%s%-22s %-12s%-22s %s\n", textIndent,
		"Total Co-Changes", formatCouplesInt64(agg.TotalCoChanges),
		"Highly Coupled Pairs", formatCouplesInt(agg.HighlyCoupledPairs))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent,
		"Avg Coupling", formatPct(agg.AvgCouplingStrength))
}

// coupleRow is a generic row for printing coupling pairs.
type coupleRow struct {
	left     string
	right    string
	count    int64
	strength float64
}

func writeFileCouples(writer io.Writer, cfg terminal.Config, couples []FileCouplingData) {
	rows := make([]coupleRow, len(couples))
	for i, cp := range couples {
		rows[i] = coupleRow{cp.File1, cp.File2, cp.CoChanges, cp.Strength}
	}

	writeCoupleRows(writer, cfg, "Top File Couples", rows, textMaxFileCouples)
}

func writeDevCouples(writer io.Writer, cfg terminal.Config, couples []DeveloperCouplingData) {
	rows := make([]coupleRow, len(couples))
	for i, cp := range couples {
		rows[i] = coupleRow{cp.Developer1, cp.Developer2, cp.SharedFiles, cp.Strength}
	}

	writeCoupleRows(writer, cfg, "Top Developer Couples", rows, textMaxDevCouples)
}

func writeCoupleRows(writer io.Writer, cfg terminal.Config, title string, rows []coupleRow, maxRows int) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize(title, terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*textIndentBothSide))

	shown := min(len(rows), maxRows)

	for _, r := range rows[:shown] {
		left := terminal.TruncateWithEllipsis(r.left, textNameWidth)
		right := terminal.TruncateWithEllipsis(r.right, textNameWidth)
		strengthColor := colorForStrength(r.strength)

		fmt.Fprintf(writer, "%s%-*s %s %-*s %4d%s  %s\n",
			textIndent,
			textNameWidth, left,
			cfg.Colorize("\u2194", terminal.ColorGray), // ↔
			textNameWidth, right,
			r.count, "\u00d7", // ×
			cfg.Colorize(formatPct(r.strength), strengthColor),
		)
	}

	if len(rows) > maxRows {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  and %d more...", len(rows)-maxRows), terminal.ColorGray))
	}
}

func writeOwnershipRisk(writer io.Writer, cfg terminal.Config, ownership []FileOwnershipData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("File Ownership Risk", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*textIndentBothSide))

	// Show files with fewest contributors first (highest risk).
	sorted := SortOwnershipByRisk(ownership)
	shown := min(len(sorted), textMaxOwnership)

	for _, fo := range sorted[:shown] {
		file := terminal.TruncateWithEllipsis(fo.File, textOwnershipWidth)

		risk := ""
		if fo.Contributors <= singleContributor {
			risk = cfg.Colorize(" !!", terminal.ColorRed)
		}

		fmt.Fprintf(writer, "%s%-*s %5d lines  %d contributors%s\n",
			textIndent,
			textOwnershipWidth, file,
			fo.Lines,
			fo.Contributors,
			risk,
		)
	}

	if len(sorted) > textMaxOwnership {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  and %d more...", len(sorted)-textMaxOwnership), terminal.ColorGray))
	}
}

func colorForStrength(strength float64) terminal.Color {
	const (
		highThreshold = 0.7
		medThreshold  = 0.4
	)

	switch {
	case strength >= highThreshold:
		return terminal.ColorRed
	case strength >= medThreshold:
		return terminal.ColorYellow
	default:
		return terminal.ColorGreen
	}
}

func formatPct(v float64) string {
	return fmt.Sprintf("%.0f%%", v*pctMultiplier)
}

func formatCouplesInt(n int) string {
	return strconv.Itoa(n)
}

func formatCouplesInt64(n int64) string {
	return strconv.FormatInt(n, 10)
}
