package framework

import (
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/dustin/go-humanize"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
)

// Sentinel errors for configuration.
var (
	ErrInvalidSizeFormat = errors.New("invalid size format")
	ErrInvalidGCPercent  = errors.New("invalid GC percent")
)

// ConfigParams holds raw CLI parameter values for building a CoordinatorConfig.
// All size strings use humanize format (e.g. "256MB", "1GiB").
type ConfigParams struct {
	Workers         int
	BufferSize      int
	CommitBatchSize int
	BlobCacheSize   string
	DiffCacheSize   int
	BlobArenaSize   string
	MemoryBudget    string
	GCPercent       int
	BallastSize     string

	// Advanced pipeline tuning (zero = use defaults).
	UASTSpillThreshold           int
	IntraCommitParallelThreshold int
	MaxIntraCommitWorkers        int
	MaxUASTBlobSize              int
	UASTParseTimeout             string
	MaxChangesPerCommit          int
	MaxDiffBatchSize             int
	MemoryBudgetRatio            int
	MemoryBudgetCap              string
	MemoryLimitRatio             int

	// Extended pipeline tuning.
	UASTSpillTrimInterval   int
	NativeTrimInterval      int
	MaxStreamingBuffering   int
	DrainPrefetchTimeout    string
	SamplerInterval         string
	WorkerRatio             int
	UASTWorkerRatio         int
	LeafWorkerDivisor       int
	MinLeafWorkers          int
	BufferSizeMultiplier    int
	BudgetLimitRatio        int
	SystemRAMLimitRatio     int
	DiffJobBufferMultiplier int
}

// CheckpointParams holds checkpoint-related configuration.
type CheckpointParams struct {
	Enabled   bool
	Dir       string
	Resume    bool
	ClearPrev bool
}

// BudgetSolver resolves a memory budget (in bytes) to a CoordinatorConfig.
type BudgetSolver func(budgetBytes int64) (CoordinatorConfig, error)

// defaultMemoryBudgetRatio is the fraction of system memory to use as default budget.
const defaultMemoryBudgetRatio = 50

// percentDenominator is the divisor for converting a percentage ratio to a fraction.
const percentDenominator = 100

// defaultMemoryBudgetCap is the maximum auto-detected memory budget (2 GiB).
// This forces smaller chunks on large repos, keeping peak RSS bounded.
// Native C memory (libgit2 mwindow, object cache, glibc arenas) adds ~1.5 GiB
// on top of the Go heap budget, so a 2 GiB budget targets ~3.5 GiB total RSS.
const defaultMemoryBudgetCap = int64(2 * 1024 * 1024 * 1024)

// DefaultMemoryBudget returns a sensible memory budget based on available system memory.
// Returns min(50% of total RAM, 2 GiB), or 0 if detection fails.
func DefaultMemoryBudget() int64 {
	return DefaultMemoryBudgetWithParams(defaultMemoryBudgetRatio, "")
}

// DefaultMemoryBudgetWithParams returns a memory budget with configurable ratio and cap.
// Empty cap string uses defaultMemoryBudgetCap.
func DefaultMemoryBudgetWithParams(ratio int, capStr string) int64 {
	total := detectTotalMemoryBytes()
	if total == 0 {
		return 0
	}

	budgetCap := defaultMemoryBudgetCap

	if capStr != "" {
		parsed, err := humanize.ParseBytes(capStr)
		if err == nil && parsed > 0 {
			budgetCap = safeconv.SafeInt64(parsed)
		}
	}

	budget := safeconv.SafeInt64(total * uint64(ratio) / percentDenominator)

	return min(budget, budgetCap)
}

