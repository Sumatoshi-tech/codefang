package filehistory

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestHibernate_ClearsMergesTracker(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Add some merge entries.
	h.merges.SeenOrAdd(gitlib.NewHash("abc123"))
	h.merges.SeenOrAdd(gitlib.NewHash("def456"))

	err := h.Hibernate()
	require.NoError(t, err)

	// After Reset(), the tracker should be empty: previously-added hashes are no longer seen.
	require.False(t, h.merges.SeenOrAdd(gitlib.NewHash("abc123")), "tracker should be cleared after hibernate")
}

func TestHibernate_Succeeds(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Hibernate clears merges; lastCommitHash is preserved for Finalize.
	err := h.Hibernate()
	require.NoError(t, err)
}

func TestHibernate_ClearsFiles(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Add file history data.
	h.files["main.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{
			0: {Added: 100, Removed: 50},
		},
		Hashes: []gitlib.Hash{gitlib.NewHash("abc123")},
	}
	h.files["util.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{
			1: {Added: 30},
		},
	}

	err := h.Hibernate()
	require.NoError(t, err)

	// Files must be cleared to prevent unbounded cross-chunk growth.
	// The aggregator independently tracks file history from TCs.
	require.Empty(t, h.files)
}

func TestBoot_InitializesMergesMap(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	// Don't initialize - simulate loading from checkpoint with nil merges.
	h.merges = nil

	err := h.Boot()
	require.NoError(t, err)

	require.NotNil(t, h.merges, "merges map should be initialized after boot")
}

func TestHibernateBoot_RoundTrip(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Set up state.
	h.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 42}},
		Hashes: []gitlib.Hash{gitlib.NewHash("abc")},
	}
	h.merges.SeenOrAdd(gitlib.NewHash("merge1"))

	// Hibernate.
	require.NoError(t, h.Hibernate())
	// After hibernate, tracker is reset: previously-added hashes are no longer seen.
	require.False(t, h.merges.SeenOrAdd(gitlib.NewHash("merge1")), "tracker should be cleared after hibernate")

	// Boot.
	require.NoError(t, h.Boot())
	require.NotNil(t, h.merges)

	// Files cleared by Hibernate (aggregator tracks them independently).
	require.Empty(t, h.files)

	// Can add new merges after boot.
	require.False(t, h.merges.SeenOrAdd(gitlib.NewHash("merge2")), "new merge should not be seen yet")
	require.True(t, h.merges.SeenOrAdd(gitlib.NewHash("merge2")), "new merge should be seen after add")
}

func TestHibernate_ClearsFilesMap(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Simulate processing: populate h.files with 1000 file entries.
	// At kubernetes scale (50K files), this would be 50K × 310B = 15.5 MB.
	const fileCount = 1000
	for i := range fileCount {
		name := fmt.Sprintf("pkg/file_%04d.go", i)
		h.files[name] = &FileHistory{
			People: map[int]pkgplumbing.LineStats{0: {Added: 10}},
			Hashes: []gitlib.Hash{gitlib.NewHash("abc123")},
		}
	}

	require.Len(t, h.files, fileCount)

	// Hibernate must clear files to prevent cross-chunk state leak.
	// The aggregator (SpillStore[FileHistory]) independently tracks file
	// history from TCs, so h.files is redundant between chunks.
	require.NoError(t, h.Hibernate())
	require.Empty(t, h.files,
		"h.files must be cleared on Hibernate to prevent unbounded growth across chunks")
}
