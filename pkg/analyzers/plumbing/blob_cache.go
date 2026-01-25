package plumbing

import (
	"io"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
)

type BlobCacheAnalyzer struct {
	// Configuration
	FailOnMissingSubmodules bool

	// Dependencies
	TreeDiff *TreeDiffAnalyzer

	// State
	repository *git.Repository
	cache      map[gitplumbing.Hash]*pkgplumbing.CachedBlob
	
	// Output
	Cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob

	// Internal
	l interface {
		Errorf(format string, args ...interface{})
	}
}

const (
	ConfigBlobCacheFailOnMissingSubmodules = "BlobCache.FailOnMissingSubmodules"
)

func (b *BlobCacheAnalyzer) Name() string {
	return "BlobCache"
}

func (b *BlobCacheAnalyzer) Flag() string {
	return "blob-cache"
}

func (b *BlobCacheAnalyzer) Description() string {
	return "Loads the blobs which correspond to the changed files in a commit."
}

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

func (b *BlobCacheAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigBlobCacheFailOnMissingSubmodules].(bool); exists {
		b.FailOnMissingSubmodules = val
	}
	return nil
}

func (b *BlobCacheAnalyzer) Initialize(repository *git.Repository) error {
	b.repository = repository
	b.cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
	return nil
}

func (b *BlobCacheAnalyzer) Consume(ctx *analyze.Context) error {
	// Need TreeDiff changes
	changes := b.TreeDiff.Changes
	commit := ctx.Commit
	
	cache := map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
	newCache := map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
	
	for _, change := range changes {
		action, err := change.Action()
		if err != nil {
			return err
		}
		var exists bool
		var blob *object.Blob
		switch action {
		case merkletrie.Insert:
			cache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
			newCache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
			blob, err = b.getBlob(&change.To, commit.File)
			if err != nil {
				// Log error
			} else {
				cb := &pkgplumbing.CachedBlob{Blob: *blob}
				err = cb.Cache()
				if err == nil {
					cache[change.To.TreeEntry.Hash] = cb
					newCache[change.To.TreeEntry.Hash] = cb
				}
			}
		case merkletrie.Delete:
			cache[change.From.TreeEntry.Hash], exists = b.cache[change.From.TreeEntry.Hash]
			if !exists {
				cache[change.From.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
				blob, err = b.getBlob(&change.From, commit.File)
				if err != nil {
					if err.Error() == gitplumbing.ErrObjectNotFound.Error() {
						blob, _ = pkgplumbing.CreateDummyBlob(change.From.TreeEntry.Hash)
						cache[change.From.TreeEntry.Hash] = &pkgplumbing.CachedBlob{Blob: *blob}
					}
				} else {
					cb := &pkgplumbing.CachedBlob{Blob: *blob}
					err = cb.Cache()
					if err == nil {
						cache[change.From.TreeEntry.Hash] = cb
					}
				}
			}
		case merkletrie.Modify:
			// To
			blob, err = b.getBlob(&change.To, commit.File)
			cache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
			newCache[change.To.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
			if err == nil {
				cb := &pkgplumbing.CachedBlob{Blob: *blob}
				err = cb.Cache()
				if err == nil {
					cache[change.To.TreeEntry.Hash] = cb
					newCache[change.To.TreeEntry.Hash] = cb
				}
			}
			// From
			cache[change.From.TreeEntry.Hash], exists = b.cache[change.From.TreeEntry.Hash]
			if !exists {
				cache[change.From.TreeEntry.Hash] = &pkgplumbing.CachedBlob{}
				blob, err = b.getBlob(&change.From, commit.File)
				if err == nil {
					cb := &pkgplumbing.CachedBlob{Blob: *blob}
					err = cb.Cache()
					if err == nil {
						cache[change.From.TreeEntry.Hash] = cb
					}
				}
			}
		}
	}
	b.cache = newCache
	b.Cache = cache
	return nil
}

type FileGetter func(path string) (*object.File, error)

func (b *BlobCacheAnalyzer) getBlob(entry *object.ChangeEntry, fileGetter FileGetter) (*object.Blob, error) {
	blob, err := b.repository.BlobObject(entry.TreeEntry.Hash)
	if err != nil {
		// Simplified submodule handling for now
		return nil, err
	}
	return blob, nil
}

func (b *BlobCacheAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil
}

func (b *BlobCacheAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *b
		// Deep copy cache?
		clone.cache = map[gitplumbing.Hash]*pkgplumbing.CachedBlob{}
		for k, v := range b.cache {
			clone.cache[k] = v
		}
		res[i] = &clone
	}
	return res
}

func (b *BlobCacheAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (b *BlobCacheAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	return nil
}
