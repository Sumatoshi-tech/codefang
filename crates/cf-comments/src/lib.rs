//! Static comment placement / documentation coverage analysis (analyzer id:
//! `static/comments`).
//!
//! The analyzer walks a parsed UAST, groups adjacent `Comment` nodes into
//! blocks, places every block against the file's declarations (function,
//! method, class, interface, struct) and derives the per-file metrics. It is
//! also consumed by the history `quality` analyzer.
//!
//! # Placement rules
//!
//! * **above** — the block ends on the line directly above a declaration
//!   (attribute / decorator lines belong to the declaration): it documents
//!   that declaration and is a good comment;
//! * **inline** — the block lies inside a declaration's span: a body comment;
//! * **unassociated** — anything else (file headers, orphans, a comment
//!   separated from the declaration by a blank line).
//!
//! Documentation is keyed by declaration identity (line + type + name), so
//! same-named declarations are documented independently; a nameless
//! class/struct/interface node is not a declaration; nested comment pieces
//! count as one comment.
//!
//! # Serialization
//!
//! All report serialization is routed through [`cf_gojson::GoValue`] — never
//! `serde_json`. The analyzer returns a *map-origin*
//! [`cf_gojson::GoValue::Map`] whose keys the `cf-gojson` encoder byte-sorts at
//! encode time.
//!
//! # Module map
//!
//! [`analyzer`] (discovery, metrics, report building), [`placement`] (the
//! pure line rules), [`types`] (data types), [`aggregator`] (cross-file
//! sums), [`traverse`] (UAST traversal helpers). The terminal/HTML sections
//! live in the command layer.
//!
//! # Example
//!
//! A comment placed directly above a function documents it:
//!
//! ```
//! use cf_comments::Analyzer;
//! use cf_gojson::GoValue;
//! use cf_uast_node::{Builder, Node, Positions};
//!
//! fn pos(start: u64, end: u64) -> Positions {
//!     Positions { start_line: start, end_line: end, ..Default::default() }
//! }
//!
//! // // doc comment   (line 1)
//! // function foo()   (lines 2-4)
//! let comment = Builder::new()
//!     .with_type("Comment")
//!     .with_token("// documents foo")
//!     .with_position(Some(pos(1, 1)))
//!     .build();
//! let name = Builder::new().with_type("Identifier").with_token("foo")
//!     .with_roles(vec!["Name".into()]).build();
//! let mut func = Builder::new().with_type("Function")
//!     .with_position(Some(pos(2, 4))).build();
//! func.add_child(name);
//!
//! let mut root = Builder::new().with_type("File").build();
//! root.add_child(comment);
//! root.add_child(func);
//!
//! let report = Analyzer::new().analyze(Some(&root)).unwrap();
//! let m = report.as_map().unwrap();
//! assert_eq!(m.get("total_comments"), Some(&GoValue::Int(1)));
//! assert_eq!(m.get("good_comments"), Some(&GoValue::Int(1)));
//! assert_eq!(m.get("documented_functions"), Some(&GoValue::Int(1)));
//! ```
//!
//! A missing root is an error:
//!
//! ```
//! use cf_comments::Analyzer;
//! assert!(Analyzer::new().analyze(None).is_err());
//! ```

pub mod aggregator;
pub mod analyzer;
pub mod placement;
pub mod traverse;
pub mod types;

