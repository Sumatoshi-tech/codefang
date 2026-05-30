//! Implicit treap [`Timeline`] (position = implicit key). No key shifting on
//! `replace`.
//!
//! Port of `internal/burndown/timeline_treap.go`.
//!
//! # Memory model
//!
//! The Go implementation manages `*treapNode` pointers via a free-list
//! `nodePool`. To preserve the exact reuse/recycling behavior (LIFO free-list,
//! zero-on-release, `shrink`, post-order subtree release) without `unsafe`, this
//! Rust port stores nodes in an arena (`Vec<TreapNode>`) and references children
//! by index ([`NodeIdx`]). The arena plus its free-list together play the role
//! of the Go `nodePool`. Node "pointers" are arena indices, so the Go tests
//! that assert pointer reuse (`acquire` returns the just-`release`d node)
//! translate to index reuse.

use crate::timeline::{DeltaReport, TimeKey, Timeline};
use crate::{TREE_END, TREE_MERGE_MARK};

/// Used to compute the midpoint index when splitting a range in half.
const MIDPOINT_DIVISOR: usize = 2;

/// The initial non-zero seed for the xorshift64 PRNG.
///
/// Any non-zero value produces a full-period sequence (2^64 - 1). Matches the Go
/// constant `defaultPRNGSeed`.
pub(crate) const DEFAULT_PRNG_SEED: u64 = 0x2545_F491_4F6C_DD1D;

/// First left-shift constant in the xorshift64 algorithm.
const XORSHIFT64_SHIFT_A: u32 = 13;
/// Right-shift constant in the xorshift64 algorithm.
const XORSHIFT64_SHIFT_B: u32 = 7;
/// Second left-shift constant in the xorshift64 algorithm.
const XORSHIFT64_SHIFT_C: u32 = 17;
/// Extracts the upper 32 bits from a 64-bit state.
const XORSHIFT64_UPPER_SHIFT: u32 = 32;

/// Advance the PRNG state and return a `u32` priority from the upper bits.
///
/// The algorithm is Marsaglia's xorshift64 with period 2^64 - 1; the state must
/// be non-zero. Byte-for-byte equivalent to the Go `xorshift64` function, so
/// treap shapes are identical between the Go and Rust implementations given the
/// same sequence of node allocations.
pub(crate) fn xorshift64(state: &mut u64) -> u32 {
    let mut s = *state;
    s ^= s << XORSHIFT64_SHIFT_A;
    s ^= s >> XORSHIFT64_SHIFT_B;
    s ^= s << XORSHIFT64_SHIFT_C;
    *state = s;

    (s >> XORSHIFT64_UPPER_SHIFT) as u32
}

/// A contiguous run of `length` lines with the same time `value`.
///
/// Used for compact serialization of treap state (segments vs per-line
/// expansion). Mirrors the Go `Segment` struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Number of lines in this run.
    pub length: i64,
    /// The time (tick) value owning this run.
    pub value: TimeKey,
}

/// An index into the arena, identifying a [`TreapNode`].
type NodeIdx = usize;

/// A single segment: `length` lines with `value`. Position is implicit (the sum
/// of left-subtree sizes). Mirrors the Go `treapNode` struct.
#[derive(Debug, Clone, Copy, Default)]
struct TreapNode {
    left: Option<NodeIdx>,
    right: Option<NodeIdx>,
    length: i64,
    value: TimeKey,
    size: i64,
    priority: u32,
}

/// An implicit-treap [`Timeline`] (position = size of left subtree). `replace`
/// is O(log N) without shifting keys.
///
/// Mirrors the Go `treapTimeline` struct. The `arena` + `free` list together are
/// the equivalent of the Go `nodePool` embedded in `treapTimeline`.
#[derive(Debug, Default)]
pub struct TreapTimeline {
    root: Option<NodeIdx>,
    total_length: i64,
    prng_state: u64,
    arena: Vec<TreapNode>,
    free: Vec<NodeIdx>,
}

impl TreapTimeline {
    /// Create a [`Timeline`] backed by an implicit treap with initial
    /// `[0, length)` at time `time`.
    ///
    /// Mirrors Go `NewTreapTimeline`.
    ///
    /// # Panics
    ///
    /// Panics if `time` or `length` are out of the `[0, u32::MAX]` range.
    pub fn new(time: i64, length: i64) -> Self {
        if !(0..=i64::from(u32::MAX)).contains(&time) {
            panic!("time out of range: {time}");
        }

        if !(0..=i64::from(u32::MAX)).contains(&length) {
            panic!("length out of range: {length}");
        }

        let mut t = Self {
            root: None,
            total_length: length,
            prng_state: DEFAULT_PRNG_SEED,
            arena: Vec::new(),
            free: Vec::new(),
        };

        if length > 0 {
            let a = t.new_node(length, time as TimeKey);
            let b = t.new_node(0, TREE_END);
            t.root = t.merge(Some(a), Some(b));
        }

        t
    }

