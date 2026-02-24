package filehistory

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestHibernate_ClearsMergesMap(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Add some merge entries.
	h.merges[gitlib.NewHash("abc123")] = true
	h.merges[gitlib.NewHash("def456")] = true
	require.Len(t, h.merges, 2)

	err := h.Hibernate()
	require.NoError(t, err)

	require.Empty(t, h.merges, "merges map should be cleared after hibernate")
}

func TestHibernate_Succeeds(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))

	// Hibernate clears merges; lastCommitHash is preserved for Finalize.
	err := h.Hibernate()
	require.NoError(t, err)
}

func TestHibernate_PreservesFiles(t *testing.T) {
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

	// Files must be preserved.
	require.Len(t, h.files, 2)
	require.NotNil(t, h.files["main.go"])
	require.Equal(t, 100, h.files["main.go"].People[0].Added)
	require.NotNil(t, h.files["util.go"])
	require.Equal(t, 30, h.files["util.go"].People[1].Added)
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
	h.merges[gitlib.NewHash("merge1")] = true

	// Hibernate.
	require.NoError(t, h.Hibernate())
	require.Empty(t, h.merges)

	// Boot.
	require.NoError(t, h.Boot())
	require.NotNil(t, h.merges)

	// Files still preserved.
	require.Len(t, h.files, 1)
	require.Equal(t, 42, h.files["test.go"].People[0].Added)

	// Can add new merges after boot.
	h.merges[gitlib.NewHash("merge2")] = true
	require.Len(t, h.merges, 1)
}
