package uast

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/go-git/go-git/v6/plumbing/object"
)

// DependencyUastChanges is the name of the dependency provided by Changes.
const DependencyUastChanges = "uast_changes"

// Change represents a structural change between two versions of code
type Change struct {
	Before *node.Node
	After  *node.Node
	Change *object.Change
}