pub use aggregator::{Aggregator, FileReport, FileStamp};
pub use analyzer::{build_report, comment_preview, Analyzer, NilRootNode};
pub use types::{
    quality_message, CommentConfig, CommentCounts, CommentDetail, CommentMetrics, FunctionInfo,
    Placement, NO_COMMENTS_MESSAGE,
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
            GoValue::Map(m) => {
                &m.entries()
                    .iter()
                    .find(|(k, _)| k == key)
                    .expect("key present")
                    .1
            }
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
    fn ident(name: &str, roles: &[&str]) -> Node {
        Builder::new()
            .with_type("Identifier")
            .with_token(name)
            .with_roles(roles.iter().map(|r| (*r).to_string()).collect())
            .build()
    }
    fn positioned(node_type: &str, start: u64, end: u64) -> Node {
        Builder::new()
            .with_type(node_type)
            .with_position(Some(pos(start, end)))
            .build()
    }
    fn named(node_type: &str, name: &str, start: u64, end: u64) -> Node {
        let mut n = positioned(node_type, start, end);
        n.add_child(ident(name, &["Name"]));
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
    fn analyze(root: &Node) -> CommentMetrics {
        Analyzer::new()
            .analyze_metrics(Some(root), None)
            .expect("analysis")
    }
    fn names(metrics: &CommentMetrics) -> Vec<(&str, bool)> {
        metrics
            .functions
            .iter()
            .map(|f| (f.name.as_str(), f.is_documented()))
            .collect()
    }
    fn placements(metrics: &CommentMetrics) -> Vec<(i64, Placement, &str)> {
        metrics
            .comments
            .iter()
            .map(|c| (c.line, c.placement, c.target_name.as_str()))
            .collect()
    }

    #[test]
    fn analyzer_name() {
        assert_eq!(Analyzer::new().name(), "comments");
    }

    #[test]
    fn default_config() {
        let cfg = Analyzer::new().default_config();
        assert!((cfg.reward_score - 1.0).abs() < 1e-9);
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
        assert_eq!(
            get(&res, "message"),
            &GoValue::Str(NO_COMMENTS_MESSAGE.to_string())
        );
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
        assert!((get_float(&res, "documentation_coverage") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn analyze_bad_comment_placement() {
        let mut body = positioned("Block", 2, 4);
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
        let metrics = analyze(&root);
        assert_eq!(
            placements(&metrics),
            vec![
                (1, Placement::Above, "TestClass"),
                (4, Placement::Above, "testMethod")
            ]
        );
        assert_eq!(
            names(&metrics),
            vec![("TestClass", true), ("testMethod", true)]
        );
    }

    #[test]
    fn analyze_unassociated_comment() {
        let mut root = file();
        root.add_child(comment(
            "// This comment is not associated with anything",
            1,
            1,
        ));
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        assert_eq!(get_int(&res, "total_comments"), 1);
        assert_eq!(get_int(&res, "good_comments"), 0);
        assert_eq!(get_int(&res, "bad_comments"), 1);
        assert_eq!(get_float(&res, "overall_score"), 0.0);
        assert_eq!(get_int(&res, "total_functions"), 0);
        assert_eq!(get_int(&res, "documented_functions"), 0);
    }

    #[test]
    fn zero_comments_keep_the_functions() {
        let mut root = file();
        root.add_child(function("Add", 3, 5));
        root.add_child(function("Sub", 7, 9));
        let metrics = analyze(&root);
        assert_eq!(metrics.counts.total_comments, 0);
        assert_eq!(names(&metrics), vec![("Add", false), ("Sub", false)]);
        assert_eq!(
            metrics.counts.message(),
            "Poor comment quality - significant improvement needed"
        );
    }

    #[test]
    fn same_named_functions_are_documented_independently() {
        let mut root = file();
        root.add_child(comment("// String renders A.", 6, 6));
        root.add_child(method("String", 7, 9));
        root.add_child(method("String", 11, 13));
        let metrics = analyze(&root);
        assert_eq!(names(&metrics), vec![("String", true), ("String", false)]);
        assert_eq!(metrics.counts.documented_functions, 1);
        assert_eq!(metrics.functions[0].doc_line, Some(6));
        assert_eq!(metrics.functions[1].doc_line, None);
    }

    #[test]
    fn nested_comment_pieces_count_once_and_document_the_next_item() {
        // The Rust mapping lowers `/// Documents foo.` on line 3 as a Comment
        // spanning 3-4 with two nested Comment pieces; the function starts
        // on line 4.
        let mut outer = comment("", 3, 4);
        outer.add_child(comment("/", 3, 3));
        outer.add_child(comment(" Documents foo.", 3, 4));
        let mut root = file();
        root.add_child(outer);
        root.add_child(function("foo", 4, 7));
        let metrics = analyze(&root);
        assert_eq!(metrics.counts.total_comments, 1);
        assert_eq!(placements(&metrics), vec![(3, Placement::Above, "foo")]);
        assert_eq!(metrics.comments[0].text, "/ Documents foo.");
        assert_eq!(names(&metrics), vec![("foo", true)]);
    }

    #[test]
    fn anonymous_struct_is_not_a_declaration() {
        let anonymous = Builder::new()
            .with_type("Struct")
            .with_token("struct{}")
            .with_position(Some(pos(3, 3)))
            .build();
        let mut root = file();
        root.add_child(anonymous);
        root.add_child(named("Struct", "Named", 5, 7));
        let metrics = analyze(&root);
        assert_eq!(names(&metrics), vec![("Named", false)]);
    }

    #[test]
    fn a_multi_line_token_is_never_a_name() {
        let func = Builder::new()
            .with_type("Function")
            .with_token("int f() {\n    return 1;\n}")
            .with_position(Some(pos(1, 3)))
            .build();
        let mut root = file();
        root.add_child(func);
        let metrics = analyze(&root);
        assert_eq!(names(&metrics), vec![("unknown", false)]);
    }

    #[test]
    fn a_type_identifier_is_not_a_name() {
        let mut func = positioned("Function", 1, 3);
        func.add_child(ident("int", &["Type", "Name"]));
        func.add_child(ident("add", &["Name"]));
        let mut root = file();
        root.add_child(func);
        assert_eq!(names(&analyze(&root)), vec![("add", false)]);
    }

    #[test]
    fn a_same_line_child_declaration_names_its_nameless_parent() {
        // C: function_definition (nameless, lines 2-4) over function_declarator
        // (line 2, names `add`).
        let mut outer = positioned("Function", 2, 4);
        outer.add_child(ident("int", &["Type", "Name"]));
        outer.add_child(named("Function", "add", 2, 2));
        let mut root = file();
        root.add_child(comment("/* adds */", 1, 1));
        root.add_child(outer);
        let metrics = analyze(&root);
        assert_eq!(names(&metrics), vec![("add", true)]);
    }

    #[test]
    fn a_named_parent_keeps_its_same_line_nested_declaration() {
        let mut outer = class("A", 1, 1);
        outer.add_child(method("m", 1, 1));
        let mut root = file();
        root.add_child(outer);
        assert_eq!(names(&analyze(&root)), vec![("A", false), ("m", false)]);
    }

    #[test]
    fn body_comment_is_inline_for_the_enclosing_function() {
        let mut add = function("Add", 3, 6);
        add.add_child(comment("// inline comment", 4, 4));
        let mut root = file();
        root.add_child(add);
        root.add_child(function("Sub", 8, 10));
        let metrics = analyze(&root);
        assert_eq!(placements(&metrics), vec![(4, Placement::Inline, "Add")]);
        assert_eq!(names(&metrics), vec![("Add", false), ("Sub", false)]);
    }

    #[test]
    fn trailing_comment_on_the_declaration_line_is_inline() {
        let mut root = file();
        root.add_child(function("f", 1, 1));
        root.add_child(comment("// trailing", 1, 1));
        let metrics = analyze(&root);
        assert_eq!(placements(&metrics), vec![(1, Placement::Inline, "f")]);
    }

    #[test]
    fn a_blank_line_breaks_documentation() {
        let mut root = file();
        root.add_child(comment("// Mul multiplies.", 14, 14));
        root.add_child(function("Mul", 16, 18));
        let metrics = analyze(&root);
        assert_eq!(
            placements(&metrics),
            vec![(14, Placement::Unassociated, "")]
        );
        assert_eq!(names(&metrics), vec![("Mul", false)]);
    }

    #[test]
    fn attribute_lines_belong_to_the_declaration() {
        let attribute = Builder::new()
            .with_type("Attribute")
            .with_roles(vec!["Attribute".into()])
            .with_position(Some(pos(2, 2)))
            .build();
        let mut root = file();
        root.add_child(comment("/// Doc f.", 1, 1));
        root.add_child(attribute);
        root.add_child(function("f", 3, 5));
        let metrics = analyze(&root);
        assert_eq!(names(&metrics), vec![("f", true)]);
    }

    #[test]
    fn adjacent_comments_form_one_block() {
        let mut root = file();
        root.add_child(comment("// line 1", 1, 1));
        root.add_child(comment("// line 2", 2, 2));
        root.add_child(function("f", 3, 5));
        let metrics = analyze(&root);
        assert_eq!(metrics.counts.total_comments, 2);
        assert_eq!(metrics.counts.good_comments, 2);
        assert_eq!(metrics.functions[0].doc_line, Some(1));
    }

    #[test]
    fn report_tables_carry_placement_and_assessment() {
        let mut root = file();
        root.add_child(comment("// doc", 1, 1));
        root.add_child(function("f", 2, 4));
        let res = Analyzer::new().analyze(Some(&root)).unwrap();
        let comments = match get(&res, "comments") {
            GoValue::Array(rows) => rows.clone(),
            other => panic!("comments not an array: {other:?}"),
        };
        assert_eq!(
            get(&comments[0], "placement"),
            &GoValue::Str("above".to_string())
        );
        assert_eq!(get(&comments[0], "target"), &GoValue::Str("f".to_string()));
        let functions = match get(&res, "functions") {
            GoValue::Array(rows) => rows.clone(),
            other => panic!("functions not an array: {other:?}"),
        };
        assert_eq!(get_int(&functions[0], "line"), 2);
        assert_eq!(get_int(&functions[0], "lines"), 3);
        assert_eq!(get_int(&functions[0], "doc_line"), 1);
        assert_eq!(
            get(&functions[0], "assessment"),
            &GoValue::Str("✅ Well Documented".to_string())
        );
    }

    #[test]
    fn comment_preview_shortens_long_text() {
        let long = "x".repeat(60);
        assert_eq!(comment_preview(&long), format!("{}...", "x".repeat(47)));
        assert_eq!(comment_preview("short"), "short");
    }

    // Declarations are found at any depth; a variable is not one.
    #[test]
    fn find_declarations_excludes_variable() {
        let mut root = file();
        root.add_child(typed("Function"));
        root.add_child(typed("Method"));
        root.add_child(typed("Class"));
        root.add_child(typed("Variable"));
        let found = traverse::find_nodes_by_type(
            &root,
            &["Function", "Method", "Class"],
            None,
            traverse::MatchDescent::Continue,
        );
        assert_eq!(found.len(), 3);
    }

    // Comments are found at any depth, in document order, stopping at each.
    #[test]
    fn find_comments_in_document_order_without_descending() {
        let mut func = typed("Function");
        func.add_child(comment("// Function comment", 5, 5));
        let mut nested = comment("", 1, 1);
        nested.add_child(comment("// piece", 1, 1));
        let mut root = file();
        root.add_child(nested);
        root.add_child(func);
        let comments =
            traverse::find_nodes_by_type(&root, &["Comment"], None, traverse::MatchDescent::Stop);
        assert_eq!(comments.len(), 2);
        assert_eq!(traverse::comment_text(comments[0]), "// piece");
        assert_eq!(comments[1].token, "// Function comment");
    }

    #[test]
    fn depth_cap_limits_discovery() {
        let mut inner = typed("Block");
        inner.add_child(function("deep", 2, 3));
        let mut outer = function("shallow", 1, 4);
        outer.add_child(inner);
        let mut root = file();
        root.add_child(outer);
        let capped = Analyzer::new()
            .analyze_metrics(Some(&root), Some(1))
            .expect("analysis");
        assert_eq!(names(&capped), vec![("shallow", false)]);
        assert_eq!(analyze(&root).functions.len(), 2);
    }
}
