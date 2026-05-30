//! Commit access and commit iteration, ported from `pkg/gitlib/commit.go`.
//!
//! [`Commit`] borrows a libgit2 [`git2::Commit`] (freed on [`Drop`], replacing
//! Go's `Free()`). It also supports a *test double* mode (Go's `testHash` field)
//! where the commit has no backing libgit2 object; in that mode hash returns the
//! injected value and the structural accessors return zero / errors, matching Go.
//!
//! [`CommitIter`] wraps a libgit2 revwalk, looking up full commit objects lazily
//! and honoring an optional `since` author-time filter — the exact behavior of
//! Go's `gitlib.CommitIter`.

use cf_alg::{IteratorError, PullIterator};
use cf_safeconv::{must_int_to_uint, must_uint_to_int};

use crate::error::{GitError, Result};
use crate::file::{File, FileIter};
use crate::hash::Hash;
use crate::signature::Signature;
use crate::tree::Tree;
use crate::Repository;

/// A libgit2 commit (Go `gitlib.Commit`).
///
/// Either backed by a real [`git2::Commit`] or a *test double* carrying only a
/// hash (Go's `testHash`). The structural accessors degrade gracefully on the
/// test-double path, exactly as the Go methods do when the internal commit is
/// `nil`.
pub struct Commit<'repo> {
    commit: Option<git2::Commit<'repo>>,
    repo: Option<&'repo Repository>,
    test_hash: Option<Hash>,
}

impl<'repo> Commit<'repo> {
    /// Wraps a real libgit2 commit.
    pub(crate) fn new(commit: git2::Commit<'repo>, repo: &'repo Repository) -> Self {
        Commit {
            commit: Some(commit),
            repo: Some(repo),
            test_hash: None,
        }
    }

    /// Creates a test-double commit carrying only a hash (Go `NewCommitForTest`).
    #[must_use]
    pub fn for_test(h: Hash) -> Self {
        Commit {
            commit: None,
            repo: None,
            test_hash: Some(h),
        }
    }

    /// Returns the commit hash (Go `Commit.Hash`).
    ///
    /// For a test double, returns the injected hash; for a real commit, the
    /// object id; the zero hash otherwise.
    #[must_use]
    pub fn hash(&self) -> Hash {
        match &self.commit {
            Some(c) => Hash::from_oid(&c.id()),
            None => self.test_hash.unwrap_or_else(Hash::zero),
        }
    }

    /// Returns the commit author (Go `Commit.Author`).
    ///
    /// Zero signature for a test double.
    #[must_use]
    pub fn author(&self) -> Signature {
        match &self.commit {
            Some(c) => Signature::from_git2(&c.author()),
            None => Signature::default(),
        }
    }

    /// Returns the commit committer (Go `Commit.Committer`).
    ///
    /// Zero signature for a test double.
    #[must_use]
    pub fn committer(&self) -> Signature {
        match &self.commit {
            Some(c) => Signature::from_git2(&c.committer()),
            None => Signature::default(),
        }
    }

