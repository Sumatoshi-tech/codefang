//! File-level line interval tracking for burndown analysis.
//!
//! A [`File`] encapsulates a [`TreapTimeline`] (line-interval storage) and
//! cumulative length counters via [`Updater`] callbacks.

use crate::range_query::RangeIndex;
use crate::timeline::{TimeKey, Timeline};
use crate::timeline_treap::{Segment, TreapTimeline};
use crate::{TREE_END, TREE_MERGE_MARK};

/// Callback invoked on [`File::update`] with `(current, previous, delta)`.
///
/// The boxed `dyn FnMut` lets a [`File`] own a heterogeneous set of updaters.
pub type Updater = Box<dyn FnMut(i64, i64, i64)>;

/// Encapsulates a [`TreapTimeline`] (line-interval storage) and cumulative
/// length counters via [`Updater`]s.
///
/// Construct via [`File::new`]; [`File::len`] returns the line count;
/// [`File::update`] mutates via the timeline and updaters. [`File::dump`]
/// writes the tree to a string and [`File::validate`] checks integrity.
pub struct File {
    timeline: TreapTimeline,
    updaters: Vec<Updater>,
    index: Option<RangeIndex>,
}

impl File {
    /// Initialize a new [`File`] using the default treap timeline.
    ///
    /// `time` is the starting value of the first node. `length` is the starting
    /// length of the tree. `updaters` lists the attached interval length
    /// mappings.
    ///
    /// # Panics
    ///
    /// Panics if `time` or `length` are outside the `[0, u32::MAX]` range.
    pub fn new(time: i64, length: i64, updaters: Vec<Updater>) -> Self {
        if !(0..=i64::from(u32::MAX)).contains(&time) {
            panic!("time is out of allowed range: {time}");
        }

        if length > i64::from(u32::MAX) {
            panic!("length is out of allowed range: {length}");
        }

        let timeline = TreapTimeline::new(time, length);

        let mut file = Self {
            timeline,
            updaters,
            index: None,
        };

        if length > 0 {
            file.update_time(time, time, length);
        }

        file
    }

    /// Create a [`File`] from serialized segments without triggering updaters.
    pub fn from_segments(segs: &[Segment], updaters: Vec<Updater>) -> Self {
        let mut timeline = TreapTimeline::empty();
        timeline.reconstruct_from_segments(segs);

        Self {
            timeline,
            updaters,
            index: None,
        }
    }

    fn update_time(&mut self, current_time: i64, previous_time: i64, delta: i64) {
        let mark = i64::from(TREE_MERGE_MARK);
        if previous_time & mark == mark {
            if current_time == previous_time {
                return;
            }
            panic!("previousTime cannot be TreeMergeMark");
        }

        if current_time & mark == mark {
            // Merge mode - we have already updated in one of the branches.
            return;
        }

        for update in &mut self.updaters {
            update(current_time, previous_time, delta);
        }
    }

    /// Copy the file (shallow copy of the timeline).
    ///
    /// The reference implementation shares the updater slice between the copies;
    /// boxed closures cannot be cloned, so the clone starts with no updaters —
    /// set them via [`File::replace_updaters`] if needed.
    pub fn clone_shallow(&self) -> File {
        File {
            timeline: self.timeline.clone_shallow(),
            updaters: Vec::new(),
            index: None,
        }
    }

    /// Copy the file (deep copy of the timeline). See [`File::clone_shallow`]
    /// regarding updaters.
    pub fn clone_deep(&self) -> File {
        File {
            timeline: self.timeline.clone_deep(),
            updaters: Vec::new(),
            index: None,
        }
    }

    /// Deallocate the file's timeline.
    pub fn delete(&mut self) {
        self.timeline.erase();
    }

    /// Trim the timeline's internal node pool to retain at most `keep` free
    /// nodes.
    pub fn shrink_pool(&mut self, keep: usize) {
        self.timeline.shrink_pool(keep);
    }

    /// Replace the file's updaters with a new set.
    pub fn replace_updaters(&mut self, updaters: Vec<Updater>) {
        self.updaters = updaters;
    }

    /// Return the file's timeline segments as a compact slice.
    pub fn segments(&self) -> Vec<Segment> {
        self.timeline.segments()
    }

    /// Rebuild the file's timeline from a compact segment slice.
    pub fn reconstruct_from_segments(&mut self, segs: &[Segment]) {
        self.timeline.reconstruct_from_segments(segs);
    }

    /// Number of lines in the file.
    pub fn len(&self) -> i64 {
        self.timeline.len()
    }

    /// `true` if the file has no lines.
    pub fn is_empty(&self) -> bool {
        self.timeline.len() == 0
    }

    /// Number of segments/nodes in the file.
    pub fn nodes(&self) -> i64 {
        self.timeline.nodes()
    }

