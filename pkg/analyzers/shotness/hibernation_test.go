package shotness

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestHibernate_ClearsMergesMap(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Add some merge entries
	s.merges[gitlib.NewHash("abc123")] = true
	s.merges[gitlib.NewHash("def456")] = true
	require.Len(t, s.merges, 2)

	err := s.Hibernate()
	require.NoError(t, err)

	require.Empty(t, s.merges, "merges map should be cleared after hibernate")
}

func TestHibernate_PreservesNodes(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Add node data
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   10,
		Couples: map[string]int{"func2": 5},
	}
	s.nodes["func2"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func2", File: "test.go"},
		Count:   8,
		Couples: map[string]int{"func1": 5},
	}

	err := s.Hibernate()
	require.NoError(t, err)

	// Nodes must be preserved
	require.Len(t, s.nodes, 2)
	require.NotNil(t, s.nodes["func1"])
	require.Equal(t, 10, s.nodes["func1"].Count)
	require.NotNil(t, s.nodes["func2"])
	require.Equal(t, 8, s.nodes["func2"].Count)
}

func TestHibernate_PreservesFiles(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Add node and file data
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   10,
		Couples: map[string]int{},
	}
	s.files["test.go"] = map[string]*nodeShotness{
		"func1": s.nodes["func1"],
	}

	err := s.Hibernate()
	require.NoError(t, err)

	// Files map must be preserved
	require.Len(t, s.files, 1)
	require.NotNil(t, s.files["test.go"])
}

func TestBoot_InitializesMergesMap(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	// Don't initialize - simulate loading from checkpoint with nil merges
	s.merges = nil

	err := s.Boot()
	require.NoError(t, err)

	require.NotNil(t, s.merges, "merges map should be initialized after boot")
}

func TestHibernateBoot_RoundTrip(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Set up state
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   42,
		Couples: map[string]int{"func2": 10},
	}
	s.merges[gitlib.NewHash("merge1")] = true

	// Hibernate
	require.NoError(t, s.Hibernate())
	require.Empty(t, s.merges)

	// Boot
	require.NoError(t, s.Boot())
	require.NotNil(t, s.merges)

	// Nodes still preserved
	require.Len(t, s.nodes, 1)
	require.Equal(t, 42, s.nodes["func1"].Count)
	require.Equal(t, 10, s.nodes["func1"].Couples["func2"])

	// Can add new merges after boot
	s.merges[gitlib.NewHash("merge2")] = true
	require.Len(t, s.merges, 1)
}
