package uast

import (
	"context"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// ConfigUASTProvider is the configuration key for the UAST provider.
const ConfigUASTProvider = "UAST.Provider"

// ChangeType represents the type of change between two nodes.
type ChangeType int

// Change type constants.
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

// NodeChange represents a structural change between two UAST nodes.
type NodeChange struct {
	Before *node.Node
	After  *node.Node
	File   string
	Type   ChangeType
}

// DetectChanges detects structural changes between two UAST nodes.
func DetectChanges(before, after *node.Node) []NodeChange {
	var changes []NodeChange

	if before == nil && after != nil {
		changes = append(changes, NodeChange{
			Before: nil,
			After:  after,
			File:   "",
			Type:   ChangeAdded,
		})

		return changes
	}

	if before != nil && after == nil {
		changes = append(changes, NodeChange{
			Before: before,
			After:  nil,
			File:   "",
			Type:   ChangeRemoved,
		})

		return changes
	}

	if before == nil && after == nil {
		return changes
	}

	// Check if the node itself was modified (token, type, or position).
	nodeModified := before.Token != after.Token ||
		before.Type != after.Type ||
		positionsChanged(before.Pos, after.Pos)

	// Diff children.
	childChanges := diffChildren(before, after)

	// If the node's own attributes changed, or children differ, mark as modified.
	if nodeModified || len(childChanges) > 0 {
		changes = append(changes, NodeChange{
			Before: before,
			After:  after,
			File:   "",
			Type:   ChangeModified,
		})
	}

	changes = append(changes, childChanges...)

	return changes
}

// positionsChanged checks if positions differ between two nodes.
func positionsChanged(posA, posB *node.Positions) bool {
	if posA == nil && posB == nil {
		return false
	}

	if posA == nil || posB == nil {
		return true
	}

	return posA.StartLine != posB.StartLine ||
		posA.StartCol != posB.StartCol ||
		posA.EndLine != posB.EndLine ||
		posA.EndCol != posB.EndCol
}

// childKey identifies a child node by its type and token.
type childKey struct {
	Type  node.Type
	Token string
}

// diffChildren compares the children of two nodes and returns changes.
// Children are matched by (Type, Token) pairs. Unmatched children in before
// are reported as removed; unmatched children in after are reported as added.
func diffChildren(before, after *node.Node) []NodeChange {
	beforeChildren := before.Children
	afterChildren := after.Children

	if len(beforeChildren) == 0 && len(afterChildren) == 0 {
		return nil
	}

	afterUsed := make([]bool, len(afterChildren))
	afterIndex := buildChildIndex(afterChildren)
	beforeMatched := make([]bool, len(beforeChildren))

	changes := matchChildren(beforeChildren, afterChildren, afterIndex, beforeMatched, afterUsed)
	changes = append(changes, collectRemovedChildren(beforeChildren, beforeMatched)...)
	changes = append(changes, collectAddedChildren(afterChildren, afterUsed)...)

	return changes
}

func buildChildIndex(children []*node.Node) map[childKey][]int {
	index := make(map[childKey][]int)

	for idx, child := range children {
		key := childKey{child.Type, child.Token}
		index[key] = append(index[key], idx)
	}

	return index
}

func matchChildren(
	beforeChildren, afterChildren []*node.Node,
	afterIndex map[childKey][]int,
	beforeMatched, afterUsed []bool,
) []NodeChange {
	var changes []NodeChange

	for idx, bc := range beforeChildren {
		key := childKey{bc.Type, bc.Token}

		indices, ok := afterIndex[key]
		if !ok {
			continue
		}

		for _, afterIdx := range indices {
			if afterUsed[afterIdx] {
				continue
			}

			afterUsed[afterIdx] = true
			beforeMatched[idx] = true

			childChanges := DetectChanges(bc, afterChildren[afterIdx])
			changes = append(changes, childChanges...)

			break
		}
	}

	return changes
}

func collectRemovedChildren(beforeChildren []*node.Node, beforeMatched []bool) []NodeChange {
	var changes []NodeChange

	for idx, bc := range beforeChildren {
		if !beforeMatched[idx] {
			changes = append(changes, NodeChange{
				Before: bc,
				After:  nil,
				File:   "",
				Type:   ChangeRemoved,
			})
		}
	}

	return changes
}

func collectAddedChildren(afterChildren []*node.Node, afterUsed []bool) []NodeChange {
	var changes []NodeChange

	for idx, ac := range afterChildren {
		if !afterUsed[idx] {
			changes = append(changes, NodeChange{
				Before: nil,
				After:  ac,
				File:   "",
				Type:   ChangeAdded,
			})
		}
	}

	return changes
}

// LanguageParser is responsible for parsing source code into UAST nodes.
type LanguageParser interface {
	Parse(ctx context.Context, filename string, content []byte) (*node.Node, error)
	Language() string
	Extensions() []string
}

// minExtParts is the minimum number of parts after splitting by dot for a file to have an extension.
const minExtParts = 2

// getFileExtension returns the file extension (with dot).
func getFileExtension(filename string) string {
	parts := strings.Split(filename, ".")
	if len(parts) < minExtParts {
		return ""
	}

	return "." + parts[len(parts)-1]
}

// Map represents a custom UAST mapping configuration.
type Map struct {
	UAST       string   `json:"uast"`
	Extensions []string `json:"extensions"`
}
