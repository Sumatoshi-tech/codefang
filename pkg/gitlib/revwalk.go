package gitlib

import (
	"fmt"

	git2go "github.com/libgit2/git2go/v34"
)

// RevWalk wraps a libgit2 revision walker.
type RevWalk struct {
	walk *git2go.RevWalk
	repo *Repository
}

// Push adds a commit to start walking from.
func (w *RevWalk) Push(hash Hash) error {
	err := w.walk.Push(hash.ToOid())
	if err != nil {
		return fmt.Errorf("push to revwalk: %w", err)
	}

	return nil
}

// PushHead adds HEAD to start walking from.
func (w *RevWalk) PushHead() error {
	head, err := w.repo.Head()
	if err != nil {
		return err
	}

	err = w.walk.Push(head.ToOid())
	if err != nil {
		return fmt.Errorf("push HEAD to revwalk: %w", err)
	}

	return nil
}

// Sorting sets the sorting mode for the walker.
func (w *RevWalk) Sorting(mode git2go.SortType) {
	w.walk.Sorting(mode)
}

// Next returns the next commit hash in the walk.
func (w *RevWalk) Next() (Hash, error) {
	oid := new(git2go.Oid)

	nextErr := w.walk.Next(oid)
	if nextErr != nil {
		return Hash{}, fmt.Errorf("revwalk next: %w", nextErr)
	}

	return HashFromOid(oid), nil
}

// Iterate calls the callback for each commit in the walk.
func (w *RevWalk) Iterate(cb func(*Commit) bool) error {
	err := w.walk.Iterate(func(commit *git2go.Commit) bool {
		wrappedCommit := &Commit{commit: commit, repo: w.repo}

		return cb(wrappedCommit)
	})
	if err != nil {
		return fmt.Errorf("revwalk iterate: %w", err)
	}

	return nil
}

// Free releases the walker resources.
func (w *RevWalk) Free() {
	if w.walk != nil {
		w.walk.Free()
		w.walk = nil
	}
}
