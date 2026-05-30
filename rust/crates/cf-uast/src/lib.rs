//! `cf-uast` — the aggregate UAST parser facade.
//!
//! Rust port of the Go package `pkg/uast` (the package root, *not* its
//! sub-packages, which live in sibling crates). It provides:
//!
//! * [`Parser`] — the entry point: detect support, resolve a language, parse a
//!   file or bytes into a UAST (port of `parser.go` / `parsefile.go`).
//! * [`Loader`] — the lazy language loader with a fixed 512-bit extension bloom
//!   filter (port of `loader.go`).
//! * [`detect_changes`] — structural diffing between two UAST trees (port of
//!   `changes.go`).
//! * [`LanguageParser`], [`Map`], [`NodeChange`], [`ChangeType`],
//!   [`get_file_extension`] — the facade types (port of `types.go`).
//! * [`languages`] — language-name → tree-sitter grammar dispatch (port of
//!   `languages.go`).
//!
//! # Relationship to the sibling crates
//!
//! The original Go package depends on its own sub-packages `pkg/uast/pkg/node`
//! and `pkg/uast/pkg/mapping`, plus the embedded `uastmaps/*.uastmap` data and
//! the generated `embedded_mappings.gen.go`. In the Rust workspace those are
//! separate crates (DESIGN §1):
//!
//! * [`cf_uast_node`] — the [`Node`](cf_uast_node::Node) tree and its
//!   byte-sorted `ToMap` serialization (DESIGN §2.2).
//! * [`cf_uast_mapping`] — the mapping DSL parser and native tree-sitter
//!   query/capture compiler.
//! * [`cf_uast_uastmaps`] — the regenerated embedded `.uastmap` tables (the
//!   `build.rs` port of the generator; replaces the 2.7 MB
//!   `embedded_mappings.gen.go`, DESIGN §5 / rule 6).
//!
//! This crate deliberately does **not** depend on `cf-framework`, so the `uast`
//! binary remains the first end-to-end-shippable artifact (DESIGN §1.1).
//!
//! # Byte-identity
//!
//! Any MACHINE-format report bytes produced from a parsed [`Node`] (e.g. `uast
//! parse --format json`) must route through `cf-gojson` via
//! [`cf_uast_node::encode_compact`] / the node's map-origin
//! [`GoValue`](cf_uast_node::GoValue) — never `serde_json` (DESIGN §2).

#![forbid(unsafe_code)]

mod changes;
mod loader;
mod parser;
mod parsefile;
mod types;

pub mod languages;

pub use changes::detect_changes;
pub use loader::{
    embedded_mappings_available, Loader, PrecompiledMapping,
};
pub use parser::{MappingInfo, Parser};
pub use types::{
    get_file_extension, ChangeType, LanguageParser, Map, NodeChange, ParseError,
    CONFIG_UAST_PROVIDER,
};

// Re-export the node type so facade callers can use it without an explicit
// `cf-uast-node` dependency (mirroring how Go callers use `node.Node` via the
// `uast` package surface).
pub use cf_uast_node::{Node, Positions};

/// The dependency name provided by UAST change detection (Go
/// `DependencyUastChanges`).
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
