//! Tree change computation, ported from `pkg/gitlib/changes.go`.
//!
//! [`tree_diff`] computes file-level changes between two trees; [`initial_tree_changes`]
//! treats every file in a tree as an insertion (for a root commit); [`tree_files`]
//! materializes all blob files in a tree. The [`Change`] / [`ChangeEntry`] /
//! [`ChangeAction`] types mirror the Go structs field-for-field.

use crate::error::Result;
use crate::file::File;
use crate::hash::Hash;
use crate::tree::{walk_tree, Tree, TreeEntry};
use crate::Repository;

/// The kind of change a [`Change`] represents (Go `ChangeAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeAction {
    /// A new file was added (Go `Insert`).
    Insert,
    /// A file was removed (Go `Delete`).
    Delete,
    /// A file was modified (Go `Modify`).
    Modify,
}

/// One side (old or new) of a change (Go `ChangeEntry`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeEntry {
    /// File path.
    pub name: String,
    /// Object hash.
    pub hash: Hash,
    /// File size in bytes.
    pub size: i64,
    /// File mode.
    pub mode: u16,
}

/// A single file change between two trees (Go `Change`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The kind of change.
    pub action: ChangeAction,
    /// The old-side entry (empty for inserts).
    pub from: ChangeEntry,
    /// The new-side entry (empty for deletes).
    pub to: ChangeEntry,
}

/// A collection of [`Change`]s (Go `Changes`).
pub type Changes = Vec<Change>;

/// Computes the changes between two trees (Go `TreeDiff`).
///
/// Skips the libgit2 diff entirely when both tree OIDs are equal (e.g.
/// metadata-only commits), returning an empty change set — matching Go exactly.
/// Renames and copies are surfaced as [`ChangeAction::Modify`] (as in Go);
/// unmodified / ignored / untracked / type-change / unreadable / conflicted
/// deltas are skipped.
///
/// # Errors
///
/// Returns a diff error if the libgit2 tree diff fails.
pub fn tree_diff(
    repo: &Repository,
    old_tree: Option<&Tree<'_>>,
    new_tree: Option<&Tree<'_>>,
) -> Result<Changes> {
    if let (Some(old), Some(new)) = (old_tree, new_tree) {
        if old.hash() == new.hash() {
            return Ok(Vec::new());
        }
    }

    let diff = repo.diff_tree_to_tree(old_tree, new_tree)?;
    let num_deltas = diff.num_deltas()?;
    let mut changes = Changes::with_capacity(num_deltas);

    for i in 0..num_deltas {
        let Ok(delta) = diff.delta(i) else {
            continue;
        };

        let change = match delta.status {
            git2::Delta::Added => Change {
                action: ChangeAction::Insert,
                from: ChangeEntry::default(),
                to: ChangeEntry {
                    name: delta.new_file.path,
                    hash: delta.new_file.hash,
                    size: delta.new_file.size,
                    mode: 0,
                },
            },
            git2::Delta::Deleted => Change {
                action: ChangeAction::Delete,
                from: ChangeEntry {
                    name: delta.old_file.path,
                    hash: delta.old_file.hash,
                    size: delta.old_file.size,
                    mode: 0,
                },
                to: ChangeEntry::default(),
            },
            git2::Delta::Modified | git2::Delta::Renamed | git2::Delta::Copied => Change {
                action: ChangeAction::Modify,
                from: ChangeEntry {
                    name: delta.old_file.path,
                    hash: delta.old_file.hash,
                    size: delta.old_file.size,
                    mode: 0,
                },
                to: ChangeEntry {
                    name: delta.new_file.path,
                    hash: delta.new_file.hash,
                    size: delta.new_file.size,
                    mode: 0,
                },
            },
            // Unmodified, Ignored, Untracked, Typechange, Unreadable, Conflicted.
            _ => continue,
        };

        changes.push(change);
    }

    Ok(changes)
}

/// Creates changes for an initial commit: every blob file is an insertion
/// (Go `InitialTreeChanges`).
///
/// # Errors
///
/// Propagates tree-walk errors.
pub fn initial_tree_changes(repo: &Repository, tree: Option<&Tree<'_>>) -> Result<Changes> {
    let Some(tree) = tree else {
        return Ok(Vec::new());
    };

    let mut changes = Changes::new();
    let mut cb = |path: &str, entry: &TreeEntry<'_>| -> Result<()> {
        if !entry.is_blob() {
            return Ok(());
        }
        changes.push(Change {
            action: ChangeAction::Insert,
            from: ChangeEntry::default(),
            to: ChangeEntry {
                name: path.to_string(),
                hash: entry.hash(),
                size: 0,
                mode: 0,
            },
        });
        Ok(())
    };
    walk_tree(repo, tree, "", &mut cb)?;
    Ok(changes)
}

/// Returns all blob files in a tree (Go `TreeFiles`).
///
/// # Errors
///
/// Propagates tree-walk errors.
pub(crate) fn tree_files<'repo>(
    repo: &'repo Repository,
    tree: &Tree<'_>,
) -> Result<Vec<File<'repo>>> {
    let mut files: Vec<File<'repo>> = Vec::new();
    let mut cb = |path: &str, entry: &TreeEntry<'_>| -> Result<()> {
        files.push(File::new(path.to_string(), entry.hash(), 0, repo));
        Ok(())
    };
    walk_tree(repo, tree, "", &mut cb)?;
    Ok(files)
}

/// Public wrapper matching Go's exported `TreeFiles(repo, tree) ([]*File, error)`.
///
/// # Errors
///
/// Propagates tree-walk errors.
pub fn tree_files_public<'repo>(
    repo: &'repo Repository,
    tree: &Tree<'_>,
) -> Result<Vec<File<'repo>>> {
    tree_files(repo, tree)
}

// Keep GitError referenced in the public error surface for this module.
#[allow(dead_code)]
fn _err_marker() -> Option<GitError> {
    None
}
