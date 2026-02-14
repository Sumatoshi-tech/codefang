package gitlib

import (
	"fmt"
	"strings"

	git2go "github.com/libgit2/git2go/v34"
)

// initialDiffCapacity is the initial capacity for diff result slices.
const initialDiffCapacity = 16

// LineDiff represents a single diff operation at the line level.
type LineDiff struct {
	Type      LineDiffType
	LineCount int
}

// LineDiffType represents the type of line diff.
type LineDiffType int

const (
	// LineDiffEqual means lines are unchanged.
	LineDiffEqual LineDiffType = iota
	// LineDiffInsert means lines were added.
	LineDiffInsert
	// LineDiffDelete means lines were deleted.
	LineDiffDelete
)

// BlobDiffResult holds the result of diffing two blobs.
type BlobDiffResult struct {
	OldLines int
	NewLines int
	Diffs    []LineDiff
}

// diffState tracks state during blob diff processing.
type diffState struct {
	result       *BlobDiffResult
	currentType  LineDiffType
	currentCount int
	oldLinePos   int
}

// flush writes the current accumulated diff to the result.
func (s *diffState) flush() {
	if s.currentCount > 0 {
		s.result.Diffs = append(s.result.Diffs, LineDiff{
			Type:      s.currentType,
			LineCount: s.currentCount,
		})
		s.currentCount = 0
	}
}

// processLine handles a single diff line.
func (s *diffState) processLine(line git2go.DiffLine) {
	var lineType LineDiffType

	switch line.Origin {
	case git2go.DiffLineContext:
		lineType = LineDiffEqual
		s.oldLinePos++
	case git2go.DiffLineAddition:
		lineType = LineDiffInsert
	case git2go.DiffLineDeletion:
		lineType = LineDiffDelete
		s.oldLinePos++
	case git2go.DiffLineContextEOFNL,
		git2go.DiffLineAddEOFNL,
		git2go.DiffLineDelEOFNL,
		git2go.DiffLineFileHdr,
		git2go.DiffLineHunkHdr,
		git2go.DiffLineBinary:
		return
	}

	if lineType == s.currentType {
		s.currentCount++
	} else {
		s.flush()
		s.currentType = lineType
		s.currentCount = 1
	}
}

// processHunk handles a diff hunk, inserting implicit equal blocks.
func (s *diffState) processHunk(hunk git2go.DiffHunk) {
	if hunk.OldStart > 0 && hunk.OldStart-1 > s.oldLinePos {
		skippedLines := hunk.OldStart - 1 - s.oldLinePos
		if skippedLines > 0 {
			s.flush()
			s.result.Diffs = append(s.result.Diffs, LineDiff{
				Type:      LineDiffEqual,
				LineCount: skippedLines,
			})
			s.oldLinePos = hunk.OldStart - 1
		}
	}
}

// finalize adds any trailing equal block for unchanged lines.
func (s *diffState) finalize() {
	s.flush()

	if s.result.OldLines > s.oldLinePos {
		s.result.Diffs = append(s.result.Diffs, LineDiff{
			Type:      LineDiffEqual,
			LineCount: s.result.OldLines - s.oldLinePos,
		})
	}
}

// DiffBlobs computes a line-level diff between two blobs using libgit2's native diff.
// This is much faster than loading blob contents and diffing in Go.
func DiffBlobs(oldBlob, newBlob *Blob, oldPath, newPath string) (*BlobDiffResult, error) {
	state := &diffState{
		result:      &BlobDiffResult{Diffs: make([]LineDiff, 0, initialDiffCapacity)},
		currentType: -1,
	}

	if oldBlob != nil {
		state.result.OldLines = countLines(oldBlob.Contents())
	}

	if newBlob != nil {
		state.result.NewLines = countLines(newBlob.Contents())
	}

	var oldNative, newNative *git2go.Blob

	if oldBlob != nil {
		oldNative = oldBlob.Native()
	}

	if newBlob != nil {
		newNative = newBlob.Native()
	}

	fileCallback := func(_ git2go.DiffDelta, _ float64) (git2go.DiffForEachHunkCallback, error) {
		return func(hunk git2go.DiffHunk) (git2go.DiffForEachLineCallback, error) {
			state.processHunk(hunk)

			return func(line git2go.DiffLine) error {
				state.processLine(line)

				return nil
			}, nil
		}, nil
	}

	err := git2go.DiffBlobs(oldNative, oldPath, newNative, newPath, nil, fileCallback, git2go.DiffDetailLines)
	if err != nil {
		return nil, fmt.Errorf("diff blobs: %w", err)
	}

	state.finalize()

	return state.result, nil
}

// DiffBlobsFromCache computes a line-level diff using cached blob data.
// Falls back to simple line counting if blobs aren't available.
func DiffBlobsFromCache(oldData, newData []byte) *BlobDiffResult {
	// Fast path: count lines in each.
	oldLines := countLines(oldData)
	newLines := countLines(newData)

	// If we need the actual diff, we'd need to use a Go-based diff here.
	// For now, return a simple result indicating full replacement.
	// This is a fallback - the main path should use DiffBlobs with real blobs.
	return &BlobDiffResult{
		OldLines: oldLines,
		NewLines: newLines,
		Diffs: []LineDiff{
			{Type: LineDiffDelete, LineCount: oldLines},
			{Type: LineDiffInsert, LineCount: newLines},
		},
	}
}

func countLines(data []byte) int {
	if len(data) == 0 {
		return 0
	}

	count := strings.Count(string(data), "\n")

	// If file doesn't end with newline, add 1.
	if len(data) > 0 && data[len(data)-1] != '\n' {
		count++
	}

	return count
}

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
