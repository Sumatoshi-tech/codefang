//! Test fixtures: the [`TestCommit`] / [`test_signature`] mocks and the
//! [`TestRepo`] builder.
//!
//! [`TestRepo`] builds a hermetic on-disk git repository in a temp directory
//! with deterministic author/committer signatures (init repo, stage all,
//! create commits, branches, merges). It is compiled only under `cfg(test)`.

use std::path::PathBuf;

use crate::hash::Hash;

/// Default deterministic author/committer for fixture commits.
const TEST_NAME: &str = "Test User";
const TEST_EMAIL: &str = "test@example.com";

/// A hermetic on-disk test repository.
///
/// Creates a fresh repository in a unique temp directory on construction and
/// removes it on [`Drop`]. Commit timestamps advance by one second per commit so
/// history ordering is deterministic and `--since` filtering is testable, while
/// still being reproducible run-to-run.
pub struct TestRepo {
    repo: git2::Repository,
    path: PathBuf,
    /// Monotonic commit time seed (epoch seconds), advanced per commit.
    next_time: std::cell::Cell<i64>,
}

impl TestRepo {
    /// Initializes a new non-bare test repository in a fresh temp directory.
    ///
    /// # Panics
    ///
    /// Panics if the temp directory or repository cannot be created (a test-only
    /// helper, so failure is a test bug).
    #[must_use]
    pub fn new() -> Self {
        let dir = unique_temp_dir();
        std::fs::create_dir_all(&dir).expect("create temp repo dir");
        let repo = git2::Repository::init(&dir).expect("init repository");
        TestRepo {
            repo,
            path: dir,
            // A fixed, deterministic base instant (2021-01-01T00:00:00Z).
            next_time: std::cell::Cell::new(1_609_459_200),
        }
    }

    /// Returns the repository working-directory path as a string.
    #[must_use]
    pub fn path(&self) -> &str {
        self.path.to_str().expect("utf-8 temp path")
    }

    /// Creates (or overwrites) a working-tree file, making parent dirs as needed.
    ///
    /// # Panics
    ///
    /// Panics on any filesystem error (test-only helper).
    pub fn create_file(&self, name: &str, content: &str) {
        let path = self.path.join(name);
        if let Some(parent) = path.parent() {
            if parent != self.path {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
        }
        std::fs::write(&path, content).expect("write file");
    }

    /// Removes a working-tree file.
    ///
    /// # Panics
    ///
    /// Panics if the file cannot be removed (test-only helper).
    pub fn delete_file(&self, name: &str) {
        std::fs::remove_file(self.path.join(name)).expect("remove file");
    }

    /// Stages all files and creates a commit on HEAD.
    ///
    /// Returns the new commit hash. Uses the current HEAD (if any) as the single
    /// parent.
    ///
    /// # Panics
    ///
    /// Panics on any libgit2 error (test-only helper).
    pub fn commit(&self, message: &str) -> Hash {
        let tree = self.stage_all();
        let sig = self.signature();

        let parent_commit = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| self.repo.find_commit(oid).ok());
        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .expect("create commit");
        Hash::from_oid(&oid)
    }

    /// Creates a commit on an arbitrary ref without moving HEAD.
    ///
    /// # Panics
    ///
    /// Panics on any libgit2 error (test-only helper).
    pub fn commit_to_ref(&self, ref_name: &str, message: &str, parent: Hash) -> Hash {
        let tree = self.stage_all();
        let sig = self.signature();

        let parent_commit = if parent.is_zero() {
            None
        } else {
            Some(self.repo.find_commit(parent.to_oid()).expect("find parent"))
        };
        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        let oid = self
            .repo
            .commit(Some(ref_name), &sig, &sig, message, &tree, &parents)
            .expect("create commit on ref");
        Hash::from_oid(&oid)
    }

