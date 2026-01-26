// Package uast provides a universal abstract syntax tree (UAST) representation
// and utilities for parsing, navigating, querying, and mutating code structure
// in a language-agnostic way.
package uast

import (
	"github.com/go-git/go-git/v6/plumbing/object"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// DependencyUastChanges is the name of the dependency provided by Changes.
const DependencyUastChanges = "uast_changes"

// Change represents a structural change between two versions of code.
type Change struct {
	Before *node.Node
	After  *node.Node
	Change *object.Change
}
