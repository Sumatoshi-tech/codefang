package common

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// ExtractionConfig defines configuration for data extraction
type ExtractionConfig struct {
	NameExtractors    map[string]NameExtractor
	ValueExtractors   map[string]ValueExtractor
	DefaultExtractors bool
}

// NameExtractor extracts a name from a node
type NameExtractor func(*node.Node) (string, bool)

// ValueExtractor extracts a value from a node
type ValueExtractor func(*node.Node) (interface{}, bool)

// DataExtractor provides generic data extraction capabilities
type DataExtractor struct {
	config ExtractionConfig
}

// NewDataExtractor creates a new DataExtractor with configurable extraction settings
func NewDataExtractor(config ExtractionConfig) *DataExtractor {
	if config.DefaultExtractors {
		config.NameExtractors = mergeNameExtractors(config.NameExtractors, getDefaultNameExtractors())
		config.ValueExtractors = mergeValueExtractors(config.ValueExtractors, getDefaultValueExtractors())
	}
	return &DataExtractor{
		config: config,
	}
}

// ExtractName extracts a name from a node using the specified extractor
func (de *DataExtractor) ExtractName(n *node.Node, extractorKey string) (string, bool) {
	if extractor, exists := de.config.NameExtractors[extractorKey]; exists {
		return extractor(n)
	}
	return "", false
}

// ExtractValue extracts a value from a node using the specified extractor
func (de *DataExtractor) ExtractValue(n *node.Node, extractorKey string) (interface{}, bool) {
	if extractor, exists := de.config.ValueExtractors[extractorKey]; exists {
		return extractor(n)
	}
	return nil, false
}

// ExtractNameFromProps extracts a name from node properties
func (de *DataExtractor) ExtractNameFromProps(n *node.Node, propKey string) (string, bool) {
	if n == nil || n.Props == nil {
		return "", false
	}
	if value, exists := n.Props[propKey]; exists {
		return value, true
	}
	return "", false
}

// ExtractNameFromToken extracts a name from node token
func (de *DataExtractor) ExtractNameFromToken(n *node.Node) (string, bool) {
	if n == nil || n.Token == "" {
		return "", false
	}
	return n.Token, true
}

// ExtractNameFromChildren extracts a name from node children
func (de *DataExtractor) ExtractNameFromChildren(n *node.Node, childIndex int) (string, bool) {
	if n == nil || len(n.Children) <= childIndex {
		return "", false
	}
	child := n.Children[childIndex]
	if child == nil {
		return "", false
	}
	// Try to extract from child's token first
	if name, ok := de.ExtractNameFromToken(child); ok {
		return name, true
	}
	// Try to extract from child's properties
	if name, ok := de.ExtractNameFromProps(child, "name"); ok {
		return name, true
	}
	return "", false
}

// ExtractNodeType extracts the node type
func (de *DataExtractor) ExtractNodeType(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}
	return string(n.Type), true
}

// ExtractNodeRoles extracts the node roles
func (de *DataExtractor) ExtractNodeRoles(n *node.Node) ([]string, bool) {
	if n == nil || len(n.Roles) == 0 {
		return nil, false
	}
	roles := make([]string, len(n.Roles))
	for i, role := range n.Roles {
		roles[i] = string(role)
	}
	return roles, true
}

// ExtractNodePosition extracts the node position
func (de *DataExtractor) ExtractNodePosition(n *node.Node) (map[string]interface{}, bool) {
	if n == nil || n.Pos == nil {
		return nil, false
	}
	return map[string]interface{}{
		"start_line":   n.Pos.StartLine,
		"end_line":     n.Pos.EndLine,
		"start_col":    n.Pos.StartCol,
		"end_col":      n.Pos.EndCol,
		"start_offset": n.Pos.StartOffset,
		"end_offset":   n.Pos.EndOffset,
	}, true
}

// ExtractNodeProperties extracts all node properties
func (de *DataExtractor) ExtractNodeProperties(n *node.Node) (map[string]string, bool) {
	if n == nil || n.Props == nil {
		return nil, false
	}
	// Create a copy to avoid modifying the original
	props := make(map[string]string)
	for k, v := range n.Props {
		props[k] = v
	}
	return props, true
}

// ExtractChildCount extracts the number of children
func (de *DataExtractor) ExtractChildCount(n *node.Node) (int, bool) {
	if n == nil {
		return 0, false
	}
	return len(n.Children), true
}

// Generic extraction functions that can be used by any analyzer

// ExtractFunctionName extracts a function name from a node
func ExtractFunctionName(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}

	// Try to extract from properties first
	if name, ok := ExtractNameFromProps(n, "name"); ok {
		return name, true
	}

	// Try to extract from token
	if name, ok := ExtractNameFromToken(n); ok {
		return name, true
	}

	// Try to extract from children
	return ExtractNameFromChildren(n, 0)
}

