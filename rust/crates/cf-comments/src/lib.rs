//! Static comment density / quality analysis (analyzer id: `static/comments`).
//!
//! The analyzer walks a parsed UAST, groups consecutive `Comment` nodes into
//! blocks (sorted by line), scores each block by its placement relative to the
//! closest function/class/method/interface/struct, and computes aggregate
//! metrics (good/bad counts, overall score, documentation coverage). It is also
//! consumed by the `quality` analyzer.
//!
//! # Byte-identity
//!
//! Per DESIGN §2, all report serialization is routed through
//! [`cf_gojson::GoValue`] — never `serde_json`. The analyzer returns a
//! *map-origin* [`cf_gojson::GoValue::Map`] whose keys the `cf-gojson` encoder
//! byte-sorts at encode time (report-format contract).
//!
//! # Module map
//!
//! [`analyzer`] (scoring + report building), [`types`] (data types),
//! [`aggregator`] (cross-file numeric aggregation), [`traverse`] (UAST
//! traversal helpers). The static-pipeline metric view and the terminal/HTML
//! sections (non-binding per DESIGN §2.7) live in the command layer.
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `rust/tests/compat`.

pub mod aggregator;
pub mod analyzer;
pub mod traverse;
pub mod types;

pub use aggregator::{Aggregator, NumericReport};
pub use analyzer::{Analyzer, NilRootNode};
pub use types::{
    CommentBlock, CommentConfig, CommentDetail, CommentMetrics, CommentReportItem, FunctionInfo,
    FunctionReportItem,
};

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::GoValue;
    use cf_uast_node::{Builder, Node, Positions};

    fn get_int(v: &GoValue, key: &str) -> i64 {
        match get(v, key) {
            GoValue::Int(n) => *n,
            other => panic!("{key} not int: {other:?}"),
        }
    }
    fn get_float(v: &GoValue, key: &str) -> f64 {
        match get(v, key) {
            GoValue::Float(f) => *f,
            other => panic!("{key} not float: {other:?}"),
        }
    }
    fn get<'a>(v: &'a GoValue, key: &str) -> &'a GoValue {
        match v {
            GoValue::Map(m) => &m.entries().iter().find(|(k, _)| k == key).expect("key present").1,
            other => panic!("not a map: {other:?}"),
        }
    }

    // --- node builders (isolate the cf-uast-node API) -----------------------

    fn pos(start: u64, end: u64) -> Positions {
        Positions {
            start_line: start,
            end_line: end,
            ..Default::default()
        }
    }
    fn typed(node_type: &str) -> Node {
        Builder::new().with_type(node_type).build()
    }
    fn file() -> Node {
        typed("File")
    }
    fn comment(token: &str, start: u64, end: u64) -> Node {
        Builder::new()
            .with_type("Comment")
            .with_token(token)
            .with_position(Some(pos(start, end)))
            .build()
    }
    fn named(node_type: &str, name: &str, start: u64, end: u64) -> Node {
        let ident = Builder::new()
            .with_type("Identifier")
            .with_token(name)
            .with_roles(vec!["Name".into()])
            .build();
        let mut n = Builder::new()
            .with_type(node_type)
            .with_position(Some(pos(start, end)))
            .build();
        n.add_child(ident);
        n
    }
    fn function(name: &str, start: u64, end: u64) -> Node {
        named("Function", name, start, end)
    }
    fn class(name: &str, start: u64, end: u64) -> Node {
        named("Class", name, start, end)
    }
    fn method(name: &str, start: u64, end: u64) -> Node {
        named("Method", name, start, end)
    }

    #[test]
    fn analyzer_name() {
        assert_eq!(Analyzer::new().name(), "comments");
    }

    #[test]
    fn default_config() {
        let cfg = Analyzer::new().default_config();
        assert!((cfg.reward_score - 1.0).abs() < 1e-9);
        assert_eq!(cfg.max_comment_length, 500);
        assert_eq!(cfg.penalty_scores.get("Function"), Some(&-0.5));
        assert_eq!(cfg.penalty_scores.get("Method"), Some(&-0.5));
        assert_eq!(cfg.penalty_scores.get("Class"), Some(&-0.3));
        assert_eq!(cfg.penalty_scores.get("Variable"), Some(&-0.1));
    }

    #[test]
    fn analyze_empty_tree() {
        let root = file();
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 0);
        assert_eq!(get_int(&res, "good_comments"), 0);
        assert_eq!(get_int(&res, "bad_comments"), 0);
        assert_eq!(get_float(&res, "overall_score"), 0.0);
        assert_eq!(get_int(&res, "total_functions"), 0);
        assert_eq!(get_int(&res, "documented_functions"), 0);
    }

    // analyze must reject a missing root.
    #[test]
    fn analyze_nil_root() {
        assert!(Analyzer::new().analyze(None).is_err());
    }

    #[test]
    fn analyze_good_comment_placement() {
        let mut root = file();
        root.add_child(comment("// This is a good comment", 1, 1));
        root.add_child(function("testFunction", 2, 4));
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 1);
        assert_eq!(get_int(&res, "good_comments"), 1);
        assert_eq!(get_int(&res, "bad_comments"), 0);
        assert!((get_float(&res, "overall_score") - 1.0).abs() < 1e-9);
        assert_eq!(get_int(&res, "total_functions"), 1);
        assert_eq!(get_int(&res, "documented_functions"), 1);
    }

    #[test]
    fn analyze_bad_comment_placement() {
        let mut body = Builder::new()
            .with_type("Block")
            .with_position(Some(pos(2, 4)))
            .build();
        body.add_child(comment("// This is a bad comment", 3, 3));
        let mut func = function("testFunction", 1, 5);
        func.add_child(body);
        let mut root = file();
        root.add_child(func);
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 1);
        assert_eq!(get_int(&res, "good_comments"), 0);
        assert_eq!(get_int(&res, "bad_comments"), 1);
        assert_eq!(get_float(&res, "overall_score"), 0.0);
        assert_eq!(get_int(&res, "total_functions"), 1);
        assert_eq!(get_int(&res, "documented_functions"), 0);
    }

    #[test]
    fn analyze_mixed_comment_placement() {
        let mut root = file();
        root.add_child(comment("// Good comment above function", 1, 1));
        root.add_child(function("function1", 2, 4));
        root.add_child(function("function2", 6, 8));
        root.add_child(comment("// Bad comment after function", 9, 9));
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 2);
        assert_eq!(get_int(&res, "good_comments"), 1);
        assert_eq!(get_int(&res, "bad_comments"), 1);
        assert!((get_float(&res, "overall_score") - 0.5).abs() < 1e-9);
        assert_eq!(get_int(&res, "total_functions"), 2);
        assert_eq!(get_int(&res, "documented_functions"), 1);
    }

    #[test]
    fn analyze_class_with_method() {
        let mut cls = class("TestClass", 2, 8);
        cls.add_child(comment("// This is a method", 4, 4));
        cls.add_child(method("testMethod", 5, 7));
        let mut root = file();
        root.add_child(comment("// This is a class", 1, 1));
        root.add_child(cls);
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 2);
        assert_eq!(get_int(&res, "good_comments"), 2);
        assert_eq!(get_int(&res, "bad_comments"), 0);
        assert!((get_float(&res, "overall_score") - 1.0).abs() < 1e-9);
        assert_eq!(get_int(&res, "total_functions"), 2);
        assert_eq!(get_int(&res, "documented_functions"), 2);
    }

    #[test]
    fn analyze_unassociated_comment() {
        let mut root = file();
        root.add_child(comment("// This comment is not associated with anything", 1, 1));
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 1);
        assert_eq!(get_int(&res, "good_comments"), 0);
        assert_eq!(get_int(&res, "bad_comments"), 1);
        assert_eq!(get_float(&res, "overall_score"), 0.0);
        assert_eq!(get_int(&res, "total_functions"), 0);
        assert_eq!(get_int(&res, "documented_functions"), 0);
    }

    // Function/method/class only; variable excluded.
    #[test]
    fn find_functions_excludes_variable() {
        let mut root = file();
        root.add_child(typed("Function"));
        root.add_child(typed("Method"));
        root.add_child(typed("Class"));
        root.add_child(typed("Variable"));
        let funcs = traverse::find_nodes_by_type(&root, &["Function", "Method", "Class"]);
        assert_eq!(funcs.len(), 3);
    }

    // Comments are found at any depth, in document order.
    #[test]
    fn find_comments_in_document_order() {
        let mut func = typed("Function");
        func.add_child(comment("// Function comment", 5, 5));
        let mut root = file();
        root.add_child(comment("// Root comment", 1, 1));
        root.add_child(func);
        let comments = traverse::find_nodes_by_type(&root, &["Comment"]);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].token, "// Root comment");
        assert_eq!(comments[1].token, "// Function comment");
    }
}
