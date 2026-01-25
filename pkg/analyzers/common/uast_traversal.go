package common

import (
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// NodeFilter defines criteria for filtering UAST nodes
type NodeFilter struct {
	Roles    []string
	Types    []string
	MinLines int
	MaxLines int
}

// TraversalConfig defines configuration for UAST traversal
type TraversalConfig struct {
	Filters     []NodeFilter
	MaxDepth    int
	IncludeRoot bool
}

// UASTTraverser provides generic UAST traversal capabilities
type UASTTraverser struct {
	config TraversalConfig
}

// NewUASTTraverser creates a new UASTTraverser with configurable traversal settings
func NewUASTTraverser(config TraversalConfig) *UASTTraverser {
	return &UASTTraverser{
		config: config,
	}
}

// FindNodesByType finds all nodes of specified types in the UAST
func (ut *UASTTraverser) FindNodesByType(root *node.Node, nodeTypes []string) []*node.Node {
	if root == nil {
		return nil
	}

	var nodes []*node.Node
	ut.traverse(root, 0, func(n *node.Node, depth int) bool {
		if ut.matchesTypes(n, nodeTypes) {
			nodes = append(nodes, n)
		}
		return true
	})

	return nodes
}

// FindNodesByRoles finds all nodes with specified roles in the UAST
func (ut *UASTTraverser) FindNodesByRoles(root *node.Node, roles []string) []*node.Node {
	if root == nil {
		return nil
	}

	var nodes []*node.Node
	ut.traverse(root, 0, func(n *node.Node, depth int) bool {
		if ut.matchesRoles(n, roles) {
			nodes = append(nodes, n)
		}
		return true
	})

	return nodes
}

// FindNodesByFilter finds all nodes matching the specified filter criteria
func (ut *UASTTraverser) FindNodesByFilter(root *node.Node, filter NodeFilter) []*node.Node {
	if root == nil {
		return nil
	}

	var nodes []*node.Node
	ut.traverse(root, 0, func(n *node.Node, depth int) bool {
		if ut.matchesFilter(n, filter) {
			nodes = append(nodes, n)
		}
		return true
	})

	return nodes
}

// FindNodesByFilters finds all nodes matching any of the specified filter criteria
func (ut *UASTTraverser) FindNodesByFilters(root *node.Node, filters []NodeFilter) []*node.Node {
	if root == nil {
		return nil
	}

	var nodes []*node.Node
	ut.traverse(root, 0, func(n *node.Node, depth int) bool {
		for _, filter := range filters {
			if ut.matchesFilter(n, filter) {
				nodes = append(nodes, n)
				break
			}
		}
		return true
	})

	return nodes
}

// CountLines counts the total number of lines in a node and its children
func (ut *UASTTraverser) CountLines(n *node.Node) int {
	if n == nil {
		return 0
	}

	lineCount := 0
	if n.Pos != nil {
		lineCount = safeconv.MustUintToInt(n.Pos.EndLine - n.Pos.StartLine + 1)
	}

	// Add lines from children
	for _, child := range n.Children {
		lineCount += ut.CountLines(child)
	}

	return lineCount
}

// GetNodePosition returns the position information for a node
func (ut *UASTTraverser) GetNodePosition(n *node.Node) (startLine, endLine int) {
	if n == nil || n.Pos == nil {
		return 0, 0
	}
	return safeconv.MustUintToInt(n.Pos.StartLine), safeconv.MustUintToInt(n.Pos.EndLine)
}

// traverse performs depth-first traversal of the UAST
func (ut *UASTTraverser) traverse(n *node.Node, depth int, visitor func(*node.Node, int) bool) {
	if n == nil {
		return
	}

	// Check depth limit
	if ut.config.MaxDepth > 0 && depth > ut.config.MaxDepth {
		return
	}

	// Visit current node
	if !visitor(n, depth) {
		return
	}

	// Traverse children
	for _, child := range n.Children {
		ut.traverse(child, depth+1, visitor)
	}
}

// matchesTypes checks if a node matches the specified types
func (ut *UASTTraverser) matchesTypes(n *node.Node, types []string) bool {
	if len(types) == 0 {
		return true
	}

	nodeType := string(n.Type)
	for _, t := range types {
		if nodeType == t {
			return true
		}
	}
	return false
}

// matchesRoles checks if a node matches the specified roles
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

// matchesFilter checks if a node matches the specified filter criteria
func (ut *UASTTraverser) matchesFilter(n *node.Node, filter NodeFilter) bool {
	// Check roles
	if len(filter.Roles) > 0 && !ut.matchesRoles(n, filter.Roles) {
		return false
	}

	// Check types
	if len(filter.Types) > 0 && !ut.matchesTypes(n, filter.Types) {
		return false
	}

	// Check line count
	if filter.MinLines > 0 || filter.MaxLines > 0 {
		lineCount := ut.CountLines(n)
		if filter.MinLines > 0 && lineCount < filter.MinLines {
			return false
		}
		if filter.MaxLines > 0 && lineCount > filter.MaxLines {
			return false
		}
	}

	return true
}
