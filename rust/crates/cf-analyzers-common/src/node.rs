//! Minimal UAST node model used by data extraction and traversal.
//!
//! The authoritative `Node` type lives in `pkg/uast/pkg/node/node.go` and is
//! owned by the `cf-uast-node` crate. To keep `cf-analyzers-common`'s public
//! surface independent of that crate's evolving internal representation, this
//! module defines only the fields and methods the data-extraction and traversal
//! helpers actually touch: `node_type`, `token`, `roles`, `props`, `pos`,
//! `children`, and `has_any_role`. The field/method surface here is a strict
//! subset of the Go type; consolidating onto `cf_uast_node::Node` is tracked in
//! the crate-level roadmap note in `lib.rs`.

use std::collections::BTreeMap;

/// Source position of a node, mirroring `node.Positions`.
///
/// Lines, columns, and offsets are unsigned, matching the Go `uint` fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Positions {
    /// 1-based start line.
    pub start_line: u32,
    /// 1-based end line.
    pub end_line: u32,
    /// Start column.
    pub start_col: u32,
    /// End column.
    pub end_col: u32,
    /// Byte offset of the start.
    pub start_offset: u32,
    /// Byte offset of the end.
    pub end_offset: u32,
}

/// A UAST node, mirroring the subset of `node.Node` used by this crate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    /// The node type (e.g. `"Function"`).
    pub node_type: String,
    /// The node token (literal text), empty when absent.
    pub token: String,
    /// Semantic roles attached to the node.
    pub roles: Vec<String>,
    /// String-keyed properties (byte-sorted via [`BTreeMap`]).
    pub props: BTreeMap<String, String>,
    /// Position information, absent when the node has no position.
    pub pos: Option<Positions>,
    /// Child nodes in source order.
    pub children: Vec<Node>,
}

impl Node {
    /// Reports whether the node carries the given role, mirroring
    /// `node.Node.HasAnyRole` for a single role query.
    #[must_use]
    pub fn has_any_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}
