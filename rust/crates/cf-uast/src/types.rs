//! Core facade types: the [`LanguageParser`] trait, [`Map`], [`ChangeType`],
//! [`NodeChange`], and the [`get_file_extension`] helper.
//!
//! Direct port of Go `pkg/uast/types.go`. Field names and semantics mirror the
//! Go declarations so downstream behavior is reproduced exactly.

use cf_uast_node::Node;

/// The configuration key for the UAST provider (Go `ConfigUASTProvider`).
pub const CONFIG_UAST_PROVIDER: &str = "UAST.Provider";

/// The type of change between two UAST nodes (Go `ChangeType`).
///
/// Discriminants match the Go `iota` order so the integer values are identical
/// (`Added = 0`, `Removed = 1`, `Modified = 2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// A node was added (`ChangeAdded`).
    Added = 0,
    /// A node was removed (`ChangeRemoved`).
    Removed = 1,
    /// A node was modified (`ChangeModified`).
    Modified = 2,
}

impl ChangeType {
    /// Returns the Go `String()` rendering: `added`, `removed`, `modified`, or
    /// `unknown` for any out-of-range value.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeType::Added => "added",
            ChangeType::Removed => "removed",
            ChangeType::Modified => "modified",
        }
    }
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structural change between two UAST nodes (Go `NodeChange`).
///
/// `before`/`after` are `None` to mirror Go's `nil` pointers for added/removed
/// nodes.
#[derive(Debug, Clone, PartialEq)]
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

/// A custom UAST mapping configuration (Go `Map`).
///
/// JSON field tags are `uast` and `extensions`; any serialization must route
/// through `cf-gojson` to preserve byte-identity (DESIGN §2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Map {
    /// The raw `.uastmap` DSL text (`json:"uast"`).
    pub uast: String,
    /// The supported file extensions (`json:"extensions"`).
    pub extensions: Vec<String>,
}

/// Parses source code into UAST nodes (Go `LanguageParser` interface).
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

/// Minimum number of dot-separated parts for a filename to have an extension
/// (Go `minExtParts`).
const MIN_EXT_PARTS: usize = 2;

/// Returns the file extension (including the leading dot), or an empty string.
///
/// Direct port of Go `getFileExtension`: it splits the *entire* filename on `.`
/// (not just the basename) and returns `"." + lastPart`. Consequences that must
/// be reproduced exactly:
///
/// * `"main.go"` → `".go"`
/// * `"archive.tar.gz"` → `".gz"`
/// * `"Makefile"` → `""` (no dot)
/// * `".gitignore"` → `".gitignore"` (split yields `["", "gitignore"]`)
/// * `"dir.with.dot/file"` → `".with.dot/file"` would *not* happen because the
///   last `.`-segment is taken verbatim; Go uses `strings.Split` on the raw path
///   so e.g. `"a.b/c"` → `".b/c"`. This is faithfully reproduced.
pub fn get_file_extension(filename: &str) -> String {
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() < MIN_EXT_PARTS {
        return String::new();
    }
    format!(".{}", parts[parts.len() - 1])
}

/// Errors produced by parser operations (the Go sentinel `errors`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No file extension found (Go `errNoFileExtension`). Carries the filename
    /// to reproduce Go's `"%w for %s"` formatting.
    NoFileExtension {
        /// The filename that lacked an extension.
        filename: String,
    },
    /// No parser found for the extension (Go `errNoParser`). Carries the
    /// extension to reproduce Go's `"%w %s"` formatting.
    NoParser {
        /// The (lowercased) extension that had no registered parser.
        ext: String,
    },
    /// A mapping was not found (Go `errMappingNotFound`). Carries the language
    /// to reproduce Go's `"%w: %s"` formatting.
    MappingNotFound {
        /// The language whose mapping was missing.
        language: String,
    },
    /// A panic occurred while loading a parser (Go `errParserLoadPanic`).
    ParserLoadPanic {
        /// The panic detail.
        detail: String,
    },
    /// The underlying mapping DSL failed to parse / the language could not be
    /// initialized. Carries a human-readable message.
    Other(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Go: fmt.Errorf("%w for %s", errNoFileExtension, filename)
            ParseError::NoFileExtension { filename } => {
                write!(f, "no file extension found for {filename}")
            }
            // Go: fmt.Errorf("%w %s", errNoParser, ext)
            ParseError::NoParser { ext } => write!(f, "no parser found for extension {ext}"),
            // Go: fmt.Errorf("%w: %s", errMappingNotFound, language)
            ParseError::MappingNotFound { language } => write!(f, "mapping not found: {language}"),
            // Go: fmt.Errorf("%w: %v", errParserLoadPanic, r)
            ParseError::ParserLoadPanic { detail } => {
                write!(f, "panic while loading parser: {detail}")
            }
            ParseError::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for ParseError {}

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
        // Go strings.Split on '.' returns the last segment.
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
    fn change_type_discriminants_match_go_iota() {
        assert_eq!(ChangeType::Added as i32, 0);
        assert_eq!(ChangeType::Removed as i32, 1);
        assert_eq!(ChangeType::Modified as i32, 2);
    }

    #[test]
    fn error_messages_match_go() {
        assert_eq!(
            ParseError::NoFileExtension { filename: "x".into() }.to_string(),
            "no file extension found for x"
        );
        assert_eq!(
            ParseError::NoParser { ext: ".xyz".into() }.to_string(),
            "no parser found for extension .xyz"
        );
        assert_eq!(
            ParseError::MappingNotFound { language: "go".into() }.to_string(),
            "mapping not found: go"
        );
        assert_eq!(
            ParseError::ParserLoadPanic { detail: "boom".into() }.to_string(),
            "panic while loading parser: boom"
        );
    }
}
