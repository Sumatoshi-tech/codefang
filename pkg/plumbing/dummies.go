package plumbing

import (
	"errors"
	"fmt"
	"io"

	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
)

// ErrDummyFailure is the sentinel error returned by dummy encoded objects.
var ErrDummyFailure = errors.New("dummy failure")

type dummyIO struct{}

func (dummyIO) Read(_ []byte) (int, error) {
	return 0, io.EOF
}

func (dummyIO) Write(payload []byte) (int, error) {
	return len(payload), nil
}

// Close is a no-op closer for dummyIO.
func (dummyIO) Close() error {
	return nil
}

type dummyEncodedObject struct {
	FakeHash plumbing.Hash
	Fails    bool
}

// Hash returns the fake hash of the dummy object.
func (obj dummyEncodedObject) Hash() plumbing.Hash {
	return obj.FakeHash
}

// Type returns BlobObject for the dummy object.
func (obj dummyEncodedObject) Type() plumbing.ObjectType {
	return plumbing.BlobObject
}

// SetType is a no-op for the dummy object.
func (obj dummyEncodedObject) SetType(plumbing.ObjectType) {
}

// Size returns 0 for the dummy object.
func (obj dummyEncodedObject) Size() int64 {
	return 0
}

// SetSize is a no-op for the dummy object.
func (obj dummyEncodedObject) SetSize(int64) {
}

// Reader returns a dummyIO reader or ErrDummyFailure if Fails is set.
func (obj dummyEncodedObject) Reader() (io.ReadCloser, error) {
	if !obj.Fails {
		return dummyIO{}, nil
	}

	return nil, ErrDummyFailure
}

// Writer returns a dummyIO writer or ErrDummyFailure if Fails is set.
func (obj dummyEncodedObject) Writer() (io.WriteCloser, error) {
	if !obj.Fails {
		return dummyIO{}, nil
	}

	return nil, ErrDummyFailure
}

// CreateDummyBlob constructs a fake object.Blob with empty contents.
// Optionally returns an error if read or written.
func CreateDummyBlob(hash plumbing.Hash, fails ...bool) (*object.Blob, error) {
	if len(fails) > 1 {
		panic("invalid usage of CreateDummyBlob() - this is a bug")
	}

	var realFails bool

	if len(fails) == 1 {
		realFails = fails[0]
	}

	blob, err := object.DecodeBlob(dummyEncodedObject{FakeHash: hash, Fails: realFails})
	if err != nil {
		return nil, fmt.Errorf("decoding dummy blob: %w", err)
	}

	return blob, nil
}
