package shotness

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHibernate_ClearsNodesAndFiles(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	// Simulate processing: populate nodes with 1000 function entries.
	// At kubernetes scale (50K functions), this would be 50K × 190B = 9.5 MB.
	const nodeCount = 1000
	for i := range nodeCount {
		key := fmt.Sprintf("Function_fn%d_file%d.go", i, i%50)
		s.nodes[key] = &nodeShotness{
			Summary: NodeSummary{
				Type: "Function",
				Name: fmt.Sprintf("fn%d", i),
				File: fmt.Sprintf("file%d.go", i%50),
			},
			Count: 3,
		}
	}

	s.rebuildFilesMap()

	require.Len(t, s.nodes, nodeCount)
	require.NotEmpty(t, s.files)

	// Hibernate must clear nodes and files to prevent cross-chunk state leak.
	// Per-commit data is captured independently in TCs; the aggregator
	// accumulates counts and coupling from those TCs.
	require.NoError(t, s.Hibernate())
	require.Empty(t, s.nodes,
		"s.nodes must be cleared on Hibernate to prevent unbounded growth across chunks")
	require.Empty(t, s.files,
		"s.files must be cleared on Hibernate")
}
