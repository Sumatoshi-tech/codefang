//! Linguist language types: enry's `GetLanguageType`.
//!
//! Every Linguist language is one of data, programming, markup, or prose. The
//! type answers "is this file data or code?", which is the question skip
//! decisions need. It comes from enry v2.1.0's `data.LanguagesType`, vendored
//! verbatim as `data/enry-types-v2.1.0.tsv` (see `data/README.md`).
//!
//! A type is a coarse signal, not a guarantee. SQL and GraphQL are data-typed,
//! yet they declare functions and operations, so a skip decision also needs the
//! UAST mapping's answer (see `Parser::yields_functions` in `cf-uast`).

use std::collections::HashMap;
use std::sync::OnceLock;

/// enry v2.1.0's `data.LanguagesType`, one `language<TAB>code` row per line.
const ENRY_TYPES_TSV: &str = include_str!("../data/enry-types-v2.1.0.tsv");

/// The Linguist type of a language. Codes follow enry's `Type` enum
/// (`Unknown` = 0 is never stored, so it has no variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageType {
    /// Structured data or configuration (JSON, YAML, CSV, SQL, …).
    Data,
    /// Source code.
    Programming,
    /// Markup (HTML, Markdown, XML-like documents).
    Markup,
    /// Natural-language prose (Text, AsciiDoc, Textile).
    Prose,
}

impl LanguageType {
    /// Maps enry's integer code to a type; `None` for `Unknown` and any
    /// unrecognized code.
    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Data),
            2 => Some(Self::Programming),
            3 => Some(Self::Markup),
            4 => Some(Self::Prose),
            _ => None,
        }
    }
}

/// The Linguist type of `language` (enry's canonical name, e.g. `"JSON"`), or
/// `None` when enry has no type for it.
#[must_use]
pub fn language_type(language: &str) -> Option<LanguageType> {
    types().get(language).copied()
}

/// The parsed type table, loaded once.
fn types() -> &'static HashMap<String, LanguageType> {
    static TYPES: OnceLock<HashMap<String, LanguageType>> = OnceLock::new();
    TYPES.get_or_init(|| parse_types(ENRY_TYPES_TSV))
}

/// Parses `language<TAB>code` rows; malformed or `Unknown` rows are skipped.
fn parse_types(tsv: &str) -> HashMap<String, LanguageType> {
    tsv.lines()
        .filter_map(|line| {
            let (name, code) = line.rsplit_once('\t')?;
            let kind = LanguageType::from_code(code.parse().ok()?)?;
            Some((name.to_string(), kind))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_table_loads_every_language() {
        assert_eq!(types().len(), 519);
    }

    #[test]
    fn known_languages_have_enry_types() {
        assert_eq!(language_type("JSON"), Some(LanguageType::Data));
        assert_eq!(language_type("YAML"), Some(LanguageType::Data));
        assert_eq!(language_type("Go"), Some(LanguageType::Programming));
        assert_eq!(language_type("HTML"), Some(LanguageType::Markup));
        assert_eq!(language_type("Markdown"), Some(LanguageType::Prose));
        assert_eq!(language_type("Text"), Some(LanguageType::Prose));
    }

    #[test]
    fn unknown_or_inexact_names_have_no_type() {
        assert_eq!(language_type("NotALanguage"), None);
        assert_eq!(language_type("json"), None, "enry names are exact");
    }

    #[test]
    fn parse_skips_unknown_and_malformed_rows() {
        let parsed = parse_types("A\t1\nB\t0\nC\tx\nD\t2\nNoTab\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.get("A"), Some(&LanguageType::Data));
        assert_eq!(parsed.get("D"), Some(&LanguageType::Programming));
    }
}
