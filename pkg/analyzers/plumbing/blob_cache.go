// Package plumbing provides plumbing functionality.
package plumbing

import (
	"io"
	"maps"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// BlobCacheAnalyzer loads and caches file blobs for each commit.
type BlobCacheAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Errorf(format string, args ...any)
	}
	TreeDiff                *TreeDiffAnalyzer
	Repository              *gitlib.Repository
	cache                   map[gitlib.Hash]*gitlib.CachedBlob
	Cache                   map[gitlib.Hash]*gitlib.CachedBlob //nolint:revive // intentional naming matches internal cache field.
	FailOnMissingSubmodules bool
}

const (
	// ConfigBlobCacheFailOnMissingSubmodules is the configuration key for failing on missing submodules.
	ConfigBlobCacheFailOnMissingSubmodules = "BlobCache.FailOnMissingSubmodules"
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
	return "Loads the blobs which correspond to the changed files in a commit."
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
		Default: false}}
}

// Configure sets up the analyzer with the provided facts.
func (b *BlobCacheAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigBlobCacheFailOnMissingSubmodules].(bool); exists {
		b.FailOnMissingSubmodules = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (b *BlobCacheAnalyzer) Initialize(repo *gitlib.Repository) error {
	b.Repository = repo
	b.cache = map[gitlib.Hash]*gitlib.CachedBlob{}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (b *BlobCacheAnalyzer) Consume(_ *analyze.Context) error {
	changes := b.TreeDiff.Changes

	cache := map[gitlib.Hash]*gitlib.CachedBlob{}
	newCache := map[gitlib.Hash]*gitlib.CachedBlob{}

	for _, change := range changes {
		switch change.Action {
		case gitlib.Insert:
			b.handleInsert(change, cache, newCache)
		case gitlib.Delete:
			b.handleDelete(change, cache)
		case gitlib.Modify:
			b.handleModify(change, cache, newCache)
		}
	}

	b.cache = newCache
	b.Cache = cache

	return nil
}

func (b *BlobCacheAnalyzer) handleInsert(
	change *gitlib.Change,
	cache, newCache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	hash := change.To.Hash

	// Initialize with empty blob.
	cache[hash] = &gitlib.CachedBlob{}
	newCache[hash] = &gitlib.CachedBlob{}

	// Try to load the blob.
	blob, err := gitlib.NewCachedBlobFromRepo(b.Repository, hash)
	if err == nil {
		cache[hash] = blob
		newCache[hash] = blob
	}
}

func (b *BlobCacheAnalyzer) handleDelete(
	change *gitlib.Change,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	hash := change.From.Hash

	// Check if we have it cached.
	existing, exists := b.cache[hash]
	if exists {
		cache[hash] = existing

		return
	}

	// Initialize with empty blob.
	cache[hash] = &gitlib.CachedBlob{}

	// Try to load the blob.
	blob, err := gitlib.NewCachedBlobFromRepo(b.Repository, hash)
	if err == nil {
		cache[hash] = blob
	}
}

func (b *BlobCacheAnalyzer) handleModify(
	change *gitlib.Change,
	cache, newCache map[gitlib.Hash]*gitlib.CachedBlob,
) {
	// Handle "to" side (new version).
	toHash := change.To.Hash
	cache[toHash] = &gitlib.CachedBlob{}
	newCache[toHash] = &gitlib.CachedBlob{}

	blob, err := gitlib.NewCachedBlobFromRepo(b.Repository, toHash)
	if err == nil {
		cache[toHash] = blob
		newCache[toHash] = blob
	}

	// Handle "from" side (old version).
	fromHash := change.From.Hash

	existing, exists := b.cache[fromHash]
	if exists {
		cache[fromHash] = existing

		return
	}

	cache[fromHash] = &gitlib.CachedBlob{}

	blob, err = gitlib.NewCachedBlobFromRepo(b.Repository, fromHash)
	if err == nil {
		cache[fromHash] = blob
	}
}

// Finalize completes the analysis and returns the result.
func (b *BlobCacheAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (b *BlobCacheAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *b
		// Deep copy cache.
		clone.cache = map[gitlib.Hash]*gitlib.CachedBlob{}
		maps.Copy(clone.cache, b.cache)

		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (b *BlobCacheAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (b *BlobCacheAnalyzer) Serialize(_ analyze.Report, _ bool, _ io.Writer) error {
	return nil
}
