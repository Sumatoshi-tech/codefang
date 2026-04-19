package cache

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/storage"
	"github.com/Sumatoshi-tech/codefang/pkg/textutil"
)

// metaFilename is the name of the cache metadata file.
const metaFilename = "cache.json"

// metaFilePerm is the file permission for cache metadata.
const metaFilePerm = 0o640

// cacheKeySeparator separates root SHA and branch in the cache key input.
const cacheKeySeparator = ":"

// ErrCacheNotFound is returned when the cache metadata file does not exist.
var ErrCacheNotFound = errors.New("cache metadata not found")

// ErrCacheCorrupt is returned when the cache metadata file cannot be parsed.
var ErrCacheCorrupt = errors.New("cache metadata corrupt")

// IncrementalMeta holds metadata for an incremental analysis cache.
type IncrementalMeta struct {
	Version     int       `json:"version"`
	HeadSHA     string    `json:"head_sha"`
	Branch      string    `json:"branch"`
	RootSHA     string    `json:"root_sha"`
	CommitCount int       `json:"commit_count"`
	AnalyzerIDs []string  `json:"analyzer_ids"`
	Timestamp   time.Time `json:"timestamp"`
}

// Key produces a deterministic directory name from root SHA and branch.
// The key is a SHA-256 hash of "rootSHA:branch", hex-encoded.
func Key(rootSHA, branch string) string {
	h := sha256.New()
	h.Write([]byte(rootSHA + cacheKeySeparator + branch))

	return hex.EncodeToString(h.Sum(nil))
}

// IsStale returns true when the cached root SHA does not match the current root SHA,
// indicating a force-push or history rewrite.
func IsStale(meta IncrementalMeta, currentRootSHA string) bool {
	return meta.RootSHA != currentRootSHA
}

// WriteMeta atomically writes cache metadata as indented JSON to dir/cache.json.
func WriteMeta(dir string, meta IncrementalMeta) error {
	metaPath := filepath.Join(dir, metaFilename)

	return storage.WriteAtomic(metaPath, metaFilePerm, func(w io.Writer) error {
		return textutil.WriteJSON(w, meta, true)
	})
}

// ReadMeta reads and parses cache metadata from dir/cache.json.
// Returns ErrCacheNotFound if the file does not exist.
// Returns ErrCacheCorrupt if the file cannot be parsed.
func ReadMeta(dir string) (IncrementalMeta, error) {
	metaPath := filepath.Join(dir, metaFilename)

	data, err := os.ReadFile(metaPath)
	if err != nil {
		if os.IsNotExist(err) {
			return IncrementalMeta{}, ErrCacheNotFound
		}

		return IncrementalMeta{}, fmt.Errorf("read cache meta: %w", err)
	}

	var meta IncrementalMeta

	unmarshalErr := json.Unmarshal(data, &meta)
	if unmarshalErr != nil {
		return IncrementalMeta{}, fmt.Errorf("%w: %w", ErrCacheCorrupt, unmarshalErr)
	}

	return meta, nil
}
