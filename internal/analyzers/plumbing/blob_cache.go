// Package plumbing provides plumbing functionality.
package plumbing

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"runtime"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// BlobCacheAnalyzer loads and caches file blobs for each commit.
type BlobCacheAnalyzer struct {
	TreeDiff                *TreeDiffAnalyzer
	Repository              *gitlib.Repository
	previousCache           map[gitlib.Hash]*gitlib.CachedBlob
	Cache                   map[gitlib.Hash]*gitlib.CachedBlob
	FailOnMissingSubmodules bool
	Goroutines              int
	repos                   []*gitlib.Repository
}

const (
	// ConfigBlobCacheFailOnMissingSubmodules is the configuration key for failing on missing submodules.
	ConfigBlobCacheFailOnMissingSubmodules = "BlobCache.FailOnMissingSubmodules"
	// ConfigBlobCacheGoroutines is the configuration key for parallel blob loading.
	ConfigBlobCacheGoroutines = "BlobCache.Goroutines"
)

// Name returns the name of the analyzer.
func (b *BlobCacheAnalyzer) Name() string {
	return "BlobCache"
}

// Flag returns the CLI flag for the analyzer.
func (b *BlobCacheAnalyzer) Flag() string {
	return "blob-cache"
}

// Description returns a human-readable description of the analyzer.
func (b *BlobCacheAnalyzer) Description() string {
	return b.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (b *BlobCacheAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeHistory,
		b.Name(),
		"Loads the blobs which correspond to the changed files in a commit.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (b *BlobCacheAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name: ConfigBlobCacheFailOnMissingSubmodules,
		Description: "Specifies whether to panic if any referenced submodule does " +
			"not exist in .gitmodules and thus the corresponding Git object cannot be loaded. " +
			"Override this if you want to ensure that your repository is integral.",
		Flag:    "fail-on-missing-submodules",
		Type:    pipeline.BoolConfigurationOption,
		Default: false},
		{
			Name:        ConfigBlobCacheGoroutines,
			Description: "Number of goroutines to use for parallel blob loading.",
			Flag:        "blob-cache-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     runtime.NumCPU(),
		},
	}
}

