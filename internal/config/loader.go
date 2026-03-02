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
