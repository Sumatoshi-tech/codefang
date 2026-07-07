//! `cf-analyzers-common` — placeholder crate reserved for shared analyzer
//! helpers.
//!
//! Documented in specs/rust-rewrite/DESIGN.md §1. The former stand-in modules
//! (classifier, threshold labeler, metrics processor, result builder, data
//! extraction, reporter/formatter, identity mixin, hibernation, spillable
//! collector, and the local Report/Node models) were removed as dead code:
//! their semantics live inline in the analyzer crates that need them
//! (cf-clones, cf-cohesion, cf-comments, cf-halstead, cf-typos and the
//! cf-commands handlers), and the shared models live in cf-analyze,
//! cf-uast-node, cf-streaming, cf-safeconv and cf-spillstore.

/// Crate name, used by smoke tests to confirm the module links.
///
/// ```
/// assert_eq!(cf_analyzers_common::CRATE_NAME, "cf-analyzers-common");
/// ```
pub const CRATE_NAME: &str = "cf-analyzers-common";
