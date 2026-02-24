package plumbing

import (
	"maps"
	"slices"

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

// Clone creates a shallow clone of the snapshot's reference types (maps and slices),
// enforcing immutability of the core analyzers' state while avoiding deep-copying
// immutable git objects or UAST nodes.
func (s Snapshot) Clone() Snapshot {
	clone := Snapshot{
		Tick:     s.Tick,
		AuthorID: s.AuthorID,
	}

	if s.Changes != nil {
		clone.Changes = slices.Clone(s.Changes)
	}

	if s.BlobCache != nil {
		clone.BlobCache = maps.Clone(s.BlobCache)
	}

	if s.FileDiffs != nil {
		clone.FileDiffs = maps.Clone(s.FileDiffs)
	}

	if s.LineStats != nil {
		clone.LineStats = maps.Clone(s.LineStats)
	}

	if s.Languages != nil {
		clone.Languages = maps.Clone(s.Languages)
	}

	if s.UASTChanges != nil {
		clone.UASTChanges = slices.Clone(s.UASTChanges)
	}

	return clone
}

// ReleaseSnapshotUAST releases UAST trees owned by the snapshot.
func ReleaseSnapshotUAST(s Snapshot) {
	for _, ch := range s.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}
