//! DSL runtime entry points and error type.

use crate::dsl::lowering::lower_node;
use crate::dsl::parser::ParseError;
use crate::node::Node;
use crate::types::DslNode;

/// An executable query: maps an input node set to an output node set. The Rust
/// analogue of Go's `QueryFunc = func([]*Node) []*Node`.
pub type QueryFn = Box<dyn Fn(Vec<Node>) -> Vec<Node>>;

/// Errors produced while parsing/lowering/running a DSL query. Mirrors the Go
/// sentinel set (`errEmptyQuery`, `errUnknownNodeType`, `errInvalidNodeType`)
/// and the wrapped parse/lowering errors from `FindDSL`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DslError {
    /// The query string was empty (`"query string is empty"`).
    EmptyQuery,
    /// The query failed to parse (`"DSL parse error: ..."`).
    Parse(ParseError),
    /// A DSL node had no lowerer (`"unknown node type: ..."`).
    UnknownNodeType(String),
}

impl std::fmt::Display for DslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DslError::EmptyQuery => write!(f, "query string is empty"),
            DslError::Parse(e) => write!(f, "DSL parse error: {e}"),
            DslError::UnknownNodeType(t) => write!(f, "unknown node type: {t}"),
        }
    }
}

impl std::error::Error for DslError {}

/// Lowers a parsed AST into an executable [`QueryFn`]. Mirrors `LowerDSL`.
pub fn lower(ast: &DslNode) -> Result<QueryFn, DslError> {
    lower_node(ast)
}
