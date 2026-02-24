package devs

import (
	"fmt"
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

const (
	textMaxContributors = 7
	textMaxBusFactors   = 7
	textDevNameWidth    = 18
	textIndent          = "  "
	thousandSeparatorAt = 1000
)

// generateText writes a human-readable developer summary to the writer.
func (a *Analyzer) generateText(report analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	cfg := terminal.NewConfig()
	width := cfg.Width
	agg := metrics.Aggregate

	// Header.
	header := terminal.DrawHeader(
		"Developers",
		fmt.Sprintf("%d ticks", agg.AnalysisPeriodTicks),
		width,
	)
	fmt.Fprintln(writer, header)
	fmt.Fprintln(writer)

	// Summary section.
	writeSummarySection(writer, cfg, agg)

	// Top contributors.
	if len(metrics.Developers) > 0 {
		fmt.Fprintln(writer)
		writeContributors(writer, cfg, metrics.Developers)
	}

	// Bus factor risk.
	if len(metrics.BusFactor) > 0 {
		fmt.Fprintln(writer)
		writeBusFactorRisk(writer, cfg, metrics.BusFactor)
	}

	// Churn summary.
	if len(metrics.Churn) > 0 {
		fmt.Fprintln(writer)
		writeChurnSummary(writer, cfg, metrics.Churn)
	}

	fmt.Fprintln(writer)

	return nil
}

func writeSummarySection(writer io.Writer, cfg terminal.Config, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Total Commits", formatInt(agg.TotalCommits))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Developers", formatInt(agg.TotalDevelopers))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Active Developers", formatInt(agg.ActiveDevelopers))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Project Bus Factor", formatInt(agg.ProjectBusFactor))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Languages", formatInt(agg.TotalLanguages))
}

func writeContributors(writer io.Writer, cfg terminal.Config, developers []DeveloperData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Top Contributors", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	shown := min(len(developers), textMaxContributors)

	for _, dev := range developers[:shown] {
		name := dev.Name
		if name == "" {
			name = fmt.Sprintf("dev#%d", dev.ID)
		}

		name = terminal.TruncateWithEllipsis(name, textDevNameWidth)
		primaryLang := findPrimaryLanguage(dev)

		fmt.Fprintf(writer, "%s%-*s %6s commits  %s+%s%s / %s-%s%s  net %s  %s\n",
			textIndent,
			textDevNameWidth, name,
			formatInt(dev.Commits),
			cfg.Colorize("", terminal.ColorGreen), formatInt(dev.Added), cfg.Colorize("", terminal.ColorNone),
			cfg.Colorize("", terminal.ColorRed), formatInt(dev.Removed), cfg.Colorize("", terminal.ColorNone),
			formatInt(dev.NetLines),
			cfg.Colorize(primaryLang, terminal.ColorGray),
		)
	}

	if len(developers) > textMaxContributors {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  and %d more...", len(developers)-textMaxContributors), terminal.ColorGray))
	}
}

func writeBusFactorRisk(writer io.Writer, cfg terminal.Config, busFactor []BusFactorData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Bus Factor Risk", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	shown := min(len(busFactor), textMaxBusFactors)

	for _, bf := range busFactor[:shown] {
		riskColor := riskToColor(bf.RiskLevel)
		lang := terminal.TruncateWithEllipsis(bf.Language, textDevNameWidth)

		fmt.Fprintf(writer, "%s%-*s %s  owner %5.1f%%  bf=%d/%d\n",
			textIndent,
			textDevNameWidth, lang,
			cfg.Colorize(fmt.Sprintf("%-8s", bf.RiskLevel), riskColor),
			bf.PrimaryPct,
			bf.BusFactor,
			bf.TotalContributors,
		)
	}

	if len(busFactor) > textMaxBusFactors {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  and %d more...", len(busFactor)-textMaxBusFactors), terminal.ColorGray))
	}
}

func writeChurnSummary(writer io.Writer, cfg terminal.Config, churn []ChurnData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Churn Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	totalAdded := 0
	totalRemoved := 0

	for _, c := range churn {
		totalAdded += c.Added
		totalRemoved += c.Removed
	}

	net := totalAdded - totalRemoved

	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Lines Added", formatInt(totalAdded))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Lines Removed", formatInt(totalRemoved))
	fmt.Fprintf(writer, "%s%-22s %s\n", textIndent, "Net Change", formatInt(net))
}

func riskToColor(level string) terminal.Color {
	switch level {
	case RiskCritical:
		return terminal.ColorRed
	case RiskHigh:
		return terminal.ColorRed
	case RiskMedium:
		return terminal.ColorYellow
	default:
		return terminal.ColorGreen
	}
}

// formatInt formats an int with thousand separators.
func formatInt(n int) string {
	if n < 0 {
		return "-" + formatUint(uint64(-n))
	}

	return formatUint(uint64(n))
}

func formatUint(n uint64) string {
	if n < thousandSeparatorAt {
		return strconv.FormatUint(n, 10)
	}

	return formatUint(n/thousandSeparatorAt) + "," + fmt.Sprintf("%03d", n%thousandSeparatorAt)
}
