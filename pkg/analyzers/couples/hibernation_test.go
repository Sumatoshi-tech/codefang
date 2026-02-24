package couples

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestHibernate_ClearsMerges(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	require.NoError(t, c.Initialize(nil))

	c.merges[gitlib.NewHash("abc123")] = true
	c.merges[gitlib.NewHash("def456")] = true
	require.Len(t, c.merges, 2)

	err := c.Hibernate()
	require.NoError(t, err)

	require.Empty(t, c.merges)
}

func TestHibernate_ClearsLastCommit(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	require.NoError(t, c.Initialize(nil))

	c.lastCommit = gitlib.NewTestCommit(
		gitlib.NewHash("c100000000000000000000000000000000000001"),
		gitlib.Signature{},
		"test",
	)

	require.NoError(t, c.Hibernate())
	require.Nil(t, c.lastCommit)
}

func TestBoot_InitializesMerges(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}

	err := c.Boot()
	require.NoError(t, err)

	require.NotNil(t, c.merges)
}

func TestHibernateBootCycle_PreservesSeenFiles(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	require.NoError(t, c.Initialize(nil))

	c.seenFiles["a.go"] = true
	c.seenFiles["b.go"] = true

	require.NoError(t, c.Hibernate())
	require.NoError(t, c.Boot())

	// seenFiles should be preserved across hibernate/boot.
	require.True(t, c.seenFiles["a.go"])
	require.True(t, c.seenFiles["b.go"])
}
