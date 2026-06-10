//! `uastmap2rs` — transpiles one `.uastmap` mapping file into a Rust module
//! for the `cf-uast-mappings` data crate (specs/uastmap-rust-macros, Step 5).
//!
//! The input is read through [`cf_uast_mapping::Parser`] — the authoritative
//! reader of the legacy DSL — so the emitted module's
//! [`cf_uast_mapping::LanguageMapping::to_rules`] output is structurally equal
//! to the parser's output by construction. The equality gate in
//! `cf-uast-mappings` verifies that for every language.
//!
//! # Output forms
//!
//! * **Macro form** (the default): a single `uast_language!` invocation.
//! * **Plain-literal form** (the escape hatch): a `LanguageMapping` literal
//!   whose rules array holds plain `MappingRule { .. }` literals. Selected per
//!   rule via [`Options::escape_hatch`] or automatically when a rule (or
//!   `extends` target) name is not expressible as a macro identifier — e.g.
//!   the tree-sitter wildcard rule named `_`, which the `ident` fragment
//!   matcher rejects. Because the macro produces the whole rules slice, one
//!   escape-hatched rule switches the entire module to the plain form (rule
//!   order is observable and must be preserved in a single array).
//!
//! Output is deterministic (no timestamps) and rustfmt-stable: macro bodies
//! are not reformatted by rustfmt, and plain-form statics carry
//! `#[rustfmt::skip]`.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt::Write as _;

use cf_uast_mapping::{LanguageInfo, Parser, Role, Rule, TokenSource, UastType};

/// Transpilation options.
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Rule names to force onto the plain-`MappingRule`-literal path.
    pub escape_hatch: BTreeSet<String>,
}

/// Transpiles `.uastmap` text into a complete Rust module (returned as a
/// string ending in a newline). `source_name` is the input file name recorded
/// in the generated header (provenance only; all data comes from the parse).
pub fn transpile(content: &str, source_name: &str, opts: &Options) -> Result<String, String> {
    let parser = Parser::new();
    let (rules, info) = parser
        .parse_mapping(content)
        .map_err(|e| format!("{source_name}: parse failed: {e:?}"))?;
    transpile_parsed(&rules, &info, source_name, opts)
}

/// Transpiles already-parsed rules (the output of
/// [`cf_uast_mapping::Parser::parse_mapping`]) into a Rust module.
pub fn transpile_parsed(
    rules: &[Rule],
    info: &LanguageInfo,
    source_name: &str,
    opts: &Options,
) -> Result<String, String> {
    validate(rules, info)?;

    // A rule goes to the plain-literal path when flagged explicitly or when
    // its name (or extends target) cannot appear as a macro identifier.
    let escaped: Vec<(&str, &str)> = rules
        .iter()
        .filter_map(|r| {
            if opts.escape_hatch.contains(&r.name) {
                Some((r.name.as_str(), "flagged via --escape-hatch"))
            } else if !is_macro_ident(&r.name) {
                Some((r.name.as_str(), "rule name is not a macro identifier"))
            } else if !r.extends.is_empty() && !is_macro_ident(&r.extends) {
                Some((r.name.as_str(), "extends target is not a macro identifier"))
            } else {
                None
            }
        })
        .collect();

    if escaped.is_empty() {
        Ok(emit_macro_module(rules, info, source_name))
    } else {
        Ok(emit_plain_module(rules, info, source_name, &escaped))
    }
}

