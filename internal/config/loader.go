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

	applyPipelineDefaults(viperCfg)
	applyHistoryDefaults(viperCfg)
	applyCheckpointDefaults(viperCfg)
}

func applyPipelineDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("pipeline.workers", DefaultPipelineWorkers)
	viperCfg.SetDefault("pipeline.memory_budget", DefaultPipelineMemoryBudget)
	viperCfg.SetDefault("pipeline.blob_cache_size", DefaultPipelineBlobCacheSize)
	viperCfg.SetDefault("pipeline.diff_cache_size", DefaultPipelineDiffCacheSize)
	viperCfg.SetDefault("pipeline.blob_arena_size", DefaultPipelineBlobArenaSize)
	viperCfg.SetDefault("pipeline.commit_batch_size", DefaultPipelineCommitBatchSize)
	viperCfg.SetDefault("pipeline.gogc", DefaultPipelineGOGC)
	viperCfg.SetDefault("pipeline.ballast_size", DefaultPipelineBallastSize)

	viperCfg.SetDefault("pipeline.uast_spill_threshold", DefaultPipelineUASTSpillThreshold)
	viperCfg.SetDefault("pipeline.intra_commit_parallel_threshold", DefaultPipelineIntraCommitParallelThreshold)
	viperCfg.SetDefault("pipeline.max_intra_commit_workers", DefaultPipelineMaxIntraCommitWorkers)
	viperCfg.SetDefault("pipeline.max_uast_blob_size", DefaultPipelineMaxUASTBlobSize)
	viperCfg.SetDefault("pipeline.uast_parse_timeout", DefaultPipelineUASTParseTimeout)
	viperCfg.SetDefault("pipeline.max_changes_per_commit", DefaultPipelineMaxChangesPerCommit)
	viperCfg.SetDefault("pipeline.max_diff_batch_size", DefaultPipelineMaxDiffBatchSize)
	viperCfg.SetDefault("pipeline.memory_budget_ratio", DefaultPipelineMemoryBudgetRatio)
	viperCfg.SetDefault("pipeline.memory_budget_cap", DefaultPipelineMemoryBudgetCap)
	viperCfg.SetDefault("pipeline.memory_limit_ratio", DefaultPipelineMemoryLimitRatio)
	viperCfg.SetDefault("pipeline.uast_spill_trim_interval", DefaultPipelineUASTSpillTrimInterval)
	viperCfg.SetDefault("pipeline.native_trim_interval", DefaultPipelineNativeTrimInterval)
	viperCfg.SetDefault("pipeline.max_streaming_buffering", DefaultPipelineMaxStreamingBuffering)
	viperCfg.SetDefault("pipeline.drain_prefetch_timeout", DefaultPipelineDrainPrefetchTimeout)
	viperCfg.SetDefault("pipeline.sampler_interval", DefaultPipelineSamplerInterval)
	viperCfg.SetDefault("pipeline.worker_ratio", DefaultPipelineWorkerRatio)
	viperCfg.SetDefault("pipeline.uast_worker_ratio", DefaultPipelineUASTWorkerRatio)
	viperCfg.SetDefault("pipeline.leaf_worker_divisor", DefaultPipelineLeafWorkerDivisor)
	viperCfg.SetDefault("pipeline.min_leaf_workers", DefaultPipelineMinLeafWorkers)
	viperCfg.SetDefault("pipeline.buffer_size_multiplier", DefaultPipelineBufferSizeMultiplier)
	viperCfg.SetDefault("pipeline.budget_limit_ratio", DefaultPipelineBudgetLimitRatio)
	viperCfg.SetDefault("pipeline.system_ram_limit_ratio", DefaultPipelineSystemRAMLimitRatio)
	viperCfg.SetDefault("pipeline.static_max_workers", DefaultPipelineStaticMaxWorkers)
	viperCfg.SetDefault("pipeline.malloc_trim_interval", DefaultPipelineMallocTrimInterval)
	viperCfg.SetDefault("pipeline.static_memory_limit_ratio", DefaultPipelineStaticMemoryLimitRatio)
	viperCfg.SetDefault("pipeline.diff_job_buffer_multiplier", DefaultPipelineDiffJobBufferMultiplier)
}

func applyHistoryDefaults(viperCfg *viper.Viper) {
	applyBurndownDefaults(viperCfg)
	applyCouplesDefaults(viperCfg)
	applyDevsDefaults(viperCfg)
	applyOtherHistoryDefaults(viperCfg)
}

func applyBurndownDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("history.burndown.granularity", DefaultBurndownGranularity)
	viperCfg.SetDefault("history.burndown.sampling", DefaultBurndownSampling)
	viperCfg.SetDefault("history.burndown.track_files", DefaultBurndownTrackFiles)
	viperCfg.SetDefault("history.burndown.track_people", DefaultBurndownTrackPeople)
	viperCfg.SetDefault("history.burndown.hibernation_threshold", DefaultBurndownHibernationThreshold)
	viperCfg.SetDefault("history.burndown.hibernation_to_disk", DefaultBurndownHibernationToDisk)
	viperCfg.SetDefault("history.burndown.hibernation_directory", DefaultBurndownHibernationDirectory)
	viperCfg.SetDefault("history.burndown.debug", DefaultBurndownDebug)
	viperCfg.SetDefault("history.burndown.goroutines", DefaultBurndownGoroutines)
}

func applyCouplesDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("history.couples.coupling_threshold_high", DefaultCouplesCouplingThresholdHigh)
	viperCfg.SetDefault("history.couples.ownership_few_threshold", DefaultCouplesOwnershipFewThreshold)
	viperCfg.SetDefault("history.couples.ownership_moderate_threshold", DefaultCouplesOwnershipModerateThreshold)
	viperCfg.SetDefault("history.couples.batch_coupling_threshold", DefaultCouplesBatchCouplingThreshold)
	viperCfg.SetDefault("history.couples.hll_precision", DefaultCouplesHLLPrecision)
	viperCfg.SetDefault("history.couples.top_k_per_file", DefaultCouplesTopKPerFile)
	viperCfg.SetDefault("history.couples.min_edge_weight", DefaultCouplesMinEdgeWeight)
}

func applyDevsDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("history.devs.consider_empty_commits", DefaultDevsConsiderEmptyCommits)
	viperCfg.SetDefault("history.devs.anonymize", DefaultDevsAnonymize)
	viperCfg.SetDefault("history.devs.bus_factor_threshold", DefaultDevsBusFactorThreshold)
	viperCfg.SetDefault("history.devs.risk_threshold_critical", DefaultDevsRiskThresholdCritical)
	viperCfg.SetDefault("history.devs.risk_threshold_high", DefaultDevsRiskThresholdHigh)
	viperCfg.SetDefault("history.devs.risk_threshold_medium", DefaultDevsRiskThresholdMedium)
	viperCfg.SetDefault("history.devs.active_threshold_ratio", DefaultDevsActiveThresholdRatio)
	viperCfg.SetDefault("history.devs.default_active_days", DefaultDevsDefaultActiveDays)
	viperCfg.SetDefault("history.devs.hll_precision", DefaultDevsHLLPrecision)
}

func applyOtherHistoryDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("history.file_history.hotspot_threshold_critical", DefaultFileHistoryHotspotCritical)
	viperCfg.SetDefault("history.file_history.hotspot_threshold_high", DefaultFileHistoryHotspotHigh)
	viperCfg.SetDefault("history.file_history.hotspot_threshold_medium", DefaultFileHistoryHotspotMedium)

	viperCfg.SetDefault("history.imports.goroutines", DefaultImportsGoroutines)
	viperCfg.SetDefault("history.imports.max_file_size", DefaultImportsMaxFileSize)
	viperCfg.SetDefault("history.imports.max_dependency_risk_rows", DefaultImportsMaxDependencyRiskRows)

	viperCfg.SetDefault("history.sentiment.min_comment_length", DefaultSentimentMinCommentLength)
	viperCfg.SetDefault("history.sentiment.gap", DefaultSentimentGap)
	viperCfg.SetDefault("history.sentiment.neutralizer_weight", DefaultSentimentNeutralizerWeight)
	viperCfg.SetDefault("history.sentiment.max_weight_ratio", DefaultSentimentMaxWeightRatio)
	viperCfg.SetDefault("history.sentiment.positive_threshold", DefaultSentimentPositiveThreshold)
	viperCfg.SetDefault("history.sentiment.negative_threshold", DefaultSentimentNegativeThreshold)
	viperCfg.SetDefault("history.sentiment.trend_threshold", DefaultSentimentTrendThreshold)
	viperCfg.SetDefault("history.sentiment.low_sentiment_risk_threshold", DefaultSentimentLowSentimentRiskThresh)

	viperCfg.SetDefault("history.shotness.dsl_struct", DefaultShotnessDSLStruct)
	viperCfg.SetDefault("history.shotness.dsl_name", DefaultShotnessDSLName)

	viperCfg.SetDefault("history.typos.max_distance", DefaultTyposMaxDistance)

	viperCfg.SetDefault("history.anomaly.threshold", DefaultAnomalyThreshold)
	viperCfg.SetDefault("history.anomaly.window_size", DefaultAnomalyWindowSize)

	viperCfg.SetDefault("history.clones.max_clone_pairs", DefaultClonesMaxClonePairs)
	viperCfg.SetDefault("history.clones.num_hashes", DefaultClonesNumHashes)
	viperCfg.SetDefault("history.clones.num_bands", DefaultClonesNumBands)
	viperCfg.SetDefault("history.clones.num_rows", DefaultClonesNumRows)
	viperCfg.SetDefault("history.clones.shingle_size", DefaultClonesShingleSize)
	viperCfg.SetDefault("history.clones.similarity_type2", DefaultClonesSimilarityType2)
	viperCfg.SetDefault("history.clones.similarity_type3", DefaultClonesSimilarityType3)
	viperCfg.SetDefault("history.clones.threshold_ratio_yellow", DefaultClonesThresholdRatioYellow)
	viperCfg.SetDefault("history.clones.threshold_ratio_red", DefaultClonesThresholdRatioRed)
	viperCfg.SetDefault("history.clones.threshold_pairs_yellow", DefaultClonesThresholdPairsYellow)
	viperCfg.SetDefault("history.clones.threshold_pairs_red", DefaultClonesThresholdPairsRed)
}

func applyCheckpointDefaults(viperCfg *viper.Viper) {
	viperCfg.SetDefault("checkpoint.enabled", DefaultCheckpointEnabled)
	viperCfg.SetDefault("checkpoint.dir", DefaultCheckpointDir)
	viperCfg.SetDefault("checkpoint.resume", DefaultCheckpointResume)
	viperCfg.SetDefault("checkpoint.clear_prev", DefaultCheckpointClearPrev)
}
