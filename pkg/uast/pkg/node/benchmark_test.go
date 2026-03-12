package node

import (
	"testing"
)

// buildPerfTree creates a balanced tree for traversal benchmarking.
// Different from buildBenchTree in node_test.go: uses pre-allocated Children
// slices and realistic roles/positions.
func buildPerfTree(depth, breadth int) *Node {
	if depth == 0 {
		return &Node{
			Type:  "Leaf",
			Token: "x",
			Roles: []Role{RoleLiteral},
			Pos:   &Positions{StartLine: 1, StartCol: 1, EndLine: 1, EndCol: 5},
		}
	}

	n := &Node{
		Type:     "Branch",
		Roles:    []Role{RoleFunction, RoleDeclaration},
		Pos:      &Positions{StartLine: 1, StartCol: 1, EndLine: 100, EndCol: 1},
		Children: make([]*Node, breadth),
	}

	for i := range breadth {
		n.Children[i] = buildPerfTree(depth-1, breadth)
	}

	return n
}

// BenchmarkVisitPreOrder measures direct callback pre-order traversal.
// ~21k nodes (depth=7, breadth=4).
func BenchmarkVisitPreOrder(b *testing.B) {
	root := buildPerfTree(7, 4)
	count := 0

	b.ResetTimer()

	for b.Loop() {
		count = 0

		root.VisitPreOrder(func(_ *Node) { count++ })
	}

	b.ReportMetric(float64(count), "nodes/op")
}

// BenchmarkVisitPreOrder_DeepTree measures pre-order on a deep narrow tree.
// ~1M nodes (depth=20, breadth=2).
func BenchmarkVisitPreOrder_DeepTree(b *testing.B) {
	root := buildPerfTree(20, 2)
	count := 0

	b.ResetTimer()

	for b.Loop() {
		count = 0

		root.VisitPreOrder(func(_ *Node) { count++ })
	}

	b.ReportMetric(float64(count), "nodes/op")
}

// BenchmarkVisitPostOrder measures post-order traversal.
func BenchmarkVisitPostOrder(b *testing.B) {
	root := buildPerfTree(7, 4)
	count := 0

	b.ResetTimer()

	for b.Loop() {
		count = 0

		root.VisitPostOrder(func(_ *Node) { count++ })
	}

	b.ReportMetric(float64(count), "nodes/op")
}

// BenchmarkFind measures predicate-based search (exercises pushReversedChildren).
func BenchmarkFind(b *testing.B) {
	root := buildPerfTree(7, 4)

	b.ResetTimer()

	for b.Loop() {
		_ = root.Find(func(n *Node) bool {
			return n.Type == "Leaf"
		})
	}
}

// BenchmarkAssignStableIDs measures ID assignment with hash computation.
func BenchmarkAssignStableIDs(b *testing.B) {
	root := buildPerfTree(5, 4)

	b.ResetTimer()

	for b.Loop() {
		root.AssignStableIDs()
	}
}

// BenchmarkAllocatorReleaseTree measures allocator-based tree release.
func BenchmarkAllocatorReleaseTree(b *testing.B) {
	alloc := &Allocator{}

	for b.Loop() {
		b.StopTimer()

		root := buildPerfTree(5, 4)

		b.StartTimer()

		alloc.ReleaseTree(root)
	}
}

// BenchmarkPreOrderChannel measures the channel-based PreOrder iterator.
// Compare with BenchmarkVisitPreOrder to see the difference between
// channel-based and direct callback traversal.
func BenchmarkPreOrderChannel(b *testing.B) {
	root := buildPerfTree(7, 4)
	count := 0

	b.ResetTimer()

	for b.Loop() {
		count = 0

		for range root.PreOrder() {
			count++
		}
	}

	b.ReportMetric(float64(count), "nodes/op")
}
