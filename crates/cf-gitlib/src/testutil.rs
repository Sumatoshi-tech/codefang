//! Test repository builders.
//!
//! Helpers for constructing throwaway git repositories with deterministic
//! commits, used by this crate's integration tests and available to downstream
//! crates' tests via the `testutil` feature. Commit timestamps are fixed so any
//! golden built on these repos is reproducible (DESIGN §2.8).
//!
//! [`CommitBuilder`] is the general fixture primitive: it fixes the author,
//! the (optionally skewed) committer time, the parents (side lines and
//! merges) and the tree edits — writes, deletes and pure renames. The free
//! functions ([`commit_files`], [`commit_files_as`], [`delete_files`],
//! [`rename_file`]) are thin wrappers over it for the common one-liners.

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

/// Name of the default fixture author.
pub const DEFAULT_AUTHOR_NAME: &str = "Test Author";
/// Email of the default fixture author.
pub const DEFAULT_AUTHOR_EMAIL: &str = "test@example.com";

/// Create an empty initialized repository in a fresh temp directory.
///
/// # Errors
/// Propagates filesystem and libgit2 init errors.
pub fn init_repo() -> Result<TestRepo> {
    let dir = unique_temp_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| GitError::Message(format!("create temp dir: {e}")))?;
    let g = git2::Repository::init(&dir).map_err(|e| GitError::lib("init repository", e))?;
    drop(g);
    let repo = Repository::open(
        dir.to_str()
            .ok_or_else(|| GitError::Message("temp dir path is not UTF-8".to_string()))?,
    )?;
    Ok(TestRepo { repo, dir })
}

/// A deterministic, fully specified commit under construction: author,
/// committer time, parents, and the tree edits (writes, deletes, renames)
/// applied on top of the base tree.
///
/// Every field defaults to the plain [`commit_files`] behaviour: the default
/// author, committer time == author time, HEAD as the base/first parent, and
/// HEAD moved to the new commit.
pub struct CommitBuilder<'a> {
    test: &'a TestRepo,
    message: String,
    author_name: String,
    author_email: String,
    author_time: i64,
    committer_time: Option<i64>,
    base: Option<Hash>,
    extra_parents: Vec<Hash>,
    update_head: bool,
    writes: Vec<(String, Vec<u8>)>,
    deletes: Vec<String>,
    renames: Vec<(String, String)>,
}

impl<'a> CommitBuilder<'a> {
    /// Starts a commit with `message` at `time_secs` (author and committer
    /// time) by the default fixture author, on top of HEAD.
    #[must_use]
    pub fn new(test: &'a TestRepo, message: &str, time_secs: i64) -> Self {
        CommitBuilder {
            test,
            message: message.to_string(),
            author_name: DEFAULT_AUTHOR_NAME.to_string(),
            author_email: DEFAULT_AUTHOR_EMAIL.to_string(),
            author_time: time_secs,
            committer_time: None,
            base: None,
            extra_parents: Vec::new(),
            update_head: true,
            writes: Vec::new(),
            deletes: Vec::new(),
            renames: Vec::new(),
        }
    }

    /// Sets the author (and committer) name and email.
    #[must_use]
    pub fn author(mut self, name: &str, email: &str) -> Self {
        self.author_name = name.to_string();
        self.author_email = email.to_string();
        self
    }

    /// Sets a committer time different from the author time (skewed dates).
    #[must_use]
    pub fn committer_time(mut self, secs: i64) -> Self {
        self.committer_time = Some(secs);
        self
    }

    /// Uses `base` (instead of HEAD) as the tree base and first parent.
    #[must_use]
    pub fn parent(mut self, base: Hash) -> Self {
        self.base = Some(base);
        self
    }

    /// Adds a further parent, making this a merge commit.
    #[must_use]
    pub fn merge_parent(mut self, parent: Hash) -> Self {
        self.extra_parents.push(parent);
        self
    }

    /// Leaves HEAD where it is (a commit on a side line).
    #[must_use]
    pub fn detached(mut self) -> Self {
        self.update_head = false;
        self
    }

    /// Writes (or overwrites) `path` with `contents`.
    #[must_use]
    pub fn write(mut self, path: &str, contents: &[u8]) -> Self {
        self.writes.push((path.to_string(), contents.to_vec()));
        self
    }

    /// Deletes `path` from the tree.
    #[must_use]
    pub fn delete(mut self, path: &str) -> Self {
        self.deletes.push(path.to_string());
        self
    }

