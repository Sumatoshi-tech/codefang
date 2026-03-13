// Package config provides YAML-based project configuration for codefang.
package config

// Pipeline default values.
const (
	DefaultPipelineWorkers         = 0
	DefaultPipelineMemoryBudget    = ""
	DefaultPipelineBlobCacheSize   = ""
	DefaultPipelineDiffCacheSize   = 0
	DefaultPipelineBlobArenaSize   = ""
	DefaultPipelineCommitBatchSize = 0
	DefaultPipelineGOGC            = 0
	DefaultPipelineBallastSize     = "0"
)

// Pipeline advanced tuning defaults.
const (
	DefaultPipelineUASTSpillThreshold           = 32
	DefaultPipelineIntraCommitParallelThreshold = 4
	DefaultPipelineMaxIntraCommitWorkers        = 4
	DefaultPipelineMaxUASTBlobSize              = 256 * 1024 // 256 KiB.
	DefaultPipelineUASTParseTimeout             = "10s"
	DefaultPipelineMaxChangesPerCommit          = 10000
	DefaultPipelineMaxDiffBatchSize             = 1000
	DefaultPipelineMemoryBudgetRatio            = 50
	DefaultPipelineMemoryBudgetCap              = "2GiB"
	DefaultPipelineMemoryLimitRatio             = 75
	DefaultPipelineUASTSpillTrimInterval        = 16
	DefaultPipelineNativeTrimInterval           = 10
	DefaultPipelineMaxStreamingBuffering        = 3
	DefaultPipelineDrainPrefetchTimeout         = "30s"
	DefaultPipelineSamplerInterval              = "2s"
	DefaultPipelineWorkerRatio                  = 100
	DefaultPipelineUASTWorkerRatio              = 40
	DefaultPipelineLeafWorkerDivisor            = 3
	DefaultPipelineMinLeafWorkers               = 4
	DefaultPipelineBufferSizeMultiplier         = 2
	DefaultPipelineBudgetLimitRatio             = 95
	DefaultPipelineSystemRAMLimitRatio          = 90
	DefaultPipelineStaticMaxWorkers             = 8
	DefaultPipelineMallocTrimInterval           = 50
	DefaultPipelineStaticMemoryLimitRatio       = 90
	DefaultPipelineDiffJobBufferMultiplier      = 10
)

// Burndown analyzer defaults.
const (
	DefaultBurndownGranularity          = 30
	DefaultBurndownSampling             = 30
	DefaultBurndownTrackFiles           = false
	DefaultBurndownTrackPeople          = false
	DefaultBurndownHibernationThreshold = 1000
	DefaultBurndownHibernationToDisk    = true
	DefaultBurndownHibernationDirectory = ""
	DefaultBurndownDebug                = false
	DefaultBurndownGoroutines           = 0
)

// Couples analyzer defaults.
const (
	DefaultCouplesCouplingThresholdHigh      = 10
	DefaultCouplesOwnershipFewThreshold      = 3
	DefaultCouplesOwnershipModerateThreshold = 5
	DefaultCouplesBatchCouplingThreshold     = 100
	DefaultCouplesHLLPrecision               = 10
	DefaultCouplesTopKPerFile                = 100
	DefaultCouplesMinEdgeWeight              = 2
)

// Devs analyzer defaults.
const (
	DefaultDevsConsiderEmptyCommits  = false
	DefaultDevsAnonymize             = false
	DefaultDevsBusFactorThreshold    = 0.5
	DefaultDevsRiskThresholdCritical = 90.0
	DefaultDevsRiskThresholdHigh     = 80.0
	DefaultDevsRiskThresholdMedium   = 60.0
	DefaultDevsActiveThresholdRatio  = 0.7
	DefaultDevsDefaultActiveDays     = 90
	DefaultDevsHLLPrecision          = 14
)

// File history analyzer defaults.
const (
	DefaultFileHistoryHotspotCritical = 50
	DefaultFileHistoryHotspotHigh     = 30
	DefaultFileHistoryHotspotMedium   = 15
)

// Imports analyzer defaults.
const (
	DefaultImportsGoroutines            = 4
	DefaultImportsMaxFileSize           = 1 << 20 // 1 MiB.
	DefaultImportsMaxDependencyRiskRows = 30
)

// Sentiment analyzer defaults.
const (
	DefaultSentimentMinCommentLength       = 20
	DefaultSentimentGap                    = 0.5
	DefaultSentimentNeutralizerWeight      = 0.8
	DefaultSentimentMaxWeightRatio         = 3.0
	DefaultSentimentPositiveThreshold      = 0.6
	DefaultSentimentNegativeThreshold      = 0.4
	DefaultSentimentTrendThreshold         = 0.1
	DefaultSentimentLowSentimentRiskThresh = 0.2
)

// Shotness analyzer defaults.
const (
	DefaultShotnessDSLStruct = `filter(.roles has "Function")`
	DefaultShotnessDSLName   = ".props.name"
)

// Typos analyzer defaults.
const (
	DefaultTyposMaxDistance = 4
)

// Anomaly analyzer defaults.
const (
	DefaultAnomalyThreshold  = 2.0
	DefaultAnomalyWindowSize = 20
)

// Clones analyzer defaults.
const (
	DefaultClonesMaxClonePairs        = 1000
	DefaultClonesNumHashes            = 128
	DefaultClonesNumBands             = 16
	DefaultClonesNumRows              = 8
	DefaultClonesShingleSize          = 5
	DefaultClonesSimilarityType2      = 0.8
	DefaultClonesSimilarityType3      = 0.5
	DefaultClonesThresholdRatioYellow = 0.1
	DefaultClonesThresholdRatioRed    = 0.3
	DefaultClonesThresholdPairsYellow = 5
	DefaultClonesThresholdPairsRed    = 20
)

// Checkpoint defaults.
const (
	DefaultCheckpointEnabled   = true
	DefaultCheckpointDir       = ""
	DefaultCheckpointResume    = true
	DefaultCheckpointClearPrev = false
)
