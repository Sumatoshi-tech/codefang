package sentiment

import (
	"fmt"
	"math"
	"strings"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

// Terminal output constants.
const (
	terminalBarWidth     = 20
	terminalLabelWidth   = 18
	sparklineBlocks      = 20
	sparklineChars       = "▁▂▃▄▅▆▇█"
	trendArrowUp         = "↗"
	trendArrowDown       = "↘"
	trendArrowStable     = "→"
	sentimentEmoji       = "💬"
	positiveEmoji        = "😊"
	negativeEmoji        = "😟"
	neutralEmoji         = "😐"
	riskHighEmoji        = "🔴"
	riskMediumEmoji      = "🟡"
	headerTitle          = "SENTIMENT ANALYSIS"
	sectionSummary       = "Summary"
	sectionDistribution  = "Distribution"
	sectionTrend         = "Trend"
	sectionSparkline     = "Sentiment Timeline"
	sectionRisk          = "Risk Periods"
	maxRiskPeriodsToShow = 5
	termWidth            = 60
	labelPaddingExtra    = 4
	sparklineLabelGap    = 14
)

// RenderTerminal returns a colored, human-readable terminal representation.
func RenderTerminal(metrics *ComputedMetrics) string {
	cfg := terminal.NewConfig()

	var sb strings.Builder

	sb.WriteString(terminal.DrawHeader(headerTitle, sentimentEmoji, termWidth))
	sb.WriteString("\n\n")

	renderSummarySection(&sb, cfg, metrics)
	renderDistributionSection(&sb, cfg, metrics)
	renderTrendSection(&sb, cfg, metrics)
	renderSparklineSection(&sb, cfg, metrics)
	renderRiskSection(&sb, cfg, metrics)

	return sb.String()
}

func renderSummarySection(sb *strings.Builder, cfg terminal.Config, metrics *ComputedMetrics) {
	sb.WriteString(cfg.Colorize(fmt.Sprintf("  %s\n", sectionSummary), terminal.ColorBlue))
	sb.WriteString(terminal.DrawSeparator(termWidth))
	sb.WriteString("\n")

	avgScore := float64(metrics.Aggregate.AverageSentiment)
	color := sentimentColor(avgScore)

	fmt.Fprintf(sb, "  Average Sentiment: %s %s\n",
		cfg.Colorize(terminal.FormatScoreBar(avgScore, terminalBarWidth), color),
		sentimentLabel(avgScore))

	fmt.Fprintf(sb, "  Total Ticks:       %d\n", metrics.Aggregate.TotalTicks)
	fmt.Fprintf(sb, "  Total Comments:    %d\n", metrics.Aggregate.TotalComments)
	fmt.Fprintf(sb, "  Total Commits:     %d\n", metrics.Aggregate.TotalCommits)
	sb.WriteString("\n")
}

func renderDistributionSection(sb *strings.Builder, cfg terminal.Config, metrics *ComputedMetrics) {
	if metrics.Aggregate.TotalTicks == 0 {
		return
	}

	sb.WriteString(cfg.Colorize(fmt.Sprintf("  %s\n", sectionDistribution), terminal.ColorBlue))
	sb.WriteString(terminal.DrawSeparator(termWidth))
	sb.WriteString("\n")

	total := float64(metrics.Aggregate.TotalTicks)

	items := []struct {
		label string
		count int
		color terminal.Color
		emoji string
	}{
		{"Positive (≥0.6)", metrics.Aggregate.PositiveTicks, terminal.ColorGreen, positiveEmoji},
		{"Neutral", metrics.Aggregate.NeutralTicks, terminal.ColorYellow, neutralEmoji},
		{"Negative (≤0.4)", metrics.Aggregate.NegativeTicks, terminal.ColorRed, negativeEmoji},
	}

	for _, item := range items {
		pct := float64(item.count) / total
		bar := terminal.DrawPercentBar(
			fmt.Sprintf("  %s %s", item.emoji, item.label),
			pct, item.count, terminalLabelWidth+labelPaddingExtra, terminalBarWidth,
		)
		sb.WriteString(cfg.Colorize(bar, item.color))
		sb.WriteString("\n")
	}

	sb.WriteString("\n")
}

func renderTrendSection(sb *strings.Builder, cfg terminal.Config, metrics *ComputedMetrics) {
	if metrics.Trend.TrendDirection == "" {
		return
	}

	sb.WriteString(cfg.Colorize(fmt.Sprintf("  %s\n", sectionTrend), terminal.ColorBlue))
	sb.WriteString(terminal.DrawSeparator(termWidth))
	sb.WriteString("\n")

	arrow := trendArrowStable
	color := terminal.ColorYellow

	switch metrics.Trend.TrendDirection {
	case "improving":
		arrow = trendArrowUp
		color = terminal.ColorGreen
	case "declining":
		arrow = trendArrowDown
		color = terminal.ColorRed
	}

	fmt.Fprintf(sb, "  Direction: %s %s\n",
		cfg.Colorize(arrow, color),
		cfg.Colorize(metrics.Trend.TrendDirection, color))

	fmt.Fprintf(sb, "  Start (tick %d): %.2f  →  End (tick %d): %.2f\n",
		metrics.Trend.StartTick, metrics.Trend.StartSentiment,
		metrics.Trend.EndTick, metrics.Trend.EndSentiment)

	sign := "+"
	if metrics.Trend.ChangePercent < 0 {
		sign = ""
	}

	fmt.Fprintf(sb, "  Change: %s%.1f%%\n", sign, metrics.Trend.ChangePercent)
	sb.WriteString("\n")
}

func renderSparklineSection(sb *strings.Builder, cfg terminal.Config, metrics *ComputedMetrics) {
	if len(metrics.TimeSeries) == 0 {
		return
	}

	sb.WriteString(cfg.Colorize(fmt.Sprintf("  %s\n", sectionSparkline), terminal.ColorBlue))
	sb.WriteString(terminal.DrawSeparator(termWidth))
	sb.WriteString("\n")

	sparkline := buildSparkline(metrics.TimeSeries, cfg)

	fmt.Fprintf(sb, "  %s\n", sparkline)
	fmt.Fprintf(sb, "  %s%s%s\n",
		cfg.Colorize("neg", terminal.ColorRed),
		strings.Repeat(" ", sparklineLabelGap),
		cfg.Colorize("pos", terminal.ColorGreen))

	sb.WriteString("\n")
}

func renderRiskSection(sb *strings.Builder, cfg terminal.Config, metrics *ComputedMetrics) {
	if len(metrics.LowSentimentPeriods) == 0 {
		return
	}

	sb.WriteString(cfg.Colorize(fmt.Sprintf("  %s\n", sectionRisk), terminal.ColorBlue))
	sb.WriteString(terminal.DrawSeparator(termWidth))
	sb.WriteString("\n")

	shown := min(len(metrics.LowSentimentPeriods), maxRiskPeriodsToShow)

	for i := range shown {
		period := metrics.LowSentimentPeriods[i]
		emoji := riskMediumEmoji

		color := terminal.ColorYellow

		if period.RiskLevel == "HIGH" {
			emoji = riskHighEmoji
			color = terminal.ColorRed
		}

		sb.WriteString(cfg.Colorize(
			fmt.Sprintf("  %s Tick %d: %.2f (%s)\n",
				emoji, period.Tick, period.Sentiment, period.RiskLevel),
			color))
	}

	if len(metrics.LowSentimentPeriods) > maxRiskPeriodsToShow {
		remaining := len(metrics.LowSentimentPeriods) - maxRiskPeriodsToShow
		sb.WriteString(cfg.Colorize(
			fmt.Sprintf("  ... and %d more\n", remaining),
			terminal.ColorGray))
	}

	sb.WriteString("\n")
}

func buildSparkline(timeSeries []TimeSeriesData, cfg terminal.Config) string {
	runes := []rune(sparklineChars)
	levels := len(runes)

	var sb strings.Builder

	for _, ts := range timeSeries {
		score := float64(ts.Sentiment)
		idx := max(int(math.Min(score*float64(levels), float64(levels-1))), 0)
		color := sentimentColor(score)
		sb.WriteString(cfg.Colorize(string(runes[idx]), color))
	}

	return sb.String()
}

func sentimentColor(score float64) terminal.Color {
	switch {
	case score >= SentimentPositiveThreshold:
		return terminal.ColorGreen
	case score <= SentimentNegativeThreshold:
		return terminal.ColorRed
	default:
		return terminal.ColorYellow
	}
}

func sentimentLabel(score float64) string {
	switch {
	case score >= SentimentPositiveThreshold:
		return positiveEmoji
	case score <= SentimentNegativeThreshold:
		return negativeEmoji
	default:
		return neutralEmoji
	}
}
