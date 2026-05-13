package halstead

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TestVisitor_CountsAllSameNameFunctions guards against the regression where
// per-function metrics were stored in a map keyed by function name only.
// Multiple functions in the same file sharing a name (e.g. methods named
// `Read` on different receivers in Go) were silently overwriting each other,
// and `total_functions` was reported as `len(map)` rather than the actual
// number of declared functions.
func TestVisitor_CountsAllSameNameFunctions(t *testing.T) {
	t.Parallel()

	const (
		sharedName = "Read"
		dupCount   = 5
	)

	root := &node.Node{Type: node.UASTFile}

	for range dupCount {
		fn := &node.Node{Type: node.UASTFunction}
		fn.Roles = []node.Role{node.RoleFunction, node.RoleDeclaration}

		nameNode := node.NewNodeWithToken(node.UASTIdentifier, sharedName)
		nameNode.Roles = []node.Role{node.RoleName}
		fn.AddChild(nameNode)

		root.AddChild(fn)
	}

	visitor := NewVisitor()
	traverser := analyze.NewMultiAnalyzerTraverser()
	traverser.RegisterVisitor(visitor)
	traverser.Traverse(root)

	assert.Lenf(t, visitor.functionMetrics, dupCount,
		"visitor must record one entry per function declaration, not dedup by name")

	report := visitor.GetReport()

	totalFunctions, ok := report["total_functions"].(int)
	require.True(t, ok, "total_functions must be present and int-typed")
	assert.Equalf(t, dupCount, totalFunctions,
		"reported total_functions must match declarations, not unique names")

	items, ok := analyze.ReportFunctionList(report, "functions")
	require.True(t, ok, "functions collection must be readable")
	assert.Lenf(t, items, dupCount,
		"detailed function items must include every declaration, not dedup by name")
}
