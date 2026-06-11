//! `cf-analyzers-common` — placeholder crate for Go's internal/analyzers/common.
//!
//! Port target documented in specs/rust-rewrite/DESIGN.md §1. The former
//! stand-in modules (classifier, threshold labeler, metrics processor, result
//! builder, data extraction, reporter/formatter, identity mixin, hibernation,
//! spillable collector, and the local Report/Node models) were removed as dead
//! code: every Go consumer's semantics is ported inline in the linked analyzer
//! crates (cf-clones, cf-cohesion, cf-comments, cf-halstead, cf-typos and the
//! cf-commands handlers), and the shared models live in cf-analyze,
//! cf-uast-node, cf-streaming, cf-safeconv and cf-spillstore.

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-analyzers-common";
