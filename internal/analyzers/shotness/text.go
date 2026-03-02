package shotness

import (
	"fmt"
	"io"
	"path/filepath"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

const (
	textBarWidth      = 20
	textLabelWidth    = 24
	textHalfLabel     = textLabelWidth / 2
	textIndent        = "  "
	textMaxHot        = 10
	textMaxCouplings  = 10
	textMaxHotspots   = 10
	percentFactor     = 100
	summaryLabelWidth = 22
)

// generateText writes a human-readable shotness summary to the writer.
func (s *Analyzer) generateText(report analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	cfg := terminal.NewConfig()
	width := cfg.Width

	header := terminal.DrawHeader(
		"Shotness Analysis",
		fmt.Sprintf("%d nodes", metrics.Aggregate.TotalNodes),
		width,
	)
	fmt.Fprintln(writer, header)
	fmt.Fprintln(writer)

	writeSummarySection(writer, cfg, metrics.Aggregate)

	if len(metrics.NodeHotness) > 0 {
		fmt.Fprintln(writer)
		writeHottestFunctions(writer, cfg, metrics.NodeHotness)
	}

	if len(metrics.HotspotNodes) > 0 {
		fmt.Fprintln(writer)
		writeRiskNodes(writer, cfg, metrics.HotspotNodes)
	}

	if len(metrics.NodeCoupling) > 0 {
		fmt.Fprintln(writer)
		writeStrongestCouplings(writer, cfg, metrics.NodeCoupling)
	}

	fmt.Fprintln(writer)

	return nil
}

func writeSummarySection(writer io.Writer, cfg terminal.Config, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	fmt.Fprintf(writer, "%s%-*s %d\n", textIndent, summaryLabelWidth, "Total Nodes", agg.TotalNodes)
	fmt.Fprintf(writer, "%s%-*s %d\n", textIndent, summaryLabelWidth, "Total Changes", agg.TotalChanges)
	fmt.Fprintf(writer, "%s%-*s %.1f\n", textIndent, summaryLabelWidth, "Avg Changes/Node", agg.AvgChangesPerNode)
	fmt.Fprintf(writer, "%s%-*s %d\n", textIndent, summaryLabelWidth, "Total Couplings", agg.TotalCouplings)

	strengthColor := terminal.ColorForScore(1.0 - agg.AvgCouplingStrength)
	fmt.Fprintf(writer, "%s%-*s %s\n", textIndent, summaryLabelWidth, "Avg Coupling Strength",
		cfg.Colorize(fmt.Sprintf("%.0f%%", agg.AvgCouplingStrength*percentFactor), strengthColor))

	hotColor := terminal.ColorNone
	if agg.HotNodes > 0 {
		hotColor = terminal.ColorRed
	}

	fmt.Fprintf(writer, "%s%-*s %s\n", textIndent, summaryLabelWidth, "Hot Nodes",
		cfg.Colorize(strconv.Itoa(agg.HotNodes), hotColor))
}

func writeHottestFunctions(writer io.Writer, cfg terminal.Config, nodes []NodeHotnessData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Hottest Functions", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	shown := min(len(nodes), textMaxHot)

	for _, n := range nodes[:shown] {
		label := formatNodeLabel(n.Name, n.File)
		label = terminal.TruncateWithEllipsis(label, textLabelWidth)

		bar := terminal.DrawProgressBar(n.HotnessScore, textBarWidth)
		scoreColor := hotnessColor(n.HotnessScore)

		fmt.Fprintf(writer, "%s%-*s [%s] %s  (%d changes)\n",
			textIndent,
			textLabelWidth, label,
			bar,
			cfg.Colorize(fmt.Sprintf("%.1f", n.HotnessScore), scoreColor),
			n.ChangeCount)
	}

	if len(nodes) > textMaxHot {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  ... and %d more", len(nodes)-textMaxHot), terminal.ColorGray))
	}
}

func writeRiskNodes(writer io.Writer, cfg terminal.Config, nodes []HotspotNodeData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Risk Assessment", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	shown := min(len(nodes), textMaxHotspots)

	for _, n := range nodes[:shown] {
		label := formatNodeLabel(n.Name, n.File)
		label = terminal.TruncateWithEllipsis(label, textLabelWidth)

		riskColor := riskLevelColor(n.RiskLevel)

		fmt.Fprintf(writer, "%s%-*s %s  (%d changes)\n",
			textIndent,
			textLabelWidth, label,
			cfg.Colorize(fmt.Sprintf("%-6s", n.RiskLevel), riskColor),
			n.ChangeCount)
	}

	if len(nodes) > textMaxHotspots {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  ... and %d more", len(nodes)-textMaxHotspots), terminal.ColorGray))
	}
}

func writeStrongestCouplings(writer io.Writer, cfg terminal.Config, couplings []NodeCouplingData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Strongest Couplings", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	shown := min(len(couplings), textMaxCouplings)

	for _, c := range couplings[:shown] {
		left := terminal.TruncateWithEllipsis(c.Node1Name, textHalfLabel)
		right := terminal.TruncateWithEllipsis(c.Node2Name, textHalfLabel)

		strengthPct := c.Strength * percentFactor
		strengthColor := couplingStrengthColor(c.Strength)

		fmt.Fprintf(writer, "%s%-*s %s %-*s %s  (%d co-changes)\n",
			textIndent,
			textHalfLabel, left,
			cfg.Colorize("↔", terminal.ColorGray),
			textHalfLabel, right,
			cfg.Colorize(fmt.Sprintf("%3.0f%%", strengthPct), strengthColor),
			c.CoChanges)
	}

	if len(couplings) > textMaxCouplings {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  ... and %d more", len(couplings)-textMaxCouplings), terminal.ColorGray))
	}
}

// formatNodeLabel builds "name (file)" from the node name and file path.
func formatNodeLabel(name, file string) string {
	if file == "" {
		return name
	}

	return fmt.Sprintf("%s (%s)", name, filepath.Base(file))
}

// hotnessColor returns a color based on hotness score (inverted: high = red).
func hotnessColor(score float64) terminal.Color {
	return terminal.ColorForScore(1.0 - score)
}

// riskLevelColor maps risk level to terminal color.
func riskLevelColor(level string) terminal.Color {
	switch level {
	case RiskLevelHigh:
		return terminal.ColorRed
	case RiskLevelMedium:
		return terminal.ColorYellow
	default:
		return terminal.ColorGreen
	}
}

// couplingStrengthColor maps coupling strength to terminal color (high = concerning).
func couplingStrengthColor(strength float64) terminal.Color {
	return terminal.ColorForScore(1.0 - strength)
}