    /// Returns the commit message (Go `Commit.Message`).
    ///
    /// Empty for a test double.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.commit {
            Some(c) => c.message().unwrap_or_default().to_string(),
            None => String::new(),
        }
    }

    /// Returns the number of parents (Go `Commit.NumParents`).
    ///
    /// Zero for a test double.
    #[must_use]
    pub fn num_parents(&self) -> usize {
        match &self.commit {
            Some(c) => must_uint_to_int(c.parent_count()) as usize,
            None => 0,
        }
    }

    /// Returns the nth parent commit (Go `Commit.Parent`).
    ///
    /// # Errors
    ///
    /// Returns [`GitError::ParentNotFound`] for a test double or an out-of-range
    /// index.
    pub fn parent(&self, n: usize) -> Result<Commit<'repo>> {
        let (Some(c), Some(repo)) = (&self.commit, self.repo) else {
            return Err(GitError::ParentNotFound);
        };
        // Go converts via MustIntToUint then asks git2go.Parent(n).
        let idx = must_int_to_uint(n as isize);
        match c.parent(idx) {
            Ok(parent) => Ok(Commit::new(parent, repo)),
            Err(_) => Err(GitError::ParentNotFound),
        }
    }

    /// Returns the hash of the nth parent (Go `Commit.ParentHash`).
    ///
    /// Zero hash for a test double.
    #[must_use]
    pub fn parent_hash(&self, n: usize) -> Hash {
        match &self.commit {
            Some(c) => {
                let idx = must_int_to_uint(n as isize);
                c.parent_id(idx).map(|oid| Hash::from_oid(&oid)).unwrap_or_else(Hash::zero)
            }
            None => Hash::zero(),
        }
    }

    /// Returns the hash of the commit's tree (Go `Commit.TreeHash`).
    ///
    /// Zero hash for a test double.
    #[must_use]
    pub fn tree_hash(&self) -> Hash {
        match &self.commit {
            Some(c) => Hash::from_oid(&c.tree_id()),
            None => Hash::zero(),
        }
    }

    /// Returns the commit's tree (Go `Commit.Tree`).
    ///
    /// # Errors
    ///
    /// Returns [`GitError::TestCommitNoTree`] for a test double, or
    /// [`GitError::CommitTree`] if the tree cannot be loaded.
    pub fn tree(&self) -> Result<Tree<'repo>> {
        let (Some(c), Some(repo)) = (&self.commit, self.repo) else {
            return Err(GitError::TestCommitNoTree);
        };
        let tree = c.tree().map_err(GitError::CommitTree)?;
        Ok(Tree::new(tree, repo))
    }

    /// Returns an iterator over all files in the commit's tree (Go `Commit.Files`).
    ///
    /// # Errors
    ///
    /// Propagates tree / walk errors.
    pub fn files(&self) -> Result<FileIter<'repo>> {
        let (Some(_), Some(repo)) = (&self.commit, self.repo) else {
            return Err(GitError::MockNotImplemented);
        };
        let tree = self.tree()?;
        let files = crate::changes::tree_files_public(repo, &tree)?;
        Ok(FileIter::new(files))
    }

    /// Returns a specific file from the commit's tree (Go `Commit.File`).
    ///
    /// # Errors
    ///
    /// Returns tree / entry errors when the path is not present.
    pub fn file(&self, path: &str) -> Result<File<'repo>> {
        let (Some(_), Some(repo)) = (&self.commit, self.repo) else {
            return Err(GitError::MockNotImplemented);
        };
        let tree = self.tree()?;
        let entry = tree.entry_by_path(path)?;
        Ok(File::new(path.to_string(), entry.hash(), 0, repo))
    }

    /// Returns the underlying libgit2 commit (Go `Commit.Native`), if real.
    #[must_use]
    pub fn native(&self) -> Option<&git2::Commit<'repo>> {
        self.commit.as_ref()
    }
}

/// An iterator over commits from a revwalk (Go `gitlib.CommitIter`).
///
/// Looks up full commit objects lazily and honors an optional `since` author-time
/// filter: when set, iteration stops at the first commit older than `since`
/// (matching Go's `commit.Author().When.Before(*since)` short-circuit).
///
/// Implements [`cf_alg::PullIterator`] (Go's `alg.Iterator[*Commit]`).
pub struct CommitIter<'repo> {
    walk: Option<git2::Revwalk<'repo>>,
    repo: &'repo Repository,
    since: Option<git2::Time>,
}

impl<'repo> CommitIter<'repo> {
    /// Builds a commit iterator over a configured revwalk.
    pub(crate) fn new(
        walk: git2::Revwalk<'repo>,
        repo: &'repo Repository,
        since: Option<git2::Time>,
    ) -> Self {
        CommitIter {
            walk: Some(walk),
            repo,
            since,
        }
    }

