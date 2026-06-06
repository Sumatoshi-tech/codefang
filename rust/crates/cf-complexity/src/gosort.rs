//! Faithful port of Go's `sort.Slice` (pattern-defeating quicksort, `pdqsort`).
//!
//! The implementation now lives in the shared [`cf_gosort`] crate so multiple
//! analyzers (complexity, devs, …) reproduce Go's exact unstable-sort
//! permutation for tie ordering. This module re-exports it for backward
//! compatibility within `cf-complexity`.

pub use cf_gosort::go_sort_slice;