/// Whether `name` can be written as a bare identifier in a `uast_language!`
/// rule position. The `ident` fragment matcher accepts keywords (`loop`,
/// `type`, …) but rejects the reserved identifier `_`, so `_` is excluded
/// even though it matches `[A-Za-z_][A-Za-z0-9_]*`.
fn is_macro_ident(name: &str) -> bool {
    if name == "_" {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Upper-cases the language name into the static's identifier
/// (non-alphanumerics become `_`): `go` → `GO`, `c_sharp` → `C_SHARP`.
fn static_ident(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Validates every rule against the closed vocabularies and the model's
/// invariants, so emission can never produce an `Other(_)` variant or an
/// empty `props: {}` (which would round-trip to `None` and fail the gate).
fn validate(rules: &[Rule], info: &LanguageInfo) -> Result<(), String> {
    let lang = &info.name;
    for rule in rules {
        let name = &rule.name;
        if UastType::parse(&rule.uast_spec.r#type).is_none() {
            return Err(format!(
                "{lang}/{name}: out-of-vocabulary type {:?}",
                rule.uast_spec.r#type
            ));
        }
        if TokenSource::parse(&rule.uast_spec.token).is_none() {
            return Err(format!(
                "{lang}/{name}: unrecognized token form {:?}",
                rule.uast_spec.token
            ));
        }
        for role in &rule.uast_spec.roles {
            if Role::parse(role).is_none() {
                return Err(format!("{lang}/{name}: out-of-vocabulary role {role:?}"));
            }
        }
        if let Some(props) = &rule.uast_spec.props {
            // The parser allocates props lazily, so Some(empty) never occurs;
            // an empty emission would convert back to None and fail the gate.
            assert!(!props.is_empty(), "{lang}/{name}: parser produced Some(empty) props");
        }
    }
    Ok(())
}

fn header(out: &mut String, source_name: &str) {
    let _ = writeln!(
        out,
        "//! GENERATED by uastmap2rs — transpiled from `{source_name}`. Do not edit by"
    );
    out.push_str("//! hand; re-run the transpiler and commit its output instead.\n");
}

fn emit_macro_module(rules: &[Rule], info: &LanguageInfo, source_name: &str) -> String {
    let mut out = String::new();
    header(&mut out, source_name);
    out.push('\n');
    out.push_str("use cf_uast_mapping::{uast_language, LanguageMapping};\n\n");
    let _ = writeln!(out, "/// Mapping table for the `{}` language.", info.name);
    let _ = writeln!(
        out,
        "pub static {}: LanguageMapping = uast_language! {{",
        static_ident(&info.name)
    );
    let _ = writeln!(out, "    name: {:?},", info.name);
    let _ = writeln!(out, "    extensions: [{}],", quoted_list(&info.extensions));
    if !info.files.is_empty() {
        let _ = writeln!(out, "    files: [{}],", quoted_list(&info.files));
    }
    out.push_str("    rules: {\n");
    for rule in rules {
        emit_macro_rule(&mut out, rule);
    }
    out.push_str("    }\n};\n");
    out
}

fn emit_macro_rule(out: &mut String, rule: &Rule) {
    let default_pattern = format!("({})", rule.name);
    if rule.pattern == default_pattern {
        let _ = writeln!(out, "        {} => {{", rule.name);
    } else {
        let _ = writeln!(out, "        {} ({:?}) => {{", rule.name, rule.pattern);
    }
    if !rule.extends.is_empty() {
        let _ = writeln!(out, "            extends: {},", rule.extends);
    }
    // `validate` guarantees the vocabulary lookups succeed; the Debug form of
    // a non-`Other` variant is exactly its identifier.
    let utype = UastType::parse(&rule.uast_spec.r#type).expect("validated type");
    let _ = writeln!(out, "            type: {utype:?},");
    match TokenSource::parse(&rule.uast_spec.token).expect("validated token") {
        TokenSource::None => {}
        TokenSource::SelfText => out.push_str("            token: self,\n"),
        TokenSource::Child(t) => {
            let _ = writeln!(out, "            token: child({t:?}),");
        }
        TokenSource::Capture(c) => {
            let _ = writeln!(out, "            token: capture({c:?}),");
        }
    }
    if !rule.uast_spec.roles.is_empty() {
        let roles: Vec<String> = rule
            .uast_spec
            .roles
            .iter()
            .map(|r| format!("{:?}", Role::parse(r).expect("validated role")))
            .collect();
        let _ = writeln!(out, "            roles: [{}],", roles.join(", "));
    }
    if !rule.uast_spec.children.is_empty() {
        let _ = writeln!(
            out,
            "            children: [{}],",
            quoted_list(&rule.uast_spec.children)
        );
    }
    if let Some(props) = &rule.uast_spec.props {
        let pairs: Vec<String> = props.iter().map(|(k, v)| format!("{k:?}: {v:?}")).collect();
        let _ = writeln!(out, "            props: {{ {} }},", pairs.join(", "));
    }
    if !rule.conditions.is_empty() {
        let conds: Vec<String> = rule.conditions.iter().map(|c| format!("{:?}", c.expr)).collect();
        let _ = writeln!(out, "            when: [{}],", conds.join(", "));
    }
    out.push_str("        },\n");
}

fn emit_plain_module(
    rules: &[Rule],
    info: &LanguageInfo,
    source_name: &str,
    escaped: &[(&str, &str)],
) -> String {
    let mut out = String::new();
    header(&mut out, source_name);
    out.push_str("//!\n");
    out.push_str("//! Plain-literal form (the `uast_language!` escape hatch); rules forcing it:\n");
    for (name, reason) in escaped {
        let _ = writeln!(out, "//! * `{name}` — {reason}.");
    }
    out.push('\n');
    let any_roles = rules.iter().any(|r| !r.uast_spec.roles.is_empty());
    if any_roles {
        out.push_str(
            "use cf_uast_mapping::{LanguageMapping, MappingRule, Role, TokenSource, UastType};\n\n",
        );
    } else {
        out.push_str("use cf_uast_mapping::{LanguageMapping, MappingRule, TokenSource, UastType};\n\n");
    }
    let _ = writeln!(out, "/// Mapping table for the `{}` language.", info.name);
    out.push_str("#[rustfmt::skip]\n");
    let _ = writeln!(
        out,
        "pub static {}: LanguageMapping = LanguageMapping {{",
        static_ident(&info.name)
    );
    let _ = writeln!(out, "    name: {:?},", info.name);
    let _ = writeln!(out, "    extensions: &[{}],", quoted_list(&info.extensions));
    let _ = writeln!(out, "    files: &[{}],", quoted_list(&info.files));
    out.push_str("    rules: &[\n");
    for rule in rules {
        emit_plain_rule(&mut out, rule);
    }
    out.push_str("    ],\n};\n");
    out
}

fn emit_plain_rule(out: &mut String, rule: &Rule) {
    out.push_str("        MappingRule {\n");
    let _ = writeln!(out, "            name: {:?},", rule.name);
    let _ = writeln!(out, "            pattern: {:?},", rule.pattern);
    let _ = writeln!(out, "            extends: {:?},", rule.extends);
    let utype = UastType::parse(&rule.uast_spec.r#type).expect("validated type");
    let _ = writeln!(out, "            uast_type: UastType::{utype:?},");
    let token = TokenSource::parse(&rule.uast_spec.token).expect("validated token");
    let _ = writeln!(out, "            token: TokenSource::{token:?},");
    let roles: Vec<String> = rule
        .uast_spec
        .roles
        .iter()
        .map(|r| format!("Role::{:?}", Role::parse(r).expect("validated role")))
        .collect();
    let _ = writeln!(out, "            roles: &[{}],", roles.join(", "));
    let _ = writeln!(
        out,
        "            children: &[{}],",
        quoted_list(&rule.uast_spec.children)
    );
    let props: Vec<String> = rule
        .uast_spec
        .props
        .iter()
        .flatten()
        .map(|(k, v)| format!("({k:?}, {v:?})"))
        .collect();
    let _ = writeln!(out, "            props: &[{}],", props.join(", "));
    let conds: Vec<String> = rule.conditions.iter().map(|c| format!("{:?}", c.expr)).collect();
    let _ = writeln!(out, "            conditions: &[{}],", conds.join(", "));
    out.push_str("        },\n");
}

fn quoted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_ident_rules() {
        assert!(is_macro_ident("assignment_statement"));
        assert!(is_macro_ident("loop")); // keywords are matched by `ident`
        assert!(is_macro_ident("_expression"));
        assert!(!is_macro_ident("_")); // reserved identifier — macro rejects it
        assert!(!is_macro_ident("9lives"));
        assert!(!is_macro_ident("dash-name"));
        assert!(!is_macro_ident(""));
    }

    #[test]
    fn static_ident_mapping() {
        assert_eq!(static_ident("go"), "GO");
        assert_eq!(static_ident("c_sharp"), "C_SHARP");
        assert_eq!(static_ident("nim_format_string"), "NIM_FORMAT_STRING");
    }

    #[test]
    fn pattern_with_embedded_quotes_is_escaped() {
        // The {:?} escaping must yield a valid double-quoted literal even for
        // patterns carrying quotes/backslashes (not producible by the DSL
        // parser's pattern grammar, but the emitter must stay total).
        let rule = Rule {
            name: "op".to_string(),
            pattern: "(op \"+\" \\esc)".to_string(),
            ..Rule::default()
        };
        let mut rule = rule;
        rule.uast_spec.r#type = "Synthetic".to_string();
        let info = LanguageInfo {
            name: "syn".to_string(),
            extensions: vec![],
            files: vec![],
        };
        let out = transpile_parsed(&[rule], &info, "syn.uastmap", &Options::default()).unwrap();
        assert!(out.contains(r#"op ("(op \"+\" \\esc)") => {"#), "got:\n{out}");
    }
}
