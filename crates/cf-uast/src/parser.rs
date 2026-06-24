//! The aggregate [`Parser`] facade.
//!
//! The parser owns a [`Loader`] (which registers one lazy parser per embedded
//! `.uastmap` mapping) plus a set of user-supplied custom maps. It is the
//! entry point callers use to detect support, resolve a language, and parse a
//! file into a UAST.

use std::collections::HashMap;
use std::sync::Arc;

use cf_uast_mapping::Parser as MappingParser;
use cf_uast_node::Node;

use crate::loader::Loader;
use crate::types::{get_file_extension, LanguageParser, Map, ParseError};

/// The main entry point for UAST parsing.
///
/// Holds the language [`Loader`] and any custom mappings installed via
/// [`Parser::with_map`].
///
/// # Examples
///
/// Detect support and resolve a language from a filename:
///
/// ```
/// use cf_uast::Parser;
///
/// let parser = Parser::new();
/// assert!(parser.is_supported("main.go"));
/// assert_eq!(parser.get_language("lib.rs"), "rust");
///
/// // A filename with no extension is unsupported.
/// assert!(!parser.is_supported("Makefile"));
/// assert_eq!(parser.get_language("Makefile"), "");
/// ```
pub struct Parser {
    loader: Loader,
    custom_maps: HashMap<String, Map>,
}

impl Parser {
    /// Creates a parser with language parsers loaded from the native
    /// `cf-uast-mappings` registry.
    ///
    /// Infallible: the mapping tables are compiled into the binary, so there
    /// is no I/O that can fail here.
    #[must_use]
    pub fn new() -> Self {
        Self {
            loader: Loader::new(),
            custom_maps: HashMap::new(),
        }
    }

    /// Adds custom UAST mappings, registering a parser for each.
    ///
    /// Each custom map's DSL text is parsed; on success the resulting parser
    /// is registered by its language name and by each of its (lowercased)
    /// extensions, which are also added to the loader's bloom filter. A map
    /// whose DSL fails to parse is silently skipped (reference-implementation
    /// behavior).
    #[must_use]
    pub fn with_map(mut self, uast_maps: HashMap<String, Map>) -> Self {
        for (k, v) in uast_maps {
            self.custom_maps.insert(k, v);
        }
        self.load_custom_parsers();
        self
    }

    /// Loads parsers from the custom maps.
    fn load_custom_parsers(&mut self) {
        let mp = MappingParser::new();
        for uast_map in self.custom_maps.values() {
            match mp.parse_mapping(&uast_map.uast) {
                Ok((rules, lang_info)) => {
                    let parser = Arc::new(CustomDslParser {
                        language: lang_info.name.clone(),
                        extensions: if lang_info.extensions.is_empty() {
                            uast_map.extensions.clone()
                        } else {
                            lang_info.extensions.clone()
                        },
                        rules,
                        uast: uast_map.uast.clone(),
                    });
                    self.loader.register(parser);
                }
                // A custom map that fails to parse is skipped silently.
                Err(_) => continue,
            }
        }
    }

    /// Returns whether `filename` is supported by any registered parser.
    #[must_use]
    pub fn is_supported(&self, filename: &str) -> bool {
        let ext = get_file_extension(filename).to_lowercase();
        if ext.is_empty() {
            return false;
        }
        self.loader.language_parser(&ext).is_some()
    }

    /// Returns the language name for `filename`, or an empty string if
    /// unsupported.
    #[must_use]
    pub fn get_language(&self, filename: &str) -> String {
        let ext = get_file_extension(filename).to_lowercase();
        if ext.is_empty() {
            return String::new();
        }
        match self.loader.language_parser(&ext) {
            Some(p) => p.language(),
            None => String::new(),
        }
    }

    /// Parses `content` for `filename`, returning its UAST root.
    ///
    /// # Errors
    ///
    /// * [`ParseError::NoFileExtension`] when `filename` has no extension;
    /// * [`ParseError::NoParser`] when no parser is registered for the
    ///   (lowercased) extension.
    pub fn parse(&self, filename: &str, content: &[u8]) -> Result<Node, ParseError> {
        let ext = get_file_extension(filename).to_lowercase();
        if ext.is_empty() {
            return Err(ParseError::NoFileExtension {
                filename: filename.to_string(),
            });
        }

        let lang_parser = self
            .loader
            .language_parser(&ext)
            .ok_or_else(|| ParseError::NoParser { ext: ext.clone() })?;

        lang_parser.parse(filename, content)
    }

}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// A parser built from a custom (user-supplied) mapping.
struct CustomDslParser {
    language: String,
    extensions: Vec<String>,
    #[allow(dead_code)]
    rules: Vec<cf_uast_mapping::Rule>,
    #[allow(dead_code)]
    uast: String,
}

impl LanguageParser for CustomDslParser {
    fn parse(&self, _filename: &str, content: &[u8]) -> Result<Node, ParseError> {
        let _ = content;
        // Same grammar-integration gap as the lazy embedded parser.
        let ts_language = crate::languages::get_language(&self.language);
        if ts_language.is_none() {
            return Err(ParseError::Other(format!(
                "no tree-sitter grammar wired for language {} (grammar vendoring pending)",
                self.language
            )));
        }
        Err(ParseError::Other(format!(
            "tree-sitter parse for {} not yet wired (grammar integration pending)",
            self.language
        )))
    }

    fn language(&self) -> String {
        self.language.clone()
    }

    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_parser_supports_go() {
        let p = Parser::new();
        assert!(p.is_supported("main.go"));
        assert_eq!(p.get_language("main.go"), "go");
    }

    #[test]
    fn unsupported_extension() {
        let p = Parser::new();
        assert!(!p.is_supported("file.no_such_ext_zzz"));
        assert_eq!(p.get_language("file.no_such_ext_zzz"), "");
    }

    #[test]
    fn no_extension_is_unsupported() {
        let p = Parser::new();
        assert!(!p.is_supported("Makefile"));
        assert_eq!(p.get_language("Makefile"), "");
    }

    #[test]
    fn parse_no_extension_errors() {
        let p = Parser::new();
        let err = p.parse("Makefile", b"x").unwrap_err();
        assert_eq!(
            err,
            ParseError::NoFileExtension {
                filename: "Makefile".into()
            }
        );
    }

    #[test]
    fn parse_unknown_extension_errors() {
        let p = Parser::new();
        let err = p.parse("file.no_such_ext_zzz", b"x").unwrap_err();
        assert_eq!(
            err,
            ParseError::NoParser {
                ext: ".no_such_ext_zzz".into()
            }
        );
    }

}
