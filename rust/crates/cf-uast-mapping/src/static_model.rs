//! The static (Rust-native) mapping model: [`MappingRule`] / [`LanguageMapping`].
//!
//! These are the `const`-constructible tables the `uast_language!` macro expands
//! to (specs/uastmap-rust-macros). [`LanguageMapping::to_rules`] bridges them to
//! the existing [`Rule`]/[`LanguageInfo`] model — producing **exactly** what
//! [`crate::Parser::parse_mapping`] produces for the equivalent `.uastmap` text,
//! field for field (including the `props: None` vs `Some` distinction and rule
//! order), so a static table verified equal to the parsed DSL feeds the
//! unchanged lowering with identical inputs and output bytes cannot differ.

use std::collections::BTreeMap;

use crate::dsl_parser::LanguageInfo;
use crate::mapping_types::{Condition, Rule, UastSpec};
use crate::vocab::{Role, TokenSource, UastType};

/// One mapping rule as static data (the macro-expanded form of a DSL rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappingRule {
    /// Rule name (the DSL identifier left of `<-`).
    pub name: &'static str,
    /// The S-expression pattern, verbatim including parentheses (the DSL parser
    /// stores the raw slice; the macro synthesizes `"(<name>)"` when omitted).
    pub pattern: &'static str,
    /// Base rule name for `# Extends` inheritance (`""` when absent — the
    /// corpus never uses it, kept for model totality).
    pub extends: &'static str,
    /// Target UAST type (`type:`).
    pub uast_type: UastType,
    /// Token source (`token:`); [`TokenSource::None`] when absent.
    pub token: TokenSource<'static>,
    /// Roles (`roles:`), in declaration order.
    pub roles: &'static [Role],
    /// Child rule references (`children:`), in declaration order.
    pub children: &'static [&'static str],
    /// Additional properties (any non-reserved `key: "value"` field). An EMPTY
    /// slice converts to `props: None`, mirroring the parser's lazy allocation.
    pub props: &'static [(&'static str, &'static str)],
    /// Condition expressions (`when ...`); empty in the entire corpus.
    pub conditions: &'static [&'static str],
}

impl MappingRule {
    /// Converts to the runtime [`Rule`] the lowering consumes, mirroring
    /// `extract_rule` + `apply_uast_field` exactly.
    #[must_use]
    pub fn to_rule(&self) -> Rule {
        let props = if self.props.is_empty() {
            // Parser: `props` is lazily allocated only when a non-reserved
            // field is present — absent fields leave `None`.
            None
        } else {
            let mut m = BTreeMap::new();
            for (k, v) in self.props {
                m.insert((*k).to_string(), (*v).to_string());
            }
            Some(m)
        };
        Rule {
            name: self.name.to_string(),
            pattern: self.pattern.to_string(),
            extends: self.extends.to_string(),
            uast_spec: UastSpec {
                r#type: self.uast_type.as_str().to_string(),
                token: self.token.token_string(),
                roles: self.roles.iter().map(|r| r.as_str().to_string()).collect(),
                props,
                children: self.children.iter().map(|c| (*c).to_string()).collect(),
            },
            conditions: self
                .conditions
                .iter()
                .map(|e| Condition { expr: (*e).to_string() })
                .collect(),
        }
    }
}

/// One language's complete static mapping table (the macro-expanded form of a
/// `.uastmap` file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageMapping {
    /// Language name (the `[language "<name>" ...]` header).
    pub name: &'static str,
    /// File extensions (`extensions:` header list), each including the dot.
    pub extensions: &'static [&'static str],
    /// Exact file names (`files:` header list, e.g. `"Dockerfile"`).
    pub files: &'static [&'static str],
    /// Mapping rules in file order (rule order is observable: the lowering's
    /// first-occurrence-wins index depends on it).
    pub rules: &'static [MappingRule],
}

