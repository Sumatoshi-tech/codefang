//! `cf-halstead` — Halstead complexity metrics analyzer.
//!
//! The full per-function/per-file report builder, formatter, visitor, and
//! aggregator for the static pipeline live in
//! `cf-commands/src/handlers/static_halstead.rs`. The module surface exposed
//! here is the part the **quality** history analyzer consumes: the
//! operator/operand [`detector`], the derived-metric [`calculator`], and the
//! standalone file-level [`analyze`].
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `tests/compat`.
//!
//! # Example
//!
//! Run the standalone file-level analysis over a one-function UAST and read the
//! file-level measures. The function body `x = y + 5` contributes the operators
//! `=`/`+` and the operands `x`/`y`/`5`, so the volume is positive:
//!
//! ```
//! use cf_halstead::analyze;
//! use cf_uast_node::Node;
//!
//! fn id(token: &str) -> Node {
//!     let mut n = Node::with_token("Identifier", token);
//!     n.roles = vec!["Variable".into()];
//!     n
//! }
//! fn lit(token: &str) -> Node {
//!     let mut n = Node::with_token("Literal", token);
//!     n.roles = vec!["Literal".into()];
//!     n
//! }
//!
//! // function f() { x = y + 5 }
//! let mut plus = Node::with_token("BinaryOp", "+");
//! plus.props.insert("operator".into(), "+".into());
//! plus.add_child(id("y"));
//! plus.add_child(lit("5"));
//!
//! let mut assign = Node::with_token("Assignment", "=");
//! assign.props.insert("operator".into(), "=".into());
//! assign.add_child(id("x"));
//! assign.add_child(plus);
//!
//! let mut func = Node::with_token("Function", "");
//! func.roles = vec!["Function".into(), "Declaration".into()];
//! func.add_child(assign);
//!
//! let mut root = Node::with_token("File", "");
//! root.add_child(func);
//!
//! let h = analyze(&root);
//! assert!(h.volume > 0.0, "non-empty function has positive volume");
//! assert!(h.effort > 0.0);
//! ```
//!
//! A tree with no functions yields all-zero measures:
//!
//! ```
//! use cf_halstead::{analyze, FileHalstead};
//! use cf_uast_node::Node;
//!
//! let root = Node::with_token("File", "");
//! assert_eq!(analyze(&root), FileHalstead::default());
//! ```

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-halstead";

/// Analyzer name as it appears in reports (`analyzer_name`).
pub const ANALYZER_NAME: &str = "halstead";

/// Minimum total tokens (operators + operands) before the count-min-sketch
/// path activates. The CMS path only affects the `estimated_total_*` fields,
/// which the quality analyzer does not read.
pub const CMS_TOKEN_THRESHOLD: i64 = 1000;

/// CMS approximation error bound.
pub const CMS_EPSILON: f64 = 0.001;

/// CMS failure probability.
pub const CMS_DELTA: f64 = 0.01;

/// Maximum UAST traversal depth for function discovery.
pub const MAX_DEPTH: i64 = 10;

pub mod calculator;
pub mod detector;
pub mod standalone;

pub use standalone::{analyze, FileHalstead};
