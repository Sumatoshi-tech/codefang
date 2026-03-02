package gitlib

import (
	"context"
	"errors"
	"fmt"
	"io"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/textutil"
)

// ErrBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrBinary = errors.New("binary")

// lineCountBinary is the sentinel value indicating the blob is binary.
const lineCountBinary = -1

// CachedBlob caches blob data for efficient repeated access.
type CachedBlob struct {
	hash Hash
	size int64
	// Data is the read contents of the blob object.
	Data []byte
	// lineCount caches the result of CountLines (-1 = binary).
	lineCount     int
	lineCountOnce sync.Once

	// KeepAlive holds a reference to the underlying storage if data is mmapped or unsafe.
	keepAlive any
}

// NewCachedBlobForTest creates a CachedBlob with the given data for testing purposes.
func NewCachedBlobForTest(data []byte) *CachedBlob {
	return &CachedBlob{
		hash: Hash{},
		size: int64(len(data)),
		Data: data,
	}
}

// NewCachedBlobWithHashForTest creates a CachedBlob with the given hash and data for testing.
func NewCachedBlobWithHashForTest(hash Hash, data []byte) *CachedBlob {
	return &CachedBlob{
		hash: hash,
		size: int64(len(data)),
		Data: data,
	}
}

// NewCachedBlobFromRepo loads and caches a blob from the repository.
func NewCachedBlobFromRepo(ctx context.Context, repo *Repository, blobHash Hash) (*CachedBlob, error) {
	blob, err := repo.LookupBlob(ctx, blobHash)
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
	return textutil.BytesReader(b.Data)
}

// Clone creates a deep copy of the CachedBlob, detaching the Data slice.
// This is useful when the original Data slice is part of a larger Arena.
func (b *CachedBlob) Clone() *CachedBlob {
	dataCopy := make([]byte, len(b.Data))
	copy(dataCopy, b.Data)

	return &CachedBlob{
		hash:      b.hash,
		size:      b.size,
		Data:      dataCopy,
		lineCount: b.lineCount,
	}
}

// CountLines returns the number of lines in the blob or (0, ErrBinary) if it is binary.
// The result is cached after the first call for efficiency.
func (b *CachedBlob) CountLines() (int, error) {
	b.lineCountOnce.Do(func() {
		b.lineCount = b.computeLineCount()
	})

	if b.lineCount == lineCountBinary {
		return 0, ErrBinary
	}

	return b.lineCount, nil
}

// computeLineCount calculates the line count or returns lineCountBinary for binary files.
func (b *CachedBlob) computeLineCount() int {
	if textutil.IsBinary(b.Data) {
		return lineCountBinary
	}

	return textutil.CountLines(b.Data)
}

// IsBinary returns true if the blob appears to be binary.
func (b *CachedBlob) IsBinary() bool {
	return textutil.IsBinary(b.Data)
}
