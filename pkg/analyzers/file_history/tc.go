package filehistory

import (
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// PathAction represents a file path change in a single commit.
type PathAction struct {
	Path       string
	Action     gitlib.ChangeAction
	CommitHash gitlib.Hash
	FromPath   string // For renames: source path.
	ToPath     string // For renames: destination path.
}

// LineStatUpdate represents line stat delta for one file/author in a commit.
type LineStatUpdate struct {
	Path     string
	AuthorID int
	Stats    plumbing.LineStats
}

// CommitData is the per-commit TC payload emitted by Consume().
// It captures path actions (insert/modify/delete/rename) and line stat deltas.
type CommitData struct {
	PathActions     []PathAction
	LineStatUpdates []LineStatUpdate
}
