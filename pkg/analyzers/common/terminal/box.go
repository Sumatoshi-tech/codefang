package terminal

import "strings"

// Box drawing characters - light
const (
	BoxHorizontal   = "─"
	BoxVertical     = "│"
	BoxTopLeft      = "┌"
	BoxTopRight     = "┐"
	BoxBottomLeft   = "└"
	BoxBottomRight  = "┘"
	BoxCross        = "┼"
	BoxVerticalLeft = "┤"
)

// Box drawing characters - heavy
const (
	BoxHeavyHorizontal  = "━"
	BoxHeavyVertical    = "┃"
	BoxHeavyTopLeft     = "┏"
	BoxHeavyTopRight    = "┓"
	BoxHeavyBottomLeft  = "┗"
	BoxHeavyBottomRight = "┛"
)

// Box drawing characters - rounded
const (
	BoxRoundTopLeft     = "╭"
	BoxRoundTopRight    = "╮"
	BoxRoundBottomLeft  = "╰"
	BoxRoundBottomRight = "╯"
)

// DrawSeparator draws a thin horizontal separator line.
func DrawSeparator(width int) string {
	if width <= 0 {
		return ""
	}
	return strings.Repeat(BoxHorizontal, width)
}

// HeaderPadding is the space around header content.
const HeaderPadding = 1

// DrawHeader draws a heavy-bordered section header.
// ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
// ┃ TITLE                     rightText ┃
// ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
func DrawHeader(title, rightText string, width int) string {
	// Minimum width check
	minRequired := len(title) + len(rightText) + 4 + (HeaderPadding * 2)
	if width < minRequired {
		width = minRequired
	}

	innerWidth := width - 2 // subtract left and right borders

	// Build top border
	topBorder := BoxHeavyTopLeft + strings.Repeat(BoxHeavyHorizontal, innerWidth) + BoxHeavyTopRight

	// Build content line
	contentWidth := innerWidth - (HeaderPadding * 2)
	var content string
	if rightText == "" {
		content = PadRight(title, contentWidth)
	} else {
		gap := contentWidth - len(title) - len(rightText)
		if gap < 1 {
			gap = 1
		}
		content = title + strings.Repeat(" ", gap) + rightText
	}
	contentLine := BoxHeavyVertical + strings.Repeat(" ", HeaderPadding) + content + strings.Repeat(" ", HeaderPadding) + BoxHeavyVertical

	// Build bottom border
	bottomBorder := BoxHeavyBottomLeft + strings.Repeat(BoxHeavyHorizontal, innerWidth) + BoxHeavyBottomRight

	return topBorder + "\n" + contentLine + "\n" + bottomBorder
}
