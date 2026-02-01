package gitlib_test

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	git2go "github.com/libgit2/git2go/v34"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// testRepo wraps a test repository for integration testing.
type testRepo struct {
	t       *testing.T
	path    string
	native  *git2go.Repository
	cleanup func()
}

// newTestRepo creates a new test repository.
func newTestRepo(t *testing.T) *testRepo {
	t.Helper()

	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	require.NoError(t, err)

	return &testRepo{
		t:      t,
		path:   dir,
		native: repo,
		cleanup: func() {
			repo.Free()
		},
	}
}

// createFile creates a file in the working directory.
func (tr *testRepo) createFile(name, content string) {
	tr.t.Helper()

	path := filepath.Join(tr.path, name)
	dir := filepath.Dir(path)

	if dir != tr.path {
		err := os.MkdirAll(dir, 0o755)
		require.NoError(tr.t, err)
	}

	err := os.WriteFile(path, []byte(content), 0o644)
	require.NoError(tr.t, err)
}

// commit stages all files and creates a commit.
func (tr *testRepo) commit(message string) gitlib.Hash {
	tr.t.Helper()

	index, err := tr.native.Index()
	require.NoError(tr.t, err)

	defer index.Free()

	err = index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil)
	require.NoError(tr.t, err)

	err = index.Write()
	require.NoError(tr.t, err)

	treeID, err := index.WriteTree()
	require.NoError(tr.t, err)

	tree, err := tr.native.LookupTree(treeID)
	require.NoError(tr.t, err)

	defer tree.Free()

	sig := &git2go.Signature{
		Name:  "Test User",
		Email: "test@example.com",
		When:  time.Now(),
	}

	var parents []*git2go.Commit

	head, err := tr.native.Head()
	if err == nil {
		headCommit, lookupErr := tr.native.LookupCommit(head.Target())
		require.NoError(tr.t, lookupErr)

		parents = append(parents, headCommit)

		head.Free()
	}

	oid, err := tr.native.CreateCommit("HEAD", sig, sig, message, tree, parents...)
	require.NoError(tr.t, err)

	for _, parent := range parents {
		parent.Free()
	}

	return gitlib.HashFromOid(oid)
}

// deleteFile removes a file from the working directory.
func (tr *testRepo) deleteFile(name string) {
	tr.t.Helper()

	err := os.Remove(filepath.Join(tr.path, name))
	require.NoError(tr.t, err)
}

// Repository Tests.

func TestOpenRepository(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("initial")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	assert.Equal(t, tr.path, repo.Path())
	assert.NotNil(t, repo.Native())
}

func TestOpenRepositoryNotFound(t *testing.T) {
	repo, err := gitlib.OpenRepository("/nonexistent/path/to/repo")

	assert.Nil(t, repo)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "open repository")
}

func TestRepositoryHead(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "hello")
	expectedHash := tr.commit("initial")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	head, err := repo.Head()
	require.NoError(t, err)
	assert.Equal(t, expectedHash, head)
}

func TestRepositoryFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	// Free multiple times should be safe.
	repo.Free()
	repo.Free()
}

// Commit Tests.

func TestLookupCommit(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.go", "package main")
	commitHash := tr.commit("add file")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	assert.Equal(t, commitHash, commit.Hash())
	assert.Contains(t, commit.Message(), "add file")
	assert.Equal(t, "Test User", commit.Author().Name)
	assert.Equal(t, "test@example.com", commit.Author().Email)
	assert.Equal(t, "Test User", commit.Committer().Name)
	assert.NotNil(t, commit.Native())
}

func TestLookupCommitNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	invalidHash := gitlib.NewHash("1234567890123456789012345678901234567890")
	commit, err := repo.LookupCommit(invalidHash)

	assert.Nil(t, commit)
	assert.Error(t, err)
}

func TestCommitParent(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("first.txt", "1")
	firstHash := tr.commit("first")

	tr.createFile("second.txt", "2")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer commit.Free()

	assert.Equal(t, 1, commit.NumParents())
	assert.Equal(t, firstHash, commit.ParentHash(0))

	parent, err := commit.Parent(0)
	require.NoError(t, err)

	defer parent.Free()

	assert.Equal(t, firstHash, parent.Hash())
}

func TestCommitParentNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("only.txt", "x")
	commitHash := tr.commit("only commit")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	assert.Equal(t, 0, commit.NumParents())

	parent, err := commit.Parent(0)
	assert.Nil(t, parent)
	assert.ErrorIs(t, err, gitlib.ErrParentNotFound)
}

