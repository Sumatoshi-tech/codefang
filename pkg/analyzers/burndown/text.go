package burndown

import (
	"fmt"
	"io"
	"sort"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

const (
	textBarWidth        = 20
	textLabelWidth      = 14
	textDevNameWidth    = 16
	textDevLinesWidth   = 8
	textMaxAgeBands     = 5
	textMaxDevs         = 5
	textIndent          = "  "
	thousandSeparatorAt = 1000
)

// generateText writes a human-readable burndown summary to the writer.
func (b *HistoryAnalyzer) generateText(report analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	cfg := terminal.NewConfig()
	width := cfg.Width

	agg := metrics.Aggregate
	projectName := extractProjectName(report)

	// Header.
	header := terminal.DrawHeader(
		fmt.Sprintf("Burndown: %s", projectName),
		fmt.Sprintf("%dd", agg.AnalysisPeriodDays),
		width,
	)
	fmt.Fprintln(writer, header)
	fmt.Fprintln(writer)

	// Summary section.
	writeSummary(writer, cfg, agg)

	// Code age distribution.
	if len(metrics.GlobalSurvival) > 0 {
		fmt.Fprintln(writer)
		writeAgeDistribution(writer, cfg, metrics, agg)
	}

	// Top developers.
	if len(metrics.DeveloperSurvival) > 0 {
		fmt.Fprintln(writer)
		writeTopDevelopers(writer, cfg, metrics)
	}

	fmt.Fprintln(writer)

	return nil
}

func writeSummary(writer io.Writer, cfg terminal.Config, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Summary", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	survivalPct := agg.OverallSurvivalRate
	survivalColor := terminal.ColorForScore(survivalPct)
	bar := terminal.DrawProgressBar(survivalPct, textBarWidth)

	fmt.Fprintf(writer, "%s%-18s %s\n", textIndent, "Current Lines", formatInt64(agg.TotalCurrentLines))
	fmt.Fprintf(writer, "%s%-18s %s\n", textIndent, "Peak Lines", formatInt64(agg.TotalPeakLines))
	fmt.Fprintf(writer, "%s%-18s [%s] %s\n", textIndent, "Survival Rate",
		bar,
		cfg.Colorize(fmt.Sprintf("%.1f%%", survivalPct*percentMultiplier), survivalColor))
}

func writeAgeDistribution(writer io.Writer, cfg terminal.Config, metrics *ComputedMetrics, agg AggregateData) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Code Age Distribution", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	// Get the last sample (current state).
	lastSample := metrics.GlobalSurvival[len(metrics.GlobalSurvival)-1]
	if lastSample.TotalLines == 0 {
		return
	}

	bands := buildAgeBands(lastSample.BandBreakdown, agg.NumBands)

	for _, band := range bands {
		pct := float64(band.lines) / float64(lastSample.TotalLines)
		bar := terminal.DrawPercentBar(band.label, pct, int(band.lines), textLabelWidth, textBarWidth)
		fmt.Fprintf(writer, "%s%s\n", textIndent, bar)
	}
}

type ageBand struct {
	label string
	lines int64
}

// buildAgeBands groups dense history bands into at most textMaxAgeBands display bands.
func buildAgeBands(breakdown []int64, numBands int) []ageBand {
	if numBands == 0 {
		return nil
	}

	// Define age bucket boundaries in months.
	type bucket struct {
		maxMonths int // 0 = unbounded.
		label     string
	}

	buckets := []bucket{
		{1, "< 1 month"},
		{3, "1-3 months"},
		{6, "3-6 months"},
		{12, "6-12 months"},
		{0, "> 12 months"},
	}

	bands := make([]ageBand, len(buckets))
	for i, b := range buckets {
		bands[i].label = b.label
	}

	// Band 0 is the newest code (written most recently).
	// Band N-1 is the oldest code.
	for i, val := range breakdown {
		if val <= 0 {
			continue
		}

		// Band index i → age in months = i+1 (each band ≈ 1 granularity period).
		ageMonths := i + 1

		bucketIdx := len(buckets) - 1 // Default to oldest.
		for j, b := range buckets {
			if b.maxMonths > 0 && ageMonths <= b.maxMonths {
				bucketIdx = j

				break
			}
		}

		bands[bucketIdx].lines += val
	}

	// Filter out empty bands.
	var result []ageBand

	for _, b := range bands {
		if b.lines > 0 {
			result = append(result, b)
		}
	}

	return result
}

func writeTopDevelopers(writer io.Writer, cfg terminal.Config, metrics *ComputedMetrics) {
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		cfg.Colorize("Top Developers (by surviving lines)", terminal.ColorBlue))
	fmt.Fprintf(writer, "%s%s\n", textIndent,
		terminal.DrawSeparator(cfg.Width-len(textIndent)*2))

	// Sort developers by current lines, descending.
	devs := make([]DeveloperSurvivalData, len(metrics.DeveloperSurvival))
	copy(devs, metrics.DeveloperSurvival)

	sort.Slice(devs, func(i, j int) bool {
		return devs[i].CurrentLines > devs[j].CurrentLines
	})

	shown := min(len(devs), textMaxDevs)

	for _, dev := range devs[:shown] {
		name := dev.Name
		if name == "" {
			name = fmt.Sprintf("dev#%d", dev.ID)
		}

		name = terminal.TruncateWithEllipsis(name, textDevNameWidth)
		survivalColor := terminal.ColorForScore(dev.SurvivalRate)
		bar := terminal.DrawProgressBar(dev.SurvivalRate, textBarWidth)

		fmt.Fprintf(writer, "%s%-*s %*d  [%s] %s\n",
			textIndent,
			textDevNameWidth, name,
			textDevLinesWidth, dev.CurrentLines,
			bar,
			cfg.Colorize(fmt.Sprintf("%.1f%%", dev.SurvivalRate*percentMultiplier), survivalColor))
	}

	if len(devs) > textMaxDevs {
		fmt.Fprintf(writer, "%s%s\n", textIndent,
			cfg.Colorize(fmt.Sprintf("  and %d more...", len(devs)-textMaxDevs), terminal.ColorGray))
	}
}

// formatInt64 formats an int64 with thousand separators.
func formatInt64(n int64) string {
	if n < 0 {
		return "-" + formatUint64(uint64(-n))
	}

	return formatUint64(uint64(n))
}

func formatUint64(n uint64) string {
	if n < thousandSeparatorAt {
		return strconv.FormatUint(n, 10)
	}

	return formatUint64(n/thousandSeparatorAt) + "," + fmt.Sprintf("%03d", n%thousandSeparatorAt)
}
