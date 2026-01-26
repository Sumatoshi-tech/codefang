package terminal //nolint:testpackage // testing internal implementation.

import (
	"os"
	"strings"
	"testing"
)

const (
	testDefaultWidth = 80
	testCustomWidth  = 120
)

func TestDetectWidth_Default(t *testing.T) {
	t.Parallel()

	// Unset COLUMNS to test default behavior.
	originalColumns := os.Getenv("COLUMNS")

	os.Unsetenv("COLUMNS")

	defer func() {
		if originalColumns != "" {
			os.Setenv("COLUMNS", originalColumns) //nolint:usetesting // test helper pattern is intentional.
		}
	}()

	width := DetectWidth()
	if width != testDefaultWidth {
		t.Errorf("DetectWidth() = %d, want %d", width, testDefaultWidth)
	}
}

func TestDetectWidth_FromEnv(t *testing.T) {
	t.Parallel()

	originalColumns := os.Getenv("COLUMNS")

	os.Setenv("COLUMNS", "120") //nolint:usetesting // test helper pattern is intentional.

	defer func() {
		if originalColumns != "" {
			os.Setenv("COLUMNS", originalColumns) //nolint:usetesting // test helper pattern is intentional.
		} else {
			os.Unsetenv("COLUMNS")
		}
	}()

	width := DetectWidth()
	if width != testCustomWidth {
		t.Errorf("DetectWidth() = %d, want %d", width, testCustomWidth)
	}
}

func TestDetectWidth_InvalidEnv(t *testing.T) {
	t.Parallel()

	originalColumns := os.Getenv("COLUMNS")

	os.Setenv("COLUMNS", "invalid") //nolint:usetesting // test helper pattern is intentional.

	defer func() {
		if originalColumns != "" {
			os.Setenv("COLUMNS", originalColumns) //nolint:usetesting // test helper pattern is intentional.
		} else {
			os.Unsetenv("COLUMNS")
		}
	}()

	width := DetectWidth()
	if width != testDefaultWidth {
		t.Errorf("DetectWidth() with invalid env = %d, want %d", width, testDefaultWidth)
	}
}

func TestNewConfig_Defaults(t *testing.T) {
	t.Parallel()

	originalColumns := os.Getenv("COLUMNS")

	os.Unsetenv("COLUMNS")

	defer func() {
		if originalColumns != "" {
			os.Setenv("COLUMNS", originalColumns) //nolint:usetesting // test helper pattern is intentional.
		}
	}()

	cfg := NewConfig()
	if cfg.Width != testDefaultWidth {
		t.Errorf("NewConfig().Width = %d, want %d", cfg.Width, testDefaultWidth)
	}

	if cfg.NoColor != false { //nolint:revive // explicit bool comparison needed.
		t.Errorf("NewConfig().NoColor = %v, want false", cfg.NoColor)
	}
}

func TestNewConfig_NoColorFromEnv(t *testing.T) {
	t.Parallel()

	originalNoColor := os.Getenv("NO_COLOR")

	os.Setenv("NO_COLOR", "1") //nolint:usetesting // test helper pattern is intentional.

	defer func() {
		if originalNoColor != "" {
			os.Setenv("NO_COLOR", originalNoColor) //nolint:usetesting // test helper pattern is intentional.
		} else {
			os.Unsetenv("NO_COLOR")
		}
	}()

	cfg := NewConfig()
	if cfg.NoColor != true { //nolint:revive // explicit bool comparison needed.
		t.Errorf("NewConfig().NoColor with NO_COLOR=1 = %v, want true", cfg.NoColor)
	}
}

func TestDrawProgressBar_Zero(t *testing.T) {
	t.Parallel()

	const barWidth = 10

	bar := DrawProgressBar(0.0, barWidth)

	expected := "░░░░░░░░░░"
	if bar != expected {
		t.Errorf("DrawProgressBar(0.0, %d) = %q, want %q", barWidth, bar, expected)
	}
}

func TestDrawProgressBar_Full(t *testing.T) {
	t.Parallel()

	const barWidth = 10

	bar := DrawProgressBar(1.0, barWidth)

	expected := "██████████"
	if bar != expected {
		t.Errorf("DrawProgressBar(1.0, %d) = %q, want %q", barWidth, bar, expected)
	}
}