    /// Modify the timeline to reflect line changes and notify updaters
    /// (deletions and insertions).
    ///
    /// # Panics
    ///
    /// Panics on negative `time`/`pos`, `time >= u32::MAX`, `pos > u32::MAX`, or
    /// negative `ins_length`/`del_length`.
    pub fn update(&mut self, time: i64, pos: i64, ins_length: i64, del_length: i64) {
        if time < 0 {
            panic!("time may not be negative");
        }

        if time >= i64::from(u32::MAX) {
            panic!("time may not be >= MaxUint32");
        }

        if pos < 0 {
            panic!("attempt to insert/delete at a negative position");
        }

        if pos > i64::from(u32::MAX) {
            panic!("pos may not be > MaxUint32");
        }

        if ins_length < 0 || del_length < 0 {
            panic!("insLength and delLength must be non-negative");
        }

        if ins_length | del_length == 0 {
            return;
        }

        if ins_length > 0 {
            self.update_time(time, time, ins_length);
        }

        let reports = self
            .timeline
            .replace(pos, del_length, ins_length, time as TimeKey);
        for d in reports {
            self.update_time(d.current, d.previous, d.delta);
        }

        self.invalidate_index();
    }

    /// Coalesce consecutive segments with the same time (reduces node count).
    pub fn merge_adjacent_same_value(&mut self) {
        self.timeline.merge_adjacent_same_value();
    }

    /// Combine several prepared files together.
    ///
    /// # Panics
    ///
    /// Panics if a line-count mismatch is detected between this file and any
    /// other (file corruption).
    pub fn merge(&mut self, day: i64, others: &[&File]) {
        let mut myself = self.timeline.flatten();
        merge_other_files(&mut myself, others);
        self.resolve_merge_conflicts(&mut myself, day);
        self.timeline.reconstruct(&myself);
    }

    fn resolve_merge_conflicts(&mut self, lines: &mut [i64], day: i64) {
        for line in lines.iter_mut() {
            if is_merge_marked(*line) {
                *line = day;
                self.update_time(day, day, 1);
            }
        }
    }

    /// Format the underlying line interval tree into a string. Useful for error
    /// messages and debugging.
    pub fn dump(&self) -> String {
        let mut buffer = String::new();
        self.for_each(|line, value| {
            buffer.push_str(&format!("{line} {value}\n"));
        });
        buffer
    }

    /// Check the timeline integrity.
    pub fn validate(&self) {
        self.timeline.validate();
    }

    /// Visit each segment start in the timeline in order (`line`, `value`);
    /// `value` is `-1` for the `TreeEnd` sentinel.
    pub fn for_each<F: FnMut(i64, i64)>(&self, mut callback: F) {
        self.timeline.iterate(&mut |offset, _, t| {
            let v = if t == TREE_END { -1 } else { i64::from(t) };
            callback(offset, v);
            true
        });
    }

    // --- range_query.rs support (kept here so RangeIndex stays crate-private) ---

    pub(crate) fn timeline(&self) -> &TreapTimeline {
        &self.timeline
    }

    pub(crate) fn index_mut(&mut self) -> &mut Option<RangeIndex> {
        &mut self.index
    }
}

/// Check whether a line value has the merge mark bit set.
fn is_merge_marked(value: i64) -> bool {
    value & i64::from(TREE_MERGE_MARK) == i64::from(TREE_MERGE_MARK)
}

