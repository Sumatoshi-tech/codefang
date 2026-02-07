// Package streaming provides chunk orchestration for large repository analysis.
package streaming

import "errors"

// Mode represents the streaming mode setting.
type Mode int

// Streaming mode constants.
const (
	ModeAuto Mode = iota
	ModeOn
	ModeOff
)

// ErrInvalidMode is returned when parsing an invalid mode string.
var ErrInvalidMode = errors.New("invalid streaming mode")

// ParseMode converts a string to a Mode value.
func ParseMode(s string) (Mode, error) {
	switch s {
	case "auto":
		return ModeAuto, nil
	case "on":
		return ModeOn, nil
	case "off":
		return ModeOff, nil
	default:
		return ModeAuto, ErrInvalidMode
	}
}
