//! Range (overlap) queries over a [`File`]'s line ownership.
//!
//! Builds a lazy interval-tree index from the timeline segments and answers
//! overlap queries against it.

use crate::file::File;
use crate::interval::Tree;
use crate::timeline::{TimeKey, Timeline};
use crate::TREE_END;

/// A line range `[start_line, end_line)` with a single owner (time value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnershipSegment {
    /// Inclusive start line of the run.
    pub start_line: i64,
    /// Exclusive end line of the run.
    pub end_line: i64,
    /// The owning time/tick.
    pub owner: i64,
}

/// Holds the lazy interval-tree index for range queries.
pub(crate) struct RangeIndex {
    tree: Tree,
    dirty: bool,
}

impl File {
    /// Return all ownership segments that overlap `[start_line, end_line)`.
    ///
    /// The interval-tree index is rebuilt lazily when the timeline has been
    /// modified. `TreeEnd` sentinel segments are excluded from results.
    pub fn query_range(&mut self, start_line: i64, end_line: i64) -> Vec<OwnershipSegment> {
        self.ensure_index();

        let index = match self.index_mut() {
            Some(idx) if !idx.tree.is_empty() => idx,
            _ => return Vec::new(),
        };

        let low = start_line as u32;
        let high = (end_line - 1) as u32;
        let intervals = index.tree.query_overlap(low, high);

        intervals
            .into_iter()
            .map(|iv| OwnershipSegment {
                start_line: i64::from(iv.low),
                end_line: i64::from(iv.high) + 1,
                owner: i64::from(iv.value),
            })
            .collect()
    }

    /// Mark the interval-tree index as needing a rebuild. Called automatically
    /// by [`File::update`].
    pub fn invalidate_index(&mut self) {
        if let Some(idx) = self.index_mut() {
            idx.dirty = true;
        }
    }

    /// Rebuild the interval-tree index if it is dirty or uninitialized.
    fn ensure_index(&mut self) {
        if self.index_mut().is_none() {
            *self.index_mut() = Some(RangeIndex {
                tree: Tree::new(),
                dirty: true,
            });
        }

        if !self.index_mut().as_ref().expect("index just set").dirty {
            return;
        }

        self.rebuild_index();
    }

    /// Reconstruct the interval tree from the current timeline segments.
    fn rebuild_index(&mut self) {
        // Collect segments first to avoid borrowing `self` two ways.
        let mut entries: Vec<(u32, u32, TimeKey)> = Vec::new();
        self.timeline().iterate(&mut |offset, length, t| {
            if t == TREE_END || length <= 0 {
                return true;
            }
            let low = offset as u32;
            let high = (offset + length - 1) as u32;
            entries.push((low, high, t));
            true
        });

        let idx = self.index_mut().as_mut().expect("index initialized");
        idx.tree.clear();
        for (low, high, t) in entries {
            idx.tree.insert(low, high, t);
        }
        idx.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RQ_INITIAL_LENGTH: i64 = 100;
    const RQ_LARGE_FILE_LENGTH: i64 = 10_000;

    /// Mirrors reference test `TestQueryRange_Basic`.
    #[test]
    fn query_range_basic() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        let results = file.query_range(0, RQ_INITIAL_LENGTH);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start_line, 0);
        assert_eq!(results[0].end_line, RQ_INITIAL_LENGTH);
        assert_eq!(results[0].owner, 0);
    }

    /// Mirrors reference test `TestQueryRange_AfterUpdate`.
    #[test]
    fn query_range_after_update() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        file.update(1, 50, 10, 0);
        let results = file.query_range(0, file.len());
        assert!(!results.is_empty());
        let owner1 = results
            .iter()
            .find(|s| s.owner == 1)
            .expect("owner=1 segment");
        assert_eq!(owner1.start_line, 50);
        assert_eq!(owner1.end_line, 60);
    }

    /// Mirrors reference test `TestQueryRange_PartialOverlap`.
    #[test]
    fn query_range_partial_overlap() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        file.update(1, 50, 10, 0);
        let results = file.query_range(55, 65);
        assert!(
            results.iter().any(|s| s.owner == 1),
            "should find time=1 segment"
        );
    }

    /// Mirrors reference test `TestQueryRange_NoOverlap`.
    #[test]
    fn query_range_no_overlap() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        assert!(file.query_range(200, 300).is_empty());
    }

    /// Mirrors reference test `TestQueryRange_EmptyFile`.
    #[test]
    fn query_range_empty_file() {
        let mut file = File::new(0, 0, Vec::new());
        assert!(file.query_range(0, 10).is_empty());
    }

    /// Mirrors reference test `TestQueryRange_LazyRebuild`.
    #[test]
    fn query_range_lazy_rebuild() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        let results1 = file.query_range(0, RQ_INITIAL_LENGTH);
        assert_eq!(results1.len(), 1);
        file.update(1, 50, 10, 0);
        let results2 = file.query_range(0, file.len());
        assert!(results2.len() > 1, "should have more segments after update");
    }

    /// Mirrors reference test `TestQueryRange_LargeFile`.
    #[test]
    fn query_range_large_file() {
        let mut file = File::new(0, RQ_LARGE_FILE_LENGTH, Vec::new());
        for i in 1..=10 {
            file.update(i, i * 500, 50, 0);
        }
        let results = file.query_range(0, file.len());
        assert!(!results.is_empty());
        let total_lines: i64 = results.iter().map(|s| s.end_line - s.start_line).sum();
        assert_eq!(
            total_lines,
            file.len(),
            "total segment lines should match file length"
        );
    }

    /// Mirrors reference test `TestQueryRange_ExcludesTreeEnd`.
    #[test]
    fn query_range_excludes_tree_end() {
        let mut file = File::new(0, RQ_INITIAL_LENGTH, Vec::new());
        let results = file.query_range(0, RQ_INITIAL_LENGTH + 10);
        for seg in &results {
            assert_ne!(seg.owner, -1, "TreeEnd should not appear");
            assert_ne!(seg.owner, i64::from(TREE_END), "TreeEnd should not appear");
        }
    }
}
