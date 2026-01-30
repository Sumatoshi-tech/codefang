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

// Commit wraps a libgit2 commit.
type Commit struct {
	commit *git2go.Commit
	repo   *Repository
}

// Hash returns the commit hash.
func (c *Commit) Hash() Hash {
	return HashFromOid(c.commit.Id())
}

// Author returns the commit author.
func (c *Commit) Author() Signature {
	sig := c.commit.Author()

	return Signature{
		Name:  sig.Name,
		Email: sig.Email,
		When:  sig.When,
	}
}

// Committer returns the commit committer.
func (c *Commit) Committer() Signature {
	sig := c.commit.Committer()

	return Signature{
		Name:  sig.Name,
		Email: sig.Email,
		When:  sig.When,
	}
}

// Message returns the commit message.
func (c *Commit) Message() string {
	return c.commit.Message()
}

// NumParents returns the number of parent commits.
func (c *Commit) NumParents() int {
	return safeconv.MustUintToInt(c.commit.ParentCount())
}

// Parent returns the nth parent commit.
func (c *Commit) Parent(n int) (*Commit, error) {
	parent := c.commit.Parent(safeconv.MustIntToUint(n))
	if parent == nil {
		return nil, ErrParentNotFound
	}

	return &Commit{commit: parent, repo: c.repo}, nil
}

// ParentHash returns the hash of the nth parent.
func (c *Commit) ParentHash(n int) Hash {
	return HashFromOid(c.commit.ParentId(safeconv.MustIntToUint(n)))
}

// Tree returns the tree associated with this commit.
func (c *Commit) Tree() (*Tree, error) {
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
	for {
		oid := new(git2go.Oid)

		err := ci.walk.Next(oid)
		if err != nil {
			ci.walk.Free()

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
