package sentiment

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHibernate_PreservesComments(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Add comment data.
	s.commentsByTick[0] = []string{"comment 1", "comment 2"}
	s.commentsByTick[1] = []string{"comment 3"}

	err := s.Hibernate()
	require.NoError(t, err)

	// Comments must be preserved (no temporary state to clear).
	require.Len(t, s.commentsByTick, 2)
	require.Len(t, s.commentsByTick[0], 2)
	require.Len(t, s.commentsByTick[1], 1)
	require.Equal(t, "comment 1", s.commentsByTick[0][0])
}

func TestBoot_NoError(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	// Don't initialize - simulate minimal state.

	err := s.Boot()
	require.NoError(t, err)
}

func TestHibernateBoot_RoundTrip(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Set up state.
	s.commentsByTick[0] = []string{"preserved comment"}
	s.commentsByTick[5] = []string{"another preserved"}

	// Hibernate.
	require.NoError(t, s.Hibernate())

	// Boot.
	require.NoError(t, s.Boot())

	// State unchanged.
	require.Len(t, s.commentsByTick, 2)
	require.Equal(t, "preserved comment", s.commentsByTick[0][0])
	require.Equal(t, "another preserved", s.commentsByTick[5][0])
}
