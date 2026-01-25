package plumbing

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"io/ioutil"

	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/sergi/go-diff/diffmatchpatch"
)

// FileDiffData is the type of the dependency provided by FileDiff.
type FileDiffData struct {
	OldLinesOfCode int
	NewLinesOfCode int
	Diffs          []diffmatchpatch.Diff
}

// ErrorBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrorBinary = errors.New("binary")

// CachedBlob allows to explicitly cache the binary data associated with the Blob object.
type CachedBlob struct {
	object.Blob
	// Data is the read contents of the blob object.
	Data []byte
}

// Reader returns a reader allow the access to the content of the blob
func (b *CachedBlob) Reader() (io.ReadCloser, error) {
	return ioutil.NopCloser(bytes.NewReader(b.Data)), nil
}

// Cache reads the underlying blob object and sets CachedBlob.Data.
func (b *CachedBlob) Cache() error {
	reader, err := b.Blob.Reader()
	if err != nil {
		return err
	}
	defer reader.Close()
	buf := new(bytes.Buffer)
	buf.Grow(int(b.Size))
	size, err := buf.ReadFrom(reader)
	if err != nil {
		return err
	}
	if size != b.Size {
		return fmt.Errorf("incomplete read of %s: %d while the declared size is %d",
			b.Hash.String(), size, b.Size)
	}
	b.Data = buf.Bytes()
	return nil
}

// CountLines returns the number of lines in the blob or (0, ErrorBinary) if it is binary.
func (b *CachedBlob) CountLines() (int, error) {
	if len(b.Data) == 0 {
		return 0, nil
	}
	// 8000 was taken from go-git's utils/binary.IsBinary()
	sniffLen := 8000
	sniff := b.Data
	if len(sniff) > sniffLen {
		sniff = sniff[:sniffLen]
	}
	if bytes.IndexByte(sniff, 0) >= 0 {
		return 0, ErrorBinary
	}
	lines := bytes.Count(b.Data, []byte{'\n'})
	if b.Data[len(b.Data)-1] != '\n' {
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
