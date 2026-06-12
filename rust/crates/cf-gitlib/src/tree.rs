//! Tree and tree-entry access.
//!
//! [`Tree`] borrows a libgit2 [`git2::Tree`] (freed on [`Drop`]); [`TreeEntry`]
//! borrows a single entry.

use crate::error::{GitError, Result};
use crate::file::FileIter;
use crate::hash::Hash;
use crate::Repository;

/// A libgit2 tree.
///
/// Holds a reference to its owning [`Repository`] so it can look up
/// sub-objects, and is freed on [`Drop`].
pub struct Tree<'repo> {
    tree: git2::Tree<'repo>,
    repo: &'repo Repository,
}

impl<'repo> Tree<'repo> {
    /// Wraps a libgit2 tree with its owning repository.
    pub(crate) fn new(tree: git2::Tree<'repo>, repo: &'repo Repository) -> Self {
        Tree { tree, repo }
    }

    /// Returns the tree hash.
    #[must_use]
    pub fn hash(&self) -> Hash {
        Hash::from_oid(&self.tree.id())
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn entry_count(&self) -> u64 {
        self.tree.len() as u64
    }

    /// Returns the entry at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn entry_by_index(&self, index: u64) -> Option<TreeEntry<'_>> {
        self.tree
            .get(index as usize)
            .map(|entry| TreeEntry { entry })
    }

    /// Returns the entry at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::EntryByPath`] when the path is not in the tree.
    pub fn entry_by_path(&self, path: &str) -> Result<TreeEntry<'static>> {
        let entry = self
            .tree
            .get_path(std::path::Path::new(path))
            .map_err(GitError::EntryByPath)?;
        Ok(TreeEntry { entry })
    }

    /// Returns an iterator over all blob files in the tree.
    ///
    /// A failed walk yields an empty iterator rather than an error.
    #[must_use]
    pub fn files(&self) -> FileIter<'repo> {
        match crate::changes::tree_files(self.repo, self) {
            Ok(files) => FileIter::new(files),
            Err(_) => FileIter::new(Vec::new()),
        }
    }

    /// Returns the underlying libgit2 tree.
    #[must_use]
    pub fn native(&self) -> &git2::Tree<'repo> {
        &self.tree
    }
}

/// A single tree entry.
///
/// `git2::TreeEntry<'static>` is the owned form (from `get_path`); `'tree`-bound
/// entries come from `get`.
pub struct TreeEntry<'a> {
    entry: git2::TreeEntry<'a>,
}

impl TreeEntry<'_> {
    /// Returns the entry name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.entry.name().unwrap_or_default()
    }

    /// Returns the entry object hash.
    #[must_use]
    pub fn hash(&self) -> Hash {
        Hash::from_oid(&self.entry.id())
    }

    /// Returns the entry object type.
    #[must_use]
    pub fn object_type(&self) -> Option<git2::ObjectType> {
        self.entry.kind()
    }

    /// Reports whether the entry is a blob.
    #[must_use]
    pub fn is_blob(&self) -> bool {
        self.entry.kind() == Some(git2::ObjectType::Blob)
    }
}

/// Walks a tree recursively, invoking `cb(path, entry)` for every entry.
///
/// Descends only into tree entries, silently skips sub-trees that fail to look
/// up, and joins path segments with `/` (reference-implementation traversal
/// order: the resulting file order feeds analyzer output).
///
/// # Errors
///
/// Propagates the first error returned by `cb`.
pub(crate) fn walk_tree<F>(
    repo: &Repository,
    tree: &Tree<'_>,
    prefix: &str,
    cb: &mut F,
) -> Result<()>
where
    F: FnMut(&str, &TreeEntry<'_>) -> Result<()>,
{
    let count = tree.entry_count();
    for i in 0..count {
        let Some(entry) = tree.entry_by_index(i) else {
            continue;
        };
        process_tree_entry(repo, &entry, prefix, cb)?;
    }
    Ok(())
}

/// Handles one tree entry: calls `cb` for blobs, recurses for sub-trees.
fn process_tree_entry<F>(
    repo: &Repository,
    entry: &TreeEntry<'_>,
    prefix: &str,
    cb: &mut F,
) -> Result<()>
where
    F: FnMut(&str, &TreeEntry<'_>) -> Result<()>,
{
    let path = if prefix.is_empty() {
        entry.name().to_string()
    } else {
        format!("{prefix}/{}", entry.name())
    };

    if entry.is_blob() {
        return cb(&path, entry);
    }

    if entry.object_type() != Some(git2::ObjectType::Tree) {
        return Ok(());
    }

    // Skip sub-trees we cannot look up.
    let Ok(subtree) = repo.lookup_tree(entry.hash()) else {
        return Ok(());
    };

    walk_tree(repo, &subtree, &path, cb)
}
