package plumbing

import (
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Snapshot captures the output state of all plumbing analyzers for one commit.
// Leaf analyzers use this to implement analyze.Parallelizable, keeping the
// framework agnostic of concrete plumbing types.
type Snapshot struct {
	Changes   gitlib.Changes
	BlobCache map[gitlib.Hash]*gitlib.CachedBlob
	FileDiffs map[string]pkgplumbing.FileDiffData
	LineStats map[gitlib.ChangeEntry]pkgplumbing.LineStats
	Languages map[gitlib.Hash]string
	Tick      int
	AuthorID  int
	// UASTChanges ownership is transferred to the snapshot.
	// The consumer must call ReleaseSnapshotUAST to free UAST trees.
	UASTChanges []uast.Change
}

// ReleaseSnapshotUAST releases UAST trees owned by the snapshot.
func ReleaseSnapshotUAST(s Snapshot) {
	for _, ch := range s.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}
