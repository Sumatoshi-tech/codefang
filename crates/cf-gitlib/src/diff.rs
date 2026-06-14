//! Tree-to-tree diff wrappers.
//!
//! [`Diff`] wraps a libgit2 [`git2::Diff`] (freed on [`Drop`]); [`DiffDelta`] /
//! [`DiffFile`] / [`DiffStats`] are owned snapshots of the libgit2 records.

use crate::error::{GitError, Result};
use crate::hash::Hash;

/// A run of consecutive same-kind diff lines.
///
/// Each variant carries the **line count** of the run. The producer
/// ([`diff_blob_line_ops`]) coalesces adjacent lines of the same kind into one
/// op; the resulting sequence is what the line-stat computation consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    /// Unchanged (context) lines.
    Equal(i64),
    /// Inserted (added) lines.
    Insert(i64),
    /// Deleted (removed) lines.
    Delete(i64),
}

/// Computes the libgit2 line-mode diff op sequence for a blob pair.
///
/// This is the diff engine the **runtime history pipeline** uses, NOT the
/// `diffmatchpatch` fallback. It must reproduce the reference batch-diff op
/// stream exactly: line-stat metrics (anomaly `lines_added`/`lines_removed`,
/// devs churn, …) flow straight into reports, and libgit2's Myers diff and
/// `diffmatchpatch`'s line diff group changed-vs-added-vs-removed lines
/// differently, so the two are not interchangeable. Pinned by the differential
/// gate in `tests/compat`.
///
/// The op stream rules (reference-implementation behavior):
/// * libgit2 default diff options (3 context lines) via
///   [`git2::DiffOptions::new`];
/// * per-line: context → [`LineOp::Equal`], addition → [`LineOp::Insert`],
///   deletion → [`LineOp::Delete`] (EOF-newline and header markers are
///   skipped);
/// * per-hunk: an implicit `Equal` block for lines skipped before the hunk
///   (`hunk.old_start - 1 > old_pos`);
/// * a trailing `Equal` block for the remaining unchanged tail
///   (`old_lines > old_pos`).
///
/// `old_lines` is the old blob's [`crate::blob::CachedBlob::count_lines`]
/// value, used only for the trailing block.
///
/// # Errors
///
/// Returns [`GitError::LookupBlob`] if either blob hash is missing, or
/// [`GitError::DiffForEach`] if libgit2's blob diff fails.
pub fn diff_blob_line_ops(
    repo: &git2::Repository,
    old: Hash,
    new: Hash,
    old_lines: i64,
) -> Result<Vec<LineOp>> {
    let old_blob = repo.find_blob(old.to_oid()).map_err(GitError::LookupBlob)?;
    let new_blob = repo.find_blob(new.to_oid()).map_err(GitError::LookupBlob)?;

    // Coalescing state shared by the hunk and line callbacks (the C `diff_ctx_t`).
    // A `RefCell` lets both `&mut` callbacks borrow it without aliasing.
    struct Coalesce {
        ops: Vec<LineOp>,
        // -1 = none, 0 = Equal, 1 = Insert, 2 = Delete (the C `current_type`).
        cur: i32,
        cnt: i64,
        old_pos: i64,
    }
    impl Coalesce {
        fn flush(&mut self) {
            if self.cnt > 0 {
                self.ops.push(match self.cur {
                    1 => LineOp::Insert(self.cnt),
                    2 => LineOp::Delete(self.cnt),
                    _ => LineOp::Equal(self.cnt),
                });
                self.cnt = 0;
            }
        }
        fn add(&mut self, t: i32, n: i64) {
            if t == self.cur {
                self.cnt += n;
            } else {
                self.flush();
                self.cur = t;
                self.cnt = n;
            }
        }
    }
    let state = std::cell::RefCell::new(Coalesce {
        ops: Vec::new(),
        cur: -1,
        cnt: 0,
        old_pos: 0,
    });

    let mut opts = git2::DiffOptions::new();
    let mut hunk_cb = |_d: git2::DiffDelta<'_>, hunk: git2::DiffHunk<'_>| -> bool {
        let mut s = state.borrow_mut();
        // old_start is 1-based; old_pos is the 0-based count of processed lines.
        let hunk_start = i64::from(hunk.old_start()) - 1;
        if hunk_start > s.old_pos {
            let skipped = hunk_start - s.old_pos;
            s.add(0, skipped);
            s.old_pos = hunk_start;
        }
        true
    };
    let mut line_cb =
        |_d: git2::DiffDelta<'_>, _h: Option<git2::DiffHunk<'_>>, line: git2::DiffLine<'_>| -> bool {
            let mut s = state.borrow_mut();
            match line.origin_value() {
                git2::DiffLineType::Context => {
                    s.add(0, 1);
                    s.old_pos += 1;
                }
                git2::DiffLineType::Addition => s.add(1, 1),
                git2::DiffLineType::Deletion => {
                    s.add(2, 1);
                    s.old_pos += 1;
                }
                // Header / EOF-newline markers: skipped, like the C default arm.
                _ => {}
            }
            true
        };

    repo.diff_blobs(
        Some(&old_blob),
        None,
        Some(&new_blob),
        None,
        Some(&mut opts),
        None,
        None,
        Some(&mut hunk_cb),
        Some(&mut line_cb),
    )
    .map_err(GitError::DiffForEach)?;

    let mut s = state.borrow_mut();
    s.flush();
    // Trailing equal block for the unchanged tail (C: old_lines > old_line_pos).
    if old_lines > s.old_pos {
        let remaining = old_lines - s.old_pos;
        s.add(0, remaining);
        s.flush();
    }
    Ok(std::mem::take(&mut s.ops))
}

/// A libgit2 diff.
pub struct Diff<'repo> {
    diff: git2::Diff<'repo>,
}

impl<'repo> Diff<'repo> {
    /// Wraps a libgit2 diff.
    pub(crate) fn new(diff: git2::Diff<'repo>) -> Self {
        Diff { diff }
    }

    /// Returns the number of deltas.
    ///
    /// # Errors
    ///
    /// This never fails in libgit2 (the count is read from the diff object); the
    /// `Result` is kept for signature stability, always `Ok`.
    pub fn num_deltas(&self) -> Result<usize> {
        Ok(self.diff.deltas().len())
    }

    /// Returns the delta at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::GetDelta`] when the index is out of range.
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

    /// Iterates the diff, invoking `file_cb` per file.
    ///
    /// Only the file callback is exposed; the binding analyzers use the
    /// file-level pass.
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

    /// Returns diff statistics.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::DiffStats`] on failure.
    pub fn stats(&self) -> Result<DiffStats> {
        let stats = self.diff.stats().map_err(GitError::DiffStats)?;
        Ok(DiffStats { stats })
    }
}

/// A file change in a diff.
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

/// A file within a diff delta.
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

/// Diff statistics.
pub struct DiffStats {
    stats: git2::DiffStats,
}

impl DiffStats {
    /// Number of inserted lines.
    #[must_use]
    pub fn insertions(&self) -> usize {
        self.stats.insertions()
    }

    /// Number of deleted lines.
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.stats.deletions()
    }

    /// Number of files changed.
    #[must_use]
    pub fn files_changed(&self) -> usize {
        self.stats.files_changed()
    }
}
