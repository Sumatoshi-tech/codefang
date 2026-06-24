//! The UAST query DSL: parsing, lowering, and execution.
//!
//! [`parse`] is a hand-written recursive-descent parser implementing the query
//! grammar directly. The grammar is small and unambiguous:
//!
//! ```text
//! Query        <- Pipeline EOT
//! Pipeline     <- Stage (PIPE Stage)*
//! Stage        <- MapOp / FilterOp / ReduceOp / RMapOp / RFilterOp
//!               / FunctionCall / FieldAccess
//! MapOp        <- "map"     LPAR Expr RPAR
//! FilterOp     <- "filter"  LPAR Expr RPAR
//! ReduceOp     <- "reduce"  LPAR Expr RPAR
//! RMapOp       <- "rmap"    LPAR Expr RPAR
//! RFilterOp    <- "rfilter" LPAR Expr RPAR
//! FunctionCall <- Identifier LPAR ArgList? RPAR
//! FieldAccess  <- DOT Identifier (DOT Identifier)*
//! Expr         <- Comparison / FieldAccess / FunctionCall / Literal
//! Comparison   <- (FieldAccess / FunctionCall) CompareOp (Literal / FieldAccess)
//! CompareOp    <- "==" / "!=" / "<=" / ">=" / "<" / ">"
//! Literal      <- StringLiteral / NumberLiteral / BoolLiteral
//! ```

mod field_access;
mod lowering;
mod operators;
mod parser;
mod runtime;

pub use field_access::{FieldAccessExecutor, FieldAccessStrategyRegistry};
pub use parser::{parse, ParseError};
pub use runtime::{lower, DslError, QueryFn};

use crate::node::Node;

impl Node {
    /// Runs a DSL query string against this node's subtree, returning the
    /// matching nodes (cloned).
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_uast_node::Node;
    ///
    /// let mut root = Node::with_token("File", "");
    /// root.add_child(Node::with_token("Function", "a"));
    /// root.add_child(Node::with_token("Function", "b"));
    /// root.add_child(Node::with_token("Variable", "c"));
    ///
    /// let functions = root.find_dsl("filter(.type == 'Function')").unwrap();
    /// assert_eq!(functions.len(), 2);
    /// assert!(functions.iter().all(|n| n.node_type == "Function"));
    ///
    /// // An empty query is an error.
    /// assert!(root.find_dsl("").is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error for an empty query (`"query string is empty"`), a parse
    /// failure, or a lowering failure. The error strings are part of the CLI
    /// compatibility contract.
    pub fn find_dsl(&self, query: &str) -> Result<Vec<Self>, DslError> {
        if query.is_empty() {
            return Err(DslError::EmptyQuery);
        }
        let ast = parse(query).map_err(DslError::Parse)?;
        let initial = self.determine_initial_input(&ast);
        let runtime = lower(&ast)?;
        Ok(runtime(initial))
    }

    /// Chooses the initial input set for a query: a `filter(...)` root and
    /// pipelines start from the children, except a pipeline whose first stage
    /// is exactly `map(.children)`, which starts from the node itself; any
    /// other root also starts from the node itself.
    fn determine_initial_input(&self, ast: &crate::types::DslNode) -> Vec<Self> {
        use crate::types::DslNode;
        match ast {
            DslNode::Filter(_) => self.children.clone(),
            DslNode::Pipeline(stages) => {
                if stages.is_empty() {
                    return self.children.clone();
                }
                if let DslNode::Map(expr) = &stages[0] {
                    if let DslNode::Field(fields) = expr.as_ref() {
                        if fields.len() == 1 && fields[0] == "children" {
                            return vec![self.clone()];
                        }
                    }
                    return self.children.clone();
                }
                self.children.clone()
            }
            _ => vec![self.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::DslError;
    use crate::node::Node;

    fn sample_tree() -> Node {
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Function", "a"));
        root.add_child(Node::with_token("Function", "b"));
        root.add_child(Node::with_token("Variable", "c"));
        root
    }

    #[test]
    fn empty_query_errors() {
        let root = sample_tree();
        assert!(matches!(root.find_dsl(""), Err(DslError::EmptyQuery)));
    }

    #[test]
    fn filter_by_type() {
        let root = sample_tree();
        let out = root.find_dsl("filter(.type == 'Function')").expect("query");
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|n| n.node_type == "Function"));
    }

    #[test]
    fn map_children_then_filter() {
        let root = sample_tree();
        let out = root.find_dsl("map(.children) | filter(.type == 'Variable')");
        // map(.children) over [root] yields root's children, then filter.
        assert!(out.is_ok());
    }
}
