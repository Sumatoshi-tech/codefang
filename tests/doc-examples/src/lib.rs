//! Marker crate for the Markdown-example test harness.
//!
//! All logic lives in `tests/doc_examples.rs`. This crate carries no runtime
//! code; it exists so `cargo test -p doc-examples` builds the two binaries
//! (dev-deps) and then executes the documentation examples against them.