    /// Creates a merge commit with two parents.
    ///
    /// The merge reuses the first parent's tree, with `first_parent` as the
    /// main line.
    ///
    /// # Panics
    ///
    /// Panics on any libgit2 error (test-only helper).
    pub fn create_merge_commit(&self, message: &str, first_parent: Hash, second_parent: Hash) -> Hash {
        let p1 = self
            .repo
            .find_commit(first_parent.to_oid())
            .expect("find first parent");
        let p2 = self
            .repo
            .find_commit(second_parent.to_oid())
            .expect("find second parent");
        let tree = p1.tree().expect("first parent tree");
        let sig = self.signature();
        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&p1, &p2])
            .expect("create merge commit");
        Hash::from_oid(&oid)
    }

    /// Stages every working-tree file and writes the index tree.
    fn stage_all(&self) -> git2::Tree<'_> {
        let mut index = self.repo.index().expect("open index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("index add_all");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        self.repo.find_tree(tree_id).expect("find tree")
    }

    /// Builds a deterministic signature with a per-commit advancing timestamp.
    fn signature(&self) -> git2::Signature<'static> {
        let t = self.next_time.get();
        self.next_time.set(t + 1);
        git2::Signature::new(TEST_NAME, TEST_EMAIL, &git2::Time::new(t, 0))
            .expect("build signature")
    }
}

impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        // Best-effort cleanup of the temp directory (mirrors t.TempDir cleanup).
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Builds a unique temp-directory path for a fresh fixture repository.
fn unique_temp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut base = std::env::temp_dir();
    base.push(format!("cf-gitlib-test-{pid}-{nanos}-{n}"));
    base
}

/// A mock commit for unit tests that need no real git objects.
///
/// Carries a hash, author/committer (committer defaults to author), message,
/// and parent hashes. Structural lookups (`parent`, `tree`, `files`, `file`)
/// are not implemented; the mock simply exposes its stored scalar fields.
#[derive(Debug, Clone)]
pub struct TestCommit {
    hash: Hash,
    author: crate::Signature,
    committer: crate::Signature,
    message: String,
    parent_hashes: Vec<Hash>,
}

impl TestCommit {
    /// Creates a mock commit.
    #[must_use]
    pub fn new(hash: Hash, author: crate::Signature, message: &str, parent_hashes: Vec<Hash>) -> Self {
        TestCommit {
            hash,
            committer: author.clone(),
            author,
            message: message.to_string(),
            parent_hashes,
        }
    }

    /// Returns the commit hash.
    #[must_use]
    pub fn hash(&self) -> Hash {
        self.hash
    }
    /// Returns the author.
    #[must_use]
    pub fn author(&self) -> &crate::Signature {
        &self.author
    }
    /// Returns the committer (defaults to the author).
    #[must_use]
    pub fn committer(&self) -> &crate::Signature {
        &self.committer
    }
    /// Returns the commit message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Returns the number of parents.
    #[must_use]
    pub fn num_parents(&self) -> usize {
        self.parent_hashes.len()
    }
}

/// Builds a [`crate::Signature`] for testing with `when = now`.
#[must_use]
pub fn test_signature(name: &str, email: &str) -> crate::Signature {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    crate::Signature {
        name: name.to_string(),
        email: email.to_string(),
        when: git2::Time::new(now, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors reference test TestNewTestCommit.
    #[test]
    fn new_test_commit() {
        let hash = Hash::new("abcdef1234567890abcdef1234567890abcdef12");
        let author = test_signature("Test Author", "test@example.com");
        let p1 = Hash::new("1111111111111111111111111111111111111111");
        let p2 = Hash::new("2222222222222222222222222222222222222222");
        let c = TestCommit::new(hash, author.clone(), "test message", vec![p1, p2]);

        assert_eq!(c.hash(), hash);
        assert_eq!(c.author(), &author);
        assert_eq!(c.committer(), &author); // committer defaults to author
        assert_eq!(c.message(), "test message");
        assert_eq!(c.num_parents(), 2);
    }

    // Mirrors reference test TestTestSignature.
    #[test]
    fn test_signature_fields() {
        let sig = test_signature("John Doe", "john@example.com");
        assert_eq!(sig.name, "John Doe");
        assert_eq!(sig.email, "john@example.com");
        assert_ne!(sig.when.seconds(), 0);
    }
}