// Configure sets up the analyzer with the provided facts.
func (b *BlobCacheAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigBlobCacheFailOnMissingSubmodules].(bool); exists {
		b.FailOnMissingSubmodules = val
	}

	if val, exists := facts[ConfigBlobCacheGoroutines].(int); exists {
		b.Goroutines = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (b *BlobCacheAnalyzer) Initialize(repo *gitlib.Repository) error {
	b.Repository = repo
	b.previousCache = map[gitlib.Hash]*gitlib.CachedBlob{}

	if b.Goroutines <= 0 {
		b.Goroutines = runtime.NumCPU()
	}

	// Create repository pool for parallel access.
	// We reuse the main repo for worker 0.
	b.repos = make([]*gitlib.Repository, b.Goroutines)
	b.repos[0] = repo

	for i := 1; i < b.Goroutines; i++ {
		clonedRepo, err := gitlib.OpenRepository(repo.Path())
		if err != nil {
			// Cleanup already opened repos.
			for j := 1; j < i; j++ {
				b.repos[j].Free()
			}

			return fmt.Errorf("failed to open repository clone for worker %d: %w", i, err)
		}

		b.repos[i] = clonedRepo
	}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (b *BlobCacheAnalyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	// Check if the runtime pipeline has already populated the cache.
	if ac != nil && ac.BlobCache != nil {
		// Use the pre-populated cache from the runtime pipeline.
		b.previousCache = ac.BlobCache
		b.Cache = ac.BlobCache

		return analyze.TC{}, nil
	}

	// Fall back to traditional blob loading.
	changes := b.TreeDiff.Changes

	b.consumeParallel(ctx, changes)

	return analyze.TC{}, nil
}

// consumeParallel is the original parallel blob loading implementation.
func (b *BlobCacheAnalyzer) consumeParallel(ctx context.Context, changes []*gitlib.Change) {
	cache := map[gitlib.Hash]*gitlib.CachedBlob{}
	newCache := map[gitlib.Hash]*gitlib.CachedBlob{}

	var mu sync.Mutex

	// Helper to process a batch of changes.
	process := func(repo *gitlib.Repository, changes []*gitlib.Change) {
		localCache := map[gitlib.Hash]*gitlib.CachedBlob{}
		localNewCache := map[gitlib.Hash]*gitlib.CachedBlob{}

		for _, change := range changes {
			switch change.Action {
			case gitlib.Insert:
				b.handleInsert(ctx, repo, change, localCache, localNewCache)
			case gitlib.Delete:
				b.handleDelete(ctx, repo, change, localCache)
			case gitlib.Modify:
				b.handleModify(ctx, repo, change, localCache, localNewCache)
			}
		}

		mu.Lock()
		maps.Copy(cache, localCache)
		maps.Copy(newCache, localNewCache)
		mu.Unlock()
	}

	if len(changes) < b.Goroutines || b.Goroutines <= 1 {
		process(b.Repository, changes)
	} else {
		var wg sync.WaitGroup

		batchSize := (len(changes) + b.Goroutines - 1) / b.Goroutines

		for i := range b.Goroutines {
			start := i * batchSize
			if start >= len(changes) {
				break
			}

			end := min(start+batchSize, len(changes))

			wg.Add(1)

			go func(idx int, batch []*gitlib.Change) {
				defer wg.Done()

				process(b.repos[idx], batch)
			}(i, changes[start:end])
		}

		wg.Wait()
	}

	b.previousCache = newCache
	b.Cache = cache
}

func (b *BlobCacheAnalyzer) handleInsert(
	ctx context.Context,
	repo *gitlib.Repository,
	change *gitlib.Change,
	cache, newCache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	hash := change.To.Hash

	// Initialize with empty blob.
	cache[hash] = &gitlib.CachedBlob{}
	newCache[hash] = &gitlib.CachedBlob{}
	// Try to load the blob.
	blob, err := gitlib.NewCachedBlobFromRepo(ctx, repo, hash)
	if err == nil {
		cache[hash] = blob
		newCache[hash] = blob
	}
}
func (b *BlobCacheAnalyzer) handleDelete(
	ctx context.Context,
	repo *gitlib.Repository,
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	hash := change.From.Hash

	// Check if we have it cached.
	// NOTE: b.previousCache read is safe here because it's read-only during Consume
	// phase and updated only at the end.
	existing, exists := b.previousCache[hash]

	if exists {
		cache[hash] = existing

		return
	}

	// Initialize with empty blob.
	cache[hash] = &gitlib.CachedBlob{}
	// Try to load the blob.
	blob, err := gitlib.NewCachedBlobFromRepo(ctx, repo, hash)
	if err == nil {
		cache[hash] = blob
	}
}
func (b *BlobCacheAnalyzer) handleModify(
	ctx context.Context,
	repo *gitlib.Repository,
	change *gitlib.Change,
	cache, newCache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	// Handle "to" side (new version).
	toHash := change.To.Hash
	cache[toHash] = &gitlib.CachedBlob{}
	newCache[toHash] = &gitlib.CachedBlob{}

	blob, err := gitlib.NewCachedBlobFromRepo(ctx, repo, toHash)
	if err == nil {
		cache[toHash] = blob
		newCache[toHash] = blob
	}

	// Handle "from" side (old version).
	fromHash := change.From.Hash

	existing, exists := b.previousCache[fromHash]

	if exists {
		cache[fromHash] = existing

		return
	}

	cache[fromHash] = &gitlib.CachedBlob{}

	blob, err = gitlib.NewCachedBlobFromRepo(ctx, repo, fromHash)
	if err == nil {
		cache[fromHash] = blob
	}
}

// Fork creates a copy of the analyzer for parallel processing.
func (b *BlobCacheAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := *b
		// Deep copy cache.
		clone.previousCache = map[gitlib.Hash]*gitlib.CachedBlob{}
		maps.Copy(clone.previousCache, b.previousCache)

		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (b *BlobCacheAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (b *BlobCacheAnalyzer) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

// WorkingStateSize returns 0 — plumbing analyzers are excluded from budget planning.
func (b *BlobCacheAnalyzer) WorkingStateSize() int64 { return 0 }

// AvgTCSize returns 0 — plumbing analyzers do not emit meaningful TC payloads.
func (b *BlobCacheAnalyzer) AvgTCSize() int64 { return 0 }

// NewAggregator returns nil — plumbing analyzers do not aggregate.
func (b *BlobCacheAnalyzer) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return nil
}

// SerializeTICKs returns ErrNotImplemented — plumbing analyzers do not produce TICKs.
func (b *BlobCacheAnalyzer) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

// ReportFromTICKs returns ErrNotImplemented — plumbing analyzers do not produce reports.
func (b *BlobCacheAnalyzer) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}

// InjectPreparedData sets pre-computed cache from parallel preparation.
func (b *BlobCacheAnalyzer) InjectPreparedData(
	_ []*gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
	_ map[string]any,
) {
	b.Cache = cache
}