    /// Moves `from` to `to`, keeping the blob (a pure `git mv`).
    #[must_use]
    pub fn rename(mut self, from: &str, to: &str) -> Self {
        self.renames.push((from.to_string(), to.to_string()));
        self
    }

    /// Writes the commit and returns its hash.
    ///
    /// # Errors
    /// Propagates libgit2 errors at any step, and a message error when a
    /// rename source is absent from the base tree.
    pub fn commit(self) -> Result<Hash> {
        let repo = self.test.repo.native();
        let base_commit = match self.base {
            Some(h) => Some(
                repo.find_commit(h.to_oid())
                    .map_err(|e| GitError::lib("find base commit", e))?,
            ),
            None => repo
                .head()
                .ok()
                .and_then(|h| h.target())
                .and_then(|oid| repo.find_commit(oid).ok()),
        };
        let base_tree = base_commit.as_ref().and_then(|c| c.tree().ok());

        // Build the new tree via an in-memory index so nested paths (e.g.
        // `dir/b.txt`) are materialized as subtrees; a flat treebuilder rejects
        // entry names containing `/`.
        let mut index = git2::Index::new().map_err(|e| GitError::lib("new index", e))?;
        if let Some(tree) = base_tree.as_ref() {
            index
                .read_tree(tree)
                .map_err(|e| GitError::lib("index read_tree", e))?;
        }
        for (from, to) in &self.renames {
            let entry = base_tree
                .as_ref()
                .and_then(|t| t.get_path(std::path::Path::new(from)).ok())
                .ok_or_else(|| GitError::Message(format!("rename source {from} not in tree")))?;
            add_blob_entry(&mut index, to, entry.id(), 0)?;
            index
                .remove_path(std::path::Path::new(from))
                .map_err(|e| GitError::lib("index remove_path", e))?;
        }
        for path in &self.deletes {
            index
                .remove_path(std::path::Path::new(path))
                .map_err(|e| GitError::lib("index remove_path", e))?;
        }
        for (path, contents) in &self.writes {
            let oid = repo
                .blob(contents)
                .map_err(|e| GitError::lib("write blob", e))?;
            add_blob_entry(&mut index, path, oid, contents.len())?;
        }

        let tree_oid = index
            .write_tree_to(repo)
            .map_err(|e| GitError::lib("tree write", e))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| GitError::lib("find tree", e))?;
        let author = signature(&self.author_name, &self.author_email, self.author_time)?;
        let committer = signature(
            &self.author_name,
            &self.author_email,
            self.committer_time.unwrap_or(self.author_time),
        )?;
        let mut parents: Vec<git2::Commit> = base_commit.into_iter().collect();
        for extra in &self.extra_parents {
            parents.push(
                repo.find_commit(extra.to_oid())
                    .map_err(|e| GitError::lib("find merge parent", e))?,
            );
        }
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        let update_ref = if self.update_head { Some("HEAD") } else { None };
        let commit_oid = repo
            .commit(
                update_ref,
                &author,
                &committer,
                &self.message,
                &tree,
                &parent_refs,
            )
            .map_err(|e| GitError::lib("commit", e))?;
        Ok(Hash::from_oid(&commit_oid))
    }
}

/// A fixed-time signature for test commits.
fn signature<'a>(name: &str, email: &str, secs: i64) -> Result<git2::Signature<'a>> {
    let when = git2::Time::new(secs, 0);
    git2::Signature::new(name, email, &when).map_err(|e| GitError::lib("signature", e))
}

/// Adds a regular-file blob entry to an in-memory index.
fn add_blob_entry(index: &mut git2::Index, path: &str, oid: git2::Oid, size: usize) -> Result<()> {
    let entry = git2::IndexEntry {
        ctime: git2::IndexTime::new(0, 0),
        mtime: git2::IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100_644,
        uid: 0,
        gid: 0,
        file_size: size as u32,
        id: oid,
        flags: 0,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    };
    index.add(&entry).map_err(|e| GitError::lib("index add", e))
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
    commit_files_as(
        test,
        DEFAULT_AUTHOR_NAME,
        DEFAULT_AUTHOR_EMAIL,
        message,
        time_secs,
        files,
    )
}

