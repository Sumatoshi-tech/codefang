//! Tree-to-tree diff wrappers, ported from `pkg/gitlib/diff.go`.
//!
//! [`Diff`] wraps a libgit2 [`git2::Diff`] (freed on [`Drop`], replacing Go's
//! `Free()`); [`DiffDelta`] / [`DiffFile`] / [`DiffStats`] mirror the Go structs.

use crate::error::{GitError, Result};
use crate::hash::Hash;

/// A run of consecutive same-kind diff lines, the Rust analogue of the
/// `cf_diff_op` records that `pkg/gitlib/clib/diff_ops.c` produces.
///
/// Each variant carries the **line count** of the run. The producer
/// ([`diff_blob_line_ops`]) coalesces adjacent lines of the same kind into one
/// op, exactly like the C `add_op`/`flush_op` pair, so the resulting sequence
/// is what Go feeds to `computeDiffLineStats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    /// Unchanged (context) lines.
    Equal(i64),
    /// Inserted (added) lines.
    Insert(i64),
    /// Deleted (removed) lines.
    Delete(i64),
}

/// Computes the libgit2 line-mode diff op sequence for a blob pair, mirroring
/// `pkg/gitlib/clib/diff_ops.c` (`compute_diff_generic` + the hunk/line
/// callbacks) byte-for-byte.
///
/// This is the diff engine the **runtime history pipeline** uses
/// (`framework/diff_pipeline.go` → `cf_batch_diff_blobs` → `git_diff_buffers`),
/// NOT the `diffmatchpatch` fallback. Matching it is what makes line-stat
/// metrics (anomaly `lines_added`/`lines_removed`, devs churn, …) byte-identical
/// to Go: libgit2's Myers diff and `diffmatchpatch`'s line diff group
/// changed-vs-added-vs-removed lines differently, so the two are not
/// interchangeable.
///
/// The op stream replicates the C exactly:
/// * `GIT_DIFF_OPTIONS_INIT` defaults (3 context lines) — [`git2::DiffOptions::new`]
///   calls `git_diff_init_options`, the same initializer the C uses;
/// * per-line: context → [`LineOp::Equal`], addition → [`LineOp::Insert`],
///   deletion → [`LineOp::Delete`] (EOF-newline and header markers are skipped,
///   matching the C `switch` default);
/// * per-hunk: an implicit `Equal` block for lines skipped before the hunk
///   (`hunk.old_start - 1 > old_pos`);
/// * a trailing `Equal` block for the remaining unchanged tail
///   (`old_lines > old_pos`).
///
/// `old_lines` is the old blob's [`crate::CachedBlob::count_lines`] value (Go
/// `result->old_lines`), used only for the trailing block.
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
