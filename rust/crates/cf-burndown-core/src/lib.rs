//! Burndown matrix core data model: Hercules-style line-survival treaps.
//!
//! This crate is the Rust port of the Go package `internal/burndown`. It
//! provides file-level line-interval tracking used by the burndown analyzer to
//! compute line-survival ("burndown") matrices.
//!
//! # Overview
//!
//! - [`File`] encapsulates a [`TreapTimeline`] plus a set of [`Updater`]
//!   callbacks. [`File::update`] applies an edit (insert/delete a line range at
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
//! # Byte-identity note
//!
//! The Go `internal/burndown` package performs no report serialization itself
//! (it imports no `encoding/json` / `yaml`); it only exposes a debug
//! [`File::dump`] string and [`Segment`] values that the burndown analyzer
//! serializes downstream. Per the rewrite design, any MACHINE-format report
//! bytes must be produced by the shared `cf-gojson` / `cf-goyaml` crates — so
//! this crate intentionally has no serialization dependency, and the burndown
//! analyzer crate (`cf-analyzer-burndown`, not yet ported) is responsible for
//! routing [`Segment`] / matrix data through `cf-gojson`.

#![forbid(unsafe_code)]

mod file;
mod interval;
mod range_query;
mod timeline;
mod timeline_treap;

pub use file::{File, Updater};
pub use range_query::OwnershipSegment;
pub use timeline::{DeltaReport, TimeKey, Timeline};
pub use timeline_treap::{Segment, TreapTimeline};

/// The value of the last leaf in the tree (`math.MaxUint32`).
///
/// Mirrors the Go constant `TreeEnd`.
pub const TREE_END: TimeKey = u32::MAX;

/// The binary power corresponding to the maximum tick that can be stored.
///
/// Mirrors the Go constant `TreeMaxBinPower`.
pub const TREE_MAX_BIN_POWER: u32 = 14;

/// The special "day" which disables status updates; used in [`File::merge`].
///
/// Equals `(1 << TREE_MAX_BIN_POWER) - 1`. Mirrors the Go constant
/// `TreeMergeMark`.
pub const TREE_MERGE_MARK: TimeKey = (1 << TREE_MAX_BIN_POWER) - 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported constants match the Go `internal/burndown` constants.
    #[test]
    fn constants_match_go() {
        assert_eq!(TREE_END, u32::MAX);
        assert_eq!(TREE_MAX_BIN_POWER, 14);
        assert_eq!(TREE_MERGE_MARK, 16383);
    }
}
