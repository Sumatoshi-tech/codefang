//! Analyzer descriptors, modes, and ID normalization.

use std::fmt;

/// Analyzer runtime mode.
///
/// The string values (`static`, `history`) are byte-identity relevant: they
/// appear in descriptor IDs (`static/complexity`, `history/burndown`) and in
/// `UnifiedModel` JSON/YAML output, so they are reproduced exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalyzerMode {
    /// `static` — UAST-phase analysis.
    Static,
    /// `history` — commit-history analysis.
    History,
}

impl AnalyzerMode {
    /// Returns the wire string value (`static` / `history`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::History => "history",
        }
    }

    /// Parses a mode string (`static` / `history`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "static" => Some(Self::Static),
            "history" => Some(Self::History),
            _ => None,
        }
    }
}

impl fmt::Display for AnalyzerMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable analyzer metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// `ID` — e.g. `static/complexity` or `history/burndown`.
    pub id: String,
    /// `Description` — human-readable description.
    pub description: String,
    /// `Mode` — static or history.
    pub mode: AnalyzerMode,
}

const NORMALIZE_EXTRA_CAPACITY: usize = 4;

/// Builds stable analyzer metadata from a mode, name, and description.
/// The ID is `"{mode}/{normalized-name}"`.
///
/// ```
/// use cf_analyze::new_descriptor;
/// use cf_analyze::descriptor::AnalyzerMode;
///
/// let d = new_descriptor(AnalyzerMode::Static, "Complexity", "Cyclomatic complexity");
/// assert_eq!(d.id, "static/complexity");
/// assert_eq!(d.mode, AnalyzerMode::Static);
///
/// // The name is normalized into the ID.
/// let h = new_descriptor(AnalyzerMode::History, "FileHistory", "");
/// assert_eq!(h.id, "history/file-history");
/// ```
#[must_use]
pub fn new_descriptor(mode: AnalyzerMode, name: &str, description: &str) -> Descriptor {
    Descriptor {
        id: format!("{}/{}", mode.as_str(), normalize_name(name)),
        description: description.to_string(),
        mode,
    }
}

/// Normalizes an analyzer name to kebab-case.
///
/// Rules (preserved exactly):
/// * `_` and ` ` become `-` (and reset the "previous-lower" state).
/// * An uppercase letter inserts a `-` before it **iff** the previous rune was
///   a lowercase letter, then lower-cases it (camelCase → kebab boundaries).
/// * Other runes are lower-cased.
/// * Leading/trailing `-` are trimmed.
///
/// ```
/// use cf_analyze::normalize_name;
///
/// assert_eq!(normalize_name("FileHistory"), "file-history"); // camelCase boundary
/// assert_eq!(normalize_name("file_history"), "file-history"); // underscores
/// assert_eq!(normalize_name("file history"), "file-history"); // spaces
/// assert_eq!(normalize_name("_foo_"), "foo");                 // trimmed dashes
/// assert_eq!(normalize_name("ABC"), "abc");                   // no lower precedes -> no boundary
/// ```
#[must_use]
pub fn normalize_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut builder = String::with_capacity(trimmed.len() + NORMALIZE_EXTRA_CAPACITY);
    let mut previous_lower = false;

    for current in trimmed.chars() {
        if current == '_' || current == ' ' {
            builder.push('-');
            previous_lower = false;
            continue;
        }

        if current.is_uppercase() {
            if previous_lower {
                builder.push('-');
            }
            // The reference normalizer lowers rune-by-rune (one rune in, one
            // rune out), unlike full Unicode lowercasing which may expand. Use
            // to_lowercase to cover the rare multi-char cases identically to
            // Rust's Unicode tables (parity is fine for ASCII analyzer names).
            builder.extend(current.to_lowercase());
            previous_lower = false;
            continue;
        }

        builder.extend(current.to_lowercase());
        previous_lower = current.is_alphabetic() && current.is_lowercase();
    }

    builder.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_strings() {
        assert_eq!(AnalyzerMode::Static.as_str(), "static");
        assert_eq!(AnalyzerMode::History.as_str(), "history");
    }

    #[test]
    fn descriptor_id_composition() {
        let d = new_descriptor(AnalyzerMode::Static, "Complexity", "desc");
        assert_eq!(d.id, "static/complexity");
        assert_eq!(d.mode, AnalyzerMode::Static);
        assert_eq!(d.description, "desc");
    }

    #[test]
    fn normalize_camel_case() {
        assert_eq!(normalize_name("FileHistory"), "file-history");
        assert_eq!(normalize_name("fileHistory"), "file-history");
    }

    #[test]
    fn normalize_underscores_and_spaces() {
        assert_eq!(normalize_name("file_history"), "file-history");
        assert_eq!(normalize_name("file history"), "file-history");
    }

    #[test]
    fn normalize_trims_dashes() {
        assert_eq!(normalize_name("_foo_"), "foo");
        assert_eq!(normalize_name("  Bar  "), "bar");
    }

    #[test]
    fn normalize_empty() {
        assert_eq!(normalize_name(""), "");
        assert_eq!(normalize_name("   "), "");
    }

    #[test]
    fn normalize_all_caps_no_boundaries() {
        // No lowercase precedes each uppercase, so no '-' inserted.
        assert_eq!(normalize_name("ABC"), "abc");
    }
}