    /// Returns the next commit, or `None` at end of iteration
    /// (Go `CommitIter.Next` returning `nil, io.EOF`).
    ///
    /// Frees the walk once exhausted or filtered out (mirroring Go).
    #[must_use]
    pub fn next_commit(&mut self) -> Option<Commit<'repo>> {
        let walk = self.walk.as_mut()?;

        loop {
            let oid = match walk.next() {
                Some(Ok(oid)) => oid,
                _ => {
                    self.walk = None;
                    return None;
                }
            };

            let Ok(commit) = self.repo.native().find_commit(oid) else {
                continue;
            };

            if let Some(since) = self.since {
                if time_before(commit.author().when(), since) {
                    self.walk = None;
                    return None;
                }
            }

            return Some(Commit::new(commit, self.repo));
        }
    }

    /// Calls `cb` for each commit (Go `CommitIter.ForEach`).
    ///
    /// # Errors
    ///
    /// Propagates the first error returned by `cb`.
    pub fn for_each<F>(&mut self, mut cb: F) -> Result<()>
    where
        F: FnMut(&Commit<'repo>) -> Result<()>,
    {
        while let Some(commit) = self.next_commit() {
            cb(&commit)?;
        }
        Ok(())
    }

    /// Advances the iterator by `n` commits (Go `CommitIter.Skip`).
    ///
    /// # Errors
    ///
    /// Never returns an error in practice (EOF is a clean stop); the `Result` is
    /// kept to mirror Go's signature.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        for _ in 0..n {
            if !self.skip1() {
                break;
            }
        }
        Ok(())
    }

    /// Advances by one commit without materializing a `Commit` (Go `skip1`).
    ///
    /// Returns `true` if a commit was consumed, `false` at EOF / since-cutoff.
    fn skip1(&mut self) -> bool {
        let Some(walk) = self.walk.as_mut() else {
            return false;
        };

        let oid = match walk.next() {
            Some(Ok(oid)) => oid,
            _ => {
                self.walk = None;
                return false;
            }
        };

        if let Some(since) = self.since {
            // Look up the commit to inspect its author time (Go does the same).
            let Ok(commit) = self.repo.native().find_commit(oid) else {
                return false;
            };
            if time_before(commit.author().when(), since) {
                self.walk = None;
                return false;
            }
        }

        true
    }

    /// Releases the walk (Go `CommitIter.Close`); idempotent.
    pub fn close(&mut self) {
        self.walk = None;
    }
}

impl<'repo> PullIterator<Commit<'repo>> for CommitIter<'repo> {
    fn next(&mut self) -> std::result::Result<Commit<'repo>, IteratorError> {
        self.next_commit().ok_or(IteratorError::Eof)
    }

    fn close(&mut self) {
        CommitIter::close(self);
    }
}

/// Reports whether `a` is strictly before `b` (Go `time.Time.Before`).
///
/// Compares the absolute instant (seconds since epoch), independent of UTC
/// offset, matching Go's `time.Time` comparison semantics.
fn time_before(a: git2::Time, b: git2::Time) -> bool {
    a.seconds() < b.seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from gitlib_test.go::TestTestCommit* and the for-test constructors:
    // a test-double commit yields its injected hash, zero signatures, no parents.
    #[test]
    fn test_double_commit() {
        let h = Hash::new("abcdef1234567890abcdef1234567890abcdef12");
        let c = Commit::for_test(h);
        assert_eq!(c.hash(), h);
        assert_eq!(c.author(), Signature::default());
        assert_eq!(c.committer(), Signature::default());
        assert_eq!(c.message(), "");
        assert_eq!(c.num_parents(), 0);
        assert_eq!(c.tree_hash(), Hash::zero());
        assert_eq!(c.parent_hash(0), Hash::zero());
        assert!(matches!(c.parent(0), Err(GitError::ParentNotFound)));
        assert!(matches!(c.tree(), Err(GitError::TestCommitNoTree)));
    }
}
