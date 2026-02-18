package gitlib

import (
	"context"
	"fmt"
	"time"

	git2go "github.com/libgit2/git2go/v34"
)

// Repository wraps a libgit2 repository.
type Repository struct {
	repo *git2go.Repository
	path string
}

// OpenRepository opens a git repository at the given path.
func OpenRepository(path string) (*Repository, error) {
	repo, err := git2go.OpenRepository(path)
	if err != nil {
		return nil, fmt.Errorf("open repository: %w", err)
	}

	return &Repository{repo: repo, path: path}, nil
}

// Path returns the repository path.
func (r *Repository) Path() string {
	return r.path
}

// Free releases the repository resources.
func (r *Repository) Free() {
	if r.repo != nil {
		r.repo.Free()
		r.repo = nil
	}
}

// Head returns the HEAD reference target.
func (r *Repository) Head() (Hash, error) {
	ref, err := r.repo.Head()
	if err != nil {
		return Hash{}, fmt.Errorf("get HEAD: %w", err)
	}
	defer ref.Free()

	return HashFromOid(ref.Target()), nil
}

// LookupCommit returns the commit with the given hash.
func (r *Repository) LookupCommit(_ context.Context, hash Hash) (*Commit, error) {
	commit, err := r.repo.LookupCommit(hash.ToOid())
	if err != nil {
		return nil, fmt.Errorf("lookup commit: %w", err)
	}

	return &Commit{commit: commit, repo: r}, nil
}

// LookupBlob returns the blob with the given hash.
func (r *Repository) LookupBlob(_ context.Context, hash Hash) (*Blob, error) {
	blob, err := r.repo.LookupBlob(hash.ToOid())
	if err != nil {
		return nil, fmt.Errorf("lookup blob: %w", err)
	}

	return &Blob{blob: blob}, nil
}

// LookupTree returns the tree with the given hash.
func (r *Repository) LookupTree(hash Hash) (*Tree, error) {
	tree, err := r.repo.LookupTree(hash.ToOid())
	if err != nil {
		return nil, fmt.Errorf("lookup tree: %w", err)
	}

	return &Tree{tree: tree, repo: r}, nil
}

// Walk creates a new revision walker starting from HEAD.
func (r *Repository) Walk() (*RevWalk, error) {
	walk, err := r.repo.Walk()
	if err != nil {
		return nil, fmt.Errorf("create revwalk: %w", err)
	}

	return &RevWalk{walk: walk, repo: r}, nil
}

// LogOptions configures the commit log iteration.
type LogOptions struct {
	Since       *time.Time // Only include commits after this time.
	FirstParent bool       // Follow only first parent (git log --first-parent).
}

// Log returns a commit iterator starting from HEAD.
func (r *Repository) Log(opts *LogOptions) (*CommitIter, error) {
	walk, err := r.repo.Walk()
	if err != nil {
		return nil, fmt.Errorf("create revwalk: %w", err)
	}

	// Start from HEAD.
	headRef, err := r.repo.Head()
	if err != nil {
		walk.Free()

		return nil, fmt.Errorf("get HEAD: %w", err)
	}
	defer headRef.Free()

	err = walk.Push(headRef.Target())
	if err != nil {
		walk.Free()

		return nil, fmt.Errorf("push HEAD to revwalk: %w", err)
	}

	// Topological order ensures we never diff against a descendant; prevents
	// negative burndown values when branches have different timestamps.
	walk.Sorting(git2go.SortTime | git2go.SortTopological)

	if opts != nil && opts.FirstParent {
		walk.SimplifyFirstParent()
	}

	return &CommitIter{walk: walk, repo: r, since: opts.Since}, nil
}

// DiffTreeToTree computes the diff between two trees.
func (r *Repository) DiffTreeToTree(oldTree, newTree *Tree) (*Diff, error) {
	opts, err := git2go.DefaultDiffOptions()
	if err != nil {
		return nil, fmt.Errorf("get diff options: %w", err)
	}

	var oldT, newT *git2go.Tree
	if oldTree != nil {
		oldT = oldTree.tree
	}

	if newTree != nil {
		newT = newTree.tree
	}

	diff, err := r.repo.DiffTreeToTree(oldT, newT, &opts)
	if err != nil {
		return nil, fmt.Errorf("diff trees: %w", err)
	}

	return &Diff{diff: diff}, nil
}

// Native returns the underlying libgit2 repository for advanced operations.
func (r *Repository) Native() *git2go.Repository {
	return r.repo
}
