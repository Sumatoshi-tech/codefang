//! DSL runtime entry points and error type.

use crate::dsl::lowering::lower_node;
use crate::dsl::parser::ParseError;
use crate::node::Node;
use crate::types::DslNode;

/// An executable query: maps an input node set to an output node set.
pub type QueryFn = Box<dyn Fn(Vec<Node>) -> Vec<Node>>;

/// Errors produced while parsing/lowering/running a DSL query.
///
/// The rendered error strings are part of the CLI compatibility contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DslError {
    /// The query string was empty.
    #[error("query string is empty")]
    EmptyQuery,
    /// The query failed to parse.
    #[error("DSL parse error: {0}")]
    Parse(ParseError),
    /// A DSL node kind cannot be lowered as a query root.
    #[error("unknown node type: {0}")]
    UnknownNodeType(String),
}

/// Lowers a parsed AST into an executable [`QueryFn`].
///
/// # Errors
///
/// Returns [`DslError::UnknownNodeType`] for AST kinds that cannot appear as a
/// query root (bare literals and comparisons).
pub fn lower(ast: &DslNode) -> Result<QueryFn, DslError> {
    lower_node(ast)
}