// BuildConfigFromParams builds a CoordinatorConfig from raw parameters.
// Returns the config and the memory budget in bytes (0 if not set).
// The budgetSolver is called when params.MemoryBudget is set; pass nil if
// memory-budget is not supported.
func BuildConfigFromParams(params ConfigParams, budgetSolver BudgetSolver) (CoordinatorConfig, int64, error) {
	if params.MemoryBudget != "" {
		cfg, budgetErr := buildConfigFromBudget(params.MemoryBudget, budgetSolver)
		if budgetErr != nil {
			return CoordinatorConfig{}, 0, budgetErr
		}

		runtimeErr := applyRuntimeTuningParams(&cfg, params.GCPercent, params.BallastSize)
		if runtimeErr != nil {
			return CoordinatorConfig{}, 0, runtimeErr
		}

		budgetBytes, parseErr := humanize.ParseBytes(params.MemoryBudget)
		if parseErr != nil {
			return CoordinatorConfig{}, 0, fmt.Errorf("failed to parse budget: %w", parseErr)
		}

		return cfg, safeconv.SafeInt64(budgetBytes), nil
	}

	config := DefaultCoordinatorConfig()

	applyIntParams(&config, params)

	sizeErr := applySizeParams(&config, params)
	if sizeErr != nil {
		return config, 0, sizeErr
	}

	advErr := applyAdvancedParams(&config, params)
	if advErr != nil {
		return config, 0, advErr
	}

	tuningErr := applyRuntimeTuningParams(&config, params.GCPercent, params.BallastSize)
	if tuningErr != nil {
		return config, 0, tuningErr
	}

	// Auto-detect memory budget from system memory when not explicitly set.
	budgetRatio := params.MemoryBudgetRatio
	if budgetRatio == 0 {
		budgetRatio = defaultMemoryBudgetRatio
	}

	memBudget := DefaultMemoryBudgetWithParams(budgetRatio, params.MemoryBudgetCap)

	return config, memBudget, nil
}

func buildConfigFromBudget(budgetStr string, solver BudgetSolver) (CoordinatorConfig, error) {
	budgetBytes, err := humanize.ParseBytes(budgetStr)
	if err != nil {
		return CoordinatorConfig{}, fmt.Errorf("%w for memory-budget: %s", ErrInvalidSizeFormat, budgetStr)
	}

	cfg, err := solver(safeconv.SafeInt64(budgetBytes))
	if err != nil {
		return CoordinatorConfig{}, fmt.Errorf("memory budget error: %w", err)
	}

	return cfg, nil
}

func applyIntParams(config *CoordinatorConfig, params ConfigParams) {
	if params.Workers > 0 {
		config.Workers = params.Workers
	}

	if params.BufferSize > 0 {
		config.BufferSize = params.BufferSize
	}

	if params.CommitBatchSize > 0 {
		config.CommitBatchSize = params.CommitBatchSize
	}

	if params.DiffCacheSize > 0 {
		config.DiffCacheSize = params.DiffCacheSize
	}

	if params.UASTSpillThreshold > 0 {
		config.UASTSpillThreshold = params.UASTSpillThreshold
	}

	if params.IntraCommitParallelThreshold > 0 {
		config.IntraCommitParallelThreshold = params.IntraCommitParallelThreshold
	}

	if params.MaxIntraCommitWorkers > 0 {
		config.MaxIntraCommitWorkers = params.MaxIntraCommitWorkers
	}

	if params.MaxUASTBlobSize > 0 {
		config.MaxUASTBlobSize = params.MaxUASTBlobSize
	}

	if params.MaxChangesPerCommit > 0 {
		config.MaxChangesPerCommit = params.MaxChangesPerCommit
	}

	if params.MaxDiffBatchSize > 0 {
		config.MaxDiffBatchSize = params.MaxDiffBatchSize
	}

	if params.MemoryLimitRatio > 0 {
		config.MemoryLimitRatio = params.MemoryLimitRatio
	}

	applyExtendedIntParams(config, params)
}

