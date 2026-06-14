//! `TreeDiff` provider.
//!
//! Produces the `"changes"` fact: the list of file changes between the current
//! commit's tree and the previously seen commit's tree. For the first commit
//! every file is reported as an insert.

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::git_model::{Change, ChangeEntry, Changes, Hash};

/// Source of trees and tree diffs, decoupling the provider from libgit2.
///
/// Modelling these as a trait keeps the diff state machine (previous-tree
/// tracking, first-commit handling) testable without a real repository and
/// avoids storing non-`Send` libgit2 handles in the provider.
pub trait TreeSource {
    /// The tree id of a commit, used as the previous-tree key between commits.
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when the commit or its tree cannot be
    /// resolved.
    fn commit_tree(&self, commit_hash: Hash) -> Result<Hash, AnalyzerError>;

    /// Diff two trees into [`Changes`].
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when either tree cannot be loaded or the
    /// diff fails.
    fn diff_trees(&self, old_tree: Hash, new_tree: Hash) -> Result<Changes, AnalyzerError>;

    /// Every file in a tree as an insert change (the first-commit case).
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when the tree cannot be loaded or walked.
    fn tree_inserts(&self, tree: Hash) -> Result<Changes, AnalyzerError>;
}

/// `TreeDiff` provider.
///
/// The dependency map carries the commit hash under `"commit_hash"`; the
/// tree resolution is delegated to the [`TreeSource`].
pub struct TreeDiff<S: TreeSource> {
    source: S,
    previous_tree: Option<Hash>,
}

impl<S: TreeSource> TreeDiff<S> {
    /// Construct a `TreeDiff` over the given tree source (starts with no
    /// previous tree).
    pub const fn new(source: S) -> Self {
        Self {
            source,
            previous_tree: None,
        }
    }

    /// Compute the changes for a commit, advancing the previous-tree state:
    /// diff against the previous tree if present, otherwise treat every file
    /// as an insert; then remember the current tree.
    ///
    /// # Errors
    ///
    /// Propagates [`TreeSource`] failures.
    pub fn changes_for(&mut self, commit_hash: Hash) -> Result<Changes, AnalyzerError> {
        let tree = self.source.commit_tree(commit_hash)?;
        let diff = match self.previous_tree {
            Some(prev) => self.source.diff_trees(prev, tree)?,
            None => self.source.tree_inserts(tree)?,
        };
        self.previous_tree = Some(tree);
        Ok(diff)
    }
}

impl<S: TreeSource> Analyzer for TreeDiff<S> {
    fn name(&self) -> &'static str {
        "TreeDiff"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["changes"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec![]
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let commit_hash = *dep::<Hash>(deps, "commit_hash")?;
        let changes = self.changes_for(commit_hash)?;
        let mut out = ValueMap::new();
        out.insert("changes".to_string(), Box::new(changes));
        Ok(out)
    }
}

/// A [`TreeSource`] backed by a libgit2 repository.
pub struct GitTreeSource<'r> {
    repo: &'r git2::Repository,
}

impl<'r> GitTreeSource<'r> {
    /// Wrap a borrowed repository as a tree source.
    #[must_use]
    pub const fn new(repo: &'r git2::Repository) -> Self {
        Self { repo }
    }

    fn tree_of(&self, commit_hash: Hash) -> Result<git2::Tree<'r>, AnalyzerError> {
        let oid = git2::Oid::from_bytes(&commit_hash.0)
            .map_err(|e| AnalyzerError::Git(e.to_string()))?;
        let commit = self.repo.find_commit(oid)?;
        Ok(commit.tree()?)
    }
}

