package sentiment

import (
	"strings"
	"sync"

	"github.com/jonreiter/govader"
)

var (
	vaderAnalyzer *govader.SentimentIntensityAnalyzer
	vaderOnce     sync.Once
)

func getVaderAnalyzer() *govader.SentimentIntensityAnalyzer {
	vaderOnce.Do(func() {
		vaderAnalyzer = govader.NewSentimentIntensityAnalyzer()
	})

	return vaderAnalyzer
}

// vaderCompoundRange maps VADER [-1,1] to our [0,1] via (compound+1)/2.
const vaderCompoundRange = 2

// vaderCompoundToScore maps VADER compound score (-1..1) to our [0, 1] range.
// Compound -1 (most negative) -> 0, 0 (neutral) -> 0.5, 1 (most positive) -> 1.
func vaderCompoundToScore(compound float64) float32 {
	score := (compound + 1) / vaderCompoundRange

	if score < 0 {
		return 0
	}

	if score > 1 {
		return 1
	}

	return float32(score)
}

// ComputeSentiment returns a score in [0, 1] for the given comments.
// 0 = negative, 0.5 = neutral, 1 = positive.
// Uses VADER (Valence Aware Dictionary and sEntiment Reasoner) for NLP-based analysis.
// Empty comments yield 0 (no comment implies no sentiment signal).
// Multiple comments are averaged.
func ComputeSentiment(comments []string) float32 {
	if len(comments) == 0 {
		return 0
	}

	analyzer := getVaderAnalyzer()

	var sum float32

	n := 0

	for _, c := range comments {
		c = strings.TrimSpace(c)
		if c == "" {
			continue
		}

		scores := analyzer.PolarityScores(c)
		sum += vaderCompoundToScore(scores.Compound)
		n++
	}

	if n == 0 {
		return 0
	}

	return sum / float32(n)
}
