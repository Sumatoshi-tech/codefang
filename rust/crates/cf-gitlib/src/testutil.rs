//! Test repository builders.
//!
//! Helpers for constructing throwaway git repositories with deterministic
//! commits, used by this crate's integration tests and available to downstream
//! crates' tests via the `testutil` feature. Commit timestamps are fixed so any
//! golden built on these repos is reproducible (DESIGN §2.8). This has no direct
//! Go counterpart (the Go tests used `t.TempDir()` + raw git2go), but it backs
//! the ported behavioral tests.

use crate::error::Result;
use crate::hash::Hash;
use crate::repository::Repository;
use crate::GitError;

/// A handle to a temporary repository plus its on-disk directory, removed on
/// drop.
pub struct TestRepo {
    /// The opened repository.
    pub repo: Repository,
    dir: std::path::PathBuf,
}

impl TestRepo {
    /// The repository's working directory.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.dir
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A deterministic signature for test commits (fixed name/email/time).
fn fixed_sig<'a>(secs: i64) -> std::result::Result<git2::Signature<'a>, git2::Error> {
    let when = git2::Time::new(secs, 0);
    git2::Signature::new("Test Author", "test@example.com", &when)
}

/// Create an empty initialized repository in a fresh temp directory.
///
/// # Errors
/// Propagates filesystem and libgit2 init errors.
pub fn init_repo() -> Result<TestRepo> {
    let dir = unique_temp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| GitError::Message(format!("create temp dir: {e}")))?;
    let g = git2::Repository::init(&dir).map_err(|e| GitError::lib("init repository", e))?;
    drop(g);
    let repo = Repository::open(
        dir.to_str()
            .ok_or_else(|| GitError::Message("temp dir path is not UTF-8".to_string()))?,
    )?;
    Ok(TestRepo { repo, dir })
}

/// Append a commit to HEAD, writing `files` (path → contents) into a tree built
/// on top of the current HEAD tree (or empty for the first commit). Returns the
/// new commit's hash. `time_secs` fixes the timestamp for reproducibility.
///
/// # Errors
/// Propagates libgit2 errors at any step.
pub fn commit_files(
    test: &TestRepo,
    message: &str,
    time_secs: i64,
    files: &[(&str, &[u8])],
) -> Result<Hash> {
    let repo = test.repo.native();

    let parent_commit = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let base_tree = parent_commit.as_ref().and_then(|c| c.tree().ok());

    let mut builder = repo
        .treebuilder(base_tree.as_ref())
        .map_err(|e| GitError::lib("treebuilder", e))?;

    for (path, contents) in files {
        let oid = repo.blob(contents).map_err(|e| GitError::lib("write blob", e))?;
        builder
            .insert(path, oid, 0o100644)
            .map_err(|e| GitError::lib("tree insert", e))?;
    }

    let tree_oid = builder.write().map_err(|e| GitError::lib("tree write", e))?;
    let tree = repo.find_tree(tree_oid).map_err(|e| GitError::lib("find tree", e))?;
    let sig = fixed_sig(time_secs).map_err(|e| GitError::lib("signature", e))?;
    let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
    let commit_oid = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .map_err(|e| GitError::lib("commit", e))?;

    Ok(Hash::from_oid(commit_oid))
}

fn unique_temp_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("cf-gitlib-test-{pid}-{nanos}-{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::{tree_diff, ChangeAction};
    use crate::helpers::{load_commits, CommitLoadOptions, SystemClock};
    use crate::repository::LogOptions;

    #[test]
    fn build_and_walk_repo() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "first", 1_000, &[("a.txt", b"hello")]).expect("c1");
        let c2 = commit_files(&test, "second", 2_000, &[("b.txt", b"world")]).expect("c2");

        assert_eq!(test.repo.head().expect("head"), c2);

        let commit = test.repo.lookup_commit(c2).expect("lookup");
        assert_eq!(commit.message().trim_end(), "second");
        assert_eq!(commit.author().when_secs, 2_000);
        assert_eq!(commit.num_parents(), 1);
        assert_eq!(commit.parent_hash(0), c1);

        let mut iter = test.repo.log(&LogOptions::default()).expect("log");
        let collected = iter.collect_n(0).expect("collect");
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].hash(), c2);
        assert_eq!(collected[1].hash(), c1);
    }

    #[test]
    fn commit_count_matches_log() {
        let test = init_repo().expect("init");
        commit_files(&test, "1", 1, &[("f", b"1")]).unwrap();
        commit_files(&test, "2", 2, &[("f", b"2")]).unwrap();
        commit_files(&test, "3", 3, &[("f", b"3")]).unwrap();
        assert_eq!(test.repo.commit_count(&LogOptions::default()).unwrap(), 3);
    }

    #[test]
    fn load_commits_reversed_oldest_first() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("f", b"1")]).unwrap();
        let _c2 = commit_files(&test, "2", 2, &[("f", b"2")]).unwrap();
        let opts = CommitLoadOptions::default();
        let commits = load_commits(&test.repo, &opts, &SystemClock).expect("load");
        assert_eq!(commits.first().unwrap().hash(), c1);
    }

    #[test]
    fn diff_detects_added_file() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"a")]).unwrap();
        let c2 = commit_files(&test, "2", 2, &[("a.txt", b"a"), ("b.txt", b"b")]).unwrap();

        let t1 = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let t2 = test.repo.lookup_commit(c2).unwrap().tree().unwrap();

        let changes = tree_diff(&test.repo, Some(&t1), Some(&t2)).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, ChangeAction::Insert);
        assert_eq!(changes[0].to.name, "b.txt");
    }

    #[test]
    fn equal_trees_skip_diff() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"a")]).unwrap();
        let t1 = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let t1b = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let changes = tree_diff(&test.repo, Some(&t1), Some(&t1b)).unwrap();
        assert!(changes.is_empty());
    }

    #[test]
    fn blob_contents_roundtrip() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"hello world")]).unwrap();
        let tree = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let entry = tree.entry_by_path("a.txt").unwrap();
        let blob = test.repo.lookup_blob(entry.hash).unwrap();
        assert_eq!(blob.contents(), b"hello world");
        assert_eq!(blob.size(), 11);
    }

    #[test]
    fn commit_files_iter() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"a"), ("dir/b.txt", b"b")]).unwrap();
        let commit = test.repo.lookup_commit(c1).unwrap();
        let mut iter = commit.files().unwrap();
        let mut names: Vec<String> = Vec::new();
        while let Some(f) = iter.next_file() {
            names.push(f.name().to_string());
        }
        names.sort();
        assert_eq!(names, vec!["a.txt".to_string(), "dir/b.txt".to_string()]);
    }

    #[test]
    fn batch_fetch_all() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"aaa"), ("b.txt", b"bbbb")]).unwrap();
        let tree = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let ea = tree.entry_by_path("a.txt").unwrap();
        let eb = tree.entry_by_path("b.txt").unwrap();

        let batch = crate::batch::BlobBatch::with_defaults(&test.repo);
        let results = batch.fetch_all(&[ea.hash, eb.hash]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].contents.as_deref(), Some(&b"aaa"[..]));
        assert_eq!(results[1].contents.as_deref(), Some(&b"bbbb"[..]));
    }

    #[test]
    fn cached_blob_from_repo() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"l1\nl2\n")]).unwrap();
        let tree = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let entry = tree.entry_by_path("a.txt").unwrap();
        let cb = crate::cached_blob::CachedBlob::from_repo(&test.repo, entry.hash).unwrap();
        assert_eq!(cb.data, b"l1\nl2\n");
        assert_eq!(cb.size(), 6);
    }
}
