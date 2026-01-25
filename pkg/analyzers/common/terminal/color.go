package terminal

import "fmt"

// Color represents ANSI terminal colors.
type Color int

// Color constants
const (
	ColorNone Color = iota
	ColorGreen
	ColorYellow
	ColorRed
	ColorBlue
	ColorGray
)

// ANSI color codes
const (
	ansiReset  = "\033[0m"
	ansiGreen  = "\033[32m"
	ansiYellow = "\033[33m"
	ansiRed    = "\033[31m"
	ansiBlue   = "\033[34m"
	ansiGray   = "\033[90m"
)

// Score thresholds for color assignment
const (
	ScoreThresholdGood = 0.8
	ScoreThresholdFair = 0.5
)

// Colorize applies ANSI color to text. If NoColor is true, returns text unchanged.
func (c Config) Colorize(text string, color Color) string {
	if c.NoColor {
		return text
	}

	var code string
	switch color {
	case ColorGreen:
		code = ansiGreen
	case ColorYellow:
		code = ansiYellow
	case ColorRed:
		code = ansiRed
	case ColorBlue:
		code = ansiBlue
	case ColorGray:
		code = ansiGray
	default:
		return text
	}

	return fmt.Sprintf("%s%s%s", code, text, ansiReset)
}

// ColorForScore returns the appropriate color for a 0-1 score.
func ColorForScore(score float64) Color {
	if score >= ScoreThresholdGood {
		return ColorGreen
	}
	if score >= ScoreThresholdFair {
		return ColorYellow
	}
	return ColorRed
}