func TestDrawProgressBar_Partial(t *testing.T) {
	t.Parallel()

	const barWidth = 10

	bar := DrawProgressBar(0.7, barWidth)

	expected := "███████░░░"
	if bar != expected {
		t.Errorf("DrawProgressBar(0.7, %d) = %q, want %q", barWidth, bar, expected)
	}
}

func TestDrawProgressBar_Clamps(t *testing.T) {
	t.Parallel()

	const barWidth = 10
	// Test negative clamps to 0.
	barNeg := DrawProgressBar(-0.5, barWidth)

	expectedNeg := "░░░░░░░░░░"
	if barNeg != expectedNeg {
		t.Errorf("DrawProgressBar(-0.5, %d) = %q, want %q", barWidth, barNeg, expectedNeg)
	}

	// Test >1 clamps to 1.
	barOver := DrawProgressBar(1.5, barWidth)

	expectedOver := "██████████"
	if barOver != expectedOver {
		t.Errorf("DrawProgressBar(1.5, %d) = %q, want %q", barWidth, barOver, expectedOver)
	}
}

func TestFormatScore(t *testing.T) {
	t.Parallel()

	tests := []struct {
		expected string
		score    float64
	}{
		{"0/10", 0.0},
		{"5/10", 0.5},
		{"8/10", 0.8},
		{"10/10", 1.0},
		{"8/10", 0.75}, // Rounds.
	}

	for _, tt := range tests {
		result := FormatScore(tt.score)
		if result != tt.expected {
			t.Errorf("FormatScore(%v) = %q, want %q", tt.score, result, tt.expected)
		}
	}
}

func TestFormatScoreBar(t *testing.T) {
	t.Parallel()

	const barWidth = 10

	result := FormatScoreBar(0.8, barWidth)

	expected := "[████████░░] 8/10"
	if result != expected {
		t.Errorf("FormatScoreBar(0.8, %d) = %q, want %q", barWidth, result, expected)
	}
}

func TestFormatStatus(t *testing.T) {
	t.Parallel()

	tests := []struct {
		status   string
		expected string
	}{
		{"good", StatusGood},
		{"fair", StatusWarning},
		{"poor", StatusBad},
		{"info", StatusInfo},
		{"unknown", StatusInfo}, // Default.
	}

	for _, tt := range tests {
		result := FormatStatus(tt.status)
		if result != tt.expected {
			t.Errorf("FormatStatus(%q) = %q, want %q", tt.status, result, tt.expected)
		}
	}
}

func TestTruncateWithEllipsis_Short(t *testing.T) {
	t.Parallel()

	const maxWidth = 10

	input := "hello"

	result := TruncateWithEllipsis(input, maxWidth)
	if result != input {
		t.Errorf("TruncateWithEllipsis(%q, %d) = %q, want %q", input, maxWidth, result, input)
	}
}

func TestTruncateWithEllipsis_Exact(t *testing.T) {
	t.Parallel()

	const maxWidth = 5

	input := "hello"

	result := TruncateWithEllipsis(input, maxWidth)
	if result != input {
		t.Errorf("TruncateWithEllipsis(%q, %d) = %q, want %q", input, maxWidth, result, input)
	}
}

func TestTruncateWithEllipsis_Long(t *testing.T) {
	t.Parallel()

	const maxWidth = 8

	input := "hello world"
	result := TruncateWithEllipsis(input, maxWidth)

	expected := "hello..."
	if result != expected {
		t.Errorf("TruncateWithEllipsis(%q, %d) = %q, want %q", input, maxWidth, result, expected)
	}
}

func TestTruncateWithEllipsis_TooSmall(t *testing.T) {
	t.Parallel()

	const maxWidth = 2

	input := "hello"
	result := TruncateWithEllipsis(input, maxWidth)

	expected := ".."
	if result != expected {
		t.Errorf("TruncateWithEllipsis(%q, %d) = %q, want %q", input, maxWidth, result, expected)
	}
}

func TestPadRight(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input    string
		expected string
		width    int
	}{
		{"hello", "hello     ", 10},
		{"hello", "hello", 5},
		{"hello", "hello", 3}, // Longer than width, no truncation.
		{"", "     ", 5},
	}

	for _, tt := range tests {
		result := PadRight(tt.input, tt.width)
		if result != tt.expected {
			t.Errorf("PadRight(%q, %d) = %q, want %q", tt.input, tt.width, result, tt.expected)
		}
	}
}

