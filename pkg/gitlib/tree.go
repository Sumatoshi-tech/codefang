package gitlib

import (
	"context"
	"fmt"

	git2go "github.com/libgit2/git2go/v34"
)

// Tree wraps a libgit2 tree.
type Tree struct {
	tree *git2go.Tree
	repo *Repository
}

// Hash returns the tree hash.
func (t *Tree) Hash() Hash {
	return HashFromOid(t.tree.Id())
}

// EntryCount returns the number of entries in the tree.
func (t *Tree) EntryCount() uint64 {
	return t.tree.EntryCount()
}

// EntryByIndex returns the tree entry at the given index.
func (t *Tree) EntryByIndex(i uint64) *TreeEntry {
	entry := t.tree.EntryByIndex(i)
	if entry == nil {
		return nil
	}

	return &TreeEntry{entry: entry}
}

// EntryByPath returns the tree entry at the given path.
func (t *Tree) EntryByPath(path string) (*TreeEntry, error) {
	entry, err := t.tree.EntryByPath(path)
	if err != nil {
		return nil, fmt.Errorf("entry by path: %w", err)
	}

	return &TreeEntry{entry: entry}, nil
}

// FilesContext returns an iterator over all files in the tree, accepting a context for tracing.
func (t *Tree) FilesContext(_ context.Context) *FileIter {
	files, err := TreeFiles(t.repo, t)
	if err != nil {
		// Return empty iterator on error.
		return &FileIter{files: nil, idx: 0}
	}

	return &FileIter{files: files, idx: 0}
}

// Files returns an iterator over all files in the tree.
func (t *Tree) Files() *FileIter {
	return t.FilesContext(context.Background())
}

// Free releases the tree resources.
func (t *Tree) Free() {
	if t.tree != nil {
		t.tree.Free()
		t.tree = nil
	}
}

// Native returns the underlying libgit2 tree.
func (t *Tree) Native() *git2go.Tree {
	return t.tree
}

// TreeEntry wraps a libgit2 tree entry.
type TreeEntry struct {
	entry *git2go.TreeEntry
}

// Name returns the entry name.
func (e *TreeEntry) Name() string {
	return e.entry.Name
}

// Hash returns the entry object hash.
func (e *TreeEntry) Hash() Hash {
	return HashFromOid(e.entry.Id)
}

// Type returns the entry type.
func (e *TreeEntry) Type() git2go.ObjectType {
	return e.entry.Type
}

// IsBlob returns true if the entry is a blob.
func (e *TreeEntry) IsBlob() bool {
	return e.entry.Type == git2go.ObjectBlob
}
