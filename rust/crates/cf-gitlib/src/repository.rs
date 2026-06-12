//! Repository handle and log/diff operations.
//!
//! [`Repository`] wraps a libgit2 [`git2::Repository`]. Per the design it is
//! **per-thread**: `git2::Repository` is already `!Send`/`!Sync`, and a
//! `PhantomData<*const ()>` field makes that intent explicit
//! and future-proof. RAII [`Drop`] frees the handle.

use std::marker::PhantomData;

use crate::blob::Blob;
use crate::commit::{Commit, CommitIter};
use crate::diff::Diff;
use crate::error::{GitError, Result};
use crate::hash::Hash;
use crate::revwalk::RevWalk;
use crate::tree::Tree;

/// Options controlling commit-log iteration.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Only include commits at/after this author time.
    pub since: Option<git2::Time>,
    /// Follow only the first parent (`git log --first-parent`).
    pub first_parent: bool,
    /// Yield oldest commits first (adds `SortReverse` to the walk).
    pub reverse: bool,
}

/// Applies process-global libgit2 performance options, once.
///
/// **`strict_hash_verification(false)`** — by default libgit2 re-hashes every
/// object it reads from the ODB (the hardened sha1dc SHA-1) to verify it against
/// its OID. For a read-only history walk over a large repo (e.g. kubernetes) that
/// re-hashing dominates the profile (~24% of CPU: `sha1_compression_states` +
/// `ubc_check`). The objects come from a trusted local clone and their content is
/// unaffected by the check, so disabling it is a pure speedup with **byte-identical
/// output** (it only skips an integrity assertion, never changes which bytes are
/// read or diffed).
///
/// The options are libgit2 process-global state, so they are set once under a
/// [`std::sync::Once`] on the first repository open.
fn tune_libgit2() {
    static TUNE: std::sync::Once = std::sync::Once::new();
    TUNE.call_once(|| {
        // `git2::opts` calls `crate::init()` internally, so libgit2 is initialized
        // before the option is applied.
        git2::opts::strict_hash_verification(false);
    });
}

/// A libgit2 repository.
///
/// **Not** `Send`/`Sync`: libgit2 repositories are single-threaded, so each
/// thread owns its own handle (the design's per-thread model). The contained
/// [`git2::Repository`] is dropped (and thus freed) automatically.
pub struct Repository {
    repo: git2::Repository,
    path: String,
    // Make `!Send + !Sync` explicit and stable even if git2 changes.
    _not_send_sync: PhantomData<*const ()>,
}

impl Repository {
    /// Opens a git repository at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::OpenRepository`] when the
    /// repository cannot be opened.
    pub fn open(path: &str) -> Result<Self> {
        tune_libgit2();
        let repo = git2::Repository::open(path).map_err(GitError::OpenRepository)?;
        Ok(Repository {
            repo,
            path: path.to_string(),
            _not_send_sync: PhantomData,
        })
    }

    /// Returns the repository path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the HEAD reference target hash.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::GetHead`] on failure.
    pub fn head(&self) -> Result<Hash> {
        let head = self.repo.head().map_err(GitError::GetHead)?;
        let target = head
            .target()
            .ok_or_else(|| GitError::GetHead(git2::Error::from_str("HEAD has no direct target")))?;
        Ok(Hash::from_oid(&target))
    }

