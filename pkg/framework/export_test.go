package framework

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	git2go "github.com/libgit2/git2go/v34"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// TestRepo is a temporary git repo for framework tests.
type TestRepo struct {
	t    *testing.T
	path string
	repo *git2go.Repository
}

// NewTestRepo creates a temporary git repo for tests.
func NewTestRepo(t *testing.T) *TestRepo {
	t.Helper()
	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	if err != nil {
		t.Fatalf("InitRepository: %v", err)
	}

	return &TestRepo{t: t, path: dir, repo: repo}
}

// Close frees the repository.
func (r *TestRepo) Close() {
	if r.repo != nil {
		r.repo.Free()
		r.repo = nil
	}
}

// Path returns the repo directory path.
func (r *TestRepo) Path() string { return r.path }

// CreateFile creates a file with the given content.
func (r *TestRepo) CreateFile(name, content string) {
	r.t.Helper()

	p := filepath.Join(r.path, name)

	err := os.MkdirAll(filepath.Dir(p), 0o755)
	if err != nil {
		r.t.Fatalf("MkdirAll: %v", err)
	}

	err = os.WriteFile(p, []byte(content), 0o644)
	if err != nil {
		r.t.Fatalf("WriteFile: %v", err)
	}
}

// Commit creates a commit with the given message.
func (r *TestRepo) Commit(message string) gitlib.Hash {
	r.t.Helper()

	index, err := r.repo.Index()
	if err != nil {
		r.t.Fatalf("Index: %v", err)
	}
	defer index.Free()

	addErr := index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil)
	if addErr != nil {
		r.t.Fatalf("AddAll: %v", addErr)
	}

	writeErr := index.Write()
	if writeErr != nil {
		r.t.Fatalf("Index.Write: %v", writeErr)
	}

	treeID, writeTreeErr := index.WriteTree()
	if writeTreeErr != nil {
		r.t.Fatalf("WriteTree: %v", writeTreeErr)
	}

	tree, lookupTreeErr := r.repo.LookupTree(treeID)
	if lookupTreeErr != nil {
		r.t.Fatalf("LookupTree: %v", lookupTreeErr)
	}
	defer tree.Free()

	sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: time.Now()}

	var parents []*git2go.Commit

	head, err := r.repo.Head()
	if err == nil {
		headCommit, lookupErr := r.repo.LookupCommit(head.Target())
		if lookupErr == nil && headCommit != nil {
			parents = append(parents, headCommit)
		}

		head.Free()
	}

	oid, err := r.repo.CreateCommit("HEAD", sig, sig, message, tree, parents...)
	if err != nil {
		r.t.Fatalf("CreateCommit: %v", err)
	}

	for _, p := range parents {
		p.Free()
	}

	return gitlib.HashFromOid(oid)
}

// CollectCommits returns up to limit commits (newest first) from the repo.
func CollectCommits(t *testing.T, repo *gitlib.Repository, limit int) []*gitlib.Commit {
	t.Helper()

	iter, err := repo.Log(&gitlib.LogOptions{})
	if err != nil {
		t.Fatalf("Log: %v", err)
	}

	out := make([]*gitlib.Commit, 0, max(limit, 1))
	for limit <= 0 || len(out) < limit {
		commit, nextErr := iter.Next()
		if nextErr != nil {
			break
		}

		out = append(out, commit)
	}

	iter.Close()

	return out
}

// FileDiffFromGoDiffForTest exposes fileDiffFromGoDiff for tests.
func FileDiffFromGoDiffForTest(
	p *DiffPipeline,
	oldBlob, newBlob *gitlib.CachedBlob,
	oldLines, newLines int,
) pkgplumbing.FileDiffData {
	return p.fileDiffFromGoDiff(oldBlob, newBlob, oldLines, newLines)
}
