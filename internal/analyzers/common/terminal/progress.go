package terminal

import (
	"fmt"
	"math"
	"strings"
)

// Progress bar characters.
const (
	ProgressFilled = "█"
	ProgressEmpty  = "░"
)

// DrawProgressBar draws a progress bar of the given width.
// Value is clamped to [0, 1] range.
// Example: DrawProgressBar(0.7, 10) returns "███████░░░".
func DrawProgressBar(value float64, width int) string {
	// Clamp value to [0, 1].
	if value < 0 {
		value = 0
	}

	if value > 1 {
		value = 1
	}

	filled := int(value * float64(width))
	empty := width - filled

	return strings.Repeat(ProgressFilled, filled) + strings.Repeat(ProgressEmpty, empty)
}

// ScoreMax is the maximum score value for display (N/10).
const ScoreMax = 10

// FormatScore formats a 0-1 score as "N/10".
func FormatScore(score float64) string {
	scaled := int(math.Round(score * ScoreMax))

	return fmt.Sprintf("%d/%d", scaled, ScoreMax)
}

// FormatScoreBar formats score with visual bar: "[████████░░] 8/10".
func FormatScoreBar(score float64, barWidth int) string {
	bar := DrawProgressBar(score, barWidth)
	label := FormatScore(score)

	return fmt.Sprintf("[%s] %s", bar, label)
}

// PercentMultiplier converts 0-1 to 0-100.
const PercentMultiplier = 100

// DrawPercentBar draws a labeled percentage bar.
// Example: "Simple (1-5)    ████████████████░░░░  68%  (106)".
func DrawPercentBar(label string, percent float64, count, labelWidth, barWidth int) string {
	paddedLabel := PadRight(label, labelWidth)
	bar := DrawProgressBar(percent, barWidth)
	pctValue := int(percent * PercentMultiplier)

	return fmt.Sprintf("%s %s %3d%%  (%d)", paddedLabel, bar, pctValue, count)
}
