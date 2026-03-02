package gitlib

import (
	"io"

	git2go "github.com/libgit2/git2go/v34"

	"github.com/Sumatoshi-tech/codefang/pkg/textutil"
)

// Blob wraps a libgit2 blob.
type Blob struct {
	blob *git2go.Blob
}

// Hash returns the blob hash.
func (b *Blob) Hash() Hash {
	return HashFromOid(b.blob.Id())
}

// Size returns the blob size.
func (b *Blob) Size() int64 {
	return b.blob.Size()
}

// Contents returns the blob contents.
func (b *Blob) Contents() []byte {
	return b.blob.Contents()
}

// Reader returns a reader for the blob contents.
func (b *Blob) Reader() io.Reader {
	return textutil.BytesReader(b.blob.Contents())
}

// Free releases the blob resources.
func (b *Blob) Free() {
	if b.blob != nil {
		b.blob.Free()
		b.blob = nil
	}
}

// Native returns the underlying libgit2 blob.
func (b *Blob) Native() *git2go.Blob {
	return b.blob
}
