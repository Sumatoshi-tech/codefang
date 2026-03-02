package config

import "errors"

// Config is the top-level configuration struct for codefang.
// Field tags use mapstructure for viper unmarshalling.
type Config struct {
	Analyzers  []string         `mapstructure:"analyzers"`
	Pipeline   PipelineConfig   `mapstructure:"pipeline"`
	History    HistoryConfig    `mapstructure:"history"`
	Checkpoint CheckpointConfig `mapstructure:"checkpoint"`
}

// PipelineConfig holds pipeline resource knobs.
type PipelineConfig struct {
	Workers         int    `mapstructure:"workers"`
	MemoryBudget    string `mapstructure:"memory_budget"`
	BlobCacheSize   string `mapstructure:"blob_cache_size"`
	DiffCacheSize   int    `mapstructure:"diff_cache_size"`
	BlobArenaSize   string `mapstructure:"blob_arena_size"`
	CommitBatchSize int    `mapstructure:"commit_batch_size"`
	GOGC            int    `mapstructure:"gogc"`
	BallastSize     string `mapstructure:"ballast_size"`
	MemoryLimit     string `mapstructure:"memory_limit"`
	WorkerTimeout   string `mapstructure:"worker_timeout"`
}

// HistoryConfig holds per-analyzer configuration for history analyzers.
type HistoryConfig struct {
	Burndown  BurndownConfig  `mapstructure:"burndown"`
	Devs      DevsConfig      `mapstructure:"devs"`
	Imports   ImportsConfig   `mapstructure:"imports"`
	Sentiment SentimentConfig `mapstructure:"sentiment"`
	Shotness  ShotnessConfig  `mapstructure:"shotness"`
	Typos     TyposConfig     `mapstructure:"typos"`
	Anomaly   AnomalyConfig   `mapstructure:"anomaly"`
}

// AnomalyConfig holds temporal anomaly detection analyzer settings.
type AnomalyConfig struct {
	Threshold  float64 `mapstructure:"threshold"`
	WindowSize int     `mapstructure:"window_size"`
}

// BurndownConfig holds burndown analyzer settings.
type BurndownConfig struct {
	Granularity          int    `mapstructure:"granularity"`
	Sampling             int    `mapstructure:"sampling"`
	TrackFiles           bool   `mapstructure:"track_files"`
	TrackPeople          bool   `mapstructure:"track_people"`
	HibernationThreshold int    `mapstructure:"hibernation_threshold"`
	HibernationToDisk    bool   `mapstructure:"hibernation_to_disk"`
	HibernationDirectory string `mapstructure:"hibernation_directory"`
	Debug                bool   `mapstructure:"debug"`
	Goroutines           int    `mapstructure:"goroutines"`
}

// DevsConfig holds devs analyzer settings.
type DevsConfig struct {
	ConsiderEmptyCommits bool `mapstructure:"consider_empty_commits"`
	Anonymize            bool `mapstructure:"anonymize"`
}

// ImportsConfig holds imports history analyzer settings.
type ImportsConfig struct {
	Goroutines  int `mapstructure:"goroutines"`
	MaxFileSize int `mapstructure:"max_file_size"`
}

// SentimentConfig holds sentiment analyzer settings.
type SentimentConfig struct {
	MinCommentLength int     `mapstructure:"min_comment_length"`
	Gap              float64 `mapstructure:"gap"`
}

// ShotnessConfig holds shotness analyzer settings.
type ShotnessConfig struct {
	DSLStruct string `mapstructure:"dsl_struct"`
	DSLName   string `mapstructure:"dsl_name"`
}

// TyposConfig holds typos analyzer settings.
type TyposConfig struct {
	MaxDistance int `mapstructure:"max_distance"`
}

// CheckpointConfig holds checkpoint settings.
type CheckpointConfig struct {
	Enabled   bool   `mapstructure:"enabled"`
	Dir       string `mapstructure:"dir"`
	Resume    bool   `mapstructure:"resume"`
	ClearPrev bool   `mapstructure:"clear_prev"`
}

// sentimentGapMax is the upper bound for the sentiment gap value.
const sentimentGapMax = 1.0

