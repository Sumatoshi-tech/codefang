package plumbing

import (
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
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
	// The consumer must call ReleaseSnapshot to free UAST trees.
	UASTChanges []uast.Change
}
