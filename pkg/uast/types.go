package uast

import (
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const (
	ConfigUASTProvider = "UAST.Provider"
)

// ChangeType represents the type of change between two nodes
type ChangeType int

const (
	ChangeAdded ChangeType = iota
	ChangeRemoved
	ChangeModified
)

func (ct ChangeType) String() string {
	switch ct {
	case ChangeAdded:
		return "added"
	case ChangeRemoved:
		return "removed"
	case ChangeModified:
		return "modified"
	default:
		return "unknown"
	}
}

// NodeChange represents a structural change between two UAST nodes
type NodeChange struct {
	Before *node.Node
	After  *node.Node
	Type   ChangeType
	File   string
}

// DetectChanges detects structural changes between two UAST nodes
func DetectChanges(before, after *node.Node) []NodeChange {
	var changes []NodeChange

	if before == nil && after != nil {
		changes = append(changes, NodeChange{
			Before: nil,
			After:  after,
			Type:   ChangeAdded,
		})
		return changes
	}
	if before != nil && after == nil {
		changes = append(changes, NodeChange{
			Before: before,
			After:  nil,
			Type:   ChangeRemoved,
		})
		return changes
	}
	if before == nil && after == nil {
		return changes
	}

	// Check if the node itself was modified (token, type, or position)
	nodeModified := before.Token != after.Token ||
		before.Type != after.Type ||
		positionsChanged(before.Pos, after.Pos)

	// Diff children
	childChanges := diffChildren(before, after)

	// If the node's own attributes changed, or children differ, mark as modified
	if nodeModified || len(childChanges) > 0 {
		changes = append(changes, NodeChange{
			Before: before,
			After:  after,
			Type:   ChangeModified,
		})
	}

	changes = append(changes, childChanges...)

	return changes
}

// positionsChanged checks if positions differ between two nodes
func positionsChanged(a, b *node.Positions) bool {
	if a == nil && b == nil {
		return false
	}
	if a == nil || b == nil {
		return true
	}
	return a.StartLine != b.StartLine ||
		a.StartCol != b.StartCol ||
		a.EndLine != b.EndLine ||
		a.EndCol != b.EndCol
}

// diffChildren compares the children of two nodes and returns changes.
// Children are matched by (Type, Token) pairs. Unmatched children in before
// are reported as removed; unmatched children in after are reported as added.
func diffChildren(before, after *node.Node) []NodeChange {
	var changes []NodeChange

	beforeChildren := before.Children
	afterChildren := after.Children

	if len(beforeChildren) == 0 && len(afterChildren) == 0 {
		return changes
	}

	// Match children by type+token identity
	type childKey struct {
		Type  node.Type
		Token string
	}

	// Build index of after children
	afterUsed := make([]bool, len(afterChildren))
	afterIndex := make(map[childKey][]int)
	for i, c := range afterChildren {
		k := childKey{c.Type, c.Token}
		afterIndex[k] = append(afterIndex[k], i)
	}

	beforeMatched := make([]bool, len(beforeChildren))

	// Match before children to after children
	for i, bc := range beforeChildren {
		k := childKey{bc.Type, bc.Token}
		if indices, ok := afterIndex[k]; ok {
			for _, idx := range indices {
				if !afterUsed[idx] {
					afterUsed[idx] = true
					beforeMatched[i] = true
					// Recurse into matched pairs
					childChanges := DetectChanges(bc, afterChildren[idx])
					changes = append(changes, childChanges...)
					break
				}
			}
		}
	}

	// Unmatched before children are removed
	for i, bc := range beforeChildren {
		if !beforeMatched[i] {
			changes = append(changes, NodeChange{
				Before: bc,
				Type:   ChangeRemoved,
			})
		}
	}

	// Unmatched after children are added
	for i, ac := range afterChildren {
		if !afterUsed[i] {
			changes = append(changes, NodeChange{
				After: ac,
				Type:  ChangeAdded,
			})
		}
	}

	return changes
}

// LanguageParser is responsible for parsing source code into UAST nodes
type LanguageParser interface {
	Parse(filename string, content []byte) (*node.Node, error)
	Language() string
	Extensions() []string
}

// getFileExtension returns the file extension (with dot)
func getFileExtension(filename string) string {
	parts := strings.Split(filename, ".")
	if len(parts) < 2 {
		return ""
	}
	return "." + parts[len(parts)-1]
}

// UASTMap represents a custom UAST mapping configuration
type UASTMap struct {
	Extensions []string `json:"extensions"`
	UAST       string   `json:"uast"`
}
