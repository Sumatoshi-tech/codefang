package config

import (
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/viper"
)

// configName is the config file name without extension.
const configName = ".codefang"

// configType is the config file format.
const configType = "yaml"

// envPrefix is the environment variable prefix for codefang settings.
const envPrefix = "CODEFANG"

// envKeySeparator is the nested key separator in environment variable names.
const envKeySeparator = "_"

// LoadConfig loads configuration from file, env vars, and defaults.
// If configPath is non-empty, it is used as the explicit config file path.
// Otherwise, the config file is searched in CWD and $HOME.
// Missing config file is not an error; defaults are used.
func LoadConfig(configPath string) (*Config, error) {
	viperCfg := viper.New()

	applyDefaults(viperCfg)

	viperCfg.SetConfigType(configType)
	viperCfg.SetEnvPrefix(envPrefix)
	viperCfg.SetEnvKeyReplacer(strings.NewReplacer(".", envKeySeparator))
	viperCfg.AutomaticEnv()

	if configPath != "" {
		viperCfg.SetConfigFile(configPath)
	} else {
		viperCfg.SetConfigName(configName)
		viperCfg.AddConfigPath(".")

		home, err := os.UserHomeDir()
		if err == nil {
			viperCfg.AddConfigPath(home)
		}
	}

	readErr := viperCfg.ReadInConfig()
	if readErr != nil {
		var notFound viper.ConfigFileNotFoundError
		if !errors.As(readErr, &notFound) {
			return nil, fmt.Errorf("read config: %w", readErr)
		}
	}

	var cfg Config

	unmarshalErr := viperCfg.Unmarshal(&cfg)
	if unmarshalErr != nil {
		return nil, fmt.Errorf("unmarshal config: %w", unmarshalErr)
	}

	validateErr := cfg.Validate()
	if validateErr != nil {
		return nil, fmt.Errorf("validate config: %w", validateErr)
	}

	return &cfg, nil
}

func applyDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("analyzers", []string{})

	viperCfg.SetDefault("pipeline.workers", DefaultPipelineWorkers)
	viperCfg.SetDefault("pipeline.memory_budget", DefaultPipelineMemoryBudget)
	viperCfg.SetDefault("pipeline.blob_cache_size", DefaultPipelineBlobCacheSize)
	viperCfg.SetDefault("pipeline.diff_cache_size", DefaultPipelineDiffCacheSize)
	viperCfg.SetDefault("pipeline.blob_arena_size", DefaultPipelineBlobArenaSize)
	viperCfg.SetDefault("pipeline.commit_batch_size", DefaultPipelineCommitBatchSize)
	viperCfg.SetDefault("pipeline.gogc", DefaultPipelineGOGC)
	viperCfg.SetDefault("pipeline.ballast_size", DefaultPipelineBallastSize)

	viperCfg.SetDefault("history.burndown.granularity", DefaultBurndownGranularity)
	viperCfg.SetDefault("history.burndown.sampling", DefaultBurndownSampling)
	viperCfg.SetDefault("history.burndown.track_files", DefaultBurndownTrackFiles)
	viperCfg.SetDefault("history.burndown.track_people", DefaultBurndownTrackPeople)
	viperCfg.SetDefault("history.burndown.hibernation_threshold", DefaultBurndownHibernationThreshold)
	viperCfg.SetDefault("history.burndown.hibernation_to_disk", DefaultBurndownHibernationToDisk)
	viperCfg.SetDefault("history.burndown.hibernation_directory", DefaultBurndownHibernationDirectory)
	viperCfg.SetDefault("history.burndown.debug", DefaultBurndownDebug)
	viperCfg.SetDefault("history.burndown.goroutines", DefaultBurndownGoroutines)

	viperCfg.SetDefault("history.devs.consider_empty_commits", DefaultDevsConsiderEmptyCommits)
	viperCfg.SetDefault("history.devs.anonymize", DefaultDevsAnonymize)

	viperCfg.SetDefault("history.imports.goroutines", DefaultImportsGoroutines)
	viperCfg.SetDefault("history.imports.max_file_size", DefaultImportsMaxFileSize)

	viperCfg.SetDefault("history.sentiment.min_comment_length", DefaultSentimentMinCommentLength)
	viperCfg.SetDefault("history.sentiment.gap", DefaultSentimentGap)

	viperCfg.SetDefault("history.shotness.dsl_struct", DefaultShotnessDSLStruct)
	viperCfg.SetDefault("history.shotness.dsl_name", DefaultShotnessDSLName)

	viperCfg.SetDefault("history.typos.max_distance", DefaultTyposMaxDistance)

	viperCfg.SetDefault("history.anomaly.threshold", DefaultAnomalyThreshold)
	viperCfg.SetDefault("history.anomaly.window_size", DefaultAnomalyWindowSize)

	viperCfg.SetDefault("checkpoint.enabled", DefaultCheckpointEnabled)
	viperCfg.SetDefault("checkpoint.dir", DefaultCheckpointDir)
	viperCfg.SetDefault("checkpoint.resume", DefaultCheckpointResume)
	viperCfg.SetDefault("checkpoint.clear_prev", DefaultCheckpointClearPrev)
}