impl TreeSource for GitTreeSource<'_> {
    fn commit_tree(&self, commit_hash: Hash) -> Result<Hash, AnalyzerError> {
        Ok(self.tree_of(commit_hash)?.id().into())
    }

    fn diff_trees(&self, old_tree: Hash, new_tree: Hash) -> Result<Changes, AnalyzerError> {
        let old_oid =
            git2::Oid::from_bytes(&old_tree.0).map_err(|e| AnalyzerError::Git(e.to_string()))?;
        let new_oid =
            git2::Oid::from_bytes(&new_tree.0).map_err(|e| AnalyzerError::Git(e.to_string()))?;
        let old = self.repo.find_tree(old_oid)?;
        let new = self.repo.find_tree(new_oid)?;
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&old), Some(&new), None)?;

        let mut changes: Changes = Vec::new();
        for delta in diff.deltas() {
            // Only file blobs participate in the diff; libgit2 deltas are
            // per-file already.
            let old_file = delta.old_file();
            let new_file = delta.new_file();
            let from = ChangeEntry {
                name: old_file
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                hash: oid_or_zero(old_file.id()),
            };
            let to = ChangeEntry {
                name: new_file
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                hash: oid_or_zero(new_file.id()),
            };
            // Skip pure-rename/copy entries with no content side, keeping
            // the changes file-oriented.
            if from.name.is_empty() && to.name.is_empty() {
                continue;
            }
            changes.push(Change { from, to });
        }
        Ok(changes)
    }

    fn tree_inserts(&self, tree: Hash) -> Result<Changes, AnalyzerError> {
        let oid = git2::Oid::from_bytes(&tree.0).map_err(|e| AnalyzerError::Git(e.to_string()))?;
        let tree = self.repo.find_tree(oid)?;
        let mut changes: Changes = Vec::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                let name = format!("{}{}", dir, entry.name().unwrap_or_default());
                changes.push(Change {
                    from: ChangeEntry::default(),
                    to: ChangeEntry {
                        name,
                        hash: entry.id().into(),
                    },
                });
            }
            git2::TreeWalkResult::Ok
        })?;
        Ok(changes)
    }
}

/// Map a libgit2 oid to a [`Hash`], collapsing the zero oid to [`Hash::ZERO`].
fn oid_or_zero(oid: git2::Oid) -> Hash {
    if oid.is_zero() {
        Hash::ZERO
    } else {
        oid.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    /// Scripted tree source: `commit_hash` -> `tree_hash`, and a queue of
    /// diffs to hand back for successive `diff_trees` calls.
    struct FakeSource {
        diffs: RefCell<Vec<Changes>>,
        inserts: Changes,
    }

    impl TreeSource for FakeSource {
        fn commit_tree(&self, commit_hash: Hash) -> Result<Hash, AnalyzerError> {
            // Tree id derived from commit id for determinism.
            Ok(h(commit_hash.0[0].wrapping_add(100)))
        }
        fn diff_trees(&self, _old: Hash, _new: Hash) -> Result<Changes, AnalyzerError> {
            Ok(self.diffs.borrow_mut().remove(0))
        }
        fn tree_inserts(&self, _tree: Hash) -> Result<Changes, AnalyzerError> {
            Ok(self.inserts.clone())
        }
    }

    #[test]
    fn first_commit_uses_inserts_then_diffs() {
        let inserts: Changes = vec![Change {
            from: ChangeEntry::default(),
            to: ChangeEntry { name: "a".into(), hash: h(1) },
        }];
        let later: Changes = vec![Change {
            from: ChangeEntry { name: "a".into(), hash: h(1) },
            to: ChangeEntry { name: "a".into(), hash: h(2) },
        }];
        let src = FakeSource {
            diffs: RefCell::new(vec![later.clone()]),
            inserts: inserts.clone(),
        };
        let mut td = TreeDiff::new(src);

        // First commit -> inserts (no previous tree).
        let c1 = td.changes_for(h(10)).unwrap();
        assert_eq!(c1, inserts);
        // Second commit -> diff against previous tree.
        let c2 = td.changes_for(h(11)).unwrap();
        assert_eq!(c2, later);
    }

    // Mirrors reference tests TestTreeDiff_Name / _Provides / _Requires.
    #[test]
    fn provider_metadata() {
        let src = FakeSource {
            diffs: RefCell::new(vec![]),
            inserts: vec![],
        };
        let td = TreeDiff::new(src);
        assert_eq!(td.name(), "TreeDiff");
        assert_eq!(td.provides(), vec!["changes"]);
        assert!(td.requires().is_empty());
    }
}
