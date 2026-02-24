package sentiment

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestComputeSentiment_Empty(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, float64(0), float64(ComputeSentiment(nil)), 0.01)
	assert.InDelta(t, float64(0), float64(ComputeSentiment([]string{})), 0.01)
}

func TestComputeSentiment_WhitespaceOnly(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, float64(0), float64(ComputeSentiment([]string{"  ", "\t", ""})), 0.01)
}

func TestComputeSentiment_Positive(t *testing.T) {
	t.Parallel()

	score := ComputeSentiment([]string{"This is a great fix!"})
	assert.GreaterOrEqual(t, float64(score), float64(SentimentPositiveThreshold))
}

func TestComputeSentiment_Negative(t *testing.T) {
	t.Parallel()

	score := ComputeSentiment([]string{"This code is broken and terrible."})
	assert.LessOrEqual(t, float64(score), float64(SentimentNegativeThreshold))
}

func TestComputeSentiment_Neutral(t *testing.T) {
	t.Parallel()

	// VADER classifies factual code comments as neutral.
	score := ComputeSentiment([]string{"The function handles input validation."})
	assert.Greater(t, float64(score), float64(SentimentNegativeThreshold))
	assert.Less(t, float64(score), float64(SentimentPositiveThreshold))
}

func TestComputeSentiment_Mixed(t *testing.T) {
	t.Parallel()

	// One positive, one negative comment — average should be near neutral.
	score := ComputeSentiment([]string{"This is great!", "This is broken."})
	assert.Greater(t, float64(score), float64(SentimentNegativeThreshold))
	assert.Less(t, float64(score), float64(SentimentPositiveThreshold))
}

func TestComputeSentiment_MultipleComments(t *testing.T) {
	t.Parallel()

	comments := []string{"good work", "nice refactor", "clean code"}
	score := ComputeSentiment(comments)
	assert.GreaterOrEqual(t, float64(score), float64(SentimentPositiveThreshold))
}

func TestComputeSentiment_HeavyNegative(t *testing.T) {
	t.Parallel()

	score := ComputeSentiment([]string{"This is terrible awful horrible broken bug hack"})
	assert.LessOrEqual(t, float64(score), float64(SentimentNegativeThreshold))
}
