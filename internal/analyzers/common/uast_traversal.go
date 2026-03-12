package common

import (
	"slices"

	"github.com/Sumatoshi-tech/codefang/pkg/alg"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// NodeFilter defines criteria for filtering UAST nodes.
type NodeFilter struct {
	Roles    []string
	Types    []string
	MinLines int
	MaxLines int
}

// TraversalConfig defines configuration for UAST traversal.
type TraversalConfig struct {
	Filters     []NodeFilter
	MaxDepth    int
	IncludeRoot bool
}

// UASTTraverser provides generic UAST traversal capabilities.
type UASTTraverser struct {
	config TraversalConfig
}

// NewUASTTraverser creates a new UASTTraverser with configurable traversal settings.
func NewUASTTraverser(config TraversalConfig) *UASTTraverser {
	return &UASTTraverser{
		config: config,
	}
}

// FindNodes returns all nodes for which predicate returns true.
func (ut *UASTTraverser) FindNodes(root *node.Node, predicate func(*node.Node) bool) []*node.Node {
	if root == nil {
		return nil
	}

	var nodes []*node.Node

	maxDepth := ut.config.MaxDepth

	alg.TraverseTree(root, func(n *node.Node) []*node.Node {
		return n.Children
	}, func(n *node.Node, depth int) {
		if maxDepth > 0 && depth > maxDepth {
			return
		}

		if predicate(n) {
			nodes = append(nodes, n)
		}
	})

	return nodes
}

// FindNodesByType finds all nodes of specified types in the UAST.
func (ut *UASTTraverser) FindNodesByType(root *node.Node, nodeTypes []string) []*node.Node {
	return ut.FindNodes(root, func(n *node.Node) bool {
		return ut.matchesTypes(n, nodeTypes)
	})
}

// FindNodesByRoles finds all nodes with specified roles in the UAST.
func (ut *UASTTraverser) FindNodesByRoles(root *node.Node, roles []string) []*node.Node {
	return ut.FindNodes(root, func(n *node.Node) bool {
		return ut.matchesRoles(n, roles)
	})
}

// FindNodesByFilter finds all nodes matching the specified filter criteria.
func (ut *UASTTraverser) FindNodesByFilter(root *node.Node, filter NodeFilter) []*node.Node {
	return ut.FindNodes(root, func(n *node.Node) bool {
		return ut.matchesFilter(n, filter)
	})
}

// FindNodesByFilters finds all nodes matching any of the specified filter criteria.
func (ut *UASTTraverser) FindNodesByFilters(root *node.Node, filters []NodeFilter) []*node.Node {
	return ut.FindNodes(root, func(n *node.Node) bool {
		for _, filter := range filters {
			if ut.matchesFilter(n, filter) {
				return true
			}
		}

		return false
	})
}

// CountLines counts the total number of lines in a node and its children.
func (ut *UASTTraverser) CountLines(root *node.Node) int {
	if root == nil {
		return 0
	}

	lineCount := 0
	if root.Pos != nil {
		lineCount = safeconv.MustUintToInt(root.Pos.EndLine - root.Pos.StartLine + 1)
	}

	// Add lines from children.
	for _, child := range root.Children {
		lineCount += ut.CountLines(child)
	}

	return lineCount
}

// GetNodePosition returns the position information for a node.
func (ut *UASTTraverser) GetNodePosition(n *node.Node) (startLine, endLine int) {
	if n == nil || n.Pos == nil {
		return 0, 0
	}

	return safeconv.MustUintToInt(n.Pos.StartLine), safeconv.MustUintToInt(n.Pos.EndLine)
}

// matchesTypes checks if a node matches the specified types.
func (ut *UASTTraverser) matchesTypes(n *node.Node, types []string) bool {
	if len(types) == 0 {
		return true
	}

	nodeType := string(n.Type)

	return slices.Contains(types, nodeType)
}

// matchesRoles checks if a node matches the specified roles.
func (ut *UASTTraverser) matchesRoles(n *node.Node, roles []string) bool {
	if len(roles) == 0 {
		return true
	}

	for _, role := range roles {
		if n.HasAnyRole(node.Role(role)) {
			return true
		}
	}

	return false
}

// matchesFilter checks if a node matches the specified filter criteria.
func (ut *UASTTraverser) matchesFilter(target *node.Node, filter NodeFilter) bool {
	// Check roles.
	if len(filter.Roles) > 0 && !ut.matchesRoles(target, filter.Roles) {
		return false
	}

	// Check types.
	if len(filter.Types) > 0 && !ut.matchesTypes(target, filter.Types) {
		return false
	}

	// Check line count.
	if filter.MinLines > 0 || filter.MaxLines > 0 {
		lineCount := ut.CountLines(target)
		if filter.MinLines > 0 && lineCount < filter.MinLines {
			return false
		}

		if filter.MaxLines > 0 && lineCount > filter.MaxLines {
			return false
		}
	}

	return true
}
