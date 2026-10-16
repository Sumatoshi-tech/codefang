//! The UAST query DSL: parsing, lowering, and execution.
//!
//! [`parse`] is a hand-written recursive-descent parser implementing the query
//! grammar directly; [`Query`] compiles a query once and runs it against any
//! number of roots; [`Node::find_dsl`] is the one-shot form. The grammar is
//! small and unambiguous:
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
//!
//! Stage semantics: `filter(expr)` keeps the input nodes satisfying `expr` and
//! `map(expr)` applies `expr` to each input node; both look at the input set
//! only (a `filter(...)` root starts from the root's DIRECT children).
//! `rfilter(expr)` and `rmap(expr)` are their recursive forms: they apply the
//! same predicate / expression to every node of each input's subtree in
//! pre-order (the input itself first), so `rfilter(.roles has "Function")`
//! finds class methods and nested functions that `filter(...)` never sees.
//! Field access resolves `children`, `token`, `id`, `roles`, `type`, `props`,
//! `first`, `last`, and any other name as a property lookup
//! (`.props.name` ≡ `.name`).

mod field_access;
mod lowering;
mod operators;
mod parser;
mod runtime;

pub use field_access::{FieldAccessExecutor, FieldAccessStrategyRegistry};
pub use parser::{parse, ParseError};
pub use runtime::{lower, DslError, Query, QueryFn};

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
    /// Compile the query once with [`Query::compile`] when it runs against
    /// many trees.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty query (`"query string is empty"`), a parse
    /// failure, or a lowering failure. The error strings are part of the CLI
    /// compatibility contract.
    pub fn find_dsl(&self, query: &str) -> Result<Vec<Self>, DslError> {
        Ok(Query::compile(query)?.run(self))
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::{DslError, Query};
    use crate::node::Node;

    fn sample_tree() -> Node {
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Function", "a"));
        root.add_child(Node::with_token("Function", "b"));
        root.add_child(Node::with_token("Variable", "c"));
        root
    }

    /// A declaration node carrying the `Function` role and `props.name`.
    fn declaration(node_type: &str, roles: &[&str], name: &str) -> Node {
        let mut n = Node::with_token(node_type, "");
        n.roles = roles.iter().map(|r| (*r).to_string()).collect();
        n.props.insert("name".to_string(), name.to_string());
        n
    }

    /// `File → Class K { Method km } , Function top`: one nested and one
    /// top-level function-role node.
    fn nested_tree() -> Node {
        let mut root = Node::with_token("File", "");
        let mut class = declaration("Class", &["Class", "Declaration"], "K");
        class.add_child(declaration(
            "Method",
            &["Function", "Declaration", "Member"],
            "km",
        ));
        root.add_child(class);
        root.add_child(declaration("Function", &["Function", "Declaration"], "top"));
        root
    }

    fn names(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| n.props.get("name").cloned().unwrap_or_default())
            .collect()
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

    #[test]
    fn filter_sees_only_the_direct_children() {
        let out = nested_tree()
            .find_dsl(r#"filter(.roles has "Function")"#)
            .expect("query");
        assert_eq!(names(&out), vec!["top"]);
    }

    #[test]
    fn rfilter_sees_every_descendant_in_pre_order() {
        let out = nested_tree()
            .find_dsl(r#"rfilter(.roles has "Function")"#)
            .expect("query");
        assert_eq!(names(&out), vec!["km", "top"]);
    }

    #[test]
    fn rfilter_includes_the_root_when_it_matches() {
        let mut outer = declaration("Function", &["Function", "Declaration"], "outer");
        outer.add_child(declaration(
            "Function",
            &["Function", "Declaration"],
            "inner",
        ));
        let out = outer
            .find_dsl(r#"rfilter(.roles has "Function")"#)
            .expect("query");
        assert_eq!(names(&out), vec!["outer", "inner"]);
    }

    #[test]
    fn rmap_applies_the_expression_to_every_descendant() {
        let out = nested_tree().find_dsl("rmap(.props.name)").expect("query");
        let tokens: Vec<&str> = out.iter().map(|n| n.token.as_str()).collect();
        assert_eq!(tokens, vec!["K", "km", "top"]);
    }

    #[test]
    fn props_field_resolves_a_property_to_one_literal() {
        let top = declaration("Function", &["Function"], "top");
        let out = top.find_dsl(".props.name").expect("query");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].token, "top");
        let unnamed = Node::with_token("Function", "");
        assert!(unnamed.find_dsl(".props.name").expect("query").is_empty());
    }

    #[test]
    fn rfilter_pipeline_starts_from_the_root() {
        let out = nested_tree()
            .find_dsl(r#"rfilter(.roles has "Function") | map(.props.name)"#)
            .expect("query");
        let tokens: Vec<&str> = out.iter().map(|n| n.token.as_str()).collect();
        assert_eq!(tokens, vec!["km", "top"]);
    }

    #[test]
    fn compiled_query_runs_against_many_roots() {
        let query = Query::compile(r#"rfilter(.roles has "Function")"#).expect("compile");
        assert_eq!(names(&query.run(&nested_tree())), vec!["km", "top"]);
        assert_eq!(query.run(&sample_tree()).len(), 0);
    }

    #[test]
    fn compile_rejects_a_malformed_query() {
        assert!(matches!(
            Query::compile("rfilter("),
            Err(DslError::Parse(_))
        ));
        assert!(matches!(Query::compile(""), Err(DslError::EmptyQuery)));
    }
}
