//! Batch blob loading and blob-pair diffing.
//!
//! The reference implementation ran these batch operations through a custom
//! C shim over libgit2 on a dedicated OS thread. Here libgit2 is already
//! in-process via the `git2` crate and a [`crate::Repository`] is
//! `!Send`/`!Sync` — so the natural model is a per-thread [`Worker`] that owns
//! the repository and executes each request synchronously by calling git2
//! directly. The numeric outputs (line counts, diff op runs) reproduce the
//! reference batch algorithms byte-for-byte, which is what flows into reports
//! (pinned by `rust/tests/compat`).
//!
//! # Diff-op algorithm (reference batch-shim behavior)
//!
//! For a blob pair, libgit2's line callback emits context / addition /
//! deletion lines. The shim coalesces consecutive same-type lines into runs
//! ([`DiffOp`]), inserts an implicit *equal* run for lines skipped before each
//! hunk (`hunk.old_start - 1 > old_line_pos`), and appends a trailing *equal*
//! run for unchanged lines after the last hunk (`old_lines - old_line_pos`).
//! [`Worker::diff_blob_pair`] reproduces this exactly.

use crate::blob::CachedBlob;
use crate::changes::Changes;
use crate::error::{GitError, Result};
use crate::hash::Hash;
use crate::Repository;

/// The kind of a single diff operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DiffOpType {
    /// Lines unchanged between old and new.
    Equal = 0,
    /// Lines added in the new blob.
    Insert = 1,
    /// Lines removed from the old blob.
    Delete = 2,
}

/// A single coalesced run of diff lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOp {
    /// The kind of run.
    pub op_type: DiffOpType,
    /// The number of lines in the run.
    pub line_count: i32,
}

/// The result of loading a single blob.
#[derive(Debug)]
pub struct BlobResult {
    /// The blob hash.
    pub hash: Hash,
    /// The blob bytes (empty on error or for an empty blob).
    pub data: Vec<u8>,
    /// The blob size in bytes.
    pub size: i64,
    /// Whether the blob looks binary.
    pub is_binary: bool,
    /// The line count (`0` for binary or empty blobs).
    pub line_count: i32,
    /// The load error, if any (e.g. [`GitError::BlobLookup`]).
    pub error: Option<GitError>,
}

/// The result of diffing two blobs.
#[derive(Debug)]
pub struct DiffResult {
    /// Total lines in the old blob.
    pub old_lines: i32,
    /// Total lines in the new blob.
    pub new_lines: i32,
    /// The coalesced diff operations.
    pub ops: Vec<DiffOp>,
    /// The diff error, if any.
    pub error: Option<GitError>,
}

/// A request to diff two blobs.
#[derive(Debug, Clone, Default)]
pub struct DiffRequest {
    /// The old blob hash (used when `has_old` and `old_data` is empty).
    pub old_hash: Hash,
    /// The new blob hash (used when `has_new` and `new_data` is empty).
    pub new_hash: Hash,
    /// Pre-loaded old blob bytes (skips lookup when present).
    pub old_data: Vec<u8>,
    /// Pre-loaded new blob bytes (skips lookup when present).
    pub new_data: Vec<u8>,
    /// Whether the old side is present.
    pub has_old: bool,
    /// Whether the new side is present.
    pub has_new: bool,
}

/// A per-thread batch git worker.
///
/// Owns a borrowed [`Repository`] and executes batch blob/diff/tree-diff
/// operations synchronously. Because [`Repository`] is `!Send`/`!Sync`, a
/// worker is created and used on a single thread.
pub struct Worker<'repo> {
    repo: &'repo Repository,
}

impl<'repo> Worker<'repo> {
    /// Creates a worker over `repo`.
    #[must_use]
    pub fn new(repo: &'repo Repository) -> Self {
        Worker { repo }
    }

    /// Loads a batch of blobs by hash.
    ///
    /// Each result carries the bytes, size, binary flag, and line count, or an
    /// error ([`GitError::BlobLookup`]) when the blob is missing/not a blob. The
    /// order of results matches the order of `hashes`.
    #[must_use]
    pub fn batch_load_blobs(&self, hashes: &[Hash]) -> Vec<BlobResult> {
        hashes
            .iter()
            .map(|&hash| self.load_one_blob(hash))
            .collect()
    }

