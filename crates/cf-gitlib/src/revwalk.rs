//! Revision walker.
//!
//! [`RevWalk`] wraps a libgit2 [`git2::Revwalk`] (freed on [`Drop`]). It yields
//! [`Hash`] values and implements the shared [`cf_alg::PullIterator`] contract
//! over `Hash`.

use cf_alg::{IteratorError, PullIterator};

use crate::error::{GitError, Result};
use crate::hash::Hash;
use crate::Repository;

/// A libgit2 revision walker.
pub struct RevWalk<'repo> {
    walk: Option<git2::Revwalk<'repo>>,
    repo: &'repo Repository,
}

impl<'repo> RevWalk<'repo> {
    /// Wraps a libgit2 revwalk with its owning repository.
    pub(crate) fn new(walk: git2::Revwalk<'repo>, repo: &'repo Repository) -> Self {
        RevWalk {
            walk: Some(walk),
            repo,
        }
    }

    /// Adds a commit to start walking from.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::PushRevwalk`] on failure,
    /// e.g. when the hash is not a known commit.
    pub fn push(&mut self, hash: Hash) -> Result<()> {
        let walk = self.walk_mut()?;
        walk.push(hash.to_oid()).map_err(GitError::PushRevwalk)
    }

    /// Adds HEAD to start walking from.
    ///
    /// # Errors
    ///
    /// Returns the HEAD-resolution error, or [`GitError::PushHead`] on failure.
    pub fn push_head(&mut self) -> Result<()> {
        let head = self.repo.head()?;
        let walk = self.walk_mut()?;
        walk.push(head.to_oid()).map_err(GitError::PushHead)
    }

    /// Sets the sorting mode.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::CreateRevwalk`] when libgit2 rejects the mode or the
    /// walker is closed.
    pub fn sorting(&mut self, mode: git2::Sort) -> Result<()> {
        let walk = self.walk_mut()?;
        walk.set_sorting(mode).map_err(GitError::CreateRevwalk)
    }

    /// Returns the next commit hash.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::RevwalkNext`] at end of walk
    /// or on failure.
    pub fn next_hash(&mut self) -> Result<Hash> {
        let walk = self.walk_mut()?;
        match walk.next() {
            Some(Ok(oid)) => Ok(Hash::from_oid(&oid)),
            Some(Err(e)) => Err(GitError::RevwalkNext(e)),
            None => Err(GitError::RevwalkNext(git2::Error::from_str(
                "revwalk exhausted",
            ))),
        }
    }

    /// Calls `cb` for each commit in the walk.
    ///
    /// `cb` returns `true` to continue, `false` to stop early. Each step looks
    /// up the full commit and passes it to the callback.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::RevwalkIterate`] on a walk failure.
    pub fn iterate<F>(&mut self, mut cb: F) -> Result<()>
    where
        F: FnMut(&crate::Commit<'repo>) -> bool,
    {
        let repo = self.repo;
        let walk = self.walk_mut()?;
        for step in walk.by_ref() {
            let oid = step.map_err(GitError::RevwalkIterate)?;
            let Ok(commit) = repo.native().find_commit(oid) else {
                continue;
            };
            let wrapped = crate::Commit::new(commit, repo);
            if !cb(&wrapped) {
                break;
            }
        }
        Ok(())
    }

    /// Releases the walker; idempotent.
    pub fn close(&mut self) {
        self.walk = None;
    }

    fn walk_mut(&mut self) -> Result<&mut git2::Revwalk<'repo>> {
        self.walk
            .as_mut()
            .ok_or_else(|| GitError::RevwalkNext(git2::Error::from_str("revwalk closed")))
    }
}

impl PullIterator<Hash> for RevWalk<'_> {
    fn next(&mut self) -> std::result::Result<Hash, IteratorError> {
        let Some(walk) = self.walk.as_mut() else {
            return Err(IteratorError::Eof);
        };
        match walk.next() {
            Some(Ok(oid)) => Ok(Hash::from_oid(&oid)),
            Some(Err(e)) => Err(IteratorError::Other(Box::new(GitError::RevwalkNext(e)))),
            None => Err(IteratorError::Eof),
        }
    }

    fn close(&mut self) {
        RevWalk::close(self);
    }
}
