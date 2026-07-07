//! Re-export of the shared unstable-sort port (`cf_gosort`).
//!
//! The report contract pins tie ordering to a specific pattern-defeating
//! quicksort (pdqsort) permutation; the implementation lives in the shared
//! [`cf_gosort`] crate so every analyzer reproduces the exact element
//! movement. This module re-exports it for use within `cf-complexity`.

pub use cf_gosort::go_sort_slice;
