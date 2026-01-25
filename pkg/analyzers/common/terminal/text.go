package terminal

import "strings"

// Ellipsis is appended to truncated strings.
const Ellipsis = "..."

// EllipsisLen is the length of the ellipsis string.
const EllipsisLen = 3

// TruncateWithEllipsis truncates s to maxWidth, adding "..." if truncated.
// If maxWidth is less than EllipsisLen, returns truncated ellipsis.
func TruncateWithEllipsis(s string, maxWidth int) string {
	if len(s) <= maxWidth {
		return s
	}

	if maxWidth <= EllipsisLen {
		return strings.Repeat(".", maxWidth)
	}

	return s[:maxWidth-EllipsisLen] + Ellipsis
}

// PadRight pads s with spaces on the right to reach width.
// If s is already longer than width, returns s unchanged.
func PadRight(s string, width int) string {
	if len(s) >= width {
		return s
	}
	return s + strings.Repeat(" ", width-len(s))
}

// PadLeft pads s with spaces on the left to reach width.
// If s is already longer than width, returns s unchanged.
func PadLeft(s string, width int) string {
	if len(s) >= width {
		return s
	}
	return strings.Repeat(" ", width-len(s)) + s
}
