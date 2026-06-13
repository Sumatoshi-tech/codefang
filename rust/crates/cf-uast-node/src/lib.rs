//! `cf-uast-node` — the canonical UAST (Universal Abstract Syntax Tree) node
//! representation and the operations that travel with it.
//!
//! This is the keystone data type of codefang: it is imported by `uast`,
//! `framework`, `analyze`, and nearly every analyzer. The most
//! byte-identity-critical export is [`Node::to_map`], whose output is serialized
//! into machine reports — its keys (`children`, `id`, `pos`, `props`, `roles`,
//! `token`, `type`) must emit in raw-UTF-8 byte order (report-format contract;
//! pinned by `rust/tests/compat`). To guarantee that, [`Node::to_map`] returns a
//! [`GoValue`] built from a map-origin [`GoMap`] (sort-on-encode), never
//! `serde_json`. See DESIGN.md §2.2.
//!
//! # Relationship to `cf-gojson`
//!
//! The design routes all report serialization through the shared `cf-gojson`
//! crate. [`Node::to_map`] builds its output directly from
//! [`cf_gojson::GoValue`] / [`cf_gojson::GoMap`]; those two types are
//! re-exported here so callers can construct and serialize `to_map` output
//! without naming the encoder crate themselves.
//!
//! # Module overview
//!
//! - [`Node::new`] constructs nodes directly; the per-worker free-list
//!   [`Allocator`] is an optional object pool for parse-heavy workloads
//!   (pooling is purely a performance device and never affects output bytes).
//! - Traversal comes in an eager flavor ([`Node::pre_order`] returning a `Vec`)
//!   and zero-allocation callback flavors ([`Node::visit_pre_order`] /
//!   [`Node::visit_post_order`]).
//! - The query DSL (`map`/`filter`/`reduce`/field-access pipelines) lives in the
//!   [`dsl`] module; [`dsl::parse`] is a hand-written recursive-descent parser
//!   for the PEG grammar documented there.
//!
//! # Example
//!
//! Build a small tree, traverse it, and inspect its byte-sorted map form:
//!
//! ```
//! use cf_uast_node::{GoValue, Node};
//!
//! let mut file = Node::with_token("File", "");
//! file.add_child(Node::with_token("Function", "main"));
//! file.add_child(Node::literal("42"));
//!
//! // Pre-order traversal visits the root, then children left-to-right.
//! let tokens: Vec<&str> = file.pre_order().iter().map(|n| n.token.as_str()).collect();
//! assert_eq!(tokens, ["", "main", "42"]);
//!
//! // `to_map` returns a map-origin GoValue whose keys serialize in byte order
//! // (the report-format contract). A node with children emits exactly:
//! // children, pos, roles, token, type — in that byte-sorted sequence.
//! let GoValue::Map(map) = file.to_map() else { panic!("expected object") };
//! let keys: Vec<&str> = map.encode_order().iter().map(|e| e.0.as_str()).collect();
//! assert_eq!(keys, ["children", "pos", "roles", "type"]);
//! ```

#![forbid(unsafe_code)]

mod allocator;
mod classifier;
mod comparison;
pub mod dsl;
mod node;
mod tomap;
mod traversal;
mod types;

// Re-exported so callers can build/serialize `to_map` output without naming
// the encoder crate themselves (DESIGN.md §2.2).
pub use cf_gojson::{GoMap, GoValue};

pub use allocator::{release_tree, Allocator};
pub use classifier::is_literal_type;
pub use node::{Builder, Node, Positions, Role, Type};

// Re-export the UAST type and role string constants at the crate root so
// callers can write `cf_uast_node::UAST_FUNCTION` without the module path.
pub use node::roles::*;
pub use node::uast_types::*;
