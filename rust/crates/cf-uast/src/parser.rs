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
pub struct Parser {
    loader: Loader,
    custom_maps: HashMap<String, Map>,
}

impl Parser {
    /// Creates a parser with DSL-based language parsers loaded from the
    /// embedded `.uastmap` mappings.
    ///
    /// Infallible: the embedded table is validated at build time by
    /// `cf-uast-uastmaps`, so there is no I/O that can fail here.
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

    /// Returns all embedded UAST mappings keyed by language.
    ///
    /// Each mapping's DSL is parsed for its extensions; a mapping whose DSL
    /// fails to parse is omitted.
    #[must_use]
    pub fn get_embedded_mappings(&self) -> HashMap<String, Map> {
        let mp = MappingParser::new();
        let mut mappings = HashMap::new();
        for (&language, &content) in cf_uast_uastmaps::embedded_mappings() {
            match mp.parse_mapping(content) {
                Ok((_, lang_info)) => {
                    mappings.insert(
                        language.to_string(),
                        Map {
                            uast: content.to_string(),
                            extensions: lang_info.extensions,
                        },
                    );
                }
                Err(_) => continue,
            }
        }
        mappings
    }

    /// Returns a lightweight listing of embedded mappings: each language maps
    /// to its `.uastmap` content size in bytes.
    #[must_use]
    pub fn get_embedded_mappings_list(&self) -> HashMap<String, MappingInfo> {
        let mut mappings = HashMap::new();
        for (&language, &content) in cf_uast_uastmaps::embedded_mappings() {
            mappings.insert(language.to_string(), MappingInfo { size: content.len() });
        }
        mappings
    }

    /// Returns a specific embedded mapping by language name.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::MappingNotFound`] if the language is not
    /// embedded, or [`ParseError::Other`] if its DSL fails to parse.
    pub fn get_mapping(&self, language: &str) -> Result<Map, ParseError> {
        let content = cf_uast_uastmaps::get(language).ok_or_else(|| ParseError::MappingNotFound {
            language: language.to_string(),
        })?;

        let (_, lang_info) = MappingParser::new()
            .parse_mapping(content)
            .map_err(|e| ParseError::Other(format!("parsing DSL: {e}")))?;

        Ok(Map {
            uast: content.to_string(),
            extensions: lang_info.extensions,
        })
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight per-mapping info returned by
/// [`Parser::get_embedded_mappings_list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappingInfo {
    /// The size in bytes of the `.uastmap` content.
    pub size: usize,
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

    #[test]
    fn embedded_mappings_list_has_sizes() {
        let p = Parser::new();
        let list = p.get_embedded_mappings_list();
        assert_eq!(list.len(), cf_uast_uastmaps::len());
        let go = list.get("go").expect("go mapping present");
        assert!(go.size > 0);
    }

    #[test]
    fn get_mapping_present_and_absent() {
        let p = Parser::new();
        assert!(p.get_mapping("go").is_ok());
        let err = p.get_mapping("cobol").unwrap_err();
        assert_eq!(
            err,
            ParseError::MappingNotFound {
                language: "cobol".into()
            }
        );
    }

    #[test]
    fn get_embedded_mappings_contains_go_extension() {
        let p = Parser::new();
        let mappings = p.get_embedded_mappings();
        let go = mappings.get("go").expect("go mapping");
        assert!(go.extensions.iter().any(|e| e == ".go"));
    }
}
