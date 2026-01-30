package gitlib

import (
	"errors"
	"time"
)

// ErrMockNotImplemented is returned by mock methods that are not implemented.
var ErrMockNotImplemented = errors.New("mock: operation not implemented")

// TestCommit is a mock commit for testing purposes.
// This is used in unit tests where real git operations are not needed.
type TestCommit struct {
	hash         Hash
	author       Signature
	committer    Signature
	message      string
	parentHashes []Hash
}

// NewTestCommit creates a new mock commit for testing.
func NewTestCommit(hash Hash, author Signature, message string, parentHashes ...Hash) *TestCommit {
	return &TestCommit{
		hash:         hash,
		author:       author,
		committer:    author,
		message:      message,
		parentHashes: parentHashes,
	}
}

// Hash returns the commit hash.
func (m *TestCommit) Hash() Hash { return m.hash }

// Author returns the commit author.
func (m *TestCommit) Author() Signature { return m.author }

// Committer returns the commit committer.
func (m *TestCommit) Committer() Signature { return m.committer }

// Message returns the commit message.
func (m *TestCommit) Message() string { return m.message }

// NumParents returns the number of parent commits.
func (m *TestCommit) NumParents() int { return len(m.parentHashes) }

// Parent returns the nth parent (not implemented for TestCommit).
func (m *TestCommit) Parent(_ int) (*Commit, error) { return nil, ErrMockNotImplemented }

// Tree returns an error for TestCommit (not implemented).
func (m *TestCommit) Tree() (*Tree, error) { return nil, ErrMockNotImplemented }

// Files returns an error for TestCommit (not implemented).
func (m *TestCommit) Files() (*FileIter, error) { return nil, ErrMockNotImplemented }

// File returns an error for TestCommit (not implemented).
func (m *TestCommit) File(_ string) (*File, error) { return nil, ErrMockNotImplemented }

// Free is a no-op for TestCommit.
func (m *TestCommit) Free() {
	// No resources to release for mock commit.
	_ = m.hash
}

// TestSignature creates a signature for testing.
func TestSignature(name, email string) Signature {
	return Signature{
		Name:  name,
		Email: email,
		When:  time.Now(),
	}
}
