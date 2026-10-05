//! DSL runtime entry points: the compiled [`Query`], the error type and the
//! executable stage function type.

use crate::dsl::lowering::lower_node;
use crate::dsl::parser::{parse, ParseError};
use crate::node::Node;
use crate::types::DslNode;

/// An executable stage: maps an input node set to an output node set.
pub type QueryFn = Box<dyn Fn(&[Node]) -> Vec<Node>>;

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

/// Where a query starts relative to the root it runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialInput {
    /// The root's direct children (`filter(...)` roots and most pipelines).
    Children,
    /// The root itself (recursive stages, field access, builtins).
    Root,
}

/// Chooses the initial input set for a query: a `filter(...)` root and
/// pipelines start from the children, except a pipeline whose first stage is
/// `map(.children)`, a recursive `rfilter(...)` / `rmap(...)` stage, which
/// start from the root itself; any other root also starts from the root.
fn initial_input(ast: &DslNode) -> InitialInput {
    match ast {
        DslNode::Filter(_) => InitialInput::Children,
        DslNode::Pipeline(stages) => match stages.first() {
            Some(DslNode::Map(expr)) if is_children_field(expr) => InitialInput::Root,
            Some(DslNode::RFilter(_) | DslNode::RMap(_)) => InitialInput::Root,
            _ => InitialInput::Children,
        },
        _ => InitialInput::Root,
    }
}

/// Whether `expr` is exactly the field access `.children`.
fn is_children_field(expr: &DslNode) -> bool {
    matches!(expr, DslNode::Field(fields) if fields.len() == 1 && fields[0] == "children")
}

/// A DSL query parsed and lowered once, runnable against any number of roots
/// (the per-file hot path of the history analyzers compiles the query once per
/// commit instead of once per parsed tree).
pub struct Query {
    initial: InitialInput,
    run: QueryFn,
}

impl Query {
    /// Parses and lowers `query`.
    ///
    /// # Errors
    ///
    /// [`DslError::EmptyQuery`] for an empty string, [`DslError::Parse`] for a
    /// grammar violation, [`DslError::UnknownNodeType`] for a root kind that
    /// cannot be lowered (a bare literal or comparison).
    pub fn compile(query: &str) -> Result<Self, DslError> {
        if query.is_empty() {
            return Err(DslError::EmptyQuery);
        }
        let ast = parse(query).map_err(DslError::Parse)?;
        let initial = initial_input(&ast);
        let run = lower(&ast)?;
        Ok(Self { initial, run })
    }

    /// Runs the query against `root`, returning the matching nodes (cloned).
    #[must_use]
    pub fn run(&self, root: &Node) -> Vec<Node> {
        match self.initial {
            InitialInput::Children => (self.run)(&root.children),
            InitialInput::Root => (self.run)(std::slice::from_ref(root)),
        }
    }
}

impl std::fmt::Debug for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("initial", &self.initial)
            .finish_non_exhaustive()
    }
}
