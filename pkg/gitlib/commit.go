package gitlib

import (
	"errors"
	"fmt"
	"io"
	"time"

	git2go "github.com/libgit2/git2go/v34"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
)

// ErrParentNotFound is returned when the requested parent commit is not found.
var ErrParentNotFound = errors.New("parent commit not found")

// errTestCommitNoTree is returned when attempting to get a tree from a test commit.
var errTestCommitNoTree = errors.New("get commit tree: test commit has no tree")

// Commit wraps a libgit2 commit.
type Commit struct {
	commit   *git2go.Commit
	repo     *Repository
	testHash *Hash // used for testing when commit is nil.
}

// NewCommitForTest creates a Commit with the given hash for testing.
func NewCommitForTest(h Hash) *Commit {
	return &Commit{
		testHash: &h,
	}
}

// Hash returns the commit hash.
func (c *Commit) Hash() Hash {
	if c.commit == nil {
		if c.testHash != nil {
			return *c.testHash
		}

		return Hash{}
	}

	return HashFromOid(c.commit.Id())
}

// Author returns the commit author. Zero value when commit is a test double (nil internal).
func (c *Commit) Author() Signature {
	if c.commit == nil {
		return Signature{}
	}

	sig := c.commit.Author()

	return Signature{
		Name:  sig.Name,
		Email: sig.Email,
		When:  sig.When,
	}
}

// Committer returns the commit committer. Zero value when commit is a test double (nil internal).
func (c *Commit) Committer() Signature {
	if c.commit == nil {
		return Signature{}
	}

	sig := c.commit.Committer()

	return Signature{
		Name:  sig.Name,
		Email: sig.Email,
		When:  sig.When,
	}
}

// Message returns the commit message. Empty when commit is a test double (nil internal).
func (c *Commit) Message() string {
	if c.commit == nil {
		return ""
	}

	return c.commit.Message()
}

// NumParents returns the number of parent commits. Zero when commit is a test double (nil internal).
func (c *Commit) NumParents() int {
	if c.commit == nil {
		return 0
	}

	return safeconv.MustUintToInt(c.commit.ParentCount())
}

// Parent returns the nth parent commit. ErrParentNotFound when commit is a test double (nil internal).
func (c *Commit) Parent(n int) (*Commit, error) {
	if c.commit == nil {
		return nil, ErrParentNotFound
	}

	parent := c.commit.Parent(safeconv.MustIntToUint(n))
	if parent == nil {
		return nil, ErrParentNotFound
	}

	return &Commit{commit: parent, repo: c.repo}, nil
}

// ParentHash returns the hash of the nth parent. Zero hash when commit is a test double (nil internal).
func (c *Commit) ParentHash(n int) Hash {
	if c.commit == nil {
		return Hash{}
	}

	return HashFromOid(c.commit.ParentId(safeconv.MustIntToUint(n)))
}

// TreeHash returns the hash of the tree associated with this commit. Zero when commit is a test double (nil internal).
func (c *Commit) TreeHash() Hash {
	if c.commit == nil {
		return Hash{}
	}

	return HashFromOid(c.commit.TreeId())
}

// Tree returns the tree associated with this commit. Error when commit is a test double (nil internal).
func (c *Commit) Tree() (*Tree, error) {
	if c.commit == nil {
		return nil, errTestCommitNoTree
	}

	tree, err := c.commit.Tree()
	if err != nil {
		return nil, fmt.Errorf("get commit tree: %w", err)
	}

	return &Tree{tree: tree, repo: c.repo}, nil
}

// Files returns an iterator over all files in the commit's tree.
func (c *Commit) Files() (*FileIter, error) {
	tree, err := c.Tree()
	if err != nil {
		return nil, err
	}

	files, err := TreeFiles(c.repo, tree)
	if err != nil {
		tree.Free()

		return nil, err
	}

	tree.Free()

	return &FileIter{files: files, idx: 0}, nil
}

// File returns a specific file from the commit's tree.
func (c *Commit) File(path string) (*File, error) {
	tree, err := c.Tree()
	if err != nil {
		return nil, err
	}
	defer tree.Free()

	entry, err := tree.EntryByPath(path)
	if err != nil {
		return nil, err
	}

	return &File{
		Name: path,
		Hash: entry.Hash(),
		repo: c.repo,
	}, nil
}

// Free releases the commit resources.
func (c *Commit) Free() {
	if c.commit != nil {
		c.commit.Free()
		c.commit = nil
	}
}

// Native returns the underlying libgit2 commit.
func (c *Commit) Native() *git2go.Commit {
	return c.commit
}

// CommitIter iterates over commits.
type CommitIter struct {
	walk  *git2go.RevWalk
	repo  *Repository
	since *time.Time
}

// Next returns the next commit in the iteration.
func (ci *CommitIter) Next() (*Commit, error) {
	if ci.walk == nil {
		return nil, io.EOF
	}

	for {
		oid := new(git2go.Oid)

		err := ci.walk.Next(oid)
		if err != nil {
			ci.walk.Free()
			ci.walk = nil

			return nil, io.EOF
		}

		commit, err := ci.repo.repo.LookupCommit(oid)
		if err != nil {
			continue
		}

		// Check since filter.
		if ci.since != nil && commit.Author().When.Before(*ci.since) {
			commit.Free()
			ci.walk.Free()
			ci.walk = nil

			return nil, io.EOF
		}

		return &Commit{commit: commit, repo: ci.repo}, nil
	}
}

// ForEach calls the callback for each commit.
func (ci *CommitIter) ForEach(cb func(*Commit) error) error {
	for {
		commit, err := ci.Next()
		if errors.Is(err, io.EOF) {
			return nil
		}

		if err != nil {
			return err
		}

		cbErr := cb(commit)
		commit.Free()

		if cbErr != nil {
			return cbErr
		}
	}
}

// Close releases resources.
func (ci *CommitIter) Close() {
	if ci.walk != nil {
		ci.walk.Free()
		ci.walk = nil
	}
}
