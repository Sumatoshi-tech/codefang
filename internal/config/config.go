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

	// Advanced pipeline tuning.
	UASTSpillThreshold           int    `mapstructure:"uast_spill_threshold"`
	IntraCommitParallelThreshold int    `mapstructure:"intra_commit_parallel_threshold"`
	MaxIntraCommitWorkers        int    `mapstructure:"max_intra_commit_workers"`
	MaxUASTBlobSize              int    `mapstructure:"max_uast_blob_size"`
	UASTParseTimeout             string `mapstructure:"uast_parse_timeout"`
	MaxChangesPerCommit          int    `mapstructure:"max_changes_per_commit"`
	MaxDiffBatchSize             int    `mapstructure:"max_diff_batch_size"`
	MemoryBudgetRatio            int    `mapstructure:"memory_budget_ratio"`
	MemoryBudgetCap              string `mapstructure:"memory_budget_cap"`
	MemoryLimitRatio             int    `mapstructure:"memory_limit_ratio"`
	UASTSpillTrimInterval        int    `mapstructure:"uast_spill_trim_interval"`
	NativeTrimInterval           int    `mapstructure:"native_trim_interval"`
	MaxStreamingBuffering        int    `mapstructure:"max_streaming_buffering"`
	DrainPrefetchTimeout         string `mapstructure:"drain_prefetch_timeout"`
	SamplerInterval              string `mapstructure:"sampler_interval"`
	WorkerRatio                  int    `mapstructure:"worker_ratio"`
	UASTWorkerRatio              int    `mapstructure:"uast_worker_ratio"`
	LeafWorkerDivisor            int    `mapstructure:"leaf_worker_divisor"`
	MinLeafWorkers               int    `mapstructure:"min_leaf_workers"`
	BufferSizeMultiplier         int    `mapstructure:"buffer_size_multiplier"`
	BudgetLimitRatio             int    `mapstructure:"budget_limit_ratio"`
	SystemRAMLimitRatio          int    `mapstructure:"system_ram_limit_ratio"`
	StaticMaxWorkers             int    `mapstructure:"static_max_workers"`
	MallocTrimInterval           int    `mapstructure:"malloc_trim_interval"`
	StaticMemoryLimitRatio       int    `mapstructure:"static_memory_limit_ratio"`
	DiffJobBufferMultiplier      int    `mapstructure:"diff_job_buffer_multiplier"`
}

