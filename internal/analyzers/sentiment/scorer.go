package sentiment

import (
	"math"
	"strings"
	"sync"

	"github.com/jonreiter/govader"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/sentiment/lexicons"
)

var (
	vaderAnalyzer *govader.SentimentIntensityAnalyzer
	vaderOnce     sync.Once
)

func getVaderAnalyzer() *govader.SentimentIntensityAnalyzer {
	vaderOnce.Do(func() {
		vaderAnalyzer = govader.NewSentimentIntensityAnalyzer()
		injectMultilingualLexicons(vaderAnalyzer)
	})

	return vaderAnalyzer
}

// injectMultilingualLexicons adds multilingual sentiment entries to VADER's lexicon.
// VADER's Lexicon field is a public map[string]float64 — we inject non-English
// entries from the Chen-Skiena dataset so that VADER can score comments in
// 32+ languages without requiring translation.
//
// Only words containing non-ASCII characters are injected to avoid overriding
// VADER's built-in English lexicon with lower-quality bilingual entries.
func injectMultilingualLexicons(sia *govader.SentimentIntensityAnalyzer) {
	entries := lexicons.All()

	for _, entry := range entries {
		if isASCIIOnly(entry.Word) {
			continue
		}

		lower := strings.ToLower(entry.Word)
		if _, exists := sia.Lexicon[lower]; !exists {
			sia.Lexicon[lower] = entry.Valence
		}
	}
}

// isASCIIOnly returns true if all bytes in s are ASCII (< 128).
func isASCIIOnly(s string) bool {
	for i := range len(s) {
		if s[i] >= 128 { //nolint:mnd // ASCII boundary
			return false
		}
	}

	return true
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

// SE-domain lexicon: technical terms VADER misclassifies.
// Adjustments push the compound score toward neutral (0) for false signals.
// Negative-sounding but neutral in SE: kill, abort, fatal, terminate, etc.
// Positive-sounding but neutral in SE: master, execute, exploit, etc.
// Actually negative in SE: hack, kludge, workaround, technical debt.
// seDomainNeutralizers maps technical terms that VADER misclassifies to
// a target compound score. Terms like "kill process" are violent in natural
// language but neutral in SE context.
// Only terms with strong false-signal (>0.3 VADER magnitude) are included.
var seDomainNeutralizers = map[string]float64{
	"kill":       0.0,
	"killed":     0.0,
	"killing":    0.0,
	"abort":      0.0,
	"aborted":    0.0,
	"aborting":   0.0,
	"fatal":      0.0,
	"dead":       0.0,
	"terminate":  0.0,
	"terminated": 0.0,
	"destroy":    0.0,
	"panic":      0.0,
	"deprecated": 0.0,
	"obsolete":   0.0,
	"master":     0.0,
	"execute":    0.0,
	"exploit":    0.0,
	"conflict":   0.0,
	"revert":     0.0,
	"reject":     0.0,
	"rejected":   0.0,
	"critical":   0.0,
}

// seNegativeTerms are terms that indicate genuinely negative sentiment in SE context.
// These push the compound toward negative.
var seNegativeTerms = map[string]float64{
	"hack":           -0.3,
	"hacky":          -0.4,
	"kludge":         -0.5,
	"workaround":     -0.2,
	"technical debt": -0.3,
	"spaghetti":      -0.4,
	"awful":          -0.3,
	"terrible":       -0.3,
	"nightmare":      -0.4,
	"horrible":       -0.3,
}

// neutralizerWeight controls how much domain adjustment affects the final score.
// 0 = no effect, 1 = full adjustment. 0.8 strongly favors domain knowledge
// because VADER aggressively penalizes terms like "kill" and "fatal" that
// are emotionally neutral in software engineering.
const neutralizerWeight = 0.8

// maxWeightRatio caps comment length weight to prevent single long comments from dominating.
const maxWeightRatio = 3.0

// applySEDomainAdjustment adjusts VADER compound score for SE-domain terms.
// Returns adjusted compound score in [-1, 1].
func applySEDomainAdjustment(text string, compound float64) float64 {
	lower := strings.ToLower(text)
	adjustment := 0.0
	count := 0

	for term, shift := range seDomainNeutralizers {
		if strings.Contains(lower, term) {
			adjustment += (shift - compound) * neutralizerWeight
			count++
		}
	}

	for term, shift := range seNegativeTerms {
		if strings.Contains(lower, term) {
			adjustment += shift
			count++
		}
	}

	if count == 0 {
		return compound
	}

	adjusted := compound + adjustment/float64(count)

	return math.Max(-1, math.Min(1, adjusted))
}

// ComputeSentiment returns a score in [0, 1] for the given comments.
// 0 = negative, 0.5 = neutral, 1 = positive.
// Uses VADER with SE-domain adjustments for NLP-based analysis.
// Empty comments yield 0 (no comment implies no sentiment signal).
// Comments are weighted by length (longer comments carry more signal).
func ComputeSentiment(comments []string) float32 {
	if len(comments) == 0 {
		return 0
	}

	analyzer := getVaderAnalyzer()

	var weightedSum float64

	var totalWeight float64

	avgLen := averageCommentLength(comments)

	for _, c := range comments {
		c = strings.TrimSpace(c)
		if c == "" {
			continue
		}

		scores := analyzer.PolarityScores(c)
		adjusted := applySEDomainAdjustment(c, scores.Compound)

		weight := commentWeight(len(c), avgLen)
		weightedSum += float64(vaderCompoundToScore(adjusted)) * weight
		totalWeight += weight
	}

	if totalWeight == 0 {
		return 0
	}

	return float32(weightedSum / totalWeight)
}

// averageCommentLength returns the mean length of non-empty comments.
func averageCommentLength(comments []string) float64 {
	total := 0
	count := 0

	for _, c := range comments {
		c = strings.TrimSpace(c)
		if c == "" {
			continue
		}

		total += len(c)
		count++
	}

	if count == 0 {
		return 1
	}

	return float64(total) / float64(count)
}

// commentWeight returns the weight for a comment based on its length relative to the average.
// Longer comments get more weight, capped at maxWeightRatio to prevent dominance.
func commentWeight(length int, avgLength float64) float64 {
	if avgLength <= 0 {
		return 1
	}

	ratio := float64(length) / avgLength

	return math.Min(ratio, maxWeightRatio)
}
