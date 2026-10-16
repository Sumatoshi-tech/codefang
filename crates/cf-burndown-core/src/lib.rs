//! Burndown matrix core data model: line-survival treaps.
//!
//! File-level line-interval tracking used by the burndown analyzer to compute
//! line-survival ("burndown") matrices.
//!
//! # Overview
//!
//! - [`File`] encapsulates a [`TreapTimeline`] plus a set of [`Updater`]
//!   callbacks. A segment value is a plain tick, or a `(person, tick)` pair
//!   packed by a [`ValuePacking`] when the caller tracks authors. [`File::update`] applies an edit (insert/delete a line range at
//!   a position) and notifies the updaters with `(current, previous, delta)`
//!   reports so the caller can maintain its burndown counters.
//! - [`TreapTimeline`] is an implicit treap where a node's position is the size
//!   of its left subtree, so edits are `O(log N)` without shifting keys. Node
//!   priorities come from a deterministic xorshift64 PRNG seeded with a fixed
//!   constant, so treap shapes (and therefore iteration and serialization) are
//!   reproducible.
//! - [`File::query_range`] answers line-ownership overlap queries via a lazily
//!   built interval tree.
//!
//! # Example
//!
//! ```
//! use cf_burndown_core::File;
//!
//! // A file that starts at tick 0 with 100 lines and no updaters.
//! let mut file = File::new(0, 100, Vec::new());
//! assert_eq!(file.len(), 100);
//! assert!(!file.is_empty());
//!
//! // At tick 1, insert 10 lines at position 50 and delete 5 there.
//! file.update(1, 50, 10, 5);
//! assert_eq!(file.len(), 100 + 10 - 5);
//!
//! // The treap shape is deterministic, so integrity always validates.
//! file.validate();
//! ```
//!
//! # Compatibility note
//!
//! This crate performs no report serialization itself; it only exposes a debug
//! [`File::dump`] string and [`Segment`] values that the burndown analyzer
//! serializes downstream. Machine-format report bytes must be produced by the
//! shared `cf-gojson` / `cf-goyaml` crates (the byte contract is pinned by
//! the report e2e suites (`bins/codefang/tests`)), so this crate intentionally has no serialization
//! dependency: `cf-analyzer-burndown` is responsible for routing [`Segment`] /
//! matrix data through `cf-gojson`.

#![forbid(unsafe_code)]

mod file;
mod interval;
mod packing;
mod range_query;
mod timeline;
mod timeline_treap;

pub use file::{File, Updater};
pub use packing::{ValuePacking, MAX_TICK_BITS};
pub use range_query::OwnershipSegment;
pub use timeline::{DeltaReport, TimeKey, Timeline};
pub use timeline_treap::{Segment, TreapTimeline};

/// The value of the last leaf in the tree (the `u32::MAX` sentinel). Every
/// packed `(person, tick)` value a [`ValuePacking`] produces is below it.
pub const TREE_END: TimeKey = u32::MAX;

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree-end sentinel is the one reserved value.
    #[test]
    fn tree_end_is_the_only_sentinel() {
        assert_eq!(TREE_END, u32::MAX);
    }
}
