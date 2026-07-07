//! Core facade types: the [`LanguageParser`] trait, [`Map`], [`ChangeType`],
//! [`NodeChange`], and the [`get_file_extension`] helper.

use cf_uast_node::Node;

/// The configuration key for the UAST provider.
pub const CONFIG_UAST_PROVIDER: &str = "UAST.Provider";

/// The type of change between two UAST nodes.
///
/// The discriminant values (`Added = 0`, `Removed = 1`, `Modified = 2`) are
/// frozen: they are observable wherever a change type renders numerically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// A node was added.
    Added = 0,
    /// A node was removed.
    Removed = 1,
    /// A node was modified.
    Modified = 2,
}

impl ChangeType {
    /// Returns the canonical rendering: `added`, `removed`, or `modified`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_uast::ChangeType;
    ///
    /// assert_eq!(ChangeType::Added.as_str(), "added");
    /// assert_eq!(ChangeType::Removed.as_str(), "removed");
    /// assert_eq!(ChangeType::Modified.as_str(), "modified");
    /// // `Display` uses the same rendering.
    /// assert_eq!(ChangeType::Modified.to_string(), "modified");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
        }
    }
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structural change between two UAST nodes.
///
/// `before`/`after` are `None` for added/removed nodes respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeChange {
    /// The node before the change (`None` for additions).
    pub before: Option<Node>,
    /// The node after the change (`None` for removals).
    pub after: Option<Node>,
    /// The file the change belongs to (always empty in the Go diff code itself).
    pub file: String,
    /// The kind of change.
    pub change_type: ChangeType,
}

/// A custom UAST mapping configuration.
///
/// The JSON keys are `uast` and `extensions`; any serialization must route
/// through `cf-gojson` to preserve byte-identity (DESIGN §2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Map {
    /// The raw `.uastmap` DSL text (JSON key `uast`).
    pub uast: String,
    /// The supported file extensions (JSON key `extensions`).
    pub extensions: Vec<String>,
}

/// Parses source code into UAST nodes.
///
/// Implementors are stored in the [`crate::Loader`] keyed by language and by
/// extension.
pub trait LanguageParser {
    /// Parses `content` for `filename`, returning the root UAST node.
    fn parse(&self, filename: &str, content: &[u8]) -> Result<Node, ParseError>;

    /// Returns the language name (e.g. `"go"`).
    fn language(&self) -> String;

    /// Returns the supported file extensions (including the leading dot).
    fn extensions(&self) -> Vec<String>;
}

/// Minimum number of dot-separated parts for a filename to have an extension.
const MIN_EXT_PARTS: usize = 2;

/// Returns the file extension (including the leading dot), or an empty string.
///
/// The *entire* filename (not just the basename) is split on `.` and the last
/// segment returned as `"." + lastPart`. The consequences are frozen
/// reference-implementation behavior (pinned by the differential gate):
///
/// * `"main.go"` → `".go"`
/// * `"archive.tar.gz"` → `".gz"`
/// * `"Makefile"` → `""` (no dot)
/// * `".gitignore"` → `".gitignore"` (split yields `["", "gitignore"]`)
/// * the last `.`-segment is taken verbatim even across path separators, so
///   e.g. `"a.b/c"` → `".b/c"`.
///
/// # Examples
///
/// ```
/// use cf_uast::get_file_extension;
///
/// assert_eq!(get_file_extension("main.go"), ".go");
/// assert_eq!(get_file_extension("archive.tar.gz"), ".gz");
/// assert_eq!(get_file_extension("Makefile"), "");
/// assert_eq!(get_file_extension(".gitignore"), ".gitignore");
/// assert_eq!(get_file_extension("a.b/c"), ".b/c");
/// ```
#[must_use]
pub fn get_file_extension(filename: &str) -> String {
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() < MIN_EXT_PARTS {
        return String::new();
    }
    format!(".{}", parts[parts.len() - 1])
}

/// Errors produced by parser operations.
///
/// The error strings are part of the CLI compatibility contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// No file extension found.
    #[error("no file extension found for {filename}")]
    NoFileExtension {
        /// The filename that lacked an extension.
        filename: String,
    },
    /// No parser found for the extension.
    #[error("no parser found for extension {ext}")]
    NoParser {
        /// The (lowercased) extension that had no registered parser.
        ext: String,
    },
    /// A mapping was not found.
    #[error("mapping not found: {language}")]
    MappingNotFound {
        /// The language whose mapping was missing.
        language: String,
    },
    /// A panic occurred while loading a parser.
    #[error("panic while loading parser: {detail}")]
    ParserLoadPanic {
        /// The panic detail.
        detail: String,
    },
    /// The underlying mapping DSL failed to parse / the language could not be
    /// initialized. Carries a human-readable message.
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_extension_basic() {
        assert_eq!(get_file_extension("main.go"), ".go");
        assert_eq!(get_file_extension("App.JAVA"), ".JAVA");
    }

    #[test]
    fn file_extension_multi_dot_takes_last() {
        assert_eq!(get_file_extension("archive.tar.gz"), ".gz");
    }

    #[test]
    fn file_extension_no_dot_is_empty() {
        assert_eq!(get_file_extension("Makefile"), "");
        assert_eq!(get_file_extension("noext"), "");
    }

    #[test]
    fn file_extension_dotfile() {
        // ".gitignore".split('.') == ["", "gitignore"], so result is ".gitignore".
        assert_eq!(get_file_extension(".gitignore"), ".gitignore");
    }

    #[test]
    fn change_type_strings() {
        assert_eq!(ChangeType::Added.to_string(), "added");
        assert_eq!(ChangeType::Removed.to_string(), "removed");
        assert_eq!(ChangeType::Modified.to_string(), "modified");
    }

    #[test]
    fn change_type_discriminants_are_frozen() {
        assert_eq!(ChangeType::Added as i32, 0);
        assert_eq!(ChangeType::Removed as i32, 1);
        assert_eq!(ChangeType::Modified as i32, 2);
    }

    #[test]
    fn error_messages_are_frozen() {
        assert_eq!(
            ParseError::NoFileExtension {
                filename: "x".into()
            }
            .to_string(),
            "no file extension found for x"
        );
        assert_eq!(
            ParseError::NoParser { ext: ".xyz".into() }.to_string(),
            "no parser found for extension .xyz"
        );
        assert_eq!(
            ParseError::MappingNotFound {
                language: "go".into()
            }
            .to_string(),
            "mapping not found: go"
        );
        assert_eq!(
            ParseError::ParserLoadPanic {
                detail: "boom".into()
            }
            .to_string(),
            "panic while loading parser: boom"
        );
    }
}