// HistoryConfig holds per-analyzer configuration for history analyzers.
type HistoryConfig struct {
	Burndown    BurndownConfig    `mapstructure:"burndown"`
	Couples     CouplesConfig     `mapstructure:"couples"`
	Devs        DevsConfig        `mapstructure:"devs"`
	FileHistory FileHistoryConfig `mapstructure:"file_history"`
	Imports     ImportsConfig     `mapstructure:"imports"`
	Sentiment   SentimentConfig   `mapstructure:"sentiment"`
	Shotness    ShotnessConfig    `mapstructure:"shotness"`
	Typos       TyposConfig       `mapstructure:"typos"`
	Anomaly     AnomalyConfig     `mapstructure:"anomaly"`
	Clones      ClonesConfig      `mapstructure:"clones"`
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

// CouplesConfig holds couples analyzer settings.
type CouplesConfig struct {
	CouplingThresholdHigh      int `mapstructure:"coupling_threshold_high"`
	OwnershipFewThreshold      int `mapstructure:"ownership_few_threshold"`
	OwnershipModerateThreshold int `mapstructure:"ownership_moderate_threshold"`
	BatchCouplingThreshold     int `mapstructure:"batch_coupling_threshold"`
	HLLPrecision               int `mapstructure:"hll_precision"`
	TopKPerFile                int `mapstructure:"top_k_per_file"`
	MinEdgeWeight              int `mapstructure:"min_edge_weight"`
}

// DevsConfig holds devs analyzer settings.
type DevsConfig struct {
	ConsiderEmptyCommits  bool    `mapstructure:"consider_empty_commits"`
	Anonymize             bool    `mapstructure:"anonymize"`
	BusFactorThreshold    float64 `mapstructure:"bus_factor_threshold"`
	RiskThresholdCritical float64 `mapstructure:"risk_threshold_critical"`
	RiskThresholdHigh     float64 `mapstructure:"risk_threshold_high"`
	RiskThresholdMedium   float64 `mapstructure:"risk_threshold_medium"`
	ActiveThresholdRatio  float64 `mapstructure:"active_threshold_ratio"`
	DefaultActiveDays     int     `mapstructure:"default_active_days"`
	HLLPrecision          int     `mapstructure:"hll_precision"`
}

// FileHistoryConfig holds file history analyzer settings.
type FileHistoryConfig struct {
	HotspotThresholdCritical int `mapstructure:"hotspot_threshold_critical"`
	HotspotThresholdHigh     int `mapstructure:"hotspot_threshold_high"`
	HotspotThresholdMedium   int `mapstructure:"hotspot_threshold_medium"`
}

// ImportsConfig holds imports history analyzer settings.
type ImportsConfig struct {
	Goroutines            int `mapstructure:"goroutines"`
	MaxFileSize           int `mapstructure:"max_file_size"`
	MaxDependencyRiskRows int `mapstructure:"max_dependency_risk_rows"`
}

// SentimentConfig holds sentiment analyzer settings.
type SentimentConfig struct {
	MinCommentLength       int     `mapstructure:"min_comment_length"`
	Gap                    float64 `mapstructure:"gap"`
	NeutralizerWeight      float64 `mapstructure:"neutralizer_weight"`
	MaxWeightRatio         float64 `mapstructure:"max_weight_ratio"`
	PositiveThreshold      float64 `mapstructure:"positive_threshold"`
	NegativeThreshold      float64 `mapstructure:"negative_threshold"`
	TrendThreshold         float64 `mapstructure:"trend_threshold"`
	LowSentimentRiskThresh float64 `mapstructure:"low_sentiment_risk_threshold"`
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

// ClonesConfig holds clones analyzer settings.
type ClonesConfig struct {
	MaxClonePairs        int     `mapstructure:"max_clone_pairs"`
	NumHashes            int     `mapstructure:"num_hashes"`
	NumBands             int     `mapstructure:"num_bands"`
	NumRows              int     `mapstructure:"num_rows"`
	ShingleSize          int     `mapstructure:"shingle_size"`
	SimilarityType2      float64 `mapstructure:"similarity_type2"`
	SimilarityType3      float64 `mapstructure:"similarity_type3"`
	ThresholdRatioYellow float64 `mapstructure:"threshold_ratio_yellow"`
	ThresholdRatioRed    float64 `mapstructure:"threshold_ratio_red"`
	ThresholdPairsYellow int     `mapstructure:"threshold_pairs_yellow"`
	ThresholdPairsRed    int     `mapstructure:"threshold_pairs_red"`
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

// ratioMax is the upper bound for ratio values (0.0 to 1.0).
const ratioMax = 1.0

// percentMax is the upper bound for percentage values (0 to 100).
const percentMax = 100.0

// HLL precision bounds (algorithm constraint).
const (
	minHLLPrecision = 4
	maxHLLPrecision = 18
)

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
	// ErrInvalidUASTSpillThreshold indicates the UAST spill threshold is negative.
	ErrInvalidUASTSpillThreshold = errors.New("pipeline.uast_spill_threshold must be non-negative")
	// ErrInvalidIntraCommitParallelThreshold indicates the intra-commit parallel threshold is negative.
	ErrInvalidIntraCommitParallelThreshold = errors.New("pipeline.intra_commit_parallel_threshold must be non-negative")
	// ErrInvalidMaxIntraCommitWorkers indicates the max intra-commit workers is negative.
	ErrInvalidMaxIntraCommitWorkers = errors.New("pipeline.max_intra_commit_workers must be non-negative")
	// ErrInvalidMaxUASTBlobSize indicates the max UAST blob size is negative.
	ErrInvalidMaxUASTBlobSize = errors.New("pipeline.max_uast_blob_size must be non-negative")
	// ErrInvalidMaxChangesPerCommit indicates the max changes per commit is negative.
	ErrInvalidMaxChangesPerCommit = errors.New("pipeline.max_changes_per_commit must be non-negative")
	// ErrInvalidMaxDiffBatchSize indicates the max diff batch size is negative.
	ErrInvalidMaxDiffBatchSize = errors.New("pipeline.max_diff_batch_size must be non-negative")
	// ErrInvalidMemoryBudgetRatio indicates the memory budget ratio is out of range.
	ErrInvalidMemoryBudgetRatio = errors.New("pipeline.memory_budget_ratio must be between 0 and 100")
	// ErrInvalidMemoryLimitRatio indicates the memory limit ratio is out of range.
	ErrInvalidMemoryLimitRatio = errors.New("pipeline.memory_limit_ratio must be between 0 and 100")
	// ErrInvalidBurndownGranularity indicates the granularity is not positive.
	ErrInvalidBurndownGranularity = errors.New("history.burndown.granularity must be positive")
	// ErrInvalidBurndownSampling indicates the sampling is not positive.
	ErrInvalidBurndownSampling = errors.New("history.burndown.sampling must be positive")
	// ErrInvalidCouplingThreshold indicates the coupling threshold is negative.
	ErrInvalidCouplingThreshold = errors.New("history.couples.coupling_threshold_high must be non-negative")
	// ErrInvalidOwnershipFewThreshold indicates the ownership few threshold is negative.
	ErrInvalidOwnershipFewThreshold = errors.New("history.couples.ownership_few_threshold must be non-negative")
	// ErrInvalidOwnershipModerateThreshold indicates the ownership moderate threshold is negative.
	ErrInvalidOwnershipModerateThreshold = errors.New("history.couples.ownership_moderate_threshold must be non-negative")
	// ErrInvalidCouplesHLLPrecision indicates the HLL precision is out of range.
	ErrInvalidCouplesHLLPrecision = errors.New("history.couples.hll_precision must be between 4 and 18")
	// ErrInvalidBusFactorThreshold indicates the bus factor threshold is out of range.
	ErrInvalidBusFactorThreshold = errors.New("history.devs.bus_factor_threshold must be between 0 and 1")
	// ErrInvalidDevsRiskThresholdCritical indicates the critical risk threshold is out of range.
	ErrInvalidDevsRiskThresholdCritical = errors.New("history.devs.risk_threshold_critical must be between 0 and 100")
	// ErrInvalidDevsRiskThresholdHigh indicates the high risk threshold is out of range.
	ErrInvalidDevsRiskThresholdHigh = errors.New("history.devs.risk_threshold_high must be between 0 and 100")
	// ErrInvalidDevsRiskThresholdMedium indicates the medium risk threshold is out of range.
	ErrInvalidDevsRiskThresholdMedium = errors.New("history.devs.risk_threshold_medium must be between 0 and 100")
	// ErrInvalidDevsActiveThresholdRatio indicates the active threshold ratio is out of range.
	ErrInvalidDevsActiveThresholdRatio = errors.New("history.devs.active_threshold_ratio must be between 0 and 1")
	// ErrInvalidDevsDefaultActiveDays indicates the default active days is negative.
	ErrInvalidDevsDefaultActiveDays = errors.New("history.devs.default_active_days must be non-negative")
	// ErrInvalidDevsHLLPrecision indicates the HLL precision is out of range.
	ErrInvalidDevsHLLPrecision = errors.New("history.devs.hll_precision must be between 4 and 18")
	// ErrInvalidHotspotThresholdCritical indicates the critical hotspot threshold is negative.
	ErrInvalidHotspotThresholdCritical = errors.New("history.file_history.hotspot_threshold_critical must be non-negative")
	// ErrInvalidHotspotThresholdHigh indicates the high hotspot threshold is negative.
	ErrInvalidHotspotThresholdHigh = errors.New("history.file_history.hotspot_threshold_high must be non-negative")
	// ErrInvalidHotspotThresholdMedium indicates the medium hotspot threshold is negative.
	ErrInvalidHotspotThresholdMedium = errors.New("history.file_history.hotspot_threshold_medium must be non-negative")
	// ErrInvalidSentimentMinLength indicates the min comment length is not positive.
	ErrInvalidSentimentMinLength = errors.New("history.sentiment.min_comment_length must be positive")
	// ErrInvalidSentimentGap indicates the sentiment gap is out of range.
	ErrInvalidSentimentGap = errors.New("history.sentiment.gap must be between 0 and 1")
	// ErrInvalidNeutralizerWeight indicates the neutralizer weight is out of range.
	ErrInvalidNeutralizerWeight = errors.New("history.sentiment.neutralizer_weight must be between 0 and 1")
	// ErrInvalidMaxWeightRatio indicates the max weight ratio is negative.
	ErrInvalidMaxWeightRatio = errors.New("history.sentiment.max_weight_ratio must be non-negative")
	// ErrInvalidTyposMaxDistance indicates the max distance is not positive.
	ErrInvalidTyposMaxDistance = errors.New("history.typos.max_distance must be positive")
	// ErrInvalidImportsGoroutines indicates the goroutines value is not positive.
	ErrInvalidImportsGoroutines = errors.New("history.imports.goroutines must be positive")
	// ErrInvalidImportsMaxFileSize indicates the max file size is not positive.
	ErrInvalidImportsMaxFileSize = errors.New("history.imports.max_file_size must be positive")
	// ErrInvalidImportsMaxDependencyRiskRows indicates the max dependency risk rows is negative.
	ErrInvalidImportsMaxDependencyRiskRows = errors.New("history.imports.max_dependency_risk_rows must be non-negative")
	// ErrInvalidAnomalyThreshold indicates the threshold is not positive.
	ErrInvalidAnomalyThreshold = errors.New("history.anomaly.threshold must be positive")
	// ErrInvalidAnomalyWindowSize indicates the window size is less than 2.
	ErrInvalidAnomalyWindowSize = errors.New("history.anomaly.window_size must be at least 2")
	// ErrInvalidClonesMaxClonePairs indicates the max clone pairs is negative.
	ErrInvalidClonesMaxClonePairs = errors.New("history.clones.max_clone_pairs must be non-negative")
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

	if c.Pipeline.UASTSpillThreshold < 0 {
		return ErrInvalidUASTSpillThreshold
	}

	if c.Pipeline.IntraCommitParallelThreshold < 0 {
		return ErrInvalidIntraCommitParallelThreshold
	}

	if c.Pipeline.MaxIntraCommitWorkers < 0 {
		return ErrInvalidMaxIntraCommitWorkers
	}

	if c.Pipeline.MaxUASTBlobSize < 0 {
		return ErrInvalidMaxUASTBlobSize
	}

	if c.Pipeline.MaxChangesPerCommit < 0 {
		return ErrInvalidMaxChangesPerCommit
	}

	if c.Pipeline.MaxDiffBatchSize < 0 {
		return ErrInvalidMaxDiffBatchSize
	}

	if c.Pipeline.MemoryBudgetRatio < 0 || c.Pipeline.MemoryBudgetRatio > int(percentMax) {
		return ErrInvalidMemoryBudgetRatio
	}

	if c.Pipeline.MemoryLimitRatio < 0 || c.Pipeline.MemoryLimitRatio > int(percentMax) {
		return ErrInvalidMemoryLimitRatio
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

	err := c.validateCouples()
	if err != nil {
		return err
	}

	err = c.validateDevs()
	if err != nil {
		return err
	}

	err = c.validateFileHistory()
	if err != nil {
		return err
	}

	err = c.validateSentiment()
	if err != nil {
		return err
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

	if c.History.Imports.MaxDependencyRiskRows < 0 {
		return ErrInvalidImportsMaxDependencyRiskRows
	}

	if c.History.Anomaly.Threshold < 0 {
		return ErrInvalidAnomalyThreshold
	}

	if c.History.Anomaly.WindowSize != 0 && c.History.Anomaly.WindowSize < minAnomalyWindowSize {
		return ErrInvalidAnomalyWindowSize
	}

	if c.History.Clones.MaxClonePairs < 0 {
		return ErrInvalidClonesMaxClonePairs
	}

	return nil
}

func (c *Config) validateCouples() error {
	cp := c.History.Couples

	if cp.CouplingThresholdHigh < 0 {
		return ErrInvalidCouplingThreshold
	}

	if cp.OwnershipFewThreshold < 0 {
		return ErrInvalidOwnershipFewThreshold
	}

	if cp.OwnershipModerateThreshold < 0 {
		return ErrInvalidOwnershipModerateThreshold
	}

	if cp.HLLPrecision != 0 && (cp.HLLPrecision < minHLLPrecision || cp.HLLPrecision > maxHLLPrecision) {
		return ErrInvalidCouplesHLLPrecision
	}

	return nil
}

func (c *Config) validateDevs() error {
	dv := c.History.Devs

	if dv.BusFactorThreshold < 0 || dv.BusFactorThreshold > ratioMax {
		return ErrInvalidBusFactorThreshold
	}

	if dv.RiskThresholdCritical < 0 || dv.RiskThresholdCritical > percentMax {
		return ErrInvalidDevsRiskThresholdCritical
	}

	if dv.RiskThresholdHigh < 0 || dv.RiskThresholdHigh > percentMax {
		return ErrInvalidDevsRiskThresholdHigh
	}

	if dv.RiskThresholdMedium < 0 || dv.RiskThresholdMedium > percentMax {
		return ErrInvalidDevsRiskThresholdMedium
	}

	if dv.ActiveThresholdRatio < 0 || dv.ActiveThresholdRatio > ratioMax {
		return ErrInvalidDevsActiveThresholdRatio
	}

	if dv.DefaultActiveDays < 0 {
		return ErrInvalidDevsDefaultActiveDays
	}

	if dv.HLLPrecision != 0 && (dv.HLLPrecision < minHLLPrecision || dv.HLLPrecision > maxHLLPrecision) {
		return ErrInvalidDevsHLLPrecision
	}

	return nil
}

func (c *Config) validateFileHistory() error {
	fh := c.History.FileHistory

	if fh.HotspotThresholdCritical < 0 {
		return ErrInvalidHotspotThresholdCritical
	}

	if fh.HotspotThresholdHigh < 0 {
		return ErrInvalidHotspotThresholdHigh
	}

	if fh.HotspotThresholdMedium < 0 {
		return ErrInvalidHotspotThresholdMedium
	}

	return nil
}

func (c *Config) validateSentiment() error {
	se := c.History.Sentiment

	if se.MinCommentLength < 0 {
		return ErrInvalidSentimentMinLength
	}

	if se.Gap < 0 || se.Gap > sentimentGapMax {
		return ErrInvalidSentimentGap
	}

	if se.NeutralizerWeight < 0 || se.NeutralizerWeight > ratioMax {
		return ErrInvalidNeutralizerWeight
	}

	if se.MaxWeightRatio < 0 {
		return ErrInvalidMaxWeightRatio
	}

	return nil
}

// minAnomalyWindowSize is the minimum valid sliding window for anomaly detection.
const minAnomalyWindowSize = 2
