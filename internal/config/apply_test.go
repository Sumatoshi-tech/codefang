// FRD: specs/frds/FRD-20260302-config-loader-facts.md.
package config_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/config"
)

const (
	factBurndownGranularity          = "Burndown.Granularity"
	factBurndownSampling             = "Burndown.Sampling"
	factBurndownTrackFiles           = "Burndown.TrackFiles"
	factBurndownTrackPeople          = "Burndown.TrackPeople"
	factBurndownHibernationThreshold = "Burndown.HibernationThreshold"
	factBurndownHibernationOnDisk    = "Burndown.HibernationOnDisk"
	factBurndownHibernationDirectory = "Burndown.HibernationDirectory"
	factBurndownDebug                = "Burndown.Debug"
	factBurndownGoroutines           = "Burndown.Goroutines"
	factDevsConsiderEmpty            = "Devs.ConsiderEmptyCommits"
	factDevsAnonymize                = "Devs.Anonymize"
	factImportsGoroutines            = "Imports.Goroutines"
	factImportsMaxFileSize           = "Imports.MaxFileSize"
	factSentimentMinLength           = "CommentSentiment.MinLength"
	factSentimentGap                 = "CommentSentiment.Gap"
	factShotnessDSLStruct            = "Shotness.DSLStruct"
	factShotnessDSLName              = "Shotness.DSLName"
	factTyposMaxDistance             = "TyposDatasetBuilder.MaximumAllowedDistance"
	factAnomalyThreshold             = "TemporalAnomaly.Threshold"
	factAnomalyWindowSize            = "TemporalAnomaly.WindowSize"
)

func TestApplyToFacts_Burndown(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Burndown: config.BurndownConfig{
				Granularity:          60,
				Sampling:             60,
				TrackFiles:           true,
				TrackPeople:          true,
				HibernationThreshold: 2000,
				HibernationToDisk:    true,
				HibernationDirectory: "/tmp/hib",
				Debug:                true,
				Goroutines:           16,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	expectedGranularity := 60
	expectedSampling := 60
	expectedHibThreshold := 2000
	expectedGoroutines := 16

	assert.Equal(t, expectedGranularity, facts[factBurndownGranularity])
	assert.Equal(t, expectedSampling, facts[factBurndownSampling])
	assert.Equal(t, true, facts[factBurndownTrackFiles])
	assert.Equal(t, true, facts[factBurndownTrackPeople])
	assert.Equal(t, expectedHibThreshold, facts[factBurndownHibernationThreshold])
	assert.Equal(t, true, facts[factBurndownHibernationOnDisk])
	assert.Equal(t, "/tmp/hib", facts[factBurndownHibernationDirectory])
	assert.Equal(t, true, facts[factBurndownDebug])
	assert.Equal(t, expectedGoroutines, facts[factBurndownGoroutines])
}

func TestApplyToFacts_Devs(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Devs: config.DevsConfig{
				ConsiderEmptyCommits: true,
				Anonymize:            true,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	assert.Equal(t, true, facts[factDevsConsiderEmpty])
	assert.Equal(t, true, facts[factDevsAnonymize])
}

func TestApplyToFacts_Imports(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Imports: config.ImportsConfig{
				Goroutines:  16,
				MaxFileSize: 4194304,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	expectedGoroutines := 16
	expectedMaxFileSize := 4194304

	assert.Equal(t, expectedGoroutines, facts[factImportsGoroutines])
	assert.Equal(t, expectedMaxFileSize, facts[factImportsMaxFileSize])
}

func TestApplyToFacts_Sentiment(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Sentiment: config.SentimentConfig{
				MinCommentLength: 40,
				Gap:              0.8,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	expectedMinLength := 40

	assert.Equal(t, expectedMinLength, facts[factSentimentMinLength])
	assert.InDelta(t, float32(0.8), facts[factSentimentGap], 0.001)
}

func TestApplyToFacts_Shotness(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Shotness: config.ShotnessConfig{
				DSLStruct: `filter(.roles has "Class")`,
				DSLName:   ".props.identifier",
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	assert.Equal(t, `filter(.roles has "Class")`, facts[factShotnessDSLStruct])
	assert.Equal(t, ".props.identifier", facts[factShotnessDSLName])
}

func TestApplyToFacts_Typos(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Typos: config.TyposConfig{
				MaxDistance: 6,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	expectedMaxDistance := 6

	assert.Equal(t, expectedMaxDistance, facts[factTyposMaxDistance])
}

func TestApplyToFacts_Anomaly(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Anomaly: config.AnomalyConfig{
				Threshold:  3.5,
				WindowSize: 30,
			},
		},
	}

	facts := make(map[string]any)
	cfg.ApplyToFacts(facts)

	expectedWindowSize := 30

	assert.InDelta(t, float32(3.5), facts[factAnomalyThreshold], 0.001)
	assert.Equal(t, expectedWindowSize, facts[factAnomalyWindowSize])
}

func TestApplyToFacts_ZeroValues_SkipsNumericOverrides(t *testing.T) {
	t.Parallel()

	cfg := config.Config{}
	facts := map[string]any{
		factBurndownGranularity: 30,
		factTyposMaxDistance:    4,
	}

	cfg.ApplyToFacts(facts)

	assert.Equal(t, 30, facts[factBurndownGranularity])
	assert.Equal(t, 4, facts[factTyposMaxDistance])
}

func TestApplyToFacts_BooleanFields_AlwaysApplied(t *testing.T) {
	t.Parallel()

	cfg := config.Config{
		History: config.HistoryConfig{
			Burndown: config.BurndownConfig{
				TrackFiles:  false,
				TrackPeople: false,
				Debug:       false,
			},
			Devs: config.DevsConfig{
				ConsiderEmptyCommits: false,
				Anonymize:            false,
			},
		},
	}

	facts := map[string]any{
		factBurndownTrackFiles:  true,
		factBurndownTrackPeople: true,
	}

	cfg.ApplyToFacts(facts)

	assert.Equal(t, false, facts[factBurndownTrackFiles])
	assert.Equal(t, false, facts[factBurndownTrackPeople])
	assert.Equal(t, false, facts[factDevsConsiderEmpty])
	assert.Equal(t, false, facts[factDevsAnonymize])
}
