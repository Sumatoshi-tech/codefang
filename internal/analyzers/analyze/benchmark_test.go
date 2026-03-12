package analyze_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// generateLargeUAST creates a balanced tree for benchmarking.
// Approx nodes = breadth^depth.
// Depth=7, breadth=4 => ~21k nodes.
func generateLargeUAST(depth, breadth int) *node.Node {
	if depth == 0 {
		return &node.Node{
			Type:  "Leaf",
			Roles: []node.Role{node.RoleLiteral},
		}
	}

	n := &node.Node{
		Type:  "Branch",
		Roles: []node.Role{node.RoleFunction, node.RoleDeclaration},
	}
	n.Children = make([]*node.Node, breadth)

	for i := range breadth {
		n.Children[i] = generateLargeUAST(depth-1, breadth)
	}

	return n
}

// Benchmark tree parameters.
const (
	benchTreeDepth     = 7
	benchTreeBreadth   = 4
	benchVisitorCount  = 4
	benchVisitorCount8 = 8
	benchDeepDepth     = 20
	benchDeepBreadth   = 2
)

// BenchmarkSinglePass measures the performance of the MultiAnalyzerTraverser
// which visits the AST once for multiple analyzers.
func BenchmarkSinglePass(b *testing.B) {
	root := generateLargeUAST(benchTreeDepth, benchTreeBreadth)
	traverser := analyze.NewMultiAnalyzerTraverser()

	// Register visitors (simulating multiple analyzers).
	for range benchVisitorCount {
		traverser.RegisterVisitor(&MockNodeVisitor{&MockVisitorAnalyzer{}})
	}

	b.ResetTimer()

	for b.Loop() {
		traverser.Traverse(root)
	}
}

// BenchmarkSinglePass_8Visitors measures scaling with 8 visitors.
func BenchmarkSinglePass_8Visitors(b *testing.B) {
	root := generateLargeUAST(benchTreeDepth, benchTreeBreadth)
	traverser := analyze.NewMultiAnalyzerTraverser()

	for range benchVisitorCount8 {
		traverser.RegisterVisitor(&MockNodeVisitor{&MockVisitorAnalyzer{}})
	}

	b.ResetTimer()

	for b.Loop() {
		traverser.Traverse(root)
	}
}

// BenchmarkSinglePass_DeepTree measures performance on a deep, narrow tree
// (depth=20, breadth=2 => ~1M nodes), the scenario where the old
// recursive approach risked stack overflow.
func BenchmarkSinglePass_DeepTree(b *testing.B) {
	root := generateLargeUAST(benchDeepDepth, benchDeepBreadth)
	traverser := analyze.NewMultiAnalyzerTraverser()

	for range benchVisitorCount {
		traverser.RegisterVisitor(&MockNodeVisitor{&MockVisitorAnalyzer{}})
	}

	b.ResetTimer()

	for b.Loop() {
		traverser.Traverse(root)
	}
}

// BenchmarkMultiPass measures the performance of sequential traversal,
// simulating the independent approach where each analyzer traversed the AST separately.
func BenchmarkMultiPass(b *testing.B) {
	root := generateLargeUAST(benchTreeDepth, benchTreeBreadth)

	// Create a generic traverser configuration.
	config := common.TraversalConfig{
		IncludeRoot: true,
	}

	// Pre-allocate visitors to simulate distinct analysis passes.
	visitors := make([]*common.UASTTraverser, benchVisitorCount)
	for i := range benchVisitorCount {
		visitors[i] = common.NewUASTTraverser(config)
	}

	// Simulate searching for "Leaf" nodes, which requires full traversal.
	nodeTypes := []string{"Leaf"}

	b.ResetTimer()

	for b.Loop() {
		// Run independent traversals.
		for _, v := range visitors {
			_ = v.FindNodesByType(root, nodeTypes)
		}
	}
}
