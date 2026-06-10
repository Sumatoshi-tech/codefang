//! `cf-uast-mapping` — UAST mapping PEG DSL, rule engine, and native
//! tree-sitter query/capture compiler.
//!
//! Port of the Go package `pkg/uast/pkg/mapping`. Used by `cf-uast` and the
//! `uast` binary.
//!
//! # Modules
//!
//! - [`mapping_types`] — core data model (`NodeTypeInfo`, `FieldInfo`,
//!   `ChildInfo`, `NodeCategory`, `Rule`, `Condition`, `UastSpec`). Port of
//!   `mapping_types.go`.
//! - [`dsl_parser`] — the mapping DSL parser. Ports `dsl_parser.go` plus the
//!   `mapping.peg` grammar as a hand-written PEG parser (the generated
//!   `mapping.peg.go` is intentionally NOT hand-translated; see
//!   [`dsl_parser`] docs and DESIGN rule 6).
//! - [`grammar_analysis`] — node-type parsing, heuristic classification,
//!   coverage analysis, and DSL generation. Port of `grammar_analysis.go`.
//! - [`pattern_matcher`] — the native tree-sitter query/capture compiler and
//!   matcher (the piece tree-sitter does not provide). Port of
//!   `pattern_matcher.go`.
//! - [`vocab`] — the closed mapping vocabularies (`UastType`, `Role`,
//!   `TokenSource`) extracted from the `.uastmap` corpus; the typed foundation
//!   of the Rust-native mapping definitions (specs/uastmap-rust-macros).
//!
//! # Serialization rule
//!
//! Any MACHINE-format report bytes produced from this crate's types MUST be
//! routed through the shared `cf-gojson` / `cf-goyaml` crates, never raw
//! `serde_json`/`serde_yaml`, per DESIGN §2.

#![forbid(unsafe_code)]

pub mod dsl_parser;
pub mod grammar_analysis;
pub mod mapping_types;
pub mod pattern_matcher;
#[macro_use]
pub mod macros;
pub mod static_model;
pub mod vocab;

pub use dsl_parser::{LanguageInfo, Parser};
pub use mapping_types::{
    ChildInfo, Condition, FieldInfo, NodeCategory, NodeTypeInfo, Rule, UastSpec,
};
pub use pattern_matcher::{MatchError, PatternMatcher};
pub use static_model::{LanguageMapping, MappingRule};
pub use vocab::{Role, TokenSource, UastType};
