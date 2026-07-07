//! Timeline interface for line-interval storage.
//!
//! A [`Timeline`] stores line intervals by (implicit or explicit) position and
//! supports `replace` without O(N) key shifting. The default implementation is
//! the implicit treap in [`crate::timeline_treap`].

use crate::timeline_treap::{Segment, TreapTimeline};

/// The time (tick) associated with a line interval. Same semantics as the tree
/// node value.
pub type TimeKey = u32;

/// A single `(current, previous, delta)` tuple for updater callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaReport {
    /// The current time (tick) of the edit producing this report.
    pub current: i64,
    /// The previous time (tick) of the lines being deleted.
    pub previous: i64,
    /// The signed line delta (negative for deletions).
    pub delta: i64,
}

/// Stores line intervals by position and supports `replace` without O(N) key
/// shifting.
///
/// The `clone_shallow` / `clone_deep` methods return [`TreapTimeline`] directly
/// (the only implementation). Implementors must uphold the invariants checked
/// by [`Timeline::validate`].
pub trait Timeline {
    /// Apply delete `[pos, pos+del_lines)` then insert `ins_lines` at `pos` with
    /// time `t`. Returns delta reports for the caller to apply to updaters (e.g.
    /// from deleted intervals).
    fn replace(&mut self, pos: i64, del_lines: i64, ins_lines: i64, t: TimeKey)
        -> Vec<DeltaReport>;

    /// Call `f(offset, length, time_key)` for each segment in order; return
    /// `false` from `f` to stop early.
    fn iterate(&self, f: &mut dyn FnMut(i64, i64, TimeKey) -> bool);

    /// Total line count (file length).
    fn len(&self) -> i64;

    /// `true` if the timeline contains no lines.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of segments/nodes (for diagnostics).
    fn nodes(&self) -> i64;

    /// Panics if invariants are violated.
    fn validate(&self);

    /// Returns a shallow copy of the timeline.
    fn clone_shallow(&self) -> TreapTimeline;

    /// Returns a deep copy of the timeline.
    fn clone_deep(&self) -> TreapTimeline;

    /// Clears all nodes (for [`crate::File::delete`]).
    fn erase(&mut self);

    /// Returns line→time as a slice (for [`crate::File::merge`]).
    fn flatten(&self) -> Vec<i64>;

    /// Rebuilds from a line→time slice (for [`crate::File::merge`]).
    fn reconstruct(&mut self, lines: &[i64]);

    /// Coalesces consecutive segments with the same time (reduces node count).
    fn merge_adjacent_same_value(&mut self);

    /// Returns the treap's segments as a compact slice (excludes the `TreeEnd`
    /// sentinel).
    fn segments(&self) -> Vec<Segment>;

    /// Rebuilds from a compact segment slice (inverse of [`Timeline::segments`]).
    fn reconstruct_from_segments(&mut self, segs: &[Segment]);
}