// Tree Tests.

func TestCommitTree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("main.go", "package main\n\nfunc main() {}\n")
	commitHash := tr.commit("add main")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	assert.False(t, tree.Hash().IsZero())
	assert.Equal(t, uint64(1), tree.EntryCount())
	assert.NotNil(t, tree.Native())
}

func TestTreeEntry(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("entry.txt", "content")
	commitHash := tr.commit("add entry")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	entry := tree.EntryByIndex(0)
	require.NotNil(t, entry)

	assert.Equal(t, "entry.txt", entry.Name())
	assert.False(t, entry.Hash().IsZero())
	assert.True(t, entry.IsBlob())
	assert.Equal(t, git2go.ObjectBlob, entry.Type())
}

func TestTreeEntryByPath(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("sub/deep/file.txt", "nested")
	commitHash := tr.commit("add nested")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	entry, err := tree.EntryByPath("sub/deep/file.txt")
	require.NoError(t, err)

	assert.Equal(t, "file.txt", entry.Name())
	assert.True(t, entry.IsBlob())
}

func TestTreeEntryByPathNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("exists.txt", "x")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	entry, err := tree.EntryByPath("nonexistent.txt")

	assert.Nil(t, entry)
	assert.Error(t, err)
}

func TestTreeEntryByIndexOutOfBounds(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("single.txt", "x")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	entry := tree.EntryByIndex(999)
	assert.Nil(t, entry)
}

// File Tests.

func TestCommitFiles(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "aaa")
	tr.createFile("b.txt", "bbb")
	tr.createFile("dir/c.txt", "ccc")
	commitHash := tr.commit("add files")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	iter, err := commit.Files()
	require.NoError(t, err)

	var fileNames []string

	for {
		file, nextErr := iter.Next()
		if errors.Is(nextErr, io.EOF) {
			break
		}

		require.NoError(t, nextErr)

		fileNames = append(fileNames, file.Name)
	}

	assert.Len(t, fileNames, 3)
	assert.Contains(t, fileNames, "a.txt")
	assert.Contains(t, fileNames, "b.txt")
	assert.Contains(t, fileNames, "dir/c.txt")
}

func TestCommitFile(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.go", "package test\n")
	commitHash := tr.commit("add test")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("test.go")
	require.NoError(t, err)

	assert.Equal(t, "test.go", file.Name)
	assert.False(t, file.Hash.IsZero())

	contents, err := file.Contents()
	require.NoError(t, err)
	assert.Equal(t, "package test\n", string(contents))
}

func TestCommitFileNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("exists.txt", "x")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("nonexistent.txt")

	assert.Nil(t, file)
	assert.Error(t, err)
}

func TestFileReader(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	content := "readable content"
	tr.createFile("readable.txt", content)
	commitHash := tr.commit("add readable")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("readable.txt")
	require.NoError(t, err)

	reader, err := file.Reader()
	require.NoError(t, err)

	defer reader.Close()

	data, err := io.ReadAll(reader)
	require.NoError(t, err)
	assert.Equal(t, content, string(data))
}

func TestFileBlob(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	content := "blob via file"
	tr.createFile("viafile.txt", content)
	commitHash := tr.commit("add via file")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("viafile.txt")
	require.NoError(t, err)

	blob, err := file.Blob()
	require.NoError(t, err)

	defer blob.Free()

	assert.Equal(t, file.Hash, blob.Hash())
	assert.Equal(t, []byte(content), blob.Contents())
}

// Blob Tests.

func TestLookupBlob(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("blob.txt", "blob content")
	commitHash := tr.commit("add blob")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("blob.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	defer blob.Free()

	assert.Equal(t, file.Hash, blob.Hash())
	assert.Equal(t, int64(12), blob.Size())
	assert.Equal(t, []byte("blob content"), blob.Contents())
	assert.NotNil(t, blob.Native())
}

func TestLookupBlobNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	invalidHash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	blob, err := repo.LookupBlob(invalidHash)

	assert.Nil(t, blob)
	assert.Error(t, err)
}

// CachedBlob Tests.

