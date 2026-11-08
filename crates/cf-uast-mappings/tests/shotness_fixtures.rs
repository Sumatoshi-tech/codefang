//! BUG: specs/bugs/BUG-shotness-language-names.md
//! Ensure the CLI language matrix uses valid syntax for the pinned grammars.

#[test]
fn shotness_language_fixtures_have_no_parse_errors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bins/codefang/tests/fixtures/shotness_languages");
    let parser = cf_uast::Parser::new();
    for entry in std::fs::read_dir(root).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        let filename = path.file_name().expect("filename").to_str().expect("UTF-8");
        let language = parser.get_language(filename);
        let grammar = cf_uast::languages::get_language(&language).expect("wired grammar");
        let mut raw = tree_sitter::Parser::new();
        raw.set_language(&grammar).expect("set grammar");
        let source = std::fs::read(&path).expect("read fixture");
        let tree = raw.parse(source, None).expect("parse fixture");
        assert!(
            !tree.root_node().has_error(),
            "{filename}: {}",
            tree.root_node().to_sexp()
        );
    }
}

#[test]
fn latex_old_definition_exposes_its_declared_name() {
    // The pinned grammar models only the \def header, not its implementation.
    let source =
        include_bytes!("../../../bins/codefang/tests/fixtures/shotness_languages/latex_old.tex");
    let root = cf_uast::Parser::new()
        .parse("old.tex", source)
        .expect("parse");
    let functions = root.find(|node| node.node_type == "Function");
    assert_eq!(functions.len(), 1);
    assert_eq!(
        functions[0].props.get("name").map(String::as_str),
        Some("\\probe")
    );
}

#[test]
fn every_filename_selected_function_mapping_has_a_history_fixture() {
    let parser = cf_uast::Parser::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bins/codefang/tests/fixtures/shotness_languages");
    let covered: std::collections::BTreeSet<_> = std::fs::read_dir(root)
        .expect("fixtures")
        .map(|entry| {
            parser.get_language(entry.expect("entry").file_name().to_str().expect("UTF-8"))
        })
        .collect();
    assert_eq!(
        parser.get_language("chart.yaml"),
        "yaml",
        "review Helm routing exclusion"
    );
    assert!(
        cf_uast_mappings::by_name("rust_with_rstml")
            .expect("RSTML mapping")
            .extensions
            .is_empty(),
        "review RSTML routing exclusion"
    );
    for (name, mapping) in cf_uast_mappings::ALL {
        if matches!(*name, "helm" | "rust_with_rstml") {
            continue;
        }
        let (rules, _) = mapping.to_rules();
        let declares_functions = rules.iter().any(|rule| {
            matches!(rule.uast_spec.r#type.as_str(), "Function" | "Method")
                || (rule.uast_spec.roles.iter().any(|role| role == "Function")
                    && rule
                        .uast_spec
                        .roles
                        .iter()
                        .any(|role| role == "Declaration"))
        });
        assert!(
            !declares_functions || covered.contains(*name),
            "missing Shotness fixture: {name}"
        );
    }
}