impl LanguageMapping {
    /// Converts to the `(Vec<Rule>, LanguageInfo)` pair
    /// [`crate::Parser::parse_mapping`] returns for the equivalent DSL text.
    #[must_use]
    pub fn to_rules(&self) -> (Vec<Rule>, LanguageInfo) {
        let rules = self.rules.iter().map(MappingRule::to_rule).collect();
        let info = LanguageInfo {
            name: self.name.to_string(),
            extensions: self.extensions.iter().map(|e| (*e).to_string()).collect(),
            files: self.files.iter().map(|f| (*f).to_string()).collect(),
        };
        (rules, info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `const`-constructibility (the macro emits exactly this shape).
    const TWO_RULES: LanguageMapping = LanguageMapping {
        name: "t",
        extensions: &[".t"],
        files: &[],
        rules: &[
            MappingRule {
                name: "assignment_statement",
                pattern: "(assignment_statement)",
                extends: "",
                uast_type: UastType::Assignment,
                token: TokenSource::SelfText,
                roles: &[Role::Assignment],
                children: &["expression_list"],
                props: &[],
                conditions: &[],
            },
            MappingRule {
                name: "qualified_type",
                pattern: "(qualified_type package: (package_identifier) @pkg)",
                extends: "",
                uast_type: UastType::Synthetic,
                token: TokenSource::Capture("pkg"),
                roles: &[],
                children: &[],
                props: &[("custom_prop", "v")],
                conditions: &[],
            },
        ],
    };

    /// The equivalent `.uastmap` text for [`TWO_RULES`].
    const TWO_RULES_DSL: &str = r#"[language "t", extensions: ".t"]

assignment_statement <- (assignment_statement) => uast(
    token: "self",
    type: "Assignment",
    roles: "Assignment",
    children: "expression_list"
)

qualified_type <- (qualified_type package: (package_identifier) @pkg) => uast(
    type: "Synthetic",
    token: "@pkg",
    custom_prop: "v"
)
"#;

    #[test]
    fn to_rules_field_semantics() {
        let (rules, info) = TWO_RULES.to_rules();
        assert_eq!(info.name, "t");
        assert_eq!(info.extensions, vec![".t".to_string()]);
        assert!(info.files.is_empty());

        // Default-form pattern passthrough + token "self" + roles/children.
        let r0 = &rules[0];
        assert_eq!(r0.name, "assignment_statement");
        assert_eq!(r0.pattern, "(assignment_statement)");
        assert_eq!(r0.extends, "");
        assert_eq!(r0.uast_spec.r#type, "Assignment");
        assert_eq!(r0.uast_spec.token, "self");
        assert_eq!(r0.uast_spec.roles, vec!["Assignment".to_string()]);
        assert_eq!(r0.uast_spec.children, vec!["expression_list".to_string()]);
        // Empty props slice ⇒ None (the parser's nil mirror).
        assert!(r0.uast_spec.props.is_none());
        assert!(r0.conditions.is_empty());

        // Explicit pattern + capture token + populated props.
        let r1 = &rules[1];
        assert_eq!(r1.pattern, "(qualified_type package: (package_identifier) @pkg)");
        assert_eq!(r1.uast_spec.token, "@pkg");
        let props = r1.uast_spec.props.as_ref().expect("props Some");
        assert_eq!(props.get("custom_prop").map(String::as_str), Some("v"));
    }

    #[test]
    fn token_forms_and_extends_conditions() {
        // All four token forms + extends + conditions convert faithfully.
        let rule = MappingRule {
            name: "x",
            pattern: "(x)",
            extends: "base_rule",
            uast_type: UastType::Other("Custom"),
            token: TokenSource::Child("identifier"),
            roles: &[],
            children: &[],
            props: &[],
            conditions: &["field == \"v\""],
        };
        let r = rule.to_rule();
        assert_eq!(r.extends, "base_rule");
        assert_eq!(r.uast_spec.r#type, "Custom");
        assert_eq!(r.uast_spec.token, "child:identifier");
        assert_eq!(r.conditions.len(), 1);
        assert_eq!(r.conditions[0].expr, "field == \"v\"");

        let none = MappingRule { token: TokenSource::None, ..rule };
        assert_eq!(none.to_rule().uast_spec.token, "");
    }

    /// The first micro equality proof: the static table converts to EXACTLY what
    /// the DSL parser produces for the equivalent text.
    #[test]
    fn micro_equality_vs_parser() {
        let parser = crate::Parser::new();
        let (parsed_rules, parsed_info) =
            parser.parse_mapping(TWO_RULES_DSL).expect("DSL parses");
        let (static_rules, static_info) = TWO_RULES.to_rules();
        assert_eq!(parsed_info, static_info);
        assert_eq!(parsed_rules, static_rules);
    }
}