func TestCachedBlobFromRepo(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	content := "cached blob content"
	tr.createFile("cached.txt", content)
	commitHash := tr.commit("add cached")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("cached.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	assert.Equal(t, file.Hash, cached.Hash())
	assert.Equal(t, int64(len(content)), cached.Size())
	assert.Equal(t, []byte(content), cached.Data)
}

func TestCachedBlobCountLines(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("lines.txt", "line1\nline2\nline3\n")
	commitHash := tr.commit("add lines")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("lines.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	lines, err := cached.CountLines()
	require.NoError(t, err)
	assert.Equal(t, 3, lines)
}

func TestCachedBlobIsBinary(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("text.txt", "plain text")
	commitHash := tr.commit("add text")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("text.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	assert.False(t, cached.IsBinary())
}

func TestCachedBlobReader(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	content := "readable cached content"
	tr.createFile("readable.txt", content)
	commitHash := tr.commit("add readable")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("readable.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	reader := cached.Reader()
	data, err := io.ReadAll(reader)
	require.NoError(t, err)
	assert.Equal(t, content, string(data))
}

// Diff Tests.

func TestDiffTreeToTree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	// First commit.
	tr.createFile("unchanged.txt", "unchanged")
	tr.createFile("modified.txt", "original")
	tr.createFile("deleted.txt", "to delete")
	firstHash := tr.commit("first")

	// Second commit.
	tr.createFile("modified.txt", "modified")
	tr.createFile("added.txt", "new file")
	tr.deleteFile("deleted.txt")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	defer diff.Free()

	numDeltas, err := diff.NumDeltas()
	require.NoError(t, err)
	// Modified, added, deleted.
	assert.Equal(t, 3, numDeltas)
}

func TestDiffStats(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.txt", "original")
	firstHash := tr.commit("first")

	tr.createFile("file.txt", "modified content here")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	defer diff.Free()

	stats, err := diff.Stats()
	require.NoError(t, err)

	defer stats.Free()

	assert.Equal(t, 1, stats.FilesChanged())
	assert.Positive(t, stats.Insertions())
	assert.Positive(t, stats.Deletions())
}

// TreeDiff (Changes) Tests.

func TestTreeDiff(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("unchanged.txt", "same")
	tr.createFile("modified.txt", "original")
	tr.createFile("deleted.txt", "gone")
	firstHash := tr.commit("first")

	tr.createFile("modified.txt", "changed")
	tr.createFile("added.txt", "new")
	tr.deleteFile("deleted.txt")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	assert.Len(t, changes, 3)

	var foundInsert, foundDelete, foundModify bool

	for _, change := range changes {
		switch change.Action {
		case gitlib.Insert:
			foundInsert = true

			assert.Equal(t, "added.txt", change.To.Name)
		case gitlib.Delete:
			foundDelete = true

			assert.Equal(t, "deleted.txt", change.From.Name)
		case gitlib.Modify:
			foundModify = true

			assert.Equal(t, "modified.txt", change.From.Name)
		}
	}

	assert.True(t, foundInsert)
	assert.True(t, foundDelete)
	assert.True(t, foundModify)
}

func TestInitialTreeChanges(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "a")
	tr.createFile("b.txt", "b")
	tr.createFile("sub/c.txt", "c")
	commitHash := tr.commit("initial")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	changes, err := gitlib.InitialTreeChanges(repo, tree)
	require.NoError(t, err)
	assert.Len(t, changes, 3)

	for _, change := range changes {
		assert.Equal(t, gitlib.Insert, change.Action)
	}
}

func TestInitialTreeChangesNilTree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	changes, err := gitlib.InitialTreeChanges(repo, nil)
	require.NoError(t, err)
	assert.Empty(t, changes)
}

// RevWalk Tests.

func TestRevWalk(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	firstHash := tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	defer walk.Free()

	err = walk.Push(firstHash)
	require.NoError(t, err)

	hash, err := walk.Next()
	require.NoError(t, err)
	assert.Equal(t, firstHash, hash)
}

func TestRevWalkPushHead(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	defer walk.Free()

	err = walk.PushHead()
	require.NoError(t, err)

	hash, err := walk.Next()
	require.NoError(t, err)
	assert.Equal(t, secondHash, hash)
}

func TestRevWalkIterate(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	tr.createFile("3.txt", "3")
	tr.commit("third")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	defer walk.Free()

	err = walk.PushHead()
	require.NoError(t, err)

	var count int

	err = walk.Iterate(func(_ *gitlib.Commit) bool {
		count++

		return true
	})

	require.NoError(t, err)
	assert.Equal(t, 3, count)
}

// Log Tests.

func TestRepositoryLog(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	tr.createFile("3.txt", "3")
	tr.commit("third")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	var count int

	err = iter.ForEach(func(_ *gitlib.Commit) error {
		count++

		return nil
	})

	require.NoError(t, err)
	assert.Equal(t, 3, count)
}

// FileIter ForEach Tests.

func TestFileIterForEach(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "a")
	tr.createFile("b.txt", "b")
	commitHash := tr.commit("add files")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	iter, err := commit.Files()
	require.NoError(t, err)

	var names []string

	err = iter.ForEach(func(f *gitlib.File) error {
		names = append(names, f.Name)

		return nil
	})

	require.NoError(t, err)
	assert.Len(t, names, 2)
}

