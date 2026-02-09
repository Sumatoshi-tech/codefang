package framework

import (
	"errors"
	"fmt"
	"strings"

	"github.com/dustin/go-humanize"
)

// Sentinel errors for configuration.
var (
	ErrInvalidSizeFormat = errors.New("invalid size format")
	ErrInvalidGCPercent  = errors.New("invalid GC percent")
)

// Maximum integer values for safe conversion from uint64.
const (
	maxInt   = int(^uint(0) >> 1)
	maxInt64 = int64(^uint64(0) >> 1)
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

		return cfg, SafeInt64(budgetBytes), nil
	}

	config := DefaultCoordinatorConfig()

	applyIntParams(&config, params)

	sizeErr := applySizeParams(&config, params)
	if sizeErr != nil {
		return config, 0, sizeErr
	}

	tuningErr := applyRuntimeTuningParams(&config, params.GCPercent, params.BallastSize)
	if tuningErr != nil {
		return config, 0, tuningErr
	}

	return config, 0, nil
}

func buildConfigFromBudget(budgetStr string, solver BudgetSolver) (CoordinatorConfig, error) {
	budgetBytes, err := humanize.ParseBytes(budgetStr)
	if err != nil {
		return CoordinatorConfig{}, fmt.Errorf("%w for memory-budget: %s", ErrInvalidSizeFormat, budgetStr)
	}

	cfg, err := solver(SafeInt64(budgetBytes))
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
}

func applySizeParams(config *CoordinatorConfig, params ConfigParams) error {
	if params.BlobCacheSize != "" {
		size, parseErr := humanize.ParseBytes(params.BlobCacheSize)
		if parseErr != nil {
			return fmt.Errorf("%w for blob-cache-size: %s", ErrInvalidSizeFormat, params.BlobCacheSize)
		}

		config.BlobCacheSize = SafeInt64(size)
	}

	if params.BlobArenaSize != "" {
		size, parseErr := humanize.ParseBytes(params.BlobArenaSize)
		if parseErr != nil {
			return fmt.Errorf("%w for blob-arena-size: %s", ErrInvalidSizeFormat, params.BlobArenaSize)
		}

		config.BlobArenaSize = SafeInt(size)
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

	return SafeInt64(parsed), nil
}

// SafeInt64 converts uint64 to int64, clamping to maxInt64 to prevent overflow.
func SafeInt64(v uint64) int64 {
	if v > uint64(maxInt64) {
		return maxInt64
	}

	return int64(v)
}

// SafeInt converts uint64 to int, clamping to maxInt to prevent overflow.
func SafeInt(v uint64) int {
	if v > uint64(maxInt) {
		return maxInt
	}

	return int(v)
}
