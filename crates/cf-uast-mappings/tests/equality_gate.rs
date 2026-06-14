//! The migration keystone (specs/uastmap-rust-macros, Step 6): for EVERY
//! embedded `.uastmap` language, the static table's `to_rules()` output must
//! be EXACTLY equal — field for field, in rule order, including the
//! `props: None` vs `Some` distinction — to what the legacy DSL parser
//! produces for the same text. Nothing cuts over until this gate is green for
//! all 68 languages; divergences are transpiler bugs (the parser is the spec).

use cf_uast_mapping::{Parser, Role, Rule, UastType};

/// Field-level diff of two rules, so a gate failure is actionable.
fn diff_rules(s: &Rule, p: &Rule) -> String {
    let mut out = String::new();
    let mut field = |name: &str, sv: String, pv: String| {
        if sv != pv {
            out.push_str(&format!("  field `{name}`:\n    static: {sv}\n    parsed: {pv}\n"));
        }
    };
    field("name", format!("{:?}", s.name), format!("{:?}", p.name));
    field("pattern", format!("{:?}", s.pattern), format!("{:?}", p.pattern));
    field("extends", format!("{:?}", s.extends), format!("{:?}", p.extends));
    field(
        "uast_spec.type",
        format!("{:?}", s.uast_spec.r#type),
        format!("{:?}", p.uast_spec.r#type),
    );
    field(
        "uast_spec.token",
        format!("{:?}", s.uast_spec.token),
        format!("{:?}", p.uast_spec.token),
    );
    field(
        "uast_spec.roles",
        format!("{:?}", s.uast_spec.roles),
        format!("{:?}", p.uast_spec.roles),
    );
    field(
        "uast_spec.props",
        format!("{:?}", s.uast_spec.props),
        format!("{:?}", p.uast_spec.props),
    );
    field(
        "uast_spec.children",
        format!("{:?}", s.uast_spec.children),
        format!("{:?}", p.uast_spec.children),
    );
    field(
        "conditions",
        format!("{:?}", s.conditions),
        format!("{:?}", p.conditions),
    );
    out
}

/// Exact structural equality, all 68 languages: `static.to_rules() ==
/// parser.parse_mapping(embedded dsl)`.
#[test]
fn static_tables_equal_parsed_dsl_for_all_languages() {
    let parser = Parser::new();
    let mut checked = 0usize;
    for (&lang, &content) in cf_uast_uastmaps::embedded_mappings() {
        let (parsed_rules, parsed_info) = parser
            .parse_mapping(content)
            .unwrap_or_else(|e| panic!("{lang}: DSL parse failed: {e:?}"));
        let mapping = cf_uast_mappings::by_name(lang)
            .unwrap_or_else(|| panic!("{lang}: no static mapping in the registry"));
        let (static_rules, static_info) = mapping.to_rules();

        assert_eq!(
            static_info, parsed_info,
            "{lang}: LanguageInfo diverges (static vs parsed)"
        );
        assert_eq!(
            static_rules.len(),
            parsed_rules.len(),
            "{lang}: rule count diverges (static {} vs parsed {})",
            static_rules.len(),
            parsed_rules.len()
        );
        for (i, (s, p)) in static_rules.iter().zip(parsed_rules.iter()).enumerate() {
            assert_eq!(
                s,
                p,
                "{lang}: first divergent rule at index {i} (rule `{}`):\n{}",
                p.name,
                diff_rules(s, p)
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 68, "expected to gate all 68 languages");
}

/// The registry's language listing equals the embedded map's, exactly.
#[test]
fn supported_languages_match_embedded_exactly() {
    assert_eq!(
        cf_uast_mappings::supported_languages(),
        cf_uast_uastmaps::supported_languages()
    );
}

/// `ALL` is sorted by registry key (binary-search precondition).
#[test]
fn registry_is_key_sorted() {
    let keys: Vec<&str> = cf_uast_mappings::ALL.iter().map(|(k, _)| *k).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "ALL must stay sorted by registry key");
}

/// No `Other(_)` vocabulary variants in the generated tables: the closed
/// vocabularies cover the entire corpus.
#[test]
fn no_other_vocabulary_variants() {
    for (key, mapping) in cf_uast_mappings::ALL {
        for rule in mapping.rules {
            assert!(
                !matches!(rule.uast_type, UastType::Other(_)),
                "{key}/{}: Other(_) type {:?}",
                rule.name,
                rule.uast_type
            );
            for role in rule.roles {
                assert!(
                    !matches!(role, Role::Other(_)),
                    "{key}/{}: Other(_) role {role:?}",
                    rule.name
                );
            }
        }
    }
}