func TestFileIterForEachError(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "a")
	tr.createFile("b.txt", "b")
	tr.createFile("c.txt", "c")
	commitHash := tr.commit("add files")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	iter, err := commit.Files()
	require.NoError(t, err)

	expectedErr := errors.New("stop at 2")
	count := 0

	err = iter.ForEach(func(_ *gitlib.File) error {
		count++
		if count == 2 {
			return expectedErr
		}

		return nil
	})

	assert.Equal(t, expectedErr, err)
	assert.Equal(t, 2, count)
}

func TestTreeFilesMethod(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("root.txt", "root")
	tr.createFile("dir/nested.txt", "nested")
	commitHash := tr.commit("add files")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	iter := tree.Files()

	var names []string

	for {
		file, nextErr := iter.Next()
		if errors.Is(nextErr, io.EOF) {
			break
		}

		require.NoError(t, nextErr)

		names = append(names, file.Name)
	}

	assert.Len(t, names, 2)
	assert.Contains(t, names, "root.txt")
	assert.Contains(t, names, "dir/nested.txt")
}

// Additional Coverage Tests.

func TestBlobReader(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	content := "blob reader content"
	tr.createFile("reader.txt", content)
	commitHash := tr.commit("add reader test")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("reader.txt")
	require.NoError(t, err)

	blob, err := repo.LookupBlob(file.Hash)
	require.NoError(t, err)

	defer blob.Free()

	reader := blob.Reader()
	data, err := io.ReadAll(reader)
	require.NoError(t, err)
	assert.Equal(t, content, string(data))
}

func TestCommitIterClose(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("x.txt", "x")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	// Close before consuming.
	iter.Close()

	// Close again should be safe.
	iter.Close()
}

func TestFileIterClose(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	iter, err := commit.Files()
	require.NoError(t, err)

	// Iterate through.
	for {
		_, nextErr := iter.Next()
		if errors.Is(nextErr, io.EOF) {
			break
		}

		require.NoError(t, nextErr)
	}

	// Close after iteration.
	iter.Close()
}

func TestDiffForEach(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.txt", "original")
	firstHash := tr.commit("first")

	tr.createFile("file.txt", "modified")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	defer diff.Free()

	var deltaCount int

	err = diff.ForEach(func(_ gitlib.DiffDelta, _ float64) (git2go.DiffForEachHunkCallback, error) {
		deltaCount++

		return nil, nil //nolint:nilnil // Test callback.
	}, git2go.DiffDetailFiles)

	require.NoError(t, err)
	assert.Equal(t, 1, deltaCount)
}

func TestDiffDelta(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.txt", "original")
	firstHash := tr.commit("first")

	tr.createFile("file.txt", "modified")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	defer diff.Free()

	numDeltas, err := diff.NumDeltas()
	require.NoError(t, err)
	assert.Equal(t, 1, numDeltas)

	delta, err := diff.Delta(0)
	require.NoError(t, err)
	assert.Equal(t, git2go.DeltaModified, delta.Status)
	assert.Equal(t, "file.txt", delta.OldFile.Path)
	assert.Equal(t, "file.txt", delta.NewFile.Path)
}

func TestCachedBlobCountLinesNoNewline(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	// File without trailing newline.
	tr.createFile("no_newline.txt", "line1\nline2")
	commitHash := tr.commit("add file")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("no_newline.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	lines, err := cached.CountLines()
	require.NoError(t, err)
	assert.Equal(t, 2, lines)
}

func TestCachedBlobEmpty(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("empty.txt", "")
	commitHash := tr.commit("add empty")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("empty.txt")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	lines, err := cached.CountLines()
	require.NoError(t, err)
	assert.Equal(t, 0, lines)

	assert.False(t, cached.IsBinary())
}

func TestLookupTree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	treeHash := tree.Hash()
	tree.Free()

	lookedUp, err := repo.LookupTree(treeHash)
	require.NoError(t, err)

	defer lookedUp.Free()

	assert.Equal(t, treeHash, lookedUp.Hash())
}