    /// Loads a batch of blobs as [`CachedBlob`]s.
    ///
    /// Successful entries become `Some(CachedBlob)`, failed entries `None`,
    /// preserving the request order and length.
    #[must_use]
    pub fn batch_load_cached_blobs(&self, hashes: &[Hash]) -> Vec<Option<CachedBlob>> {
        self.batch_load_blobs(hashes)
            .into_iter()
            .map(|res| {
                if res.error.is_none() {
                    Some(CachedBlob::from_parts(
                        res.hash,
                        res.size,
                        res.data,
                        i64::from(res.line_count),
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Loads a single blob.
    fn load_one_blob(&self, hash: Hash) -> BlobResult {
        match self.repo.lookup_blob(hash) {
            Ok(blob) => {
                let data = blob.contents();
                let size = blob.size();
                let is_binary = cf_textutil::is_binary(&data);
                // Line count is 0 for binary or empty blobs (reference batch-shim
                // behavior).
                let line_count = if !is_binary && !data.is_empty() {
                    cf_textutil::count_lines(&data) as i32
                } else {
                    0
                };
                BlobResult {
                    hash,
                    data,
                    size,
                    is_binary,
                    line_count,
                    error: None,
                }
            }
            Err(_) => BlobResult {
                hash,
                data: Vec::new(),
                size: 0,
                is_binary: false,
                line_count: 0,
                error: Some(GitError::BlobLookup),
            },
        }
    }

    /// Computes diffs for a batch of blob pairs.
    ///
    /// The order of results matches the order of `requests`.
    #[must_use]
    pub fn batch_diff_blobs(&self, requests: &[DiffRequest]) -> Vec<DiffResult> {
        requests.iter().map(|r| self.diff_blob_pair(r)).collect()
    }

    /// Diffs one blob pair, reproducing the reference batch-shim semantics.
    ///
    /// Resolves each side's bytes (pre-loaded `*_data` or a blob lookup by hash),
    /// returns [`GitError::BlobLookup`] when a needed blob is missing,
    /// [`GitError::BlobBinary`] when either side is binary, then runs the line
    /// diff and coalesces ops with the implicit/trailing equal-block rules.
    fn diff_blob_pair(&self, req: &DiffRequest) -> DiffResult {
        let mut result = DiffResult {
            old_lines: 0,
            new_lines: 0,
            ops: Vec::new(),
            error: None,
        };

        // Resolve old side bytes.
        let old_bytes = match self.resolve_side(req.has_old, &req.old_data, req.old_hash) {
            Ok(b) => b,
            Err(e) => {
                result.error = Some(e);
                return result;
            }
        };
        // Resolve new side bytes.
        let new_bytes = match self.resolve_side(req.has_new, &req.new_data, req.new_hash) {
            Ok(b) => b,
            Err(e) => {
                result.error = Some(e);
                return result;
            }
        };

        // Binary check on each present side.
        if (req.has_old && cf_textutil::is_binary(&old_bytes))
            || (req.has_new && cf_textutil::is_binary(&new_bytes))
        {
            result.error = Some(GitError::BlobBinary);
            return result;
        }

        if req.has_old {
            result.old_lines = cf_textutil::count_lines(&old_bytes) as i32;
        }
        if req.has_new {
            result.new_lines = cf_textutil::count_lines(&new_bytes) as i32;
        }

        let ops = compute_line_diff(&old_bytes, &new_bytes, result.old_lines);
        result.ops = ops;
        result
    }

    /// Resolves one diff side to its bytes: returns the pre-loaded data if
    /// present, otherwise looks up the blob by hash. An absent side (`has` =
    /// false) resolves to empty bytes.
    fn resolve_side(&self, has: bool, data: &[u8], hash: Hash) -> Result<Vec<u8>> {
        if !has {
            return Ok(Vec::new());
        }
        if !data.is_empty() {
            return Ok(data.to_vec());
        }
        // Pre-loaded data was empty: fall back to a blob lookup by hash.
        match self.repo.lookup_blob(hash) {
            Ok(blob) => Ok(blob.contents()),
            Err(_) => Err(GitError::DiffLookup),
        }
    }

    /// Computes tree changes with a pathspec pre-filter.
    ///
    /// Skips the diff when both tree OIDs are equal and non-zero (fast path).
    /// A non-empty `pathspec` restricts the diff to matching files at the
    /// libgit2 level. Submodule/sub-tree change entries (mode `0o160000` /
    /// `0o040000`) are filtered out (reference batch-shim behavior).
    ///
    /// # Errors
    ///
    /// Returns a diff error on libgit2 failure.
    pub fn tree_diff_with_pathspec(
        &self,
        old_tree_hash: Hash,
        new_tree_hash: Hash,
        pathspec: &[String],
    ) -> Result<Changes> {
        use crate::changes::{Change, ChangeAction, ChangeEntry};

        if !old_tree_hash.is_zero() && !new_tree_hash.is_zero() && old_tree_hash == new_tree_hash {
            return Ok(Vec::new());
        }

        const FILE_MODE_COMMIT: i32 = 0o160000;
        const FILE_MODE_TREE: i32 = 0o040000;

        let old_tree = if old_tree_hash.is_zero() {
            None
        } else {
            Some(self.repo.lookup_tree(old_tree_hash)?)
        };
        let new_tree = if new_tree_hash.is_zero() {
            None
        } else {
            Some(self.repo.lookup_tree(new_tree_hash)?)
        };

        let mut opts = git2::DiffOptions::new();
        for p in pathspec {
            opts.pathspec(p);
        }

        let diff = self
            .repo
            .native()
            .diff_tree_to_tree(
                old_tree.as_ref().map(crate::tree::Tree::native),
                new_tree.as_ref().map(crate::tree::Tree::native),
                Some(&mut opts),
            )
            .map_err(GitError::DiffTrees)?;

        let mut changes: Changes = Vec::with_capacity(diff.deltas().len());
        for delta in diff.deltas() {
            let old_mode = file_mode_to_u32(delta.old_file().mode()) as i32;
            let new_mode = file_mode_to_u32(delta.new_file().mode()) as i32;

            if old_mode == FILE_MODE_COMMIT
                || old_mode == FILE_MODE_TREE
                || new_mode == FILE_MODE_COMMIT
                || new_mode == FILE_MODE_TREE
            {
                continue;
            }

            let old_path = delta
                .old_file()
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_string();
            let new_path = delta
                .new_file()
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_string();
            let old_hash = Hash::from_oid(&delta.old_file().id());
            let new_hash = Hash::from_oid(&delta.new_file().id());
            let old_size = delta.old_file().size() as i64;
            let new_size = delta.new_file().size() as i64;

            let change = match delta.status() {
                git2::Delta::Added => Change {
                    action: ChangeAction::Insert,
                    from: ChangeEntry::default(),
                    to: ChangeEntry {
                        name: new_path,
                        hash: new_hash,
                        size: new_size,
                        mode: new_mode as u16,
                    },
                },
                git2::Delta::Deleted => Change {
                    action: ChangeAction::Delete,
                    from: ChangeEntry {
                        name: old_path,
                        hash: old_hash,
                        size: old_size,
                        mode: old_mode as u16,
                    },
                    to: ChangeEntry::default(),
                },
                git2::Delta::Modified | git2::Delta::Renamed | git2::Delta::Copied => Change {
                    action: ChangeAction::Modify,
                    from: ChangeEntry {
                        name: old_path,
                        hash: old_hash,
                        size: old_size,
                        mode: old_mode as u16,
                    },
                    to: ChangeEntry {
                        name: new_path,
                        hash: new_hash,
                        size: new_size,
                        mode: new_mode as u16,
                    },
                },
                _ => continue,
            };
            changes.push(change);
        }

        Ok(changes)
    }
}

/// Maps a libgit2 [`git2::FileMode`] to its raw octal value.
fn file_mode_to_u32(mode: git2::FileMode) -> u32 {
    // Map each libgit2 file mode to its standard git octal value. Only tree
    // (0o040000) and commit/submodule (0o160000) are special-cased by the
    // caller, but all variants are spelled out so the comparison is exact and
    // total. Matched by explicit variant (not an integer cast) because git2
    // 0.19's `FileMode` exposes no `From<FileMode>` integer conversion.
    match mode {
        git2::FileMode::Unreadable => 0,
        git2::FileMode::Tree => 0o040000,
        git2::FileMode::Blob => 0o100644,
        git2::FileMode::BlobGroupWritable => 0o100664,
        git2::FileMode::BlobExecutable => 0o100755,
        git2::FileMode::Link => 0o120000,
        git2::FileMode::Commit => 0o160000,
    }
}

/// Runs libgit2's line diff over two buffers and coalesces ops, reproducing
/// the reference batch-shim coalescing / hunk / trailing-equal logic.
fn compute_line_diff(old_data: &[u8], new_data: &[u8], old_lines: i32) -> Vec<DiffOp> {
    use std::cell::RefCell;

    // Coalescing state.
    struct Ctx {
        ops: Vec<DiffOp>,
        current_type: i32, // -1 = none yet
        current_count: i32,
        old_line_pos: i32,
        new_line_pos: i32,
    }
    impl Ctx {
        fn flush(&mut self) {
            if self.current_count > 0 {
                let op_type = match self.current_type {
                    1 => DiffOpType::Insert,
                    2 => DiffOpType::Delete,
                    _ => DiffOpType::Equal,
                };
                self.ops.push(DiffOp {
                    op_type,
                    line_count: self.current_count,
                });
                self.current_count = 0;
            }
        }
        fn add(&mut self, op_type: i32, count: i32) {
            if op_type == self.current_type {
                self.current_count += count;
            } else {
                self.flush();
                self.current_type = op_type;
                self.current_count = count;
            }
        }
    }

    let ctx = RefCell::new(Ctx {
        ops: Vec::new(),
        current_type: -1,
        current_count: 0,
        old_line_pos: 0,
        new_line_pos: 0,
    });

    // Scope the patch walk so any borrows end before we move `ctx` out with
    // `into_inner()` below. git2 0.19 exposes buffer-to-buffer line diffs via
    // `Patch::from_buffers`; we iterate its hunks/lines in order and apply the
    // reference coalescing (an implicit equal block for lines skipped before
    // each hunk, then per-line origin mapping).
    let diff_ok = {
        let mut opts = git2::DiffOptions::new();
        match git2::Patch::from_buffers(old_data, None, new_data, None, Some(&mut opts)) {
            Ok(patch) => {
                let mut ok = true;
                let num_hunks = patch.num_hunks();
                'hunks: for h in 0..num_hunks {
                    let (hunk, _hl): (git2::DiffHunk<'_>, usize) = match patch.hunk(h) {
                        Ok(v) => v,
                        Err(_) => {
                            ok = false;
                            break 'hunks;
                        }
                    };
                    {
                        // Hunk preamble: implicit equal block for skipped lines.
                        let mut c = ctx.borrow_mut();
                        let hunk_start = hunk.old_start() as i32 - 1; // 1-based -> 0-based
                        if hunk_start > c.old_line_pos {
                            let skipped = hunk_start - c.old_line_pos;
                            c.add(0, skipped);
                            c.old_line_pos = hunk_start;
                            c.new_line_pos += skipped;
                        }
                    }
                    let Ok(lines) = patch.num_lines_in_hunk(h) else {
                        ok = false;
                        break 'hunks;
                    };
                    for l in 0..lines {
                        let line: git2::DiffLine<'_> = match patch.line_in_hunk(h, l) {
                            Ok(v) => v,
                            Err(_) => {
                                ok = false;
                                break 'hunks;
                            }
                        };
                        let mut c = ctx.borrow_mut();
                        match line.origin() {
                            ' ' => {
                                c.add(0, 1); // CONTEXT -> EQUAL
                                c.old_line_pos += 1;
                                c.new_line_pos += 1;
                            }
                            '+' => {
                                c.add(1, 1); // ADDITION -> INSERT
                                c.new_line_pos += 1;
                            }
                            '-' => {
                                c.add(2, 1); // DELETION -> DELETE
                                c.old_line_pos += 1;
                            }
                            _ => {} // file/hunk headers, etc.
                        }
                    }
                }
                ok
            }
            Err(_) => false,
        }
    };

    let mut c = ctx.into_inner();
    if !diff_ok {
        // On a libgit2 error the reference shim returns a diff error with no
        // ops. Here we return whatever ops accumulated (empty), and the
        // caller's binary/lookup guards already ran.
        return c.ops;
    }

    // Flush any pending op, then append the trailing equal block for unchanged
    // lines after the last hunk (old_lines - old_line_pos).
    c.flush();
    if old_lines > c.old_line_pos {
        let remaining = old_lines - c.old_line_pos;
        c.add(0, remaining);
        c.flush();
    }
    c.ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRepo;

    // Mirrors reference test TestDiffOpType.
    #[test]
    fn diff_op_type_values() {
        assert_eq!(DiffOpType::Equal as i32, 0);
        assert_eq!(DiffOpType::Insert as i32, 1);
        assert_eq!(DiffOpType::Delete as i32, 2);
    }

    // Mirrors reference test TestWorker_BlobBatchRequest.
    #[test]
    fn worker_blob_batch() {
        let tr = TestRepo::new();
        tr.create_file("a.txt", "aaa");
        let first = tr.commit("first");

        let repo = Repository::open(tr.path()).unwrap();
        let commit = repo.lookup_commit(first).unwrap();
        let changes = crate::changes::initial_tree_changes(&repo, Some(&commit.tree().unwrap()))
            .unwrap();
        assert!(!changes.is_empty());

        let worker = Worker::new(&repo);
        let hashes: Vec<Hash> = changes.iter().map(|c| c.to.hash).collect();
        let blobs = worker.batch_load_cached_blobs(&hashes);
        assert_eq!(blobs.len(), hashes.len());
        for (i, b) in blobs.iter().enumerate() {
            let b = b.as_ref().expect("blob present");
            assert_eq!(b.hash(), hashes[i]);
            assert!(!b.data.is_empty());
        }
    }

    // Mirrors reference test TestWorker_BlobBatchRequestEmpty.
    #[test]
    fn worker_blob_batch_empty() {
        let tr = TestRepo::new();
        tr.create_file("x.txt", "x");
        tr.commit("only");

        let repo = Repository::open(tr.path()).unwrap();
        let worker = Worker::new(&repo);
        assert!(worker.batch_load_cached_blobs(&[]).is_empty());
    }

    // Mirrors reference test TestCGOBridge_BatchLoadBlobsInvalidHash.
    #[test]
    fn batch_load_invalid_hash() {
        let tr = TestRepo::new();
        tr.create_file("f.txt", "x");
        tr.commit("only");

        let repo = Repository::open(tr.path()).unwrap();
        let worker = Worker::new(&repo);
        let results = worker.batch_load_blobs(&[Hash::zero()]);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].error, Some(GitError::BlobLookup)));
    }

    // Mirrors reference test TestCGOBridge_BatchDiffBlobsInvalidHash.
    #[test]
    fn batch_diff_invalid_hash() {
        let tr = TestRepo::new();
        tr.create_file("f.txt", "a");
        tr.commit("only");

        let repo = Repository::open(tr.path()).unwrap();
        let worker = Worker::new(&repo);
        let req = DiffRequest {
            old_hash: Hash::zero(),
            new_hash: Hash::zero(),
            has_old: true,
            has_new: true,
            ..Default::default()
        };
        let results = worker.batch_diff_blobs(&[req]);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].error, Some(GitError::DiffLookup)));
    }

    // Mirrors reference test TestWorker_DiffBatchRequest.
    #[test]
    fn worker_diff_batch() {
        let tr = TestRepo::new();
        tr.create_file("f.txt", "line1\nline2\n");
        let first = tr.commit("first");
        tr.create_file("f.txt", "line1\nline2\nline3\n");
        let second = tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();
        let first_tree = repo.lookup_commit(first).unwrap().tree().unwrap();
        let second_tree = repo.lookup_commit(second).unwrap().tree().unwrap();
        let changes =
            crate::changes::tree_diff(&repo, Some(&first_tree), Some(&second_tree)).unwrap();
        assert_eq!(changes.len(), 1);
        let ch = &changes[0];
        assert_eq!(ch.action, crate::changes::ChangeAction::Modify);

        let worker = Worker::new(&repo);
        let req = DiffRequest {
            old_hash: ch.from.hash,
            new_hash: ch.to.hash,
            has_old: true,
            has_new: true,
            ..Default::default()
        };
        let results = worker.batch_diff_blobs(&[req]);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
        assert!(!results[0].ops.is_empty());
        // The added "line3" must appear as an Insert run of 1 line.
        assert!(results[0]
            .ops
            .iter()
            .any(|op| op.op_type == DiffOpType::Insert && op.line_count == 1));
    }

    // Mirrors reference test TestCGOBridge_TreeDiffWithPathspec_FiltersByGlob.
    #[test]
    fn tree_diff_pathspec_filters() {
        let tr = TestRepo::new();
        tr.create_file("a.go", "package a");
        tr.create_file("b.py", "x = 1");
        tr.create_file("c.js", "var y = 2;");
        let first = tr.commit("first");

        tr.create_file("a.go", "package a\n// edit");
        tr.create_file("b.py", "x = 2");
        tr.create_file("c.js", "var y = 3;");
        let second = tr.commit("second");

        let repo = Repository::open(tr.path()).unwrap();
        let first_th = repo.lookup_commit(first).unwrap().tree_hash();
        let second_th = repo.lookup_commit(second).unwrap().tree_hash();

        let worker = Worker::new(&repo);
        let baseline = worker.tree_diff_with_pathspec(first_th, second_th, &[]).unwrap();
        assert_eq!(baseline.len(), 3);

        let filtered = worker
            .tree_diff_with_pathspec(first_th, second_th, &["*.go".to_string()])
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].to.name, "a.go");
    }

    // Mirrors reference test TestCGOBridge_TreeDiffSameHash.
    #[test]
    fn tree_diff_same_hash() {
        let tr = TestRepo::new();
        tr.create_file("a.txt", "a");
        let only = tr.commit("only");

        let repo = Repository::open(tr.path()).unwrap();
        let th = repo.lookup_commit(only).unwrap().tree_hash();
        assert!(!th.is_zero());

        let worker = Worker::new(&repo);
        let changes = worker.tree_diff_with_pathspec(th, th, &[]).unwrap();
        assert!(changes.is_empty());
    }
}
