package imports

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHibernate_ReleasesParser(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Parser should be initialized.
	require.NotNil(t, h.parser)

	err := h.Hibernate()
	require.NoError(t, err)

	// Parser should be released.
	require.Nil(t, h.parser, "parser should be nil after hibernate")
}

func TestHibernate_PreservesImports(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Add import data.
	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 5}},
	}
	h.imports[1] = map[string]map[string]map[int]int64{
		"python": {"os": {0: 10}},
	}

	err := h.Hibernate()
	require.NoError(t, err)

	// Imports must be preserved.
	require.Len(t, h.imports, 2)
	require.Equal(t, int64(5), h.imports[0]["go"]["fmt"][0])
	require.Equal(t, int64(10), h.imports[1]["python"]["os"][0])
}

func TestBoot_RecreatesParser(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Hibernate to release parser.
	require.NoError(t, h.Hibernate())
	require.Nil(t, h.parser)

	// Boot should recreate parser.
	err := h.Boot()
	require.NoError(t, err)
	require.NotNil(t, h.parser, "parser should be recreated after boot")
}

func TestHibernateBoot_RoundTrip(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	// Set up state.
	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 42}},
	}

	// Hibernate.
	require.NoError(t, h.Hibernate())
	require.Nil(t, h.parser)

	// Boot.
	require.NoError(t, h.Boot())
	require.NotNil(t, h.parser)

	// Imports still preserved.
	require.Len(t, h.imports, 1)
	require.Equal(t, int64(42), h.imports[0]["go"]["fmt"][0])
}
