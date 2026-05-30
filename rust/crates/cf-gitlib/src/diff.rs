//! Tree-to-tree diff wrappers, ported from `pkg/gitlib/diff.go`.
//!
//! [`Diff`] wraps a libgit2 [`git2::Diff`] (freed on [`Drop`], replacing Go's
//! `Free()`); [`DiffDelta`] / [`DiffFile`] / [`DiffStats`] mirror the Go structs.

use crate::error::{GitError, Result};
use crate::hash::Hash;

/// A libgit2 diff (Go `gitlib.Diff`).
pub struct Diff<'repo> {
    diff: git2::Diff<'repo>,
}

impl<'repo> Diff<'repo> {
    /// Wraps a libgit2 diff.
    pub(crate) fn new(diff: git2::Diff<'repo>) -> Self {
        Diff { diff }
    }

    /// Returns the number of deltas (Go `Diff.NumDeltas`).
    ///
    /// # Errors
    ///
    /// This never fails in libgit2 (the count is read from the diff object); the
    /// `Result` is kept to mirror the Go signature, always `Ok`.
    pub fn num_deltas(&self) -> Result<usize> {
        Ok(self.diff.deltas().len())
    }

    /// Returns the delta at `index` (Go `Diff.Delta`).
    ///
    /// # Errors
    ///
    /// Returns [`GitError::GetDelta`] when the index is out of range (Go returns
    /// `get delta: %w`).
    pub fn delta(&self, index: usize) -> Result<DiffDelta> {
        let delta = self.diff.get_delta(index).ok_or_else(|| {
            GitError::GetDelta(git2::Error::from_str("index out of range"))
        })?;

        Ok(DiffDelta {
            status: delta.status(),
            old_file: DiffFile::from(delta.old_file()),
            new_file: DiffFile::from(delta.new_file()),
            flags: delta.flags(),
            num_hunks: 0,
        })
    }

    /// Iterates the diff, invoking `file_cb` per file (Go `Diff.ForEach`).
    ///
    /// Only the file callback is exposed (the Go wrapper passes through hunk/line
    /// callbacks but the binding analyzers use the file-level pass). Mirrors Go's
    /// `DiffDetailFiles` traversal.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::DiffForEach`] if libgit2's foreach fails.
    pub fn for_each<F>(&self, mut file_cb: F) -> Result<()>
    where
        F: FnMut(DiffDelta, f32) -> bool,
    {
        self.diff
            .foreach(
                &mut |delta, progress| {
                    let wrapped = DiffDelta {
                        status: delta.status(),
                        old_file: DiffFile::from(delta.old_file()),
                        new_file: DiffFile::from(delta.new_file()),
                        flags: delta.flags(),
                        num_hunks: 0,
                    };
                    file_cb(wrapped, progress)
                },
                None,
                None,
                None,
            )
            .map_err(GitError::DiffForEach)
    }

    /// Returns diff statistics (Go `Diff.Stats`).
    ///
    /// # Errors
    ///
    /// Returns [`GitError::DiffStats`] on failure.
    pub fn stats(&self) -> Result<DiffStats> {
        let stats = self.diff.stats().map_err(GitError::DiffStats)?;
        Ok(DiffStats { stats })
    }
}

/// A file change in a diff (Go `gitlib.DiffDelta`).
#[derive(Debug, Clone)]
pub struct DiffDelta {
    /// The delta status (added / deleted / modified / …).
    pub status: git2::Delta,
    /// The old-side file.
    pub old_file: DiffFile,
    /// The new-side file.
    pub new_file: DiffFile,
    /// Diff flags.
    pub flags: git2::DiffFlags,
    /// Number of hunks (set by hunk traversal; `0` from the file-level pass).
    pub num_hunks: i32,
}

/// A file within a diff delta (Go `gitlib.DiffFile`).
#[derive(Debug, Clone)]
pub struct DiffFile {
    /// File path (empty when the side is absent).
    pub path: String,
    /// Object hash.
    pub hash: Hash,
    /// File size in bytes.
    pub size: i64,
}

impl From<git2::DiffFile<'_>> for DiffFile {
    fn from(f: git2::DiffFile<'_>) -> Self {
        DiffFile {
            path: f
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_string(),
            hash: Hash::from_oid(&f.id()),
            size: f.size() as i64,
        }
    }
}

/// Diff statistics (Go `gitlib.DiffStats`).
pub struct DiffStats {
    stats: git2::DiffStats,
}

impl DiffStats {
    /// Number of inserted lines (Go `DiffStats.Insertions`).
    #[must_use]
    pub fn insertions(&self) -> usize {
        self.stats.insertions()
    }

    /// Number of deleted lines (Go `DiffStats.Deletions`).
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.stats.deletions()
    }

    /// Number of files changed (Go `DiffStats.FilesChanged`).
    #[must_use]
    pub fn files_changed(&self) -> usize {
        self.stats.files_changed()
    }
}
