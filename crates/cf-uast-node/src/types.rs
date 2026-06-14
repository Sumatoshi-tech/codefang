//! DSL AST node types.
//!
//! The DSL AST is a single enum ([`DslNode`]); lowering pattern-matches on the
//! variants.

/// A literal value parsed from a DSL expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DslLiteral {
    /// A quoted string literal.
    Str(String),
    /// A numeric literal (kept as text to preserve the exact token).
    Number(String),
    /// A boolean literal.
    Bool(bool),
}

/// A node in the DSL abstract syntax tree.
///
/// Each variant's canonical name string is exposed via [`DslNode::type_name`].
#[derive(Debug, Clone, PartialEq)]
pub enum DslNode {
    /// `map(<expr>)` — applies `expr` to each input node.
    Map(Box<Self>),
    /// `filter(<expr>)` — keeps nodes for which `expr` holds.
    Filter(Box<Self>),
    /// `reduce(<expr>)` — fold (currently identity).
    Reduce(Box<Self>),
    /// Field access: `.a.b.c` → `["a","b","c"]`.
    Field(Vec<String>),
    /// A literal value.
    Literal(DslLiteral),
    /// A function call `name(arg, ...)`.
    Call { name: String, args: Vec<Self> },
    /// A pipeline of stages separated by `|`.
    Pipeline(Vec<Self>),
    /// `rmap(<expr>)` — reverse map (identity).
    RMap(Box<Self>),
    /// `rfilter(<expr>)` — reverse filter (identity).
    RFilter(Box<Self>),
    /// A comparison `<lhs> <op> <rhs>` produced by the grammar's `Comparison`.
    Comparison { lhs: Box<Self>, op: String, rhs: Box<Self> },
}

impl DslNode {
    /// Returns the canonical name string for this node kind (`"Map"`,
    /// `"Filter"`, `"Reduce"`, `"Field"`, `"Literal"`, `"Call"`, `"Pipeline"`,
    /// `"RMap"`, `"RFilter"`, `"Comparison"`).
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Map(_) => "Map",
            Self::Filter(_) => "Filter",
            Self::Reduce(_) => "Reduce",
            Self::Field(_) => "Field",
            Self::Literal(_) => "Literal",
            Self::Call { .. } => "Call",
            Self::Pipeline(_) => "Pipeline",
            Self::RMap(_) => "RMap",
            Self::RFilter(_) => "RFilter",
            Self::Comparison { .. } => "Comparison",
        }
    }
}
