package analyze_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// generateLargeUAST creates a balanced tree for benchmarking.
// Approx nodes = breadth^depth.
// Depth=7, breadth=4 => ~21k nodes.
func generateLargeUAST(depth, breadth int) *node.Node {
	if depth == 0 {
		return &node.Node{Type: "Leaf"}
	}

	n := &node.Node{Type: "Branch"}
	for range breadth {
		n.Children = append(n.Children, generateLargeUAST(depth-1, breadth))
	}

	return n
}

// BenchmarkSinglePass measures the performance of the MultiAnalyzerTraverser
// which visits the AST once for multiple analyzers.
func BenchmarkSinglePass(b *testing.B) {
	root := generateLargeUAST(7, 4)
	traverser := analyze.NewMultiAnalyzerTraverser()

	// Register 4 visitors (simulating 4 analyzers).
	for range 4 {
		traverser.RegisterVisitor(&MockNodeVisitor{&MockVisitorAnalyzer{}})
	}

	b.ResetTimer()

	for b.Loop() {
		traverser.Traverse(root)
	}
}

// BenchmarkMultiPass measures the performance of sequential traversal
// simulating the legacy approach where each analyzer traversed the AST independently.
func BenchmarkMultiPass(b *testing.B) {
	root := generateLargeUAST(7, 4)

	// Create a generic traverser configuration.
	config := common.TraversalConfig{
		IncludeRoot: true,
	}

	// Pre-allocate visitors to simulate 4 distinct analysis passes.
	visitors := make([]*common.UASTTraverser, 4)
	for i := range 4 {
		visitors[i] = common.NewUASTTraverser(config)
	}

	// We'll simulate searching for "Leaf" nodes, which requires full traversal.
	nodeTypes := []string{"Leaf"}

	b.ResetTimer()

	for b.Loop() {
		// Run 4 independent traversals.
		for _, v := range visitors {
			_ = v.FindNodesByType(root, nodeTypes)
		}
	}
}
