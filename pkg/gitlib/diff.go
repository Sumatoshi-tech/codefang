package gitlib

import (
	"fmt"

	git2go "github.com/libgit2/git2go/v34"
)

// Diff wraps a libgit2 diff.
type Diff struct {
	diff *git2go.Diff
}

// NumDeltas returns the number of deltas in the diff.
func (d *Diff) NumDeltas() (int, error) {
	numDeltas, err := d.diff.NumDeltas()
	if err != nil {
		return 0, fmt.Errorf("get num deltas: %w", err)
	}

	return numDeltas, nil
}

// Delta returns the delta at the given index.
func (d *Diff) Delta(index int) (DiffDelta, error) {
	delta, err := d.diff.Delta(index)
	if err != nil {
		return DiffDelta{}, fmt.Errorf("get delta: %w", err)
	}

	return DiffDelta{
		Status:   delta.Status,
		OldFile:  DiffFile{Path: delta.OldFile.Path, Hash: HashFromOid(delta.OldFile.Oid), Size: int64(delta.OldFile.Size)},
		NewFile:  DiffFile{Path: delta.NewFile.Path, Hash: HashFromOid(delta.NewFile.Oid), Size: int64(delta.NewFile.Size)},
		Flags:    delta.Flags,
		NumHunks: 0, // Will be set by ForEach.
	}, nil
}

// ForEach iterates over the diff with callbacks for files, hunks, and lines.
func (d *Diff) ForEach(
	fileCallback func(delta DiffDelta, progress float64) (git2go.DiffForEachHunkCallback, error),
	detail git2go.DiffDetail,
) error {
	err := d.diff.ForEach(func(delta git2go.DiffDelta, progress float64) (git2go.DiffForEachHunkCallback, error) {
		wrappedDelta := DiffDelta{
			Status:  delta.Status,
			OldFile: DiffFile{Path: delta.OldFile.Path, Hash: HashFromOid(delta.OldFile.Oid), Size: int64(delta.OldFile.Size)},
			NewFile: DiffFile{Path: delta.NewFile.Path, Hash: HashFromOid(delta.NewFile.Oid), Size: int64(delta.NewFile.Size)},
			Flags:   delta.Flags,
		}

		return fileCallback(wrappedDelta, progress)
	}, detail)
	if err != nil {
		return fmt.Errorf("diff foreach: %w", err)
	}

	return nil
}

// Stats returns the diff stats.
func (d *Diff) Stats() (*DiffStats, error) {
	stats, err := d.diff.Stats()
	if err != nil {
		return nil, fmt.Errorf("get diff stats: %w", err)
	}

	return &DiffStats{stats: stats}, nil
}

// Free releases the diff resources.
func (d *Diff) Free() {
	if d.diff == nil {
		return
	}

	err := d.diff.Free()
	d.diff = nil
	// Consume error - Free() errors are non-actionable in cleanup.
	if err != nil {
		return
	}
}

// DiffDelta represents a file change in a diff.
type DiffDelta struct {
	Status   git2go.Delta
	OldFile  DiffFile
	NewFile  DiffFile
	Flags    git2go.DiffFlag
	NumHunks int
}

// DiffFile represents a file in a diff delta.
type DiffFile struct {
	Path string
	Hash Hash
	Size int64
}

// DiffStats wraps libgit2 diff stats.
type DiffStats struct {
	stats *git2go.DiffStats
}

// Insertions returns the number of insertions.
func (s *DiffStats) Insertions() int {
	return s.stats.Insertions()
}

// Deletions returns the number of deletions.
func (s *DiffStats) Deletions() int {
	return s.stats.Deletions()
}

// FilesChanged returns the number of files changed.
func (s *DiffStats) FilesChanged() int {
	return s.stats.FilesChanged()
}

// Free releases the stats resources.
func (s *DiffStats) Free() {
	if s.stats == nil {
		return
	}

	err := s.stats.Free()
	s.stats = nil
	// Consume error - Free() errors are non-actionable in cleanup.
	if err != nil {
		return
	}
}
