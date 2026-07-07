//! `cf-uast` — the aggregate UAST parser facade.
//!
//! It provides:
//!
//! * [`Parser`] — the entry point: detect support, resolve a language, parse a
//!   file or bytes into a UAST.
//! * [`Loader`] — the lazy language loader with a fixed 512-bit extension bloom
//!   filter.
//! * [`detect_changes`] — structural diffing between two UAST trees.
//! * [`LanguageParser`], [`Map`], [`NodeChange`], [`ChangeType`],
//!   [`get_file_extension`] — the facade types.
//! * [`languages`] — language-name → tree-sitter grammar dispatch.
//!
//! # Relationship to the sibling crates
//!
//! The UAST stack is split across crates (DESIGN §1):
//!
//! * [`cf_uast_node`] — the [`Node`](cf_uast_node::Node) tree and its
//!   byte-sorted `ToMap` serialization (DESIGN §2.2).
//! * [`cf_uast_mapping`] — the mapping DSL parser and native tree-sitter
//!   query/capture compiler.
//! * [`cf_uast_mappings`] — the native per-language mapping tables that drive
//!   parsing (the mapping system of record).
//!
//! This crate deliberately does **not** depend on `cf-framework`, so the `uast`
//! binary remains the first end-to-end-shippable artifact (DESIGN §1.1).
//!
//! Compatibility: parse trees and report bytes are pinned against the
//! reference implementation by `tests/compat`.
//!
//! # Byte-identity
//!
//! Any MACHINE-format report bytes produced from a parsed [`Node`] (e.g. `uast
//! parse --format json`) must route through `cf-gojson`: build the node's
//! map-origin [`GoValue`](cf_uast_node::GoValue) with
//! [`Node::to_map`](cf_uast_node::Node::to_map), then encode it with the
//! `cf-gojson` marshaller — never `serde_json` (DESIGN §2).

// `deny` rather than `forbid`: the only `unsafe` in this crate is the FFI call
// into the vendored tree-sitter grammar C entry points in `languages.rs`, which
// is locally `#[allow(unsafe_code)]`-gated and documented with a SAFETY note.
#![deny(unsafe_code)]

mod changes;
mod loader;
mod lowering;
mod parsefile;
mod parser;
mod types;

pub mod languages;

pub use changes::detect_changes;
pub use loader::{embedded_mappings_available, Loader, PrecompiledMapping};
pub use parser::Parser;
pub use types::{
    get_file_extension, ChangeType, LanguageParser, Map, NodeChange, ParseError,
    CONFIG_UAST_PROVIDER,
};

// Re-export the node type so facade callers can use it without an explicit
// `cf-uast-node` dependency.
pub use cf_uast_node::{Node, Positions};

/// The dependency name provided by UAST change detection.
pub const DEPENDENCY_UAST_CHANGES: &str = "uast_changes";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_smoke() {
        let p = Parser::new();
        assert!(p.is_supported("main.go"));
        assert_eq!(p.get_language("lib.rs"), "rust");
    }

    #[test]
    fn dependency_constant() {
        assert_eq!(DEPENDENCY_UAST_CHANGES, "uast_changes");
        assert_eq!(CONFIG_UAST_PROVIDER, "UAST.Provider");
    }
}
