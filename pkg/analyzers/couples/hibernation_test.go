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

	// Add some merges.
	c.merges[gitlib.NewHash("abc123")] = true
	c.merges[gitlib.NewHash("def456")] = true
	require.Len(t, c.merges, 2)

	// Hibernate.
	err := c.Hibernate()
	require.NoError(t, err)

	// Merges should be cleared.
	require.Empty(t, c.merges)
}

func TestBoot_InitializesMerges(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{}
	// Don't initialize - merges is nil.

	err := c.Boot()
	require.NoError(t, err)

	// Merges should be initialized.
	require.NotNil(t, c.merges)
}

func TestHibernateBootCycle_PreservesAccumulatedState(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{PeopleNumber: 2}
	require.NoError(t, c.Initialize(nil))

	// Add accumulated state.
	c.files["a.go"] = map[string]int{"b.go": 10}
	c.people[0]["a.go"] = 5
	c.peopleCommits[0] = 100
	*c.renames = append(*c.renames, rename{FromName: "old.go", ToName: "new.go"})

	// Hibernate and boot.
	require.NoError(t, c.Hibernate())
	require.NoError(t, c.Boot())

	// Accumulated state should be preserved.
	require.Equal(t, 10, c.files["a.go"]["b.go"])
	require.Equal(t, 5, c.people[0]["a.go"])
	require.Equal(t, 100, c.peopleCommits[0])
	require.Len(t, *c.renames, 1)
}
