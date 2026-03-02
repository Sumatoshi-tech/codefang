package clones

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Benchmark constants.
const (
	benchFunctionCount   = 100
	benchChildCountLarge = 20
	benchShingleSize     = 5
)

// benchmarkChildTypes returns a realistic set of child types for benchmark functions.
func benchmarkChildTypes() []node.Type {
	return []node.Type{
		node.UASTBlock, node.UASTAssignment, node.UASTIdentifier,
		node.UASTCall, node.UASTIdentifier, node.UASTReturn,
		node.UASTBinaryOp, node.UASTLiteral, node.UASTVariable,
		node.UASTParameter, node.UASTIf, node.UASTBlock,
		node.UASTLoop, node.UASTAssignment, node.UASTIdentifier,
		node.UASTCall, node.UASTReturn, node.UASTBinaryOp,
		node.UASTLiteral, node.UASTVariable,
	}
}

// BenchmarkCloneDetection_100Functions benchmarks clone detection on 100 functions.
func BenchmarkCloneDetection_100Functions(b *testing.B) {
	a := NewAnalyzer()
	childTypes := benchmarkChildTypes()

	functions := make([]*node.Node, 0, benchFunctionCount)

	for i := range benchFunctionCount {
		name := string(rune('A' + i%26))
		fn := buildFunctionNode(name, childTypes)
		functions = append(functions, fn)
	}

	root := buildRootWithFunctions(functions...)

	b.ResetTimer()

	for range b.N {
		report, err := a.Analyze(root)
		_ = report
		_ = err
	}
}

// BenchmarkShingling benchmarks shingle extraction.
func BenchmarkShingling(b *testing.B) {
	s := NewShingler(benchShingleSize)
	fn := buildFunctionNode("bench", benchmarkChildTypes())

	b.ResetTimer()

	for range b.N {
		shingles := s.ExtractShingles(fn)
		_ = shingles
	}
}

// BenchmarkVisitor_100Functions benchmarks the visitor pattern on 100 functions.
func BenchmarkVisitor_100Functions(b *testing.B) {
	childTypes := benchmarkChildTypes()

	functions := make([]*node.Node, 0, benchFunctionCount)

	for i := range benchFunctionCount {
		name := string(rune('A' + i%26))
		fn := buildFunctionNode(name, childTypes)
		functions = append(functions, fn)
	}

	b.ResetTimer()

	for range b.N {
		v := NewVisitor()

		for _, fn := range functions {
			v.OnEnter(fn, 0)

			for _, child := range fn.Children {
				v.OnEnter(child, 1)
			}
		}

		report := v.GetReport()
		_ = report
	}
}
