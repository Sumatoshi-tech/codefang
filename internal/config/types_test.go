package config_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/config"
)

func validConfig() config.Config {
	return config.Config{
		Pipeline: config.PipelineConfig{
			Workers:         4,
			DiffCacheSize:   1000,
			CommitBatchSize: 100,
			GOGC:            200,
			BallastSize:     "0",
		},
		History: config.HistoryConfig{
			Burndown: config.BurndownConfig{
				Granularity:          30,
				Sampling:             30,
				HibernationThreshold: 1000,
			},
			Imports: config.ImportsConfig{
				Goroutines:  4,
				MaxFileSize: 1 << 20,
			},
			Sentiment: config.SentimentConfig{
				MinCommentLength: 20,
				Gap:              0.5,
			},
			Typos: config.TyposConfig{
				MaxDistance: 4,
			},
		},
		Checkpoint: config.CheckpointConfig{
			Enabled: true,
			Resume:  true,
		},
	}
}

func TestValidate_ValidConfig_NoError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	require.NoError(t, cfg.Validate())
}

func TestValidate_ZeroConfig_NoError(t *testing.T) {
	t.Parallel()

	cfg := config.Config{}
	require.NoError(t, cfg.Validate())
}

func TestValidate_InvalidWorkers_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.Pipeline.Workers = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidWorkers)
}

func TestValidate_InvalidDiffCacheSize_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.Pipeline.DiffCacheSize = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidDiffCacheSize)
}

func TestValidate_InvalidCommitBatchSize_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.Pipeline.CommitBatchSize = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidCommitBatchSize)
}

func TestValidate_InvalidGOGC_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.Pipeline.GOGC = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidGOGC)
}

func TestValidate_InvalidBurndownGranularity_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Burndown.Granularity = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidBurndownGranularity)
}

func TestValidate_InvalidBurndownSampling_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Burndown.Sampling = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidBurndownSampling)
}

func TestValidate_InvalidSentimentMinLength_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Sentiment.MinCommentLength = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidSentimentMinLength)
}

func TestValidate_InvalidSentimentGap_Negative_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Sentiment.Gap = -0.1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidSentimentGap)
}

func TestValidate_InvalidSentimentGap_TooHigh_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Sentiment.Gap = 1.1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidSentimentGap)
}

func TestValidate_InvalidTyposMaxDistance_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Typos.MaxDistance = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidTyposMaxDistance)
}

func TestValidate_InvalidImportsGoroutines_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Imports.Goroutines = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidImportsGoroutines)
}

func TestValidate_InvalidImportsMaxFileSize_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg := validConfig()
	cfg.History.Imports.MaxFileSize = -1

	err := cfg.Validate()
	assert.ErrorIs(t, err, config.ErrInvalidImportsMaxFileSize)
}