/// Merge the flattened lines of `others` into `myself`.
///
/// # Panics
///
/// Panics on a line-count mismatch between `myself` and any other file.
fn merge_other_files(myself: &mut [i64], others: &[&File]) {
    let mark = i64::from(TREE_MERGE_MARK);
    for other in others {
        let lines = other.timeline.flatten();

        if myself.len() != lines.len() {
            panic!(
                "file corruption, lines number mismatch during merge {} != {}",
                myself.len(),
                lines.len()
            );
        }

        for (my_line, &other_line) in myself.iter_mut().zip(lines.iter()) {
            if is_merge_marked(other_line) {
                continue;
            }

            if is_merge_marked(*my_line) || (*my_line & mark) > (other_line & mark) {
                *my_line = other_line;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Mirrors reference test `TestMergeAdjacentSameValue`. Verifies merge does
    /// not increase node count and preserves effective line→time.
    #[test]
    fn merge_adjacent_same_value() {
        let mut file = File::new(0, 1000, Vec::new());
        // Build adjacent same-value segments: (50,1) and (60,1).
        file.update(0, 0, 100, 0);
        file.update(1, 50, 10, 0);
        file.update(1, 60, 40, 0);

        let before = file.nodes();

        let value_at_line = |f: &File, line: i64| -> i64 {
            let mut last = 0i64;
            f.for_each(|l, v| {
                if l <= line {
                    last = v;
                }
            });
            last
        };
        let sample_lines = [0, 25, 50, 55, 60, 70, 100, 500];
        let before_values: Vec<i64> =
            sample_lines.iter().map(|&ln| value_at_line(&file, ln)).collect();

        file.merge_adjacent_same_value();

        assert!(file.nodes() <= before, "merge should not increase nodes");

        for (i, &ln) in sample_lines.iter().enumerate() {
            assert_eq!(
                value_at_line(&file, ln),
                before_values[i],
                "merge must preserve value at line {ln}"
            );
        }

        file.validate();
    }

    /// Mirrors reference test `TestTreapTimeline_FileWithTimeline`.
    #[test]
    fn file_with_timeline_update_sequence() {
        let mut file = File::new(0, 1000, Vec::new());
        file.update(0, 0, 100, 0);
        file.update(1, 50, 10, 0);
        file.update(1, 60, 40, 0);
        file.validate();

        let value_at = |f: &File, line: i64| -> i64 {
            let mut last = 0i64;
            f.for_each(|l, v| {
                if l <= line {
                    last = v;
                }
            });
            last
        };
        assert_eq!(value_at(&file, 50), 1);
        assert_eq!(value_at(&file, 55), 1);
    }

    /// Mirrors reference test `TestNewFileFromSegments`.
    #[test]
    fn new_file_from_segments() {
        let mut original = File::new(1, 100, Vec::new());
        original.update(2, 50, 20, 10);

        let segs = original.segments();
        let restored = File::from_segments(&segs, Vec::new());

        assert_eq!(restored.len(), original.len());

        let mut orig_entries = Vec::new();
        original.for_each(|line, value| orig_entries.push((line, value)));
        let mut restored_entries = Vec::new();
        restored.for_each(|line, value| restored_entries.push((line, value)));

        assert_eq!(orig_entries, restored_entries);
    }

    /// `new` invokes the updater once with `(time, time, length)` for the
    /// initial fill.
    #[test]
    fn new_file_invokes_updater_on_initial_length() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let c = calls.clone();
        let updater: Updater = Box::new(move |cur, prev, delta| c.borrow_mut().push((cur, prev, delta)));
        let _file = File::new(3, 10, vec![updater]);
        assert_eq!(*calls.borrow(), vec![(3, 3, 10)]);
    }

    /// A pure-deletion `update` reports `(time, previous, -deleted)` for each
    /// deleted interval.
    #[test]
    fn update_deletion_reports_negative_delta() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let c = calls.clone();
        let updater: Updater = Box::new(move |cur, prev, delta| c.borrow_mut().push((cur, prev, delta)));
        let mut file = File::new(5, 100, vec![updater]);
        calls.borrow_mut().clear(); // drop the construction call
        file.update(9, 0, 0, 10);
        let recorded = calls.borrow().clone();
        let total_deleted: i64 = recorded.iter().map(|&(_, _, d)| -d).sum();
        assert_eq!(total_deleted, 10);
        assert!(recorded.iter().all(|&(cur, prev, _)| cur == 9 && prev == 5));
    }

    /// `dump` formats `line value\n` per visited segment start. The `TreeEnd`
    /// sentinel has length 0, so (the tree walk only visits segments with
    /// `length > 0`) it is NOT emitted: a fresh `File(0, 5)` dumps just its
    /// single data segment.
    #[test]
    fn dump_formats_segments() {
        let file = File::new(0, 5, Vec::new());
        assert_eq!(file.dump(), "0 0\n");
    }

    /// After an edit creates a second segment, `dump` lists both starts in
    /// order, still excluding the zero-length `TreeEnd` sentinel.
    #[test]
    fn dump_formats_multiple_segments() {
        let mut file = File::new(0, 100, Vec::new());
        file.update(1, 50, 10, 0);
        // Segments: [0,50)@0, [50,60)@1, [60,110)@0; TreeEnd (len 0) excluded.
        assert_eq!(file.dump(), "0 0\n50 1\n60 0\n");
    }

    /// `merge` resolves merge-marked lines to `day` and reconstructs cleanly
    /// (equal-length files).
    #[test]
    fn merge_two_files() {
        let mut file1 = File::new(0, 1000, Vec::new());
        let mut file2 = File::new(0, 1000, Vec::new());
        // Grow both equally so merge's equal-length invariant holds.
        file1.update(1, 100, 50, 0);
        file2.update(1, 100, 50, 0);
        file1.merge(2, &[&file2]);
        file1.validate();
        assert_eq!(file1.len(), 1050);
    }

    /// Node-pool shrink integration via `File`.
    #[test]
    fn file_shrink_pool() {
        let mut file = File::new(0, 1000, Vec::new());
        file.update(1, 0, 100, 50);
        file.shrink_pool(10);
        file.validate();
    }

    /// `clone_deep` is independent of subsequent mutations of the original.
    #[test]
    fn clone_deep_independent() {
        let file = File::new(0, 100, Vec::new());
        let clone = file.clone_deep();
        let mut file = file;
        file.update(1, 0, 0, 50);
        assert_eq!(clone.len(), 100);
        assert_eq!(file.len(), 50);
    }
}