// Sentinel errors for configuration validation.
var (
	// ErrInvalidWorkers indicates the workers value is negative.
	ErrInvalidWorkers = errors.New("pipeline.workers must be non-negative")
	// ErrInvalidDiffCacheSize indicates the diff cache size is negative.
	ErrInvalidDiffCacheSize = errors.New("pipeline.diff_cache_size must be non-negative")
	// ErrInvalidCommitBatchSize indicates the commit batch size is negative.
	ErrInvalidCommitBatchSize = errors.New("pipeline.commit_batch_size must be non-negative")
	// ErrInvalidGOGC indicates the GOGC value is negative.
	ErrInvalidGOGC = errors.New("pipeline.gogc must be non-negative")
	// ErrInvalidBurndownGranularity indicates the granularity is not positive.
	ErrInvalidBurndownGranularity = errors.New("history.burndown.granularity must be positive")
	// ErrInvalidBurndownSampling indicates the sampling is not positive.
	ErrInvalidBurndownSampling = errors.New("history.burndown.sampling must be positive")
	// ErrInvalidSentimentMinLength indicates the min comment length is not positive.
	ErrInvalidSentimentMinLength = errors.New("history.sentiment.min_comment_length must be positive")
	// ErrInvalidSentimentGap indicates the sentiment gap is out of range.
	ErrInvalidSentimentGap = errors.New("history.sentiment.gap must be between 0 and 1")
	// ErrInvalidTyposMaxDistance indicates the max distance is not positive.
	ErrInvalidTyposMaxDistance = errors.New("history.typos.max_distance must be positive")
	// ErrInvalidImportsGoroutines indicates the goroutines value is not positive.
	ErrInvalidImportsGoroutines = errors.New("history.imports.goroutines must be positive")
	// ErrInvalidImportsMaxFileSize indicates the max file size is not positive.
	ErrInvalidImportsMaxFileSize = errors.New("history.imports.max_file_size must be positive")
	// ErrInvalidAnomalyThreshold indicates the threshold is not positive.
	ErrInvalidAnomalyThreshold = errors.New("history.anomaly.threshold must be positive")
	// ErrInvalidAnomalyWindowSize indicates the window size is less than 2.
	ErrInvalidAnomalyWindowSize = errors.New("history.anomaly.window_size must be at least 2")
)

// Validate checks Config invariants and returns the first error found.
func (c *Config) Validate() error {
	pipelineErr := c.validatePipeline()
	if pipelineErr != nil {
		return pipelineErr
	}

	return c.validateHistory()
}

func (c *Config) validatePipeline() error {
	if c.Pipeline.Workers < 0 {
		return ErrInvalidWorkers
	}

	if c.Pipeline.DiffCacheSize < 0 {
		return ErrInvalidDiffCacheSize
	}

	if c.Pipeline.CommitBatchSize < 0 {
		return ErrInvalidCommitBatchSize
	}

	if c.Pipeline.GOGC < 0 {
		return ErrInvalidGOGC
	}

	return nil
}

func (c *Config) validateHistory() error {
	if c.History.Burndown.Granularity < 0 {
		return ErrInvalidBurndownGranularity
	}

	if c.History.Burndown.Sampling < 0 {
		return ErrInvalidBurndownSampling
	}

	if c.History.Sentiment.MinCommentLength < 0 {
		return ErrInvalidSentimentMinLength
	}

	if c.History.Sentiment.Gap < 0 || c.History.Sentiment.Gap > sentimentGapMax {
		return ErrInvalidSentimentGap
	}

	if c.History.Typos.MaxDistance < 0 {
		return ErrInvalidTyposMaxDistance
	}

	if c.History.Imports.Goroutines < 0 {
		return ErrInvalidImportsGoroutines
	}

	if c.History.Imports.MaxFileSize < 0 {
		return ErrInvalidImportsMaxFileSize
	}

	if c.History.Anomaly.Threshold < 0 {
		return ErrInvalidAnomalyThreshold
	}

	if c.History.Anomaly.WindowSize != 0 && c.History.Anomaly.WindowSize < minAnomalyWindowSize {
		return ErrInvalidAnomalyWindowSize
	}

	return nil
}

// minAnomalyWindowSize is the minimum valid sliding window for anomaly detection.
const minAnomalyWindowSize = 2
