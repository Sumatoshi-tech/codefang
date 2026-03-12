// Package clones provides clone detection analysis using MinHash and LSH.
package clones

import (
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Shingling constants.
const (
	// defaultShingleSize is the default k-gram window size for shingling.
	defaultShingleSize = 5

	// shingleSeparator separates node types in a shingle string.
	shingleSeparator = "|"
)

// Shingler extracts k-gram shingles from UAST function subtrees.
// A shingle is a sequence of k consecutive node types from a pre-order traversal.
type Shingler struct {
	k int
}

// NewShingler creates a new Shingler with the given k-gram size.
func NewShingler(k int) *Shingler {
	return &Shingler{k: k}
}

// ExtractShingles returns k-gram shingles from a function's UAST subtree.
// Each shingle is a byte slice representing k consecutive node types joined by "|".
// Returns nil if the subtree has fewer than k nodes.
func (s *Shingler) ExtractShingles(funcNode *node.Node) [][]byte {
	if funcNode == nil {
		return nil
	}

	types := collectNodeTypes(funcNode)
	if len(types) < s.k {
		return nil
	}

	shingleCount := len(types) - s.k + 1
	shingles := make([][]byte, 0, shingleCount)

	for i := range shingleCount {
		shingle := buildShingle(types[i : i+s.k])
		shingles = append(shingles, shingle)
	}

	return shingles
}

// collectNodeTypes performs a pre-order traversal and collects node types.
func collectNodeTypes(root *node.Node) []string {
	if root == nil {
		return nil
	}

	var types []string

	root.VisitPreOrder(func(n *node.Node) {
		if n.Type != "" {
			types = append(types, string(n.Type))
		}
	})

	return types
}

// buildShingle concatenates k node types into a single byte slice.
func buildShingle(types []string) []byte {
	return []byte(joinTypes(types))
}

// joinTypes joins node type strings with the shingle separator.
func joinTypes(types []string) string {
	if len(types) == 0 {
		return ""
	}

	var b strings.Builder

	b.WriteString(types[0])

	for _, t := range types[1:] {
		b.WriteString(shingleSeparator)
		b.WriteString(t)
	}

	return b.String()
}
