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
	s.commentsByCommit["aaa"] = []string{"comment 1", "comment 2"}
	s.commentsByCommit["bbb"] = []string{"comment 3"}

	err := s.Hibernate()
	require.NoError(t, err)

	// Comments must be preserved (no temporary state to clear).
	require.Len(t, s.commentsByCommit, 2)
	require.Len(t, s.commentsByCommit["aaa"], 2)
	require.Len(t, s.commentsByCommit["bbb"], 1)
	require.Equal(t, "comment 1", s.commentsByCommit["aaa"][0])
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
	s.commentsByCommit["aaa"] = []string{"preserved comment"}
	s.commentsByCommit["bbb"] = []string{"another preserved"}

	// Hibernate.
	require.NoError(t, s.Hibernate())

	// Boot.
	require.NoError(t, s.Boot())

	// State unchanged.
	require.Len(t, s.commentsByCommit, 2)
	require.Equal(t, "preserved comment", s.commentsByCommit["aaa"][0])
	require.Equal(t, "another preserved", s.commentsByCommit["bbb"][0])
}