/// [`commit_files`] with an explicit author `name` / `email`.
///
/// # Errors
/// Propagates libgit2 errors at any step.
pub fn commit_files_as(
    test: &TestRepo,
    name: &str,
    email: &str,
    message: &str,
    time_secs: i64,
    files: &[(&str, &[u8])],
) -> Result<Hash> {
    let mut b = CommitBuilder::new(test, message, time_secs).author(name, email);
    for (path, contents) in files {
        b = b.write(path, contents);
    }
    b.commit()
}

/// Append a commit to HEAD that deletes `paths`.
///
/// # Errors
/// Propagates libgit2 errors at any step.
pub fn delete_files(
    test: &TestRepo,
    message: &str,
    time_secs: i64,
    paths: &[&str],
) -> Result<Hash> {
    let mut b = CommitBuilder::new(test, message, time_secs);
    for path in paths {
        b = b.delete(path);
    }
    b.commit()
}

/// Append a commit to HEAD that moves `from` to `to` without changing the blob
/// (a pure `git mv`).
///
/// # Errors
/// Propagates libgit2 errors at any step.
pub fn rename_file(
    test: &TestRepo,
    message: &str,
    time_secs: i64,
    from: &str,
    to: &str,
) -> Result<Hash> {
    CommitBuilder::new(test, message, time_secs)
        .rename(from, to)
        .commit()
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
    use crate::helpers::{load_commits, CommitLoadOptions};
    use crate::repository::LogOptions;

    /// A 100-line blob, large enough for libgit2's rename similarity to
    /// recognise an unchanged copy under a new name.
    fn hundred_lines() -> Vec<u8> {
        (1..=100)
            .map(|i| format!("line {i}\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn build_and_walk_repo() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "first", 1_000, &[("a.txt", b"hello")]).expect("c1");
        let c2 = commit_files(&test, "second", 2_000, &[("b.txt", b"world")]).expect("c2");

        assert_eq!(test.repo.head().expect("head"), c2);

        let commit = test.repo.lookup_commit(c2).expect("lookup");
        assert_eq!(commit.message().trim_end(), "second");
        assert_eq!(commit.author().when.seconds(), 2_000);
        assert_eq!(commit.num_parents(), 1);
        assert_eq!(commit.parent_hash(0), c1);

        let mut iter = test.repo.log(&LogOptions::default()).expect("log");
        let mut collected: Vec<_> = Vec::new();
        while let Some(c) = iter.next_commit() {
            collected.push(c);
        }
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
        let commits = load_commits(&test.repo, &opts).expect("load");
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

    /// `delete_files` removes the path from the tree, and the diff reports it
    /// as a Delete carrying the FROM side.
    #[test]
    fn diff_detects_deleted_file() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1, &[("a.txt", b"a"), ("b.txt", b"b")]).unwrap();
        let c2 = delete_files(&test, "2", 2, &["b.txt"]).unwrap();

        let t1 = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let t2 = test.repo.lookup_commit(c2).unwrap().tree().unwrap();
        let changes = tree_diff(&test.repo, Some(&t1), Some(&t2)).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, ChangeAction::Delete);
        assert_eq!(changes[0].from.name, "b.txt");
        assert!(changes[0].to.name.is_empty());
    }

    /// A pure `git mv` is ONE Modify whose from/to names differ and whose blob
    /// hash is unchanged — not a Delete + Insert pair (rename detection).
    #[test]
    fn pure_rename_is_one_modify_with_different_names() {
        let test = init_repo().expect("init");
        let body = hundred_lines();
        let c1 = commit_files(&test, "1", 1, &[("a.txt", &body)]).unwrap();
        let c2 = rename_file(&test, "mv", 2, "a.txt", "b.txt").unwrap();

        let t1 = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let t2 = test.repo.lookup_commit(c2).unwrap().tree().unwrap();
        let changes = tree_diff(&test.repo, Some(&t1), Some(&t2)).unwrap();
        assert_eq!(
            changes.len(),
            1,
            "a rename must be one change, got {changes:?}"
        );
        let c = &changes[0];
        assert_eq!(c.action, ChangeAction::Modify);
        assert_eq!(c.from.name, "a.txt");
        assert_eq!(c.to.name, "b.txt");
        assert_eq!(c.from.hash, c.to.hash, "a pure rename keeps the blob");
    }

    /// A rename whose content also changed (above the similarity threshold)
    /// is still one Modify with differing names.
    #[test]
    fn edited_rename_is_still_one_modify() {
        let test = init_repo().expect("init");
        let body = hundred_lines();
        let c1 = commit_files(&test, "1", 1, &[("a.txt", &body)]).unwrap();
        let mut edited = body.clone();
        edited.extend_from_slice(b"line 101\n");
        let c2 = CommitBuilder::new(&test, "mv+edit", 2)
            .delete("a.txt")
            .write("b.txt", &edited)
            .commit()
            .unwrap();

        let t1 = test.repo.lookup_commit(c1).unwrap().tree().unwrap();
        let t2 = test.repo.lookup_commit(c2).unwrap().tree().unwrap();
        let changes = tree_diff(&test.repo, Some(&t1), Some(&t2)).unwrap();
        assert_eq!(changes.len(), 1, "got {changes:?}");
        assert_eq!(changes[0].action, ChangeAction::Modify);
        assert_eq!(changes[0].from.name, "a.txt");
        assert_eq!(changes[0].to.name, "b.txt");
        assert_ne!(changes[0].from.hash, changes[0].to.hash);
    }

    /// The builder records distinct authors and skewed committer times, and a
    /// side line + merge yields a two-parent commit on HEAD.
    #[test]
    fn builder_authors_dates_and_merges() {
        let test = init_repo().expect("init");
        let c1 = commit_files(&test, "1", 1_000, &[("a.txt", b"a")]).unwrap();
        let side = CommitBuilder::new(&test, "side", 2_000)
            .author("Bob", "bob@example.com")
            .committer_time(5_000)
            .parent(c1)
            .detached()
            .write("b.txt", b"b")
            .commit()
            .unwrap();
        assert_eq!(test.repo.head().unwrap(), c1, "detached must not move HEAD");
        let side_commit = test.repo.lookup_commit(side).unwrap();
        assert_eq!(side_commit.author().name, "Bob");
        assert_eq!(side_commit.author().when.seconds(), 2_000);
        assert_eq!(side_commit.committer().when.seconds(), 5_000);

        let merge = CommitBuilder::new(&test, "merge", 3_000)
            .merge_parent(side)
            .write("b.txt", b"b")
            .commit()
            .unwrap();
        let m = test.repo.lookup_commit(merge).unwrap();
        assert_eq!(m.num_parents(), 2);
        assert_eq!(m.parent_hash(0), c1);
        assert_eq!(m.parent_hash(1), side);
        assert_eq!(test.repo.head().unwrap(), merge);
    }

    /// `--since` is an AUTHOR-time filter and must be a SKIP filter: a commit
    /// authored after the cutoff but committed before it (so it sorts below
    /// younger commits in the committer-time walk) is still included, and a
    /// commit authored before the cutoff but committed after it is dropped
    /// without ending the walk.
    #[test]
    fn since_filter_skips_instead_of_stopping_on_skewed_dates() {
        let test = init_repo().expect("init");
        const CUTOFF: i64 = 10_000;
        // A: authored AFTER the cutoff, committed BEFORE it.
        let a = CommitBuilder::new(&test, "a", CUTOFF + 100)
            .committer_time(CUTOFF - 900)
            .write("a.txt", b"a")
            .commit()
            .unwrap();
        // B: authored BEFORE the cutoff, committed AFTER it.
        let _b = CommitBuilder::new(&test, "b", CUTOFF - 100)
            .committer_time(CUTOFF + 100)
            .write("b.txt", b"b")
            .commit()
            .unwrap();
        // C: both after.
        let c = commit_files(&test, "c", CUTOFF + 500, &[("c.txt", b"c")]).unwrap();

        let mut iter = test
            .repo
            .log(&LogOptions {
                since: Some(crate::repository::time_from_unix_secs(CUTOFF)),
                ..LogOptions::default()
            })
            .unwrap();
        let mut got = Vec::new();
        while let Some(commit) = iter.next_commit() {
            got.push(commit.hash());
        }
        assert_eq!(
            got,
            vec![c, a],
            "every commit with author time >= cutoff must be kept, newest-first"
        );
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
        let blob = test.repo.lookup_blob(entry.hash()).unwrap();
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
            names.push(f.name.to_string());
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
        let results = batch.fetch_all(&[ea.hash(), eb.hash()]);
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
        let cb = crate::blob::CachedBlob::from_repo(&test.repo, entry.hash()).unwrap();
        assert_eq!(cb.data, b"l1\nl2\n");
        assert_eq!(cb.size(), 6);
    }
}
