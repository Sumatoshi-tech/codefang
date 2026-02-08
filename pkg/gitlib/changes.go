package gitlib

import (
	"fmt"
	"io"

	git2go "github.com/libgit2/git2go/v34"
)

// ChangeAction represents the type of change in a diff.
type ChangeAction int

const (
	// Insert indicates a new file was added.
	Insert ChangeAction = iota
	// Delete indicates a file was removed.
	Delete
	// Modify indicates a file was modified.
	Modify
)

// Change represents a single file change between two trees.
type Change struct {
	Action ChangeAction
	From   ChangeEntry
	To     ChangeEntry
}

// ChangeEntry represents one side of a change (old or new file).
type ChangeEntry struct {
	Name string
	Hash Hash
	Size int64
	Mode uint16
}

// Changes is a collection of Change objects.
type Changes []*Change

// TreeDiff computes the changes between two trees using libgit2.
// Skips diff when both tree OIDs are equal (e.g. metadata-only commits).
func TreeDiff(repo *Repository, oldTree, newTree *Tree) (Changes, error) {
	if oldTree != nil && newTree != nil && oldTree.Hash() == newTree.Hash() {
		return make(Changes, 0), nil
	}

	diff, err := repo.DiffTreeToTree(oldTree, newTree)
	if err != nil {
		return nil, fmt.Errorf("diff trees: %w", err)
	}
	defer diff.Free()

	numDeltas, numErr := diff.NumDeltas()
	if numErr != nil {
		return nil, fmt.Errorf("get num deltas: %w", numErr)
	}

	changes := make(Changes, 0, numDeltas)

	for i := range numDeltas {
		delta, deltaErr := diff.Delta(i)
		if deltaErr != nil {
			continue
		}

		change := &Change{}

		switch delta.Status {
		case git2go.DeltaAdded:
			change.Action = Insert
			change.To = ChangeEntry{
				Name: delta.NewFile.Path,
				Hash: delta.NewFile.Hash,
				Size: delta.NewFile.Size,
			}
		case git2go.DeltaDeleted:
			change.Action = Delete
			change.From = ChangeEntry{
				Name: delta.OldFile.Path,
				Hash: delta.OldFile.Hash,
				Size: delta.OldFile.Size,
			}
		case git2go.DeltaModified, git2go.DeltaRenamed, git2go.DeltaCopied:
			change.Action = Modify
			change.From = ChangeEntry{
				Name: delta.OldFile.Path,
				Hash: delta.OldFile.Hash,
				Size: delta.OldFile.Size,
			}
			change.To = ChangeEntry{
				Name: delta.NewFile.Path,
				Hash: delta.NewFile.Hash,
				Size: delta.NewFile.Size,
			}
		case git2go.DeltaUnmodified, git2go.DeltaIgnored, git2go.DeltaUntracked,
			git2go.DeltaTypeChange, git2go.DeltaUnreadable, git2go.DeltaConflicted:
			// Skip these delta types as they don't represent meaningful changes.
			continue
		}

		changes = append(changes, change)
	}

	return changes, nil
}

// InitialTreeChanges creates changes for an initial commit (all files are insertions).
func InitialTreeChanges(repo *Repository, tree *Tree) (Changes, error) {
	if tree == nil {
		return nil, nil
	}

	changes := make(Changes, 0)

	err := walkTree(repo, tree, "", func(path string, entry *TreeEntry) error {
		if !entry.IsBlob() {
			return nil
		}

		changes = append(changes, &Change{
			Action: Insert,
			To: ChangeEntry{
				Name: path,
				Hash: entry.Hash(),
			},
		})

		return nil
	})
	if err != nil {
		return nil, err
	}

	return changes, nil
}

// walkTree recursively walks a tree and calls the callback for each entry.
func walkTree(repo *Repository, tree *Tree, prefix string, cb func(path string, entry *TreeEntry) error) error {
	count := tree.EntryCount()

	for i := range count {
		entry := tree.EntryByIndex(i)
		if entry == nil {
			continue
		}

		walkErr := processTreeEntry(repo, entry, prefix, cb)
		if walkErr != nil {
			return walkErr
		}
	}

	return nil
}

// processTreeEntry handles a single tree entry, either calling cb for blobs or recursing for subtrees.
func processTreeEntry(repo *Repository, entry *TreeEntry, prefix string, cb func(path string, entry *TreeEntry) error) error {
	path := entry.Name()
	if prefix != "" {
		path = prefix + "/" + path
	}

	if entry.IsBlob() {
		return cb(path, entry)
	}

	if entry.Type() != git2go.ObjectTree {
		return nil
	}

	subtree, lookupErr := repo.LookupTree(entry.Hash())
	if lookupErr != nil {
		return nil // Skip entries we can't look up.
	}
	defer subtree.Free()

	return walkTree(repo, subtree, path, cb)
}

// File represents a file in a tree with its content accessible.
type File struct {
	Name string
	Hash Hash
	Mode uint16
	repo *Repository
}

// Contents returns the file contents.
func (f *File) Contents() ([]byte, error) {
	blob, err := f.repo.LookupBlob(f.Hash)
	if err != nil {
		return nil, err
	}
	defer blob.Free()

	return blob.Contents(), nil
}

// Reader returns a reader for the file contents.
func (f *File) Reader() (io.ReadCloser, error) {
	contents, err := f.Contents()
	if err != nil {
		return nil, err
	}

	return io.NopCloser(&blobReader{data: contents}), nil
}

// Blob returns the blob object for this file.
func (f *File) Blob() (*Blob, error) {
	return f.repo.LookupBlob(f.Hash)
}

// TreeFiles returns all files in a tree.
func TreeFiles(repo *Repository, tree *Tree) ([]*File, error) {
	var files []*File

	err := walkTree(repo, tree, "", func(path string, entry *TreeEntry) error {
		files = append(files, &File{
			Name: path,
			Hash: entry.Hash(),
			repo: repo,
		})

		return nil
	})
	if err != nil {
		return nil, err
	}

	return files, nil
}
