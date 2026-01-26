package plumbing

import (
	"bytes"
	"errors"
	"fmt"
	"io"

	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/sergi/go-diff/diffmatchpatch"
)

// FileDiffData is the type of the dependency provided by FileDiff.
type FileDiffData struct {
	Diffs          []diffmatchpatch.Diff
	OldLinesOfCode int
	NewLinesOfCode int
}

// ErrBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrBinary = errors.New("binary")

// ErrIncompleteRead is raised when the blob data size does not match the declared size.
var ErrIncompleteRead = errors.New("incomplete read")

// CachedBlob allows to explicitly cache the binary data associated with the Blob object.
type CachedBlob struct {
	object.Blob

	// Data is the read contents of the blob object.
	Data []byte
}

// Reader returns a reader allowing access to the content of the blob.
func (blob *CachedBlob) Reader() (io.ReadCloser, error) {
	return io.NopCloser(bytes.NewReader(blob.Data)), nil
}

// Cache reads the underlying blob object and sets CachedBlob.Data.
func (blob *CachedBlob) Cache() error {
	reader, err := blob.Blob.Reader()
	if err != nil {
		return fmt.Errorf("opening blob reader: %w", err)
	}

	defer reader.Close()

	buf := new(bytes.Buffer)
	buf.Grow(int(blob.Size))

	size, err := buf.ReadFrom(reader)
	if err != nil {
		return fmt.Errorf("reading blob data: %w", err)
	}

	if size != blob.Size {
		return fmt.Errorf("%w of %s: %d while the declared size is %d",
			ErrIncompleteRead, blob.Hash.String(), size, blob.Size)
	}

	blob.Data = buf.Bytes()

	return nil
}

// binarySniffLength is the number of bytes to scan for null bytes when detecting binary content.
// 8000 was taken from go-git's utils/binary.IsBinary().
const binarySniffLength = 8000

// CountLines returns the number of lines in the blob or (0, ErrBinary) if it is binary.
func (blob *CachedBlob) CountLines() (int, error) {
	if len(blob.Data) == 0 {
		return 0, nil
	}

	sniff := blob.Data
	if len(sniff) > binarySniffLength {
		sniff = sniff[:binarySniffLength]
	}

	if bytes.IndexByte(sniff, 0) >= 0 {
		return 0, ErrBinary
	}

	lines := bytes.Count(blob.Data, []byte{'\n'})

	if blob.Data[len(blob.Data)-1] != '\n' {
		lines++
	}

	return lines, nil
}

// LineStats holds the numbers of inserted, deleted and changed lines.
type LineStats struct {
	// Added is the number of added lines by a particular developer in a particular day.
	Added int
	// Removed is the number of removed lines by a particular developer in a particular day.
	Removed int
	// Changed is the number of changed lines by a particular developer in a particular day.
	Changed int
}