func TestLookupTreeNotFound(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	invalidHash := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
	tree, err := repo.LookupTree(invalidHash)

	assert.Nil(t, tree)
	assert.Error(t, err)
}

func TestRevWalkPushInvalid(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	defer walk.Free()

	invalidHash := gitlib.NewHash("cccccccccccccccccccccccccccccccccccccccc")
	err = walk.Push(invalidHash)

	assert.Error(t, err)
}

func TestCommitIterNext(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	// Read using Next directly.
	var count int

	for {
		commit, nextErr := iter.Next()
		if errors.Is(nextErr, io.EOF) {
			break
		}

		require.NoError(t, nextErr)
		commit.Free()

		count++
	}

	assert.Equal(t, 2, count)
}

func TestTreeFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	// Free multiple times should be safe.
	tree.Free()
	tree.Free()
}

func TestCommitFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	// Free multiple times should be safe.
	commit.Free()
	commit.Free()
}

func TestRevWalkFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	// Free multiple times should be safe.
	walk.Free()
	walk.Free()
}

func TestDiffFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.txt", "original")
	firstHash := tr.commit("first")

	tr.createFile("file.txt", "modified")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	// Free multiple times should be safe.
	diff.Free()
	diff.Free()
}

func TestRepositoryLogWithSince(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	// Create commits with artificial time gaps.
	tr.createFile("first.txt", "1")
	tr.commit("first commit")

	tr.createFile("second.txt", "2")
	tr.commit("second commit")

	tr.createFile("third.txt", "3")
	tr.commit("third commit")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	// Get time of the second commit to use as filter.
	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	var commitTimes []time.Time

	err = iter.ForEach(func(c *gitlib.Commit) error {
		commitTimes = append(commitTimes, c.Author().When)

		return nil
	})

	require.NoError(t, err)
	require.GreaterOrEqual(t, len(commitTimes), 3)

	// Use time just before the second commit (which is at index 1).
	// Commits are returned in reverse order (newest first).
	sinceTime := commitTimes[1].Add(-1 * time.Second)

	iter2, err := repo.Log(&gitlib.LogOptions{Since: &sinceTime})
	require.NoError(t, err)

	var count int

	err = iter2.ForEach(func(_ *gitlib.Commit) error {
		count++

		return nil
	})

	require.NoError(t, err)
	// Should return at least 2 commits (second and third).
	assert.GreaterOrEqual(t, count, 2)
}

func TestFileContentsError(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	// Read contents.
	contents, err := file.Contents()
	require.NoError(t, err)
	assert.Equal(t, "content", string(contents))
}

func TestFileBlobMethod(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	content := "test content"
	tr.createFile("test.txt", content)
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	blob, err := file.Blob()
	require.NoError(t, err)

	defer blob.Free()

	assert.Equal(t, []byte(content), blob.Contents())
}

func TestTreeDiffRename(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("original.txt", "content")
	firstHash := tr.commit("first")

	// Rename: delete original, create new with same content.
	tr.deleteFile("original.txt")
	tr.createFile("renamed.txt", "content")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	changes, err := gitlib.TreeDiff(repo, firstTree, secondTree)
	require.NoError(t, err)
	// May be delete+insert or rename depending on libgit2 detection.
	assert.NotEmpty(t, changes)
}

func TestTreeDiffNilTrees(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	// Both nil - should produce empty changes.
	changes, err := gitlib.TreeDiff(repo, nil, nil)
	require.NoError(t, err)
	assert.Empty(t, changes)
}

// TestTreeDiffSameTreeOID verifies TreeDiff returns empty when both trees have the same OID (skip path).
func TestTreeDiffSameTreeOID(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("a.txt", "a")
	hash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)
	defer repo.Free()

	commit, err := repo.LookupCommit(hash)
	require.NoError(t, err)
	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)
	defer tree.Free()

	// Same tree OID: must skip libgit2 diff and return empty changes.
	changes, err := gitlib.TreeDiff(repo, tree, tree)
	require.NoError(t, err)
	assert.Empty(t, changes)
}