func TestPadLeft(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input    string
		expected string
		width    int
	}{
		{"hello", "     hello", 10},
		{"hello", "hello", 5},
		{"hello", "hello", 3}, // Longer than width, no truncation.
		{"", "     ", 5},
	}

	for _, tt := range tests {
		result := PadLeft(tt.input, tt.width)
		if result != tt.expected {
			t.Errorf("PadLeft(%q, %d) = %q, want %q", tt.input, tt.width, result, tt.expected)
		}
	}
}

func TestDrawSeparator(t *testing.T) {
	t.Parallel()

	const width = 10

	result := DrawSeparator(width)

	expected := "──────────"
	if result != expected {
		t.Errorf("DrawSeparator(%d) = %q, want %q", width, result, expected)
	}
}

func TestDrawSeparator_Zero(t *testing.T) {
	t.Parallel()

	result := DrawSeparator(0)
	if result != "" {
		t.Errorf("DrawSeparator(0) = %q, want empty string", result)
	}
}

func TestDrawHeader(t *testing.T) {
	t.Parallel()

	const width = 40

	result := DrawHeader("COMPLEXITY", "Score: 8/10", width)

	// Should contain title and right text.
	if !strings.Contains(result, "COMPLEXITY") {
		t.Errorf("DrawHeader should contain title, got %q", result)
	}

	if !strings.Contains(result, "Score: 8/10") {
		t.Errorf("DrawHeader should contain right text, got %q", result)
	}
	// Should have heavy box characters.
	if !strings.Contains(result, BoxHeavyTopLeft) {
		t.Errorf("DrawHeader should contain heavy top-left corner, got %q", result)
	}

	if !strings.Contains(result, BoxHeavyBottomLeft) {
		t.Errorf("DrawHeader should contain heavy bottom-left corner, got %q", result)
	}
}

func TestDrawHeader_TitleOnly(t *testing.T) {
	t.Parallel()

	const width = 30

	result := DrawHeader("IMPORTS", "", width)

	if !strings.Contains(result, "IMPORTS") {
		t.Errorf("DrawHeader should contain title, got %q", result)
	}
}

func TestColorize_Enabled(t *testing.T) {
	t.Parallel()

	cfg := Config{Width: testDefaultWidth, NoColor: false}
	result := cfg.Colorize("hello", ColorGreen)

	// Should contain ANSI escape codes.
	if !strings.Contains(result, "\033[") {
		t.Errorf("Colorize with color enabled should contain ANSI codes, got %q", result)
	}

	if !strings.Contains(result, "hello") {
		t.Errorf("Colorize should contain original text, got %q", result)
	}
}

func TestColorize_Disabled(t *testing.T) {
	t.Parallel()

	cfg := Config{Width: testDefaultWidth, NoColor: true}
	result := cfg.Colorize("hello", ColorGreen)

	// Should NOT contain ANSI escape codes.
	if strings.Contains(result, "\033[") {
		t.Errorf("Colorize with NoColor should not contain ANSI codes, got %q", result)
	}

	if result != "hello" {
		t.Errorf("Colorize with NoColor = %q, want %q", result, "hello")
	}
}

func TestColorForScore(t *testing.T) {
	t.Parallel()

	tests := []struct {
		score    float64
		expected Color
	}{
		{0.9, ColorGreen},
		{0.7, ColorYellow},
		{0.3, ColorRed},
	}

	for _, tt := range tests {
		result := ColorForScore(tt.score)
		if result != tt.expected {
			t.Errorf("ColorForScore(%v) = %v, want %v", tt.score, result, tt.expected)
		}
	}
}

func TestDrawPercentBar(t *testing.T) {
	t.Parallel()

	const labelWidth = 15

	const barWidth = 20

	const count = 68

	const percent = 0.68

	result := DrawPercentBar("Simple (1-5)", percent, count, labelWidth, barWidth)

	// Should contain label.
	if !strings.Contains(result, "Simple (1-5)") {
		t.Errorf("DrawPercentBar should contain label, got %q", result)
	}
	// Should contain percentage.
	if !strings.Contains(result, "68%") {
		t.Errorf("DrawPercentBar should contain percentage, got %q", result)
	}
	// Should contain count.
	if !strings.Contains(result, "(68)") {
		t.Errorf("DrawPercentBar should contain count, got %q", result)
	}
	// Should contain progress bar characters.
	if !strings.Contains(result, ProgressFilled) {
		t.Errorf("DrawPercentBar should contain filled progress chars, got %q", result)
	}
}
