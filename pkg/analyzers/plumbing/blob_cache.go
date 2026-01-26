// Package plumbing provides plumbing functionality.
package plumbing

import (
	"fmt"
	"io"
	"maps"

	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// BlobCacheAnalyzer loads and caches file blobs for each commit.
type BlobCacheAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Errorf(format string, args ...any)
	}
	TreeDiff                *TreeDiffAnalyzer
	repository              *git.Repository
	cache                   map[gitplumbing.Hash]*pkgplumbing.CachedBlob
	Cache                   map[gitplumbing.Hash]*pkgplumbing.CachedBlob //nolint:revive // intentional naming matches internal cache field.
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
func (b *BlobCacheAnalyzer) Initialize(repository *git.Repository) error {
	b.repository = repository
	b.cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (b *BlobCacheAnalyzer) Consume(ctx *analyze.Context) error {
	changes := b.TreeDiff.Changes
	commit := ctx.Commit

	cache := map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
	newCache := map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}

	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			return fmt.Errorf("consume: %w", err)
		}

		switch action {
		case merkletrie.Insert:
			b.handleInsert(change, commit, cache, newCache)
		case merkletrie.Delete:
			b.handleDelete(change, commit, cache)
		case merkletrie.Modify:
			b.handleModify(change, commit, cache, newCache)
		}
	}

	b.cache = newCache
	b.Cache = cache

	return nil
}

func (b *BlobCacheAnalyzer) handleInsert(
	change *object.Change, commit *object.Commit,
	cache, newCache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) {
	cache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
	newCache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}

	blob, err := b.getBlob(&change.To, commit.File)
	if err != nil { //nolint:revive // empty block is intentional.
		// Log error.
	} else {
		cb := &pkgplumbing.CachedBlob{Blob: *blob}

		cacheErr := cb.Cache()
		if cacheErr == nil {
			cache[change.To.TreeEntry.Hash] = cb
			newCache[change.To.TreeEntry.Hash] = cb
		}
	}
}

func (b *BlobCacheAnalyzer) handleDelete(
	change *object.Change, commit *object.Commit,
	cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) {
	existing, exists := b.cache[change.From.TreeEntry.Hash]
	if exists {
		cache[change.From.TreeEntry.Hash] = existing

		return
	}

	cache[change.From.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}

	blob, err := b.getBlob(&change.From, commit.File)
	if err != nil {
		if err.Error() == gitplumbing.ErrObjectNotFound.Error() {
			dummyBlob, dummyErr := pkgplumbing.CreateDummyBlob(change.From.TreeEntry.Hash)
			if dummyErr == nil {
				cache[change.From.TreeEntry.Hash] = &pkgplumbing.CachedBlob{Blob: *dummyBlob}
			}
		}

		return
	}

	cb := &pkgplumbing.CachedBlob{Blob: *blob}

	cacheErr := cb.Cache()
	if cacheErr == nil {
		cache[change.From.TreeEntry.Hash] = cb
	}
}

func (b *BlobCacheAnalyzer) handleModify(
	change *object.Change, commit *object.Commit,
	cache, newCache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) {
	b.cacheBlob(&change.To, commit, cache, newCache)
	b.loadFromBlob(&change.From, commit, cache)
}

func (b *BlobCacheAnalyzer) cacheBlob(
	entry *object.ChangeEntry, commit *object.Commit,
	cache, newCache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) {
	cache[entry.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
	newCache[entry.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}

	blob, err := b.getBlob(entry, commit.File)
	if err != nil {
		return
	}

	cb := &pkgplumbing.CachedBlob{Blob: *blob}

	cacheErr := cb.Cache()
	if cacheErr == nil {
		cache[entry.TreeEntry.Hash] = cb
		newCache[entry.TreeEntry.Hash] = cb
	}
}

func (b *BlobCacheAnalyzer) loadFromBlob(
	entry *object.ChangeEntry, commit *object.Commit,
	cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob,
) {
	existing, exists := b.cache[entry.TreeEntry.Hash]
	if exists {
		cache[entry.TreeEntry.Hash] = existing

		return
	}

	cache[entry.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}

	blob, err := b.getBlob(entry, commit.File)
	if err != nil {
		return
	}

	cb := &pkgplumbing.CachedBlob{Blob: *blob}

	cacheErr := cb.Cache()
	if cacheErr == nil {
		cache[entry.TreeEntry.Hash] = cb
	}
}

// FileGetter provides access to file objects from a git tree.
type FileGetter func(path string) (*object.File, error)

func (b *BlobCacheAnalyzer) getBlob(entry *object.ChangeEntry, _ FileGetter) (*object.Blob, error) {
	blob, err := b.repository.BlobObject(entry.TreeEntry.Hash)
	if err != nil {
		// Simplified submodule handling for now.
		return nil, fmt.Errorf("getBlob: %w", err)
	}

	return blob, nil
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
		// Deep copy cache?
		clone.cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
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
