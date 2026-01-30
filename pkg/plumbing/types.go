package plumbing

import (
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// FileDiffData is the type of the dependency provided by FileDiff.
type FileDiffData struct {
	Diffs          []diffmatchpatch.Diff
	OldLinesOfCode int
	NewLinesOfCode int
}

// CachedBlob is an alias for gitlib.CachedBlob for backward compatibility.
type CachedBlob = gitlib.CachedBlob

// ErrBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrBinary = gitlib.ErrBinary

// LineStats holds the numbers of inserted, deleted and changed lines.
type LineStats struct {
	// Added is the number of added lines by a particular developer in a particular day.
	Added int
	// Removed is the number of removed lines by a particular developer in a particular day.
	Removed int
	// Changed is the number of changed lines by a particular developer in a particular day.
	Changed int
}
