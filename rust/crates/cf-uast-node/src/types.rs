//! DSL AST node types. Ported from `types.go`.
//!
//! Go models the DSL AST as a set of separate struct types behind the empty
//! interface `DSLNode any`. In Rust the natural encoding is a single enum
//! ([`DslNode`]) — pattern matching replaces Go's type switches in `lowering.go`.

/// A literal value parsed from a DSL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum DslLiteral {
    /// A quoted string literal.
    Str(String),
    /// A numeric literal (kept as text to preserve the exact token, as Go does).
    Number(String),
    /// A boolean literal.
    Bool(bool),
}

/// A node in the DSL abstract syntax tree.
///
/// Each variant corresponds to one of Go's `*MapNode`, `*FilterNode`, etc. The
/// `DSLNodeType` string constants from Go are exposed via [`DslNode::type_name`].
#[derive(Debug, Clone, PartialEq)]
pub enum DslNode {
    /// `map(<expr>)` — applies `expr` to each input node.
    Map(Box<DslNode>),
    /// `filter(<expr>)` — keeps nodes for which `expr` holds.
    Filter(Box<DslNode>),
    /// `reduce(<expr>)` — fold (currently identity, as in Go).
    Reduce(Box<DslNode>),
    /// Field access: `.a.b.c` → `["a","b","c"]`.
    Field(Vec<String>),
    /// A literal value.
    Literal(DslLiteral),
    /// A function call `name(arg, ...)`.
    Call { name: String, args: Vec<DslNode> },
    /// A pipeline of stages separated by `|`.
    Pipeline(Vec<DslNode>),
    /// `rmap(<expr>)` — reverse map (identity in Go).
    RMap(Box<DslNode>),
    /// `rfilter(<expr>)` — reverse filter (identity in Go).
    RFilter(Box<DslNode>),
    /// A comparison `<lhs> <op> <rhs>` produced by the grammar's `Comparison`.
    Comparison { lhs: Box<DslNode>, op: String, rhs: Box<DslNode> },
}

impl DslNode {
    /// Returns the Go `DSLNodeType` string for this node (`"Map"`, `"Filter"`,
    /// `"Reduce"`, `"Field"`, `"Literal"`, `"Call"`, `"Pipeline"`, `"RMap"`,
    /// `"RFilter"`). Comparisons have no Go `DSLNodeType` (they are an `Expr`
    /// sub-form), so they return `"Comparison"`.
    pub fn type_name(&self) -> &'static str {
        match self {
            DslNode::Map(_) => "Map",
            DslNode::Filter(_) => "Filter",
            DslNode::Reduce(_) => "Reduce",
            DslNode::Field(_) => "Field",
            DslNode::Literal(_) => "Literal",
            DslNode::Call { .. } => "Call",
            DslNode::Pipeline(_) => "Pipeline",
            DslNode::RMap(_) => "RMap",
            DslNode::RFilter(_) => "RFilter",
            DslNode::Comparison { .. } => "Comparison",
        }
    }
}
