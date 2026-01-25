// Package terminal provides terminal rendering utilities for beautiful CLI output.
package terminal

import (
	"os"
	"strconv"
)

// Default width constants
const (
	DefaultWidth = 80
	MinWidth     = 60
	MaxWidth     = 120
)

// Config holds terminal rendering configuration.
type Config struct {
	Width   int
	NoColor bool
}

// NewConfig creates a Config with sensible defaults from environment.
func NewConfig() Config {
	return Config{
		Width:   DetectWidth(),
		NoColor: os.Getenv("NO_COLOR") != "",
	}
}

// DetectWidth returns the terminal width from COLUMNS environment variable,
// or DefaultWidth if not set or invalid.
func DetectWidth() int {
	columnsEnv := os.Getenv("COLUMNS")
	if columnsEnv == "" {
		return DefaultWidth
	}

	width, err := strconv.Atoi(columnsEnv)
	if err != nil {
		return DefaultWidth
	}

	return width
}
