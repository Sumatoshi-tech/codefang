//! Top-level generic algorithm utilities.
//!
//! Port of the Go top-level `pkg/alg` package (the package-level files
//! `chunk.go`, `iter.go`, `pairs.go`, `tree.go`). The sub-packages of
//! `pkg/alg` (`bloom`, `cms`, `hll`, `interval`, `levenshtein`, `lru`, `lsh`,
//! `mapx`, `minhash`, `stats`, `internal/hashutil`) are ported into their own
//! dedicated crates (`cf-alg-bloom`, `cf-alg-cms`, ...), per the rewrite design.
//!
//! These are pure, allocation-light helpers: range chunking, a pull-based
//! iterator collector, unordered-pair enumeration, and iterative tree
//! traversal. None of them participate in report serialization, so no
//! Go-compat serializer is required here.

mod chunk;
mod iter;
mod pairs;
mod tree;

pub use chunk::{chunk, Range};
pub use iter::{collect_n, IteratorError, PullIterator};
pub use pairs::for_each_pair;
pub use tree::traverse_tree;