func TestCommitIterForEachError(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("1.txt", "1")
	tr.commit("first")

	tr.createFile("2.txt", "2")
	tr.commit("second")

	tr.createFile("3.txt", "3")
	tr.commit("third")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	iter, err := repo.Log(&gitlib.LogOptions{})
	require.NoError(t, err)

	expectedErr := errors.New("stop at 2")
	count := 0

	err = iter.ForEach(func(_ *gitlib.Commit) error {
		count++
		if count == 2 {
			return expectedErr
		}

		return nil
	})

	assert.Equal(t, expectedErr, err)
	assert.Equal(t, 2, count)
}

func TestCachedBlobBinary(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	// Create binary file (null byte).
	tr.createFile("binary.bin", "binary\x00content")
	commitHash := tr.commit("add binary")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("binary.bin")
	require.NoError(t, err)

	cached, err := gitlib.NewCachedBlobFromRepo(repo, file.Hash)
	require.NoError(t, err)

	assert.True(t, cached.IsBinary())

	lines, err := cached.CountLines()
	assert.Equal(t, 0, lines)
	assert.ErrorIs(t, err, gitlib.ErrBinary)
}

func TestRevWalkSorting(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	walk, err := repo.Walk()
	require.NoError(t, err)

	defer walk.Free()

	// Test sorting.
	walk.Sorting(git2go.SortTime)
	walk.Sorting(git2go.SortTopological)
	walk.Sorting(git2go.SortReverse)
}

func TestDiffTreeToTreeOneNil(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("file.txt", "content")
	commitHash := tr.commit("first")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	// Test with nil old tree.
	diff, err := repo.DiffTreeToTree(nil, tree)
	require.NoError(t, err)

	defer diff.Free()

	numDeltas, err := diff.NumDeltas()
	require.NoError(t, err)
	assert.Equal(t, 1, numDeltas)
}

func TestInitialTreeChangesWithSubdirectory(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("root.txt", "root")
	tr.createFile("sub1/file1.txt", "file1")
	tr.createFile("sub1/sub2/file2.txt", "file2")
	commitHash := tr.commit("add nested files")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	changes, err := gitlib.InitialTreeChanges(repo, tree)
	require.NoError(t, err)
	assert.Len(t, changes, 3)
}

func TestFileReaderClose(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("test.txt", "content")
	commitHash := tr.commit("init")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	file, err := commit.File("test.txt")
	require.NoError(t, err)

	reader, err := file.Reader()
	require.NoError(t, err)

	// Close without reading.
	err = reader.Close()
	require.NoError(t, err)
}

func TestTreeFilesNestedStructure(t *testing.T) {
	tr := newTestRepo(t)

	defer tr.cleanup()

	tr.createFile("a/b/c/d.txt", "deep")
	tr.createFile("a/b/e.txt", "mid")
	tr.createFile("a/f.txt", "shallow")
	commitHash := tr.commit("add nested")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	commit, err := repo.LookupCommit(commitHash)
	require.NoError(t, err)

	defer commit.Free()

	tree, err := commit.Tree()
	require.NoError(t, err)

	defer tree.Free()

	files, err := gitlib.TreeFiles(repo, tree)
	require.NoError(t, err)
	assert.Len(t, files, 3)
}

func TestDiffStatsFree(t *testing.T) {
	tr := newTestRepo(t)
	defer tr.cleanup()

	tr.createFile("file.txt", "original")
	firstHash := tr.commit("first")

	tr.createFile("file.txt", "modified")
	secondHash := tr.commit("second")

	repo, err := gitlib.OpenRepository(tr.path)
	require.NoError(t, err)

	defer repo.Free()

	firstCommit, err := repo.LookupCommit(firstHash)
	require.NoError(t, err)

	defer firstCommit.Free()

	secondCommit, err := repo.LookupCommit(secondHash)
	require.NoError(t, err)

	defer secondCommit.Free()

	firstTree, err := firstCommit.Tree()
	require.NoError(t, err)

	defer firstTree.Free()

	secondTree, err := secondCommit.Tree()
	require.NoError(t, err)

	defer secondTree.Free()

	diff, err := repo.DiffTreeToTree(firstTree, secondTree)
	require.NoError(t, err)

	defer diff.Free()

	stats, err := diff.Stats()
	require.NoError(t, err)

	// Free multiple times should be safe.
	stats.Free()
	stats.Free()
}
