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

// Devs analyzer defaults.
const (
	DefaultDevsConsiderEmptyCommits = false
	DefaultDevsAnonymize            = false
)

// Imports analyzer defaults.
const (
	DefaultImportsGoroutines  = 4
	DefaultImportsMaxFileSize = 1 << 20 // 1 MiB.
)

// Sentiment analyzer defaults.
const (
	DefaultSentimentMinCommentLength = 20
	DefaultSentimentGap              = 0.5
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

// Checkpoint defaults.
const (
	DefaultCheckpointEnabled   = true
	DefaultCheckpointDir       = ""
	DefaultCheckpointResume    = true
	DefaultCheckpointClearPrev = false
)
