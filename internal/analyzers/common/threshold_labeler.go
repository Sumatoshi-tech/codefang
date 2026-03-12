package common

// ThresholdLabeler maps a float64 score to a string label using an ordered list
// of Threshold[float64] values. Thresholds must be sorted descending by Limit
// (highest first) — the first threshold where score >= Limit wins.
// A catch-all fallback can be added as the last entry with Limit set to 0 or
// the minimum possible score.
//
// Example:
//
//	labeler := common.ThresholdLabeler{
//	    {Limit: 0.8, Label: "Excellent"},
//	    {Limit: 0.6, Label: "Good"},
//	    {Limit: 0.4, Label: "Fair"},
//	    {Limit: 0.0, Label: "Poor"},
//	}
//	labeler.Label(0.75) // → "Good"
type ThresholdLabeler []Threshold[float64]

// Label returns the label of the first threshold where score >= Limit.
// Returns "" if the labeler is empty or no threshold matches.
func (l ThresholdLabeler) Label(score float64) string {
	for _, t := range l {
		if score >= t.Limit {
			return t.Label
		}
	}

	return ""
}