    /// Construct an empty timeline (no nodes). Used by callers that immediately
    /// `reconstruct_from_segments`. Mirrors the Go zero-value `&treapTimeline{}`.
    ///
    /// The Go zero value has `prngState == 0`; `reconstruct_from_segments` does
    /// not reset the seed, so the first allocation uses state `0`, which
    /// xorshift64 maps to priority `0` deterministically — identical to Go.
    pub(crate) fn empty() -> Self {
        Self {
            root: None,
            total_length: 0,
            prng_state: 0,
            arena: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Trim the internal node pool to retain at most `keep` free nodes.
    ///
    /// Mirrors Go `(*treapTimeline).ShrinkPool` / `nodePool.shrink`.
    pub fn shrink_pool(&mut self, keep: usize) {
        if self.free.len() <= keep {
            return;
        }

        self.free.truncate(keep);
    }

    // --- node pool (arena + free-list) ---

    /// Return a zeroed node index from the free-list, or allocate a new one.
    /// Mirrors `nodePool.acquire`.
    fn acquire(&mut self) -> NodeIdx {
        if let Some(idx) = self.free.pop() {
            self.arena[idx] = TreapNode::default();
            idx
        } else {
            self.arena.push(TreapNode::default());
            self.arena.len() - 1
        }
    }

    /// Zero all fields and return the node to the free-list. Mirrors
    /// `nodePool.release`.
    fn release(&mut self, idx: NodeIdx) {
        self.arena[idx] = TreapNode::default();
        self.free.push(idx);
    }

    /// Recursively release all nodes in the subtree (post-order). Mirrors
    /// `nodePool.releaseSubtree`.
    fn release_subtree(&mut self, idx: Option<NodeIdx>) {
        let Some(idx) = idx else { return };
        let (l, r) = {
            let n = &self.arena[idx];
            (n.left, n.right)
        };
        self.release_subtree(l);
        self.release_subtree(r);
        self.release(idx);
    }

    fn new_node(&mut self, length: i64, value: TimeKey) -> NodeIdx {
        let priority = xorshift64(&mut self.prng_state);
        let idx = self.acquire();
        let n = &mut self.arena[idx];
        n.length = length;
        n.value = value;
        n.size = length;
        n.priority = priority;
        idx
    }

    fn recalc_size(&mut self, idx: NodeIdx) {
        let (left, right, length) = {
            let n = &self.arena[idx];
            (n.left, n.right, n.length)
        };
        let mut size = length;
        if let Some(l) = left {
            size += self.arena[l].size;
        }
        if let Some(r) = right {
            size += self.arena[r].size;
        }
        self.arena[idx].size = size;
    }

    fn merge(&mut self, l: Option<NodeIdx>, r: Option<NodeIdx>) -> Option<NodeIdx> {
        let (Some(li), Some(ri)) = (l, r) else {
            return l.or(r);
        };

        if self.arena[li].priority >= self.arena[ri].priority {
            let lr = self.arena[li].right;
            let merged = self.merge(lr, Some(ri));
            self.arena[li].right = merged;
            self.recalc_size(li);
            Some(li)
        } else {
            let rl = self.arena[ri].left;
            let merged = self.merge(Some(li), rl);
            self.arena[ri].left = merged;
            self.recalc_size(ri);
            Some(ri)
        }
    }

    /// Split so `left` has the first `pos` lines (0-indexed), `right` has the
    /// rest. Mirrors `splitByLines`.
    fn split_by_lines(
        &mut self,
        root: Option<NodeIdx>,
        pos: i64,
    ) -> (Option<NodeIdx>, Option<NodeIdx>) {
        let Some(root) = root else {
            return (None, None);
        };

        let (left, right, length, value) = {
            let n = &self.arena[root];
            (n.left, n.right, n.length, n.value)
        };
        let left_size = left.map_or(0, |l| self.arena[l].size);

        if pos <= left_size {
            let (l, r) = self.split_by_lines(left, pos);
            self.arena[root].left = r;
            self.recalc_size(root);
            (l, Some(root))
        } else if pos >= left_size + length {
            let (l, r) = self.split_by_lines(right, pos - left_size - length);
            self.arena[root].right = l;
            self.recalc_size(root);
            (Some(root), r)
        } else {
            // Split inside root's segment: [left_size, left_size+length) at pos.
            let left_part = self.new_node(pos - left_size, value);
            let right_part = self.new_node(left_size + length - pos, value);

            // Detach children before releasing the original root.
            let orig_left = self.arena[root].left;
            let orig_right = self.arena[root].right;
            self.arena[root].left = None;
            self.arena[root].right = None;

            self.release(root);

            let l = self.merge(orig_left, Some(left_part));
            let r = self.merge(Some(right_part), orig_right);

            (l, r)
        }
    }

    fn collect_reports(
        &self,
        n: Option<NodeIdx>,
        current_time: i64,
        reports: &mut Vec<DeltaReport>,
    ) {
        let Some(n) = n else { return };
        let node = self.arena[n];
        self.collect_reports(node.left, current_time, reports);
        if node.length > 0 && node.value != TREE_END {
            reports.push(DeltaReport {
                current: current_time,
                previous: i64::from(node.value),
                delta: -node.length,
            });
        }
        self.collect_reports(node.right, current_time, reports);
    }

    fn walk_nodes(
        &self,
        n: Option<NodeIdx>,
        offset: i64,
        f: &mut dyn FnMut(i64, i64, TimeKey) -> bool,
    ) -> (i64, bool) {
        let Some(n) = n else { return (offset, true) };
        let node = self.arena[n];

        let (off, ok) = self.walk_nodes(node.left, offset, f);
        if !ok {
            return (off, false);
        }

        if node.length > 0 && !f(off, node.length, node.value) {
            return (off, false);
        }

        self.walk_nodes(node.right, off + node.length, f)
    }

    fn node_count(&self, n: Option<NodeIdx>) -> i64 {
        let Some(n) = n else { return 0 };
        let node = self.arena[n];
        1 + self.node_count(node.left) + self.node_count(node.right)
    }

    fn clone_deep_node(&mut self, src: &TreapTimeline, n: Option<NodeIdx>) -> Option<NodeIdx> {
        let n = n?;
        let snode = src.arena[n];
        let priority = xorshift64(&mut self.prng_state);
        let idx = self.acquire();
        {
            let c = &mut self.arena[idx];
            c.length = snode.length;
            c.value = snode.value;
            c.size = snode.size;
            c.priority = priority;
        }
        let left = self.clone_deep_node(src, snode.left);
        let right = self.clone_deep_node(src, snode.right);
        let c = &mut self.arena[idx];
        c.left = left;
        c.right = right;
        Some(idx)
    }

    fn validate_node(&self, n: Option<NodeIdx>, last_val: &mut TimeKey) {
        let Some(n) = n else { return };
        let node = self.arena[n];
        self.validate_node(node.left, last_val);
        if node.value == TREE_MERGE_MARK {
            panic!("unmerged lines left at segment length {}", node.length);
        }
        *last_val = node.value;
        self.validate_node(node.right, last_val);
    }

    /// Build a balanced subtree from `segs[start..end]`, then `recalc_size`.
    /// Shared by `reconstruct` and `reconstruct_from_segments`.
    fn build_from_segments(
        &mut self,
        segs: &[Segment],
        start: usize,
        end: usize,
    ) -> Option<NodeIdx> {
        if start >= end {
            return None;
        }
        let mid = (start + end) / MIDPOINT_DIVISOR;
        let s = segs[mid];
        let idx = self.new_node(s.length, s.value);
        let left = self.build_from_segments(segs, start, mid);
        let right = self.build_from_segments(segs, mid + 1, end);
        self.arena[idx].left = left;
        self.arena[idx].right = right;
        self.recalc_size(idx);
        Some(idx)
    }

    // --- test-only accessors (used by ported node-pool tests) ---

    #[cfg(test)]
    fn free_len(&self) -> usize {
        self.free.len()
    }

    #[cfg(test)]
    fn node_fields(&self, idx: NodeIdx) -> (i64, TimeKey, i64, u32) {
        let n = self.arena[idx];
        (n.length, n.value, n.size, n.priority)
    }

    #[cfg(test)]
    fn set_node_length(&mut self, idx: NodeIdx, length: i64) {
        self.arena[idx].length = length;
    }

    #[cfg(test)]
    fn set_node_children(&mut self, idx: NodeIdx, left: Option<NodeIdx>, right: Option<NodeIdx>) {
        self.arena[idx].left = left;
        self.arena[idx].right = right;
    }

    #[cfg(test)]
    fn max_depth(&self) -> i64 {
        fn depth(tl: &TreapTimeline, n: Option<NodeIdx>) -> i64 {
            let Some(n) = n else { return 0 };
            let node = tl.arena[n];
            1 + depth(tl, node.left).max(depth(tl, node.right))
        }
        depth(self, self.root)
    }
}

impl Timeline for TreapTimeline {
    fn replace(&mut self, pos: i64, del_lines: i64, ins_lines: i64, t: TimeKey) -> Vec<DeltaReport> {
        if self.root.is_none() {
            if pos != 0 || del_lines != 0 {
                panic!("Replace on empty timeline with non-zero pos or delLines");
            }

            if ins_lines > 0 {
                let a = self.new_node(ins_lines, t);
                let b = self.new_node(0, TREE_END);
                self.root = self.merge(Some(a), Some(b));
                self.total_length = ins_lines;
            }

            return Vec::new();
        }

        if pos > self.total_length {
            panic!("Replace pos {pos} > Len {}", self.total_length);
        }

        if pos + del_lines > self.total_length {
            panic!(
                "Replace [{pos},{}) out of range (Len {})",
                pos + del_lines,
                self.total_length
            );
        }

        let (left, right) = self.split_by_lines(self.root, pos);
        let (mid_seg, right2) = self.split_by_lines(right, del_lines);

        let mut reports = Vec::new();
        self.collect_reports(mid_seg, i64::from(t), &mut reports);

        // Release the deleted middle subtree back to the pool.
        self.release_subtree(mid_seg);

        let mid = if ins_lines > 0 {
            Some(self.new_node(ins_lines, t))
        } else {
            None
        };

        let merged = self.merge(mid, right2);
        self.root = self.merge(left, merged);
        self.total_length += ins_lines - del_lines;

        reports
    }

    fn iterate(&self, f: &mut dyn FnMut(i64, i64, TimeKey) -> bool) {
        self.walk_nodes(self.root, 0, f);
    }

    fn len(&self) -> i64 {
        self.total_length
    }

    fn nodes(&self) -> i64 {
        self.node_count(self.root)
    }

    fn validate(&self) {
        let Some(root) = self.root else {
            if self.total_length != 0 {
                panic!("empty root but totalLength != 0");
            }
            return;
        };

        let mut last_val: TimeKey = 0;
        self.validate_node(Some(root), &mut last_val);

        if last_val != TREE_END {
            panic!("last value must be TreeEnd, got {last_val}");
        }
    }

    fn clone_shallow(&self) -> TreapTimeline {
        // Go's shallowCopy shares the same root pointer and omits the pool.
        // Since our arena owns the nodes, a structural clone of the arena + root
        // preserves the observable behavior (independent length tracking,
        // identical iteration) without aliasing. The prng_state is carried over
        // exactly as Go does.
        TreapTimeline {
            root: self.root,
            total_length: self.total_length,
            prng_state: self.prng_state,
            arena: self.arena.clone(),
            free: self.free.clone(),
        }
    }

    fn clone_deep(&self) -> TreapTimeline {
        let mut out = TreapTimeline {
            root: None,
            total_length: self.total_length,
            prng_state: self.prng_state,
            arena: Vec::new(),
            free: Vec::new(),
        };
        out.root = out.clone_deep_node(self, self.root);
        out
    }

    fn erase(&mut self) {
        self.release_subtree(self.root);
        self.root = None;
        self.total_length = 0;
    }

    fn flatten(&self) -> Vec<i64> {
        let mut lines = Vec::with_capacity(self.total_length.max(0) as usize);
        self.walk_nodes(self.root, 0, &mut |_, length, t| {
            for _ in 0..length {
                lines.push(i64::from(t));
            }
            true
        });
        lines
    }

    fn reconstruct(&mut self, lines: &[i64]) {
        self.release_subtree(self.root);
        self.root = None;

        self.total_length = lines.len() as i64;
        if lines.is_empty() {
            return;
        }

        let mut segs: Vec<Segment> = Vec::new();
        let mut i = 0usize;
        while i < lines.len() {
            let v = lines[i] as TimeKey;
            let mut j = i + 1;
            while j < lines.len() && lines[j] == lines[i] {
                j += 1;
            }
            segs.push(Segment {
                length: (j - i) as i64,
                value: v,
            });
            i = j;
        }

        self.root = self.build_from_segments(&segs, 0, segs.len());
        let end = self.new_node(0, TREE_END);
        self.root = self.merge(self.root, Some(end));
    }

    fn merge_adjacent_same_value(&mut self) {
        let segs = self.segments();
        if segs.is_empty() {
            return;
        }

        let coalesced = coalesce_segments(&segs);
        if coalesced.len() == segs.len() {
            return;
        }

        self.reconstruct_from_segments(&coalesced);
    }

    fn segments(&self) -> Vec<Segment> {
        let mut segs = Vec::new();
        self.walk_nodes(self.root, 0, &mut |_, length, t| {
            if t == TREE_END {
                return true;
            }
            segs.push(Segment { length, value: t });
            true
        });
        segs
    }

    fn reconstruct_from_segments(&mut self, segs: &[Segment]) {
        self.release_subtree(self.root);
        self.root = None;
        self.total_length = 0;

        for s in segs {
            self.total_length += s.length;
        }

        if segs.is_empty() {
            return;
        }

        self.root = self.build_from_segments(segs, 0, segs.len());
        let end = self.new_node(0, TREE_END);
        self.root = self.merge(self.root, Some(end));
    }
}

/// Merge adjacent segments with identical `value`. Returns a new vector; the
/// input is not modified. Mirrors the Go `coalesceSegments`.
pub(crate) fn coalesce_segments(segs: &[Segment]) -> Vec<Segment> {
    if segs.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(segs.len());
    let mut current = segs[0];

    for &s in &segs[1..] {
        if s.value == current.value {
            current.length += s.length;
        } else {
            result.push(current);
            current = s;
        }
    }

    result.push(current);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ports of priority_test.go (xorshift64 PRNG) ---

    const PRIO_TEST_SEED: u64 = 0xDEAD_BEEF_CAFE_BABE;
    const PRIO_TEST_SEQUENCE_LEN: usize = 1000;

    /// Port of `TestXorshift64_NonZero`.
    #[test]
    fn xorshift64_non_zero() {
        let mut state = PRIO_TEST_SEED;
        let produced_nonzero = (0..PRIO_TEST_SEQUENCE_LEN).any(|_| xorshift64(&mut state) != 0);
        assert!(produced_nonzero, "xorshift64 produced only zeros");
    }

    /// Port of `TestXorshift64_Deterministic`.
    #[test]
    fn xorshift64_deterministic() {
        let mut state1 = PRIO_TEST_SEED;
        let mut state2 = PRIO_TEST_SEED;
        for _ in 0..PRIO_TEST_SEQUENCE_LEN {
            assert_eq!(xorshift64(&mut state1), xorshift64(&mut state2));
        }
    }

    /// Port of `TestXorshift64_StateAdvances`.
    #[test]
    fn xorshift64_state_advances() {
        let mut state = PRIO_TEST_SEED;
        let mut prev = state;
        for _ in 0..PRIO_TEST_SEQUENCE_LEN {
            xorshift64(&mut state);
            assert_ne!(state, prev, "state did not advance");
            prev = state;
        }
    }

    /// Port of `TestXorshift64_Distribution`.
    #[test]
    fn xorshift64_distribution() {
        const BUCKET_COUNT: u64 = 16;
        const DIST_LEN: usize = 100_000;
        const MIN_FRACTION: f64 = 0.03;
        const MAX_FRACTION: f64 = 0.10;
        let mut state = PRIO_TEST_SEED;
        let mut buckets = [0usize; BUCKET_COUNT as usize];
        let bucket_size = (u64::from(u32::MAX) + 1) / BUCKET_COUNT;
        for _ in 0..DIST_LEN {
            let val = u64::from(xorshift64(&mut state));
            let mut bucket = (val / bucket_size) as usize;
            if bucket >= BUCKET_COUNT as usize {
                bucket = BUCKET_COUNT as usize - 1;
            }
            buckets[bucket] += 1;
        }
        for (i, &count) in buckets.iter().enumerate() {
            let fraction = count as f64 / DIST_LEN as f64;
            assert!(
                (MIN_FRACTION..=MAX_FRACTION).contains(&fraction),
                "bucket {i}: fraction {fraction:.4} outside [{MIN_FRACTION:.2}, {MAX_FRACTION:.2}]"
            );
        }
    }

    /// Port of `TestMaxDepth_NilRoot`.
    #[test]
    fn max_depth_nil_root() {
        let tl = TreapTimeline::empty();
        assert_eq!(tl.max_depth(), 0);
    }

    /// Port of `TestMaxDepth_NonEmpty`.
    #[test]
    fn max_depth_non_empty() {
        let tl = TreapTimeline::new(0, 500);
        assert!(tl.max_depth() >= 1);
    }

    /// Port of `TestRandomPriority_Depth10K`: 10K sequential inserts keep the
    /// treap shallow (depth < 3 * log2(N)).
    #[test]
    fn random_priority_depth_10k() {
        let mut tl = TreapTimeline::new(0, 1);
        for i in 0..10_000 {
            tl.replace(0, 0, 1, (i % 20) as TimeKey);
        }
        tl.validate();
        let d = tl.max_depth();
        let max_allowed = 3 * (10_000f64.log2() as i64);
        assert!(d <= max_allowed, "tree depth {d} exceeds max {max_allowed}");
    }

    // --- ports of timeline_treap_test.go ---

    /// Port of `TestTreapTimeline_ReplaceAndIterate`.
    #[test]
    fn treap_replace_and_iterate() {
        let mut tl = TreapTimeline::new(0, 1000);
        assert_eq!(tl.len(), 1000);
        tl.validate();

        tl.replace(0, 0, 100, 0);
        assert_eq!(tl.len(), 1100);

        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 40, 1);
        tl.validate();

        let value_at = |tl: &TreapTimeline, line: i64| -> i64 {
            let mut last = 0i64;
            tl.iterate(&mut |offset, length, t| {
                if offset <= line && line < offset + length {
                    last = i64::from(t);
                    return false;
                }
                true
            });
            last
        };
        assert_eq!(value_at(&tl, 50), 1);
        assert_eq!(value_at(&tl, 55), 1);
        assert_eq!(value_at(&tl, 60), 1);
    }

    /// Port of `TestSegments_RoundTrip` (timeline_treap_test.go).
    #[test]
    fn segments_round_trip() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(100, 0, 50, 1);
        tl.replace(200, 30, 0, 2);
        tl.replace(500, 0, 100, 3);
        tl.validate();

        let original_flat = tl.flatten();
        let original_len = tl.len();

        let segs = tl.segments();
        assert!(!segs.is_empty());
        for s in &segs {
            assert_ne!(s.value, TREE_END);
        }

        let mut tl2 = TreapTimeline::empty();
        tl2.reconstruct_from_segments(&segs);
        tl2.validate();
        assert_eq!(tl2.len(), original_len);
        assert_eq!(tl2.flatten(), original_flat);
    }