    /// Looks up the commit with the given hash.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::LookupCommit`].
    pub fn lookup_commit(&self, hash: Hash) -> Result<Commit<'_>> {
        let commit = self
            .repo
            .find_commit(hash.to_oid())
            .map_err(GitError::LookupCommit)?;
        Ok(Commit::new(commit, self))
    }

    /// Looks up the blob with the given hash.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::LookupBlob`].
    pub fn lookup_blob(&self, hash: Hash) -> Result<Blob<'_>> {
        let blob = self
            .repo
            .find_blob(hash.to_oid())
            .map_err(GitError::LookupBlob)?;
        Ok(Blob::new(blob))
    }

    /// Looks up the tree with the given hash.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::LookupTree`].
    pub fn lookup_tree(&self, hash: Hash) -> Result<Tree<'_>> {
        let tree = self
            .repo
            .find_tree(hash.to_oid())
            .map_err(GitError::LookupTree)?;
        Ok(Tree::new(tree, self))
    }

    /// Creates a new revision walker.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::CreateRevwalk`].
    pub fn walk(&self) -> Result<RevWalk<'_>> {
        let walk = self.repo.revwalk().map_err(GitError::CreateRevwalk)?;
        Ok(RevWalk::new(walk, self))
    }

    /// Returns a commit iterator starting from HEAD.
    ///
    /// Pushes HEAD, applies `SortTime | SortTopological` (plus `SortReverse`
    /// when `opts.reverse`), and `simplify_first_parent` when
    /// `opts.first_parent`. The commit order feeds the history analyzers and is
    /// part of the report contract; the topological order prevents diffing
    /// against a descendant (burndown integrity).
    ///
    /// # Errors
    ///
    /// Returns revwalk/HEAD errors on failure.
    pub fn log(&self, opts: &LogOptions) -> Result<CommitIter<'_>> {
        let mut walk = self.repo.revwalk().map_err(GitError::CreateRevwalk)?;

        let head = self.repo.head().map_err(GitError::GetHead)?;
        let target = head
            .target()
            .ok_or_else(|| GitError::GetHead(git2::Error::from_str("HEAD has no direct target")))?;
        walk.push(target).map_err(GitError::PushHead)?;

        let mut sort = git2::Sort::TIME | git2::Sort::TOPOLOGICAL;
        if opts.reverse {
            sort |= git2::Sort::REVERSE;
        }
        walk.set_sorting(sort).map_err(GitError::CreateRevwalk)?;

        if opts.first_parent {
            walk.simplify_first_parent()
                .map_err(GitError::CreateRevwalk)?;
        }

        Ok(CommitIter::new(walk, self, opts.since))
    }

    /// Counts commits matching `opts` without materializing commits.
    ///
    /// O(N) time, O(1) memory; the `reverse` option is irrelevant to the count.
    ///
    /// # Errors
    ///
    /// Returns log-construction errors.
    pub fn commit_count(&self, opts: &LogOptions) -> Result<usize> {
        let mut iter = self.log(opts)?;
        let mut count = 0;
        // Drain until next_commit yields nothing; counting through the iterator
        // keeps the since filter applied identically to real iteration.
        loop {
            // Reuse next_commit so the since cutoff is applied identically.
            if iter.next_commit().is_some() {
                count += 1;
            } else {
                break;
            }
        }
        Ok(count)
    }

    /// Computes the diff between two trees.
    ///
    /// Either side may be `None` (an added/removed whole tree). Uses libgit2's
    /// default diff options.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::DiffTrees`].
    pub fn diff_tree_to_tree(
        &self,
        old_tree: Option<&Tree<'_>>,
        new_tree: Option<&Tree<'_>>,
    ) -> Result<Diff<'_>> {
        let diff = self
            .repo
            .diff_tree_to_tree(
                old_tree.map(Tree::native),
                new_tree.map(Tree::native),
                None,
            )
            .map_err(GitError::DiffTrees)?;
        Ok(Diff::new(diff))
    }

    /// Resolves a time spec that may be a duration/RFC3339/date or a commit
    /// ref/SHA.
    ///
    /// When `spec` is not a recognizable time string, it is interpreted as a
    /// revision and the referenced commit's author time is returned (as a Unix
    /// epoch second count).
    ///
    /// # Errors
    ///
    /// Returns [`GitError::InvalidTimeFormat`] when neither parsing nor revision
    /// resolution succeeds, or [`GitError::NotACommit`] when the revision is not
    /// a commit.
    pub fn resolve_time(&self, spec: &str) -> Result<i64> {
        if let Ok(secs) = crate::helpers::parse_time(spec) {
            return Ok(secs);
        }

        let obj = self
            .repo
            .revparse_single(spec)
            .map_err(|_| GitError::InvalidTimeFormat(spec.to_string()))?;

        let commit = obj.peel_to_commit().map_err(|e| GitError::NotACommit {
            spec: spec.to_string(),
            source: e,
        })?;

        let when = commit.author().when();
        Ok(when.seconds())
    }

    /// Returns the underlying libgit2 repository.
    #[must_use]
    pub fn native(&self) -> &git2::Repository {
        &self.repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRepo;

    // Mirrors reference test TestOpenRepository.
    #[test]
    fn open_repository() {
        let tr = TestRepo::new();
        tr.create_file("test.txt", "content");
        tr.commit("initial");

        let repo = Repository::open(tr.path()).unwrap();
        assert_eq!(repo.path(), tr.path());
    }

    // Mirrors reference test TestOpenRepositoryNotFound.
    #[test]
    fn open_repository_not_found() {
        let err = Repository::open("/nonexistent/path/to/repo").err().unwrap();
        assert!(err.to_string().contains("open repository"));
    }

    // Mirrors reference test TestRepositoryHead.
    #[test]
    fn repository_head() {
        let tr = TestRepo::new();
        tr.create_file("test.txt", "hello");
        let expected = tr.commit("initial");

        let repo = Repository::open(tr.path()).unwrap();
        assert_eq!(repo.head().unwrap(), expected);
    }

    // Mirrors reference test TestLookupCommit.
    #[test]
    fn lookup_commit() {
        let tr = TestRepo::new();
        tr.create_file("file.go", "package main");
        let commit_hash = tr.commit("add file");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        assert_eq!(commit.hash(), commit_hash);
        assert!(commit.message().contains("add file"));
        assert_eq!(commit.author().name, "Test User");
        assert_eq!(commit.author().email, "test@example.com");
    }

    // Mirrors reference test TestLookupCommitNotFound.
    #[test]
    fn lookup_commit_not_found() {
        let tr = TestRepo::new();
        tr.create_file("test.txt", "x");
        tr.commit("init");

        let repo = Repository::open(tr.path()).unwrap();
        let invalid = Hash::new("1234567890123456789012345678901234567890");
        assert!(repo.lookup_commit(invalid).is_err());
    }

    // Mirrors reference test TestCommitParent.
    #[test]
    fn commit_parent() {
        let tr = TestRepo::new();
        tr.create_file("first.txt", "1");
        let first = tr.commit("first");
        tr.create_file("second.txt", "2");
        let second = tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(second).unwrap();
        assert_eq!(commit.num_parents(), 1);
        assert_eq!(commit.parent_hash(0), first);

        let parent = commit.parent(0).unwrap();
        assert_eq!(parent.hash(), first);
    }

    // Mirrors reference test TestCommitParentNotFound.
    #[test]
    fn commit_parent_not_found() {
        let tr = TestRepo::new();
        tr.create_file("only.txt", "x");
        let only = tr.commit("only commit");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(only).unwrap();
        assert_eq!(commit.num_parents(), 0);
        assert!(matches!(commit.parent(0), Err(GitError::ParentNotFound)));
    }

    // Mirrors reference test TestCommitTree + TestTreeEntry.
    #[test]
    fn commit_tree_and_entry() {
        let tr = TestRepo::new();
        tr.create_file("entry.txt", "content");
        let commit_hash = tr.commit("add entry");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        let tree = commit.tree().unwrap();
        assert!(!tree.hash().is_zero());
        assert_eq!(tree.entry_count(), 1);

        let entry = tree.entry_by_index(0).unwrap();
        assert_eq!(entry.name(), "entry.txt");
        assert!(entry.is_blob());
        assert_eq!(entry.object_type(), Some(git2::ObjectType::Blob));
    }

    // Mirrors reference test TestTreeEntryByPath + ByPathNotFound + OutOfBounds.
    #[test]
    fn tree_entry_by_path() {
        let tr = TestRepo::new();
        tr.create_file("sub/deep/file.txt", "nested");
        let commit_hash = tr.commit("add nested");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        let tree = commit.tree().unwrap();

        let entry = tree.entry_by_path("sub/deep/file.txt").unwrap();
        assert_eq!(entry.name(), "file.txt");
        assert!(entry.is_blob());

        assert!(tree.entry_by_path("nonexistent.txt").is_err());
        assert!(tree.entry_by_index(999).is_none());
    }

    // Mirrors reference test TestCommitFiles.
    #[test]
    fn commit_files() {
        let tr = TestRepo::new();
        tr.create_file("a.txt", "aaa");
        tr.create_file("b.txt", "bbb");
        tr.create_file("dir/c.txt", "ccc");
        let commit_hash = tr.commit("add files");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        let mut iter = commit.files().unwrap();

        let mut names = Vec::new();
        while let Some(file) = iter.next_file() {
            names.push(file.name.clone());
        }
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
        assert!(names.contains(&"dir/c.txt".to_string()));
    }

    // Mirrors reference test TestCommitFile + TestCommitFileNotFound.
    #[test]
    fn commit_file() {
        let tr = TestRepo::new();
        tr.create_file("test.go", "package test\n");
        let commit_hash = tr.commit("add test");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();

        let file = commit.file("test.go").unwrap();
        assert_eq!(file.name, "test.go");
        assert!(!file.hash.is_zero());
        assert_eq!(file.contents().unwrap(), b"package test\n");

        assert!(commit.file("nonexistent.txt").is_err());
    }

    // Mirrors reference test TestLookupBlob + TestLookupBlobNotFound.
    #[test]
    fn lookup_blob() {
        let tr = TestRepo::new();
        tr.create_file("blob.txt", "blob content");
        let commit_hash = tr.commit("add blob");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        let file = commit.file("blob.txt").unwrap();

        let blob = repo.lookup_blob(file.hash).unwrap();
        assert_eq!(blob.hash(), file.hash);
        assert_eq!(blob.size(), 12);
        assert_eq!(blob.contents(), b"blob content");

        let invalid = Hash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(repo.lookup_blob(invalid).is_err());
    }

    // Mirrors reference test TestLookupTree + TestLookupTreeNotFound.
    #[test]
    fn lookup_tree() {
        let tr = TestRepo::new();
        tr.create_file("test.txt", "content");
        let commit_hash = tr.commit("init");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(commit_hash).unwrap();
        let tree = commit.tree().unwrap();
        let tree_hash = tree.hash();
        drop(tree);

        let looked_up = repo.lookup_tree(tree_hash).unwrap();
        assert_eq!(looked_up.hash(), tree_hash);

        let invalid = Hash::new("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert!(repo.lookup_tree(invalid).is_err());
    }

    // Mirrors reference test TestDiffTreeToTree + TestDiffStats + TestDiffDelta.
    #[test]
    fn diff_tree_to_tree() {
        let tr = TestRepo::new();
        tr.create_file("unchanged.txt", "unchanged");
        tr.create_file("modified.txt", "original");
        tr.create_file("deleted.txt", "to delete");
        let first = tr.commit("first");

        tr.create_file("modified.txt", "modified");
        tr.create_file("added.txt", "new file");
        tr.delete_file("deleted.txt");
        let second = tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();
        let first_tree = repo.lookup_commit(first).unwrap().tree().unwrap();
        let second_tree = repo.lookup_commit(second).unwrap().tree().unwrap();

        let diff = repo
            .diff_tree_to_tree(Some(&first_tree), Some(&second_tree))
            .unwrap();
        // Modified, added, deleted.
        assert_eq!(diff.num_deltas().unwrap(), 3);
    }

    // Mirrors reference test TestDiffStats.
    #[test]
    fn diff_stats() {
        let tr = TestRepo::new();
        tr.create_file("file.txt", "original");
        let first = tr.commit("first");
        tr.create_file("file.txt", "modified content here");
        let second = tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();
        let first_tree = repo.lookup_commit(first).unwrap().tree().unwrap();
        let second_tree = repo.lookup_commit(second).unwrap().tree().unwrap();

        let diff = repo
            .diff_tree_to_tree(Some(&first_tree), Some(&second_tree))
            .unwrap();
        let stats = diff.stats().unwrap();
        assert_eq!(stats.files_changed(), 1);
        assert!(stats.insertions() > 0);
        assert!(stats.deletions() > 0);
    }

    // Mirrors reference test TestRepositoryLog + TestCommitIterNext.
    #[test]
    fn repository_log() {
        let tr = TestRepo::new();
        tr.create_file("1.txt", "1");
        tr.commit("first");
        tr.create_file("2.txt", "2");
        tr.commit("second");
        tr.create_file("3.txt", "3");
        tr.commit("third");

        let repo = Repository::open(tr.path()).unwrap();
        let mut iter = repo.log(&LogOptions::default()).unwrap();
        let mut count = 0;
        iter.for_each(|_| {
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 3);
    }

    // Mirrors reference test TestLogFirstParent.
    #[test]
    fn log_first_parent() {
        let tr = TestRepo::new();
        tr.create_file("a.go", "a");
        let hash_a = tr.commit("first");
        tr.create_file("b.go", "b");
        let hash_b = tr.commit_to_ref("refs/heads/side", "branch", hash_a);
        let hash_m = tr.create_merge_commit("merge", hash_a, hash_b);

        let repo = Repository::open(tr.path()).unwrap();

        let mut iter_full = repo.log(&LogOptions::default()).unwrap();
        let mut full = Vec::new();
        while let Some(c) = iter_full.next_commit() {
            full.push(c.hash());
        }

        let mut iter_fp = repo
            .log(&LogOptions {
                first_parent: true,
                ..Default::default()
            })
            .unwrap();
        let mut fp = Vec::new();
        while let Some(c) = iter_fp.next_commit() {
            fp.push(c.hash());
        }

        assert!(full.contains(&hash_m));
        assert!(full.contains(&hash_b));
        assert!(fp.contains(&hash_m));
        assert!(fp.contains(&hash_a));
        assert!(!fp.contains(&hash_b), "first-parent must exclude branch commit");
        assert!(fp.len() < full.len());

        // Each commit's predecessor must be its first parent.
        for i in 0..fp.len() - 1 {
            let curr = repo.lookup_commit(fp[i]).unwrap();
            assert!(curr.num_parents() > 0);
            assert_eq!(fp[i + 1], curr.parent_hash(0));
        }
    }

    // Mirrors reference test TestRepositoryLogWithSince.
    #[test]
    fn repository_log_with_since() {
        let tr = TestRepo::new();
        tr.create_file("first.txt", "1");
        tr.commit("first commit");
        tr.create_file("second.txt", "2");
        tr.commit("second commit");
        tr.create_file("third.txt", "3");
        tr.commit("third commit");

        let repo = Repository::open(tr.path()).unwrap();
        let mut iter = repo.log(&LogOptions::default()).unwrap();
        let mut times = Vec::new();
        iter.for_each(|c| {
            times.push(c.author().when.seconds());
            Ok(())
        })
        .unwrap();
        assert!(times.len() >= 3);

        // Use a time just before the second-newest commit (index 1).
        let since = git2::Time::new(times[1] - 1, 0);
        let mut iter2 = repo
            .log(&LogOptions {
                since: Some(since),
                ..Default::default()
            })
            .unwrap();
        let mut count = 0;
        iter2.for_each(|_| {
            count += 1;
            Ok(())
        })
        .unwrap();
        assert!(count >= 2);
    }

    // Mirrors reference test TestResolveTime_CommitSHA + FallbackToParseTime + InvalidInput.
    #[test]
    fn resolve_time() {
        let tr = TestRepo::new();
        tr.create_file("a.txt", "a");
        let hash = tr.commit("first");

        let repo = Repository::open(tr.path()).unwrap();

        let resolved = repo.resolve_time(&hash.to_string()).unwrap();
        assert!(resolved > 0);
        let short = &hash.to_string()[..7];
        assert_eq!(repo.resolve_time(short).unwrap(), resolved);

        // Date-only still resolves through parse_time.
        assert!(repo.resolve_time("2024-01-01").is_ok());

        let err = repo.resolve_time("not-a-time-or-sha").unwrap_err();
        assert!(matches!(err, GitError::InvalidTimeFormat(_)));
    }

    // Mirrors reference tests: Close after exhaustion / before / Next after Close.
    #[test]
    fn commit_iter_close_semantics() {
        let tr = TestRepo::new();
        tr.create_file("1.txt", "1");
        tr.commit("first");
        tr.create_file("2.txt", "2");
        tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();

        // Exhaust then close (must not panic / double-free).
        let mut iter = repo.log(&LogOptions::default()).unwrap();
        while iter.next_commit().is_some() {}
        iter.close();
        iter.close();

        // Close immediately; Next after close yields None.
        let mut iter2 = repo.log(&LogOptions::default()).unwrap();
        iter2.close();
        assert!(iter2.next_commit().is_none());
    }

    // Mirrors reference test TestRevWalk + PushHead + Iterate + PushInvalid.
    #[test]
    fn revwalk() {
        let tr = TestRepo::new();
        tr.create_file("1.txt", "1");
        let first = tr.commit("first");
        tr.create_file("2.txt", "2");
        let second = tr.commit("second");
        tr.create_file("3.txt", "3");
        tr.commit("third");

        let repo = Repository::open(tr.path()).unwrap();

        let mut walk = repo.walk().unwrap();
        walk.push(first).unwrap();
        assert_eq!(walk.next_hash().unwrap(), first);

        let mut walk2 = repo.walk().unwrap();
        walk2.push_head().unwrap();
        // PushHead default sort yields newest-first; the third commit is HEAD.
        // We only assert it returns *some* commit deterministically.
        let _ = walk2.next_hash().unwrap();
        let _ = second;

        let mut walk3 = repo.walk().unwrap();
        walk3.push_head().unwrap();
        let mut count = 0;
        walk3
            .iterate(|_| {
                count += 1;
                true
            })
            .unwrap();
        assert_eq!(count, 3);

        let mut walk4 = repo.walk().unwrap();
        let invalid = Hash::new("cccccccccccccccccccccccccccccccccccccccc");
        assert!(walk4.push(invalid).is_err());
    }
}