// ExtractVariableName extracts a variable name from a node
func ExtractVariableName(n *node.Node) (string, bool) {
	if n == nil {
		return "", false
	}

	// Try to extract from properties first
	if name, ok := ExtractNameFromProps(n, "name"); ok {
		return name, true
	}

	// Try to extract from token
	if name, ok := ExtractNameFromToken(n); ok {
		return name, true
	}

	// Try to extract from children
	return ExtractNameFromChildren(n, 0)
}

// ExtractNameFromProps extracts a name from node properties
func ExtractNameFromProps(n *node.Node, propKey string) (string, bool) {
	if n == nil || n.Props == nil {
		return "", false
	}

	if value, exists := n.Props[propKey]; exists {
		return value, true
	}
	return "", false
}

// ExtractNameFromToken extracts a name from node token
func ExtractNameFromToken(n *node.Node) (string, bool) {
	if n == nil || n.Token == "" {
		return "", false
	}
	return n.Token, true
}

// ExtractNameFromChildren extracts a name from node children
func ExtractNameFromChildren(n *node.Node, childIndex int) (string, bool) {
	if n == nil || len(n.Children) <= childIndex {
		return "", false
	}

	child := n.Children[childIndex]
	if child == nil {
		return "", false
	}

	// Try to extract from child's token first
	if name, ok := ExtractNameFromToken(child); ok {
		return name, true
	}

	// Try to extract from child's properties
	if name, ok := ExtractNameFromProps(child, "name"); ok {
		return name, true
	}

	return "", false
}

// mergeNameExtractors merges custom extractors with defaults
func mergeNameExtractors(custom, defaults map[string]NameExtractor) map[string]NameExtractor {
	if custom == nil {
		return defaults
	}
	result := make(map[string]NameExtractor)
	for k, v := range defaults {
		result[k] = v
	}
	for k, v := range custom {
		result[k] = v
	}
	return result
}

// mergeValueExtractors merges custom extractors with defaults
func mergeValueExtractors(custom, defaults map[string]ValueExtractor) map[string]ValueExtractor {
	if custom == nil {
		return defaults
	}
	result := make(map[string]ValueExtractor)
	for k, v := range defaults {
		result[k] = v
	}
	for k, v := range custom {
		result[k] = v
	}
	return result
}

// getDefaultNameExtractors returns default name extractors
func getDefaultNameExtractors() map[string]NameExtractor {
	return map[string]NameExtractor{
		"token": func(n *node.Node) (string, bool) {
			if n == nil || n.Token == "" {
				return "", false
			}
			return n.Token, true
		},
		"props_name": func(n *node.Node) (string, bool) {
			if n == nil || n.Props == nil {
				return "", false
			}
			if name, exists := n.Props["name"]; exists {
				return name, true
			}
			return "", false
		},
		"props_id": func(n *node.Node) (string, bool) {
			if n == nil || n.Props == nil {
				return "", false
			}
			if id, exists := n.Props["id"]; exists {
				return id, true
			}
			return "", false
		},
	}
}

// getDefaultValueExtractors returns default value extractors
func getDefaultValueExtractors() map[string]ValueExtractor {
	return map[string]ValueExtractor{
		"type": func(n *node.Node) (interface{}, bool) {
			if n == nil {
				return nil, false
			}
			return string(n.Type), true
		},
		"roles": func(n *node.Node) (interface{}, bool) {
			if n == nil || len(n.Roles) == 0 {
				return nil, false
			}
			roles := make([]string, len(n.Roles))
			for i, role := range n.Roles {
				roles[i] = string(role)
			}
			return roles, true
		},
		"position": func(n *node.Node) (interface{}, bool) {
			if n == nil || n.Pos == nil {
				return nil, false
			}
			return map[string]interface{}{
				"start_line":   n.Pos.StartLine,
				"end_line":     n.Pos.EndLine,
				"start_col":    n.Pos.StartCol,
				"end_col":      n.Pos.EndCol,
				"start_offset": n.Pos.StartOffset,
				"end_offset":   n.Pos.EndOffset,
			}, true
		},
		"properties": func(n *node.Node) (interface{}, bool) {
			if n == nil || n.Props == nil {
				return nil, false
			}
			props := make(map[string]string)
			for k, v := range n.Props {
				props[k] = v
			}
			return props, true
		},
		"child_count": func(n *node.Node) (interface{}, bool) {
			if n == nil {
				return nil, false
			}
			return len(n.Children), true
		},
	}
}