    /// Port of `TestSegments_Empty`.
    #[test]
    fn segments_empty() {
        let tl = TreapTimeline::empty();
        assert!(tl.segments().is_empty());

        let mut tl2 = TreapTimeline::empty();
        tl2.reconstruct_from_segments(&[]);
        assert_eq!(tl2.len(), 0);
    }

    /// Port of `TestSegments_SingleSegment`.
    #[test]
    fn segments_single_segment() {
        let tl = TreapTimeline::new(5, 42);
        let segs = tl.segments();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].length, 42);
        assert_eq!(segs[0].value, 5);

        let mut tl2 = TreapTimeline::empty();
        tl2.reconstruct_from_segments(&segs);
        tl2.validate();
        assert_eq!(tl2.len(), 42);
    }

    /// Port of `TestErase_PopulatesPool`.
    #[test]
    fn erase_populates_pool() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(0, 0, 5, 1);
        let nodes_before = tl.nodes();
        tl.erase();
        assert!(tl.free_len() >= nodes_before as usize);
        assert_eq!(tl.len(), 0);
    }

    /// Port of `TestReconstruct_UsesPool`.
    #[test]
    fn reconstruct_uses_pool() {
        let mut tl = TreapTimeline::new(0, 1000);
        for i in 0..100 {
            let pos = (i * 31) % 900;
            tl.replace(pos, 3, 5, (i % 50) as TimeKey);
        }
        let nodes_before = tl.nodes();
        let lines = tl.flatten();
        tl.reconstruct(&lines);
        tl.validate();
        let nodes_after = tl.nodes();
        assert_eq!(tl.len(), lines.len() as i64);
        if nodes_before > nodes_after {
            assert!(tl.free_len() > 0, "expected free nodes after Reconstruct");
        }
    }

    /// Port of `TestReplace_PoolReuse`.
    #[test]
    fn replace_pool_reuse() {
        let mut tl = TreapTimeline::new(0, 1000);
        for i in 0..1000 {
            let pos = (i * 31) % 900;
            tl.replace(pos, 3, 5, (i % 50) as TimeKey);
        }
        tl.validate();
        assert!(tl.free_len() > 0, "expected free nodes after many replaces");
    }

    /// Port of `TestCloneDeep_PreservesPRNG` + `TestCloneDeep_IndependentPool`.
    #[test]
    fn clone_deep_independence() {
        let mut tl = TreapTimeline::new(0, 500);
        for i in 0..100 {
            let pos = (i * 31) % 400;
            tl.replace(pos, 2, 3, (i % 20) as TimeKey);
        }
        let mut clone = tl.clone_deep();
        tl.validate();
        clone.validate();
        clone.replace(0, 2, 3, 1);
        clone.validate();
        tl.validate();
        assert_ne!(tl.len(), clone.len());
    }

    // --- ports of coalesce_test.go ---

    /// Port of `TestCoalesceSegments_MergesAdjacent`.
    #[test]
    fn coalesce_segments_merges_adjacent() {
        let segs = [
            Segment { length: 10, value: 1 },
            Segment { length: 20, value: 1 },
            Segment { length: 30, value: 2 },
        ];
        let result = coalesce_segments(&segs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Segment { length: 30, value: 1 });
        assert_eq!(result[1], Segment { length: 30, value: 2 });
    }

    /// Port of `TestCoalesceSegments_NoMerge`.
    #[test]
    fn coalesce_segments_no_merge() {
        let segs = [
            Segment { length: 10, value: 1 },
            Segment { length: 20, value: 2 },
            Segment { length: 30, value: 3 },
        ];
        assert_eq!(coalesce_segments(&segs), segs.to_vec());
    }

    /// Port of `TestCoalesceSegments_Empty`.
    #[test]
    fn coalesce_segments_empty() {
        assert!(coalesce_segments(&[]).is_empty());
    }

    /// Port of `TestCoalesceSegments_SingleSegment`.
    #[test]
    fn coalesce_segments_single_segment() {
        let segs = [Segment { length: 50, value: 1 }];
        assert_eq!(coalesce_segments(&segs), segs.to_vec());
    }

    /// Port of `TestCoalesceSegments_AllSameValue`.
    #[test]
    fn coalesce_segments_all_same_value() {
        let segs = [
            Segment { length: 10, value: 1 },
            Segment { length: 20, value: 1 },
            Segment { length: 30, value: 1 },
        ];
        let result = coalesce_segments(&segs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Segment { length: 60, value: 1 });
    }

    /// Port of `TestMergeAdjacentSameValue_ReducesNodes` (coalesce_test.go).
    #[test]
    fn merge_adjacent_same_value_reduces_nodes() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 20, 1);
        let before = tl.nodes();
        tl.merge_adjacent_same_value();
        let after = tl.nodes();
        assert!(after < before, "expected fewer nodes: before={before}, after={after}");
        tl.validate();
    }

    /// Port of `TestMergeAdjacentSameValue_PreservesLen`.
    #[test]
    fn merge_adjacent_same_value_preserves_len() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 20, 1);
        let before = tl.len();
        tl.merge_adjacent_same_value();
        assert_eq!(tl.len(), before);
    }

    /// Port of `TestMergeAdjacentSameValue_PreservesIterate`.
    #[test]
    fn merge_adjacent_same_value_preserves_iterate() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 20, 1);
        let before_flat = tl.flatten();
        tl.merge_adjacent_same_value();
        assert_eq!(tl.flatten(), before_flat);
    }

    /// Port of `TestMergeAdjacentSameValue_EmptyTimeline`.
    #[test]
    fn merge_adjacent_same_value_empty_timeline() {
        let mut tl = TreapTimeline::empty();
        tl.merge_adjacent_same_value();
        assert_eq!(tl.len(), 0);
        assert_eq!(tl.nodes(), 0);
    }

    /// Port of `TestMergeAdjacentSameValue_AlreadyOptimal`.
    #[test]
    fn merge_adjacent_same_value_already_optimal() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(100, 0, 50, 1);
        let before = tl.nodes();
        let before_flat = tl.flatten();
        tl.merge_adjacent_same_value();
        assert_eq!(tl.nodes(), before);
        assert_eq!(tl.flatten(), before_flat);
    }

    /// Port of `TestMergeAdjacentSameValue_ReplaceAfterCoalesce`.
    #[test]
    fn merge_adjacent_same_value_replace_after_coalesce() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 20, 1);
        tl.merge_adjacent_same_value();
        tl.validate();
        let len_before = tl.len();
        let reports = tl.replace(50, 5, 3, 99);
        tl.validate();
        assert_eq!(tl.len(), len_before + 3 - 5);
        assert!(!reports.is_empty());
    }

    /// Port of `TestMergeAdjacentSameValue_Idempotent` (coalesce_test.go).
    #[test]
    fn merge_adjacent_same_value_idempotent() {
        let mut tl = TreapTimeline::new(0, 1000);
        tl.replace(50, 0, 10, 1);
        tl.replace(60, 0, 20, 1);
        tl.merge_adjacent_same_value();
        let nodes1 = tl.nodes();
        let flat1 = tl.flatten();
        tl.merge_adjacent_same_value();
        assert_eq!(tl.nodes(), nodes1);
        assert_eq!(tl.flatten(), flat1);
    }

    /// Port of `TestMergeAdjacentSameValue_AllSameValue`.
    #[test]
    fn merge_adjacent_same_value_all_same_value() {
        const HEAVY: i64 = 800;
        let mut tl = TreapTimeline::new(1, 10);
        for _ in 0..(HEAVY - 1) {
            tl.replace(0, 0, 10, 1);
        }
        let nodes_before = tl.nodes();
        assert!(nodes_before >= HEAVY / 2, "expected fragmentation, got {nodes_before}");
        tl.merge_adjacent_same_value();
        tl.validate();
        assert_eq!(tl.nodes(), 2, "expected 2 nodes (data + TreeEnd)");
        assert_eq!(tl.len(), HEAVY * 10);
    }

    // --- ports of node_pool_test.go ---

    /// Port of `TestNodePool_AcquireRelease`.
    #[test]
    fn node_pool_acquire_release() {
        let mut tl = TreapTimeline::empty();
        let n = tl.acquire();
        tl.set_node_length(n, 42);
        tl.release(n);
        assert_eq!(tl.free_len(), 1);
    }

    /// Port of `TestNodePool_ReleaseZerosFields`.
    #[test]
    fn node_pool_release_zeros_fields() {
        let mut tl = TreapTimeline::empty();
        let n = tl.acquire();
        tl.set_node_length(n, 42);
        tl.set_node_children(n, Some(n), Some(n));
        tl.release(n);
        let n2 = tl.acquire();
        assert_eq!(tl.node_fields(n2), (0, 0, 0, 0));
    }

    /// Port of `TestNodePool_Reuse`.
    #[test]
    fn node_pool_reuse() {
        let mut tl = TreapTimeline::empty();
        let n = tl.acquire();
        tl.release(n);
        assert_eq!(tl.acquire(), n, "expected reuse of released node");
    }

    /// Port of `TestNodePool_GrowsOnDemand`.
    #[test]
    fn node_pool_grows_on_demand() {
        let mut tl = TreapTimeline::empty();
        let nodes: Vec<_> = (0..100).map(|_| tl.acquire()).collect();
        let mut seen = std::collections::HashSet::new();
        for n in nodes {
            assert!(seen.insert(n), "duplicate node");
        }
    }

    /// Port of `TestNodePool_ReleaseSubtree`.
    #[test]
    fn node_pool_release_subtree() {
        let mut tl = TreapTimeline::empty();
        let root = tl.acquire();
        let left = tl.acquire();
        let right = tl.acquire();
        tl.set_node_children(root, Some(left), Some(right));
        tl.release_subtree(Some(root));
        assert_eq!(tl.free_len(), 3);
    }

    /// Port of `TestNodePool_ReleaseNil`.
    #[test]
    fn node_pool_release_nil() {
        let mut tl = TreapTimeline::empty();
        tl.release_subtree(None);
        assert_eq!(tl.free_len(), 0);
    }

    /// `shrink_pool` trims the free-list (Go `ShrinkPool` / `nodePool.shrink`).
    #[test]
    fn node_pool_shrink() {
        let mut tl = TreapTimeline::empty();
        let nodes: Vec<_> = (0..10).map(|_| tl.acquire()).collect();
        for n in nodes {
            tl.release(n);
        }
        tl.shrink_pool(5);
        assert_eq!(tl.free_len(), 5);
    }

    /// `shrink_pool` with `keep >= len` is a no-op.
    #[test]
    fn node_pool_shrink_no_op() {
        let mut tl = TreapTimeline::empty();
        let n = tl.acquire();
        tl.release(n);
        tl.shrink_pool(10);
        assert_eq!(tl.free_len(), 1);
    }
}
