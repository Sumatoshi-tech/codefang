//! Generic algorithm utilities: range chunking, a pull-based iterator
//! collector, unordered-pair enumeration, and iterative tree traversal.
//!
//! These are pure, allocation-light helpers. More specialized algorithm
//! families (bloom filters, sketches, LRU caches, ...) live in their own
//! dedicated `cf-alg-*` crates. Nothing here participates in report
//! serialization, so no compatibility serializer is required.

mod chunk;
mod iter;
mod pairs;
mod tree;

pub use chunk::{chunk, Range};
pub use iter::{collect_n, IteratorError, PullIterator};
pub use pairs::for_each_pair;
pub use tree::traverse_tree;
