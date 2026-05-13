package analyze

import "time"

// TickBounds holds the time boundaries of a single tick.
type TickBounds struct {
	StartTime time.Time
	EndTime   time.Time
}

// FormatStartTime returns StartTime as an RFC 3339 string, or empty if zero.
func (b TickBounds) FormatStartTime() string {
	if b.StartTime.IsZero() {
		return ""
	}

	return b.StartTime.UTC().Format(time.RFC3339)
}

// FormatEndTime returns EndTime as an RFC 3339 string, or empty if zero.
func (b TickBounds) FormatEndTime() string {
	if b.EndTime.IsZero() {
		return ""
	}

	return b.EndTime.UTC().Format(time.RFC3339)
}

// BuildTickBounds extracts tick boundaries from a slice of TICKs.
// Returns a map from tick index to its time bounds.
func BuildTickBounds(ticks []TICK) map[int]TickBounds {
	if len(ticks) == 0 {
		return nil
	}

	result := make(map[int]TickBounds, len(ticks))

	for _, tick := range ticks {
		result[tick.Tick] = TickBounds{
			StartTime: tick.StartTime,
			EndTime:   tick.EndTime,
		}
	}

	return result
}