// ApplyToFacts merges config values into the analyzer facts map.
// Only non-zero config values override existing facts; zero values
// indicate "use analyzer default" and are skipped.
func (c *Config) ApplyToFacts(facts map[string]any) {
	c.applyBurndownFacts(facts)
	c.applyDevsFacts(facts)
	c.applyImportsFacts(facts)
	c.applySentimentFacts(facts)
	c.applyShotnessFacts(facts)
	c.applyTyposFacts(facts)
	c.applyAnomalyFacts(facts)
}

func (c *Config) applyBurndownFacts(facts map[string]any) {
	if c.History.Burndown.Granularity > 0 {
		facts["Burndown.Granularity"] = c.History.Burndown.Granularity
	}

	if c.History.Burndown.Sampling > 0 {
		facts["Burndown.Sampling"] = c.History.Burndown.Sampling
	}

	facts["Burndown.TrackFiles"] = c.History.Burndown.TrackFiles
	facts["Burndown.TrackPeople"] = c.History.Burndown.TrackPeople

	if c.History.Burndown.HibernationThreshold > 0 {
		facts["Burndown.HibernationThreshold"] = c.History.Burndown.HibernationThreshold
	}

	facts["Burndown.HibernationOnDisk"] = c.History.Burndown.HibernationToDisk

	if c.History.Burndown.HibernationDirectory != "" {
		facts["Burndown.HibernationDirectory"] = c.History.Burndown.HibernationDirectory
	}

	facts["Burndown.Debug"] = c.History.Burndown.Debug

	if c.History.Burndown.Goroutines > 0 {
		facts["Burndown.Goroutines"] = c.History.Burndown.Goroutines
	}
}

func (c *Config) applyDevsFacts(facts map[string]any) {
	facts["Devs.ConsiderEmptyCommits"] = c.History.Devs.ConsiderEmptyCommits
	facts["Devs.Anonymize"] = c.History.Devs.Anonymize
}

func (c *Config) applyImportsFacts(facts map[string]any) {
	if c.History.Imports.Goroutines > 0 {
		facts["Imports.Goroutines"] = c.History.Imports.Goroutines
	}

	if c.History.Imports.MaxFileSize > 0 {
		facts["Imports.MaxFileSize"] = c.History.Imports.MaxFileSize
	}
}

func (c *Config) applySentimentFacts(facts map[string]any) {
	if c.History.Sentiment.MinCommentLength > 0 {
		facts["CommentSentiment.MinLength"] = c.History.Sentiment.MinCommentLength
	}

	if c.History.Sentiment.Gap > 0 {
		facts["CommentSentiment.Gap"] = float32(c.History.Sentiment.Gap)
	}
}

func (c *Config) applyShotnessFacts(facts map[string]any) {
	if c.History.Shotness.DSLStruct != "" {
		facts["Shotness.DSLStruct"] = c.History.Shotness.DSLStruct
	}

	if c.History.Shotness.DSLName != "" {
		facts["Shotness.DSLName"] = c.History.Shotness.DSLName
	}
}

func (c *Config) applyTyposFacts(facts map[string]any) {
	if c.History.Typos.MaxDistance > 0 {
		facts["TyposDatasetBuilder.MaximumAllowedDistance"] = c.History.Typos.MaxDistance
	}
}

func (c *Config) applyAnomalyFacts(facts map[string]any) {
	if c.History.Anomaly.Threshold > 0 {
		facts["TemporalAnomaly.Threshold"] = float32(c.History.Anomaly.Threshold)
	}

	if c.History.Anomaly.WindowSize > 0 {
		facts["TemporalAnomaly.WindowSize"] = c.History.Anomaly.WindowSize
	}
}