func applyExtendedIntParams(config *CoordinatorConfig, params ConfigParams) {
	if params.UASTSpillTrimInterval > 0 {
		config.UASTSpillTrimInterval = params.UASTSpillTrimInterval
	}

	if params.NativeTrimInterval > 0 {
		config.NativeTrimInterval = params.NativeTrimInterval
	}

	if params.MaxStreamingBuffering > 0 {
		config.MaxStreamingBuffering = params.MaxStreamingBuffering
	}

	if params.WorkerRatio > 0 {
		config.WorkerRatio = params.WorkerRatio
	}

	if params.UASTWorkerRatio > 0 {
		config.UASTWorkerRatio = params.UASTWorkerRatio
	}

	if params.LeafWorkerDivisor > 0 {
		config.LeafWorkerDivisor = params.LeafWorkerDivisor
	}

	if params.MinLeafWorkers > 0 {
		config.MinLeafWorkers = params.MinLeafWorkers
	}

	if params.BufferSizeMultiplier > 0 {
		config.BufferSizeMultiplier = params.BufferSizeMultiplier
	}

	if params.BudgetLimitRatio > 0 {
		config.BudgetLimitRatio = params.BudgetLimitRatio
	}

	if params.SystemRAMLimitRatio > 0 {
		config.SystemRAMLimitRatio = params.SystemRAMLimitRatio
	}

	if params.DiffJobBufferMultiplier > 0 {
		config.DiffJobBufferMultiplier = params.DiffJobBufferMultiplier
	}
}

func applyAdvancedParams(config *CoordinatorConfig, params ConfigParams) error {
	if params.UASTParseTimeout != "" {
		d, err := time.ParseDuration(params.UASTParseTimeout)
		if err != nil {
			return fmt.Errorf("%w for uast-parse-timeout: %s", ErrInvalidSizeFormat, params.UASTParseTimeout)
		}

		config.UASTParseTimeout = d
	}

	if params.DrainPrefetchTimeout != "" {
		d, err := time.ParseDuration(params.DrainPrefetchTimeout)
		if err != nil {
			return fmt.Errorf("%w for drain-prefetch-timeout: %s", ErrInvalidSizeFormat, params.DrainPrefetchTimeout)
		}

		config.DrainPrefetchTimeout = d
	}

	if params.SamplerInterval != "" {
		d, err := time.ParseDuration(params.SamplerInterval)
		if err != nil {
			return fmt.Errorf("%w for sampler-interval: %s", ErrInvalidSizeFormat, params.SamplerInterval)
		}

		config.SamplerInterval = d
	}

	return nil
}

func applySizeParams(config *CoordinatorConfig, params ConfigParams) error {
	if params.BlobCacheSize != "" {
		size, parseErr := humanize.ParseBytes(params.BlobCacheSize)
		if parseErr != nil {
			return fmt.Errorf("%w for blob-cache-size: %s", ErrInvalidSizeFormat, params.BlobCacheSize)
		}

		config.BlobCacheSize = safeconv.SafeInt64(size)
	}

	if params.BlobArenaSize != "" {
		size, parseErr := humanize.ParseBytes(params.BlobArenaSize)
		if parseErr != nil {
			return fmt.Errorf("%w for blob-arena-size: %s", ErrInvalidSizeFormat, params.BlobArenaSize)
		}

		config.BlobArenaSize = safeconv.SafeInt(size)
	}

	return nil
}

func applyRuntimeTuningParams(config *CoordinatorConfig, gcPercent int, ballastSize string) error {
	if gcPercent < 0 {
		return fmt.Errorf("%w: %d", ErrInvalidGCPercent, gcPercent)
	}

	config.GCPercent = gcPercent

	ballastBytes, err := ParseOptionalSize(ballastSize)
	if err != nil {
		return err
	}

	config.BallastSize = ballastBytes

	return nil
}

// ParseOptionalSize parses a human-readable size string, returning 0 for empty or "0".
func ParseOptionalSize(sizeValue string) (int64, error) {
	trimmed := strings.TrimSpace(sizeValue)
	if trimmed == "" || trimmed == "0" {
		return 0, nil
	}

	parsed, err := humanize.ParseBytes(trimmed)
	if err != nil {
		return 0, fmt.Errorf("%w for ballast-size: %s", ErrInvalidSizeFormat, sizeValue)
	}

	return safeconv.SafeInt64(parsed), nil
}
