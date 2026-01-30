package gitlib

import (
	"bytes"
	"errors"
	"fmt"
	"io"
)

// ErrBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrBinary = errors.New("binary")

// binarySniffLength is the number of bytes to scan for null bytes when detecting binary content.
const binarySniffLength = 8000

// CachedBlob caches blob data for efficient repeated access.
type CachedBlob struct {
	hash Hash
	size int64
	// Data is the read contents of the blob object.
	Data []byte
}

// NewCachedBlobFromRepo loads and caches a blob from the repository.
func NewCachedBlobFromRepo(repo *Repository, blobHash Hash) (*CachedBlob, error) {
	blob, err := repo.LookupBlob(blobHash)
	if err != nil {
		return nil, fmt.Errorf("looking up blob %s: %w", blobHash.String(), err)
	}
	defer blob.Free()

	return &CachedBlob{
		hash: blobHash,
		size: blob.Size(),
		Data: blob.Contents(),
	}, nil
}

// Hash returns the blob hash.
func (b *CachedBlob) Hash() Hash {
	return b.hash
}

// Size returns the blob size.
func (b *CachedBlob) Size() int64 {
	return b.size
}

// Reader returns a reader for the blob data.
func (b *CachedBlob) Reader() io.ReadCloser {
	return io.NopCloser(bytes.NewReader(b.Data))
}

// CountLines returns the number of lines in the blob or (0, ErrBinary) if it is binary.
func (b *CachedBlob) CountLines() (int, error) {
	if len(b.Data) == 0 {
		return 0, nil
	}

	sniff := b.Data
	if len(sniff) > binarySniffLength {
		sniff = sniff[:binarySniffLength]
	}

	if bytes.IndexByte(sniff, 0) >= 0 {
		return 0, ErrBinary
	}

	lines := bytes.Count(b.Data, []byte{'\n'})

	if b.Data[len(b.Data)-1] != '\n' {
		lines++
	}

	return lines, nil
}

// IsBinary returns true if the blob appears to be binary.
func (b *CachedBlob) IsBinary() bool {
	if len(b.Data) == 0 {
		return false
	}

	sniff := b.Data
	if len(sniff) > binarySniffLength {
		sniff = sniff[:binarySniffLength]
	}

	return bytes.IndexByte(sniff, 0) >= 0
}
