package gitlib

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"sync"
)

// ErrBinary is raised in CachedBlob.CountLines() if the file is binary.
var ErrBinary = errors.New("binary")

// binarySniffLength is the number of bytes to scan for null bytes when detecting binary content.
const binarySniffLength = 8000

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
	keepAlive interface{}
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

// Clone creates a deep copy of the CachedBlob, detaching the Data slice.
// This is useful when the original Data slice is part of a larger Arena.
func (b *CachedBlob) Clone() *CachedBlob {
	dataCopy := make([]byte, len(b.Data))
	copy(dataCopy, b.Data)

	return &CachedBlob{
		hash:      b.hash,
		size:      b.size,
		Data:      dataCopy,
		lineCount: b.lineCount, // Preserve cached line count
		// lineCountOnce is zero value (fresh), but if lineCount is set, we might want to ensure it's not recomputed.
		// But sync.Once cannot be easily copied in "done" state.
		// If lineCount is non-zero (or -1), we can set a completed Once?
		// No, better to just let it re-run once if needed, or check lineCount != 0.
		// Actually, if we set lineCount, computeLineCount won't be called if we modify CountLines to check lineCount first?
		// But CountLines uses Once.Do.
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
	if len(b.Data) == 0 {
		return 0
	}

	sniff := b.Data
	if len(sniff) > binarySniffLength {
		sniff = sniff[:binarySniffLength]
	}

	if bytes.IndexByte(sniff, 0) >= 0 {
		return lineCountBinary
	}

	lines := bytes.Count(b.Data, []byte{'\n'})

	if b.Data[len(b.Data)-1] != '\n' {
		lines++
	}

	return lines
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
