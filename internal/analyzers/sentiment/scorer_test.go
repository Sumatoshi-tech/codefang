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

func TestComputeSentiment_SEDomainNeutralTerms(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		comment string
	}{
		{"kill_process", "Kill the background process when idle"},
		{"abort_transaction", "Abort the transaction on timeout"},
		{"fatal_error", "Log fatal error and exit gracefully"},
		{"terminate_thread", "Terminate the worker thread after cleanup"},
		{"deprecated_api", "This deprecated API will be removed next version"},
		{"panic_handler", "Panic handler catches unrecoverable errors"},
		{"execute_command", "Execute the shell command with the given arguments"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			score := ComputeSentiment([]string{tt.comment})
			assert.Greater(t, float64(score), float64(SentimentNegativeThreshold),
				"SE technical term should not produce negative sentiment: %s", tt.comment)
		})
	}
}

func TestComputeSentiment_SEDomainNegativeTerms(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		comment string
	}{
		{"hacky_code", "This is a really hacky workaround for the issue"},
		{"spaghetti", "This spaghetti code needs serious refactoring"},
		{"nightmare", "This codebase is a nightmare to maintain"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			score := ComputeSentiment([]string{tt.comment})
			assert.LessOrEqual(t, float64(score), float64(SentimentNegativeThreshold),
				"SE negative term should produce negative sentiment: %s", tt.comment)
		})
	}
}

func TestComputeSentiment_LengthWeighting(t *testing.T) {
	t.Parallel()

	short := "bad"
	long := "This function is well-designed and implements the algorithm correctly with great readability"

	score := ComputeSentiment([]string{short, long})
	longOnly := ComputeSentiment([]string{long})

	assert.Greater(t, float64(score), float64(SentimentNegativeThreshold),
		"longer positive comment should outweigh short negative one")
	assert.InDelta(t, float64(longOnly), float64(score), 0.15,
		"length-weighted score should be closer to the longer comment's score")
}

func TestApplySEDomainAdjustment_NoTerms(t *testing.T) {
	t.Parallel()

	result := applySEDomainAdjustment("simple regular comment", 0.5)
	assert.InDelta(t, 0.5, result, floatDelta, "no SE terms should leave compound unchanged")
}

func TestApplySEDomainAdjustment_WithNeutralizer(t *testing.T) {
	t.Parallel()

	result := applySEDomainAdjustment("kill the process", -0.6)
	assert.Greater(t, result, -0.6, "neutralizer should push negative compound toward neutral")
}

func TestApplySEDomainAdjustment_WithNegativeTerm(t *testing.T) {
	t.Parallel()

	result := applySEDomainAdjustment("this is a terrible hack", 0.0)
	assert.Less(t, result, 0.0, "SE negative term should push compound toward negative")
}

func TestApplySEDomainAdjustment_ClampsBounds(t *testing.T) {
	t.Parallel()

	result := applySEDomainAdjustment("nightmare spaghetti awful terrible hack kludge", 0.9)
	assert.GreaterOrEqual(t, result, -1.0, "result should be >= -1")
	assert.LessOrEqual(t, result, 1.0, "result should be <= 1")
}

func TestVaderCompoundToScore_Boundaries(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		compound float64
		expected float32
	}{
		{"most_negative", -1.0, 0.0},
		{"neutral", 0.0, 0.5},
		{"most_positive", 1.0, 1.0},
		{"below_range", -2.0, 0.0},
		{"above_range", 2.0, 1.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := vaderCompoundToScore(tt.compound)
			assert.InDelta(t, float64(tt.expected), float64(result), floatDelta)
		})
	}
}

func TestCommentWeight(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		length    int
		avgLength float64
		expected  float64
	}{
		{"equal_to_avg", 50, 50.0, 1.0},
		{"double_avg", 100, 50.0, 2.0},
		{"capped_at_max", 500, 50.0, maxWeightRatio},
		{"zero_avg", 50, 0.0, 1.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := commentWeight(tt.length, tt.avgLength)
			assert.InDelta(t, tt.expected, result, floatDelta)
		})
	}
}

func TestComputeSentiment_MultilingualScoring(t *testing.T) {
	t.Parallel()

	ruPos := "отлично успешно"
	ruNeg := "плохо ошибка ужасно"

	t.Run("russian_positive", func(t *testing.T) {
		t.Parallel()

		score := ComputeSentiment([]string{ruPos})
		assert.Greater(t, float64(score), float64(SentimentNegativeThreshold))
	})

	t.Run("russian_negative", func(t *testing.T) {
		t.Parallel()

		score := ComputeSentiment([]string{ruNeg})
		assert.Less(t, float64(score), float64(SentimentPositiveThreshold))
	})
}

func TestInjectMultilingualLexicons(t *testing.T) {
	t.Parallel()

	analyzer := getVaderAnalyzer()

	assert.Greater(t, len(analyzer.Lexicon), 7500,
		"lexicon should contain more than base VADER entries after injection")
}

func TestIsASCIIOnly(t *testing.T) {
	t.Parallel()

	assert.True(t, isASCIIOnly("hello"))
	assert.True(t, isASCIIOnly("fix123"))
	assert.True(t, isASCIIOnly(""))
	assert.False(t, isASCIIOnly("\u043f\u043b\u043e\u0445\u043e"))
	assert.False(t, isASCIIOnly("\u597d"))
}

func TestAverageCommentLength(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		comments []string
		expected float64
	}{
		{"empty", nil, 1.0},
		{"single", []string{"hello"}, 5.0},
		{"whitespace_only", []string{"  ", "\t"}, 1.0},
		{"mixed", []string{"ab", "abcd", "abcdef"}, 4.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := averageCommentLength(tt.comments)
			assert.InDelta(t, tt.expected, result, floatDelta)
		})
	}
}
