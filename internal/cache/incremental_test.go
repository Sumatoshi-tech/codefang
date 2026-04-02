// FRD: specs/frds/FRD-20260328-incremental-cache-meta.md.

package cache

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCacheKey_Deterministic(t *testing.T) {
	t.Parallel()

	key1 := Key("abc123", "main")
	key2 := Key("abc123", "main")
	assert.Equal(t, key1, key2, "same inputs must produce same key")
	assert.NotEmpty(t, key1)
}

func TestCacheKey_DifferentBranch(t *testing.T) {
	t.Parallel()

	key1 := Key("abc123", "main")
	key2 := Key("abc123", "feature/x")
	assert.NotEqual(t, key1, key2, "different branches must produce different keys")
}

func TestCacheKey_DifferentRoot(t *testing.T) {
	t.Parallel()

	key1 := Key("abc123", "main")
	key2 := Key("def456", "main")
	assert.NotEqual(t, key1, key2, "different root SHAs must produce different keys")
}

func testMeta() IncrementalMeta {
	return IncrementalMeta{
		Version:     1,
		HeadSHA:     "abc123def456",
		Branch:      "main",
		RootSHA:     "root789",
		CommitCount: 1000,
		AnalyzerIDs: []string{"burndown", "couples"},
		Timestamp:   time.Date(2026, 3, 28, 12, 0, 0, 0, time.UTC),
	}
}

func TestWriteReadMeta_RoundTrip(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	original := testMeta()

	require.NoError(t, WriteMeta(dir, original))

	got, err := ReadMeta(dir)
	require.NoError(t, err)

	assert.Equal(t, original.Version, got.Version)
	assert.Equal(t, original.HeadSHA, got.HeadSHA)
	assert.Equal(t, original.Branch, got.Branch)
	assert.Equal(t, original.RootSHA, got.RootSHA)
	assert.Equal(t, original.CommitCount, got.CommitCount)
	assert.Equal(t, original.AnalyzerIDs, got.AnalyzerIDs)
	assert.True(t, original.Timestamp.Equal(got.Timestamp))
}

func TestReadMeta_MissingFile(t *testing.T) {
	t.Parallel()

	_, err := ReadMeta(t.TempDir())
	assert.ErrorIs(t, err, ErrCacheNotFound)
}

func TestReadMeta_CorruptFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "cache.json"), []byte("{not valid json"), 0o600))

	_, err := ReadMeta(dir)
	assert.ErrorIs(t, err, ErrCacheCorrupt)
}

func TestIsStale_MatchingRootSHA(t *testing.T) {
	t.Parallel()

	meta := testMeta()
	assert.False(t, IsStale(meta, meta.RootSHA))
}

func TestIsStale_MismatchingRootSHA(t *testing.T) {
	t.Parallel()

	meta := testMeta()
	assert.True(t, IsStale(meta, "different_root"))
}
