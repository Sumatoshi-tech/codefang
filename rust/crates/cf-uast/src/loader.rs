//! The lazy language [`Loader`] and its extension bloom filter.
//!
//! Direct port of Go `pkg/uast/loader.go`. The loader registers one
//! [`LazyDslParser`] per embedded `.uastmap` mapping, deferring tree-sitter
//! language initialization until the first `parse` call (matching Go's
//! `loadFromEmbeddedMappingsLazy`). A small fixed-size bloom filter over the
//! registered extensions provides a fast negative membership check.
//!
//! # Bloom-filter byte-parity
//!
//! The generic [`cf_alg_bloom::Filter`] uses FNV-128a double hashing with a
//! dynamically-sized bit array. The Go loader instead uses a **fixed 512-bit**
//! array with **two FNV-1a-variant hashes** (`bloomHashes`) seeded from two
//! different offset bases. Because `Loader::language_parser` consults the bloom
//! filter before the map (and the loader tests assert membership), the exact
//! hashing must be reproduced — so the loader keeps its own bloom kernel here
//! rather than delegating to `cf-alg-bloom`. This is internal behavior only and
//! never appears in machine output.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use cf_uast_mapping::{LanguageInfo, Parser as MappingParser, PatternMatcher, Rule};
use cf_uast_node::Node;

use crate::lowering::Lowering;
use crate::types::{LanguageParser, ParseError};

/// The bit-array length for the extension bloom filter (Go `bloomSize`).
const BLOOM_SIZE: usize = 512;

/// The number of bits per word in the bloom bit-array (Go `bloomWord`).
const BLOOM_WORD: usize = 64;

/// The number of `u64` words backing the bloom filter.
const BLOOM_WORDS: usize = BLOOM_SIZE / BLOOM_WORD;

/// Pre-compiled mapping data for one language (Go `PrecompiledMapping`).
///
/// In Go this is decoded from the generated `embedded_mappings.gen.go`; in Rust
/// it is derived from the [`cf_uast_uastmaps`] embedded `.uastmap` tables by
/// parsing each mapping's DSL header for its language name and extensions.
#[derive(Debug, Clone, Default)]
pub struct PrecompiledMapping {
    /// Language name (`json:"language"`).
    pub language: String,
    /// Supported file extensions (`json:"extensions"`).
    pub extensions: Vec<String>,
    /// Parsed mapping rules (`json:"rules"`).
    pub rules: Vec<Rule>,
    /// The raw `.uastmap` DSL text the rules were parsed from. Kept so the lazy
    /// parser can (re-)initialize the language on first use.
    pub uast: String,
}

/// Loads UAST parsers for different languages (Go `Loader`).
pub struct Loader {
    parsers: HashMap<String, Arc<dyn LanguageParser + Send + Sync>>,
    extensions: HashMap<String, Arc<dyn LanguageParser + Send + Sync>>,
    ext_bloom: [u64; BLOOM_WORDS],
}

impl Loader {
    /// Creates a loader populated from the embedded `.uastmap` mappings, with
    /// every parser registered lazily (Go `NewLoader` →
    /// `loadFromEmbeddedMappingsLazy`).
    pub fn new() -> Loader {
        let mut loader = Loader {
            parsers: HashMap::new(),
            extensions: HashMap::new(),
            ext_bloom: [0u64; BLOOM_WORDS],
        };
        loader.load_from_embedded_mappings();
        loader
    }

    /// Registers lazy parsers from the embedded mappings.
    fn load_from_embedded_mappings(&mut self) {
        for pm in embedded_mappings_data() {
            let lazy: Arc<dyn LanguageParser + Send + Sync> =
                Arc::new(LazyDslParser::new(pm.clone()));
            self.parsers.insert(pm.language.clone(), Arc::clone(&lazy));
            for ext in &pm.extensions {
                let lower = ext.to_lowercase();
                self.extensions.insert(lower.clone(), Arc::clone(&lazy));
                self.bloom_add(&lower);
            }
        }
    }

    /// Registers an additional parser for a language and its extensions.
    ///
    /// Used by [`crate::Parser::with_map`] to install custom mappings (Go
    /// `loadCustomParsers` registering into `loader.parsers`/`loader.extensions`
    /// and calling `bloomAdd`).
    pub(crate) fn register(&mut self, parser: Arc<dyn LanguageParser + Send + Sync>) {
        self.parsers.insert(parser.language(), Arc::clone(&parser));
        for ext in parser.extensions() {
            let lower = ext.to_lowercase();
            self.extensions.insert(lower.clone(), Arc::clone(&parser));
            self.bloom_add(&lower);
        }
    }

    /// Returns the parser registered for the given file extension, if any.
    ///
    /// Port of Go `LanguageParser`: the extension is lowercased, the bloom
    /// filter gives a fast negative pre-check, then the map is consulted.
    pub fn language_parser(
        &self,
        extension: &str,
    ) -> Option<Arc<dyn LanguageParser + Send + Sync>> {
        let ext = extension.to_lowercase();
        if !self.bloom_may_contain(&ext) {
            return None;
        }
        self.extensions.get(&ext).cloned()
    }

    /// Returns all loaded parsers keyed by language (Go `GetParsers`).
    pub fn get_parsers(&self) -> &HashMap<String, Arc<dyn LanguageParser + Send + Sync>> {
        &self.parsers
    }

    /// Sets the two bloom bits for `ext` (Go `bloomAdd`).
    fn bloom_add(&mut self, ext: &str) {
        let (h1, h2) = bloom_hashes(ext);
        self.ext_bloom[h1 / BLOOM_WORD] |= 1u64 << (h1 % BLOOM_WORD);
        self.ext_bloom[h2 / BLOOM_WORD] |= 1u64 << (h2 % BLOOM_WORD);
    }

    /// Returns whether both bloom bits for `ext` are set (Go `bloomMayContain`).
    fn bloom_may_contain(&self, ext: &str) -> bool {
        let (h1, h2) = bloom_hashes(ext);
        self.ext_bloom[h1 / BLOOM_WORD] & (1u64 << (h1 % BLOOM_WORD)) != 0
            && self.ext_bloom[h2 / BLOOM_WORD] & (1u64 << (h2 % BLOOM_WORD)) != 0
    }
}

impl Default for Loader {
    fn default() -> Self {
        Loader::new()
    }
}

/// Returns two independent bloom bit positions for `s` (Go `bloomHashes`).
///
/// Two FNV-1a-variant hashes are folded over the input bytes from two distinct
/// 64-bit offset bases with the standard 64-bit FNV prime; each result is taken
/// modulo [`BLOOM_SIZE`]. Go's `uint` arithmetic wraps at 64 bits, so this uses
/// `wrapping_*` on `u64` to be bit-identical.
fn bloom_hashes(s: &str) -> (usize, usize) {
    const FNV_BASIS_1: u64 = 14695981039346656037;
    const FNV_BASIS_2: u64 = 17316225907498340287;
    const FNV_PRIME: u64 = 1099511628211;

    let mut h1 = FNV_BASIS_1;
    let mut h2 = FNV_BASIS_2;

    // Go ranges over `s[i]` (bytes of the string), not runes.
    for &b in s.as_bytes() {
        h1 ^= u64::from(b);
        h1 = h1.wrapping_mul(FNV_PRIME);
        h2 ^= u64::from(b);
        h2 = h2.wrapping_mul(FNV_PRIME);
    }

    (
        (h1 % BLOOM_SIZE as u64) as usize,
        (h2 % BLOOM_SIZE as u64) as usize,
    )
}

/// Wraps a [`PrecompiledMapping`] and defers tree-sitter language
/// initialization until the first `parse` call (Go `lazyDSLParser`).
///
/// Initialization is performed at most once via a [`OnceLock`] (the analogue of
/// Go's `sync.Once`).
struct LazyDslParser {
    mapping: PrecompiledMapping,
    extensions: Vec<String>,
    language: String,
    inited: OnceLock<Result<InitState, ParseError>>,
}

/// The state produced when a lazy parser is initialized: a tree-sitter
/// [`tree_sitter::Language`] plus the parsed rules / derived lookup structures
/// needed to lower a concrete syntax tree to a UAST (Go `DSLParser` fields built
/// in `initializeLanguage`).
struct InitState {
    rules: Vec<Rule>,
    #[allow(dead_code)]
    lang_info: LanguageInfo,
    /// First-occurrence-wins rule index keyed by node type (Go `ruleIndex`).
    rule_index: HashMap<String, usize>,
    /// The tree-sitter language, looked up via [`crate::languages::get_language`].
    /// `None` means no grammar is vendored for this language yet.
    ts_language: Option<tree_sitter::Language>,
    /// Compiled-pattern matcher bound to `ts_language` (Go `patternMatcher`).
    /// `None` when no grammar is available.
    pattern_matcher: Option<PatternMatcher>,
    /// Language name (Go `langInfo.Name`).
    language: String,
}

impl LazyDslParser {
    fn new(pm: PrecompiledMapping) -> LazyDslParser {
        let extensions = pm.extensions.clone();
        let language = pm.language.clone();
        LazyDslParser {
            mapping: pm,
            extensions,
            language,
            inited: OnceLock::new(),
        }
    }

    /// Initializes the parser once (Go `lazyDSLParser.init` →
    /// `DSLParser.initializeLanguage`).
    fn init(&self) -> &Result<InitState, ParseError> {
        self.inited.get_or_init(|| {
            // Parse the DSL to obtain rules + language info (mirrors building a
            // `DSLParser` from the precompiled mapping). When the precompiled
            // mapping already carries rules, reuse them directly.
            let (rules, lang_info) = if self.mapping.rules.is_empty() {
                MappingParser::new()
                    .parse_mapping(&self.mapping.uast)
                    .map_err(|e| ParseError::Other(e.to_string()))?
            } else {
                (
                    self.mapping.rules.clone(),
                    LanguageInfo {
                        name: self.mapping.language.clone(),
                        extensions: self.mapping.extensions.clone(),
                        files: Vec::new(),
                    },
                )
            };

            // Build the O(1) rule lookup index (first occurrence wins), matching
            // Go `initializeLanguage`'s `ruleIndex` construction.
            let mut rule_index: HashMap<String, usize> = HashMap::with_capacity(rules.len());
            for (i, r) in rules.iter().enumerate() {
                rule_index.entry(r.name.clone()).or_insert(i);
            }

            let ts_language = crate::languages::get_language(&self.mapping.language);
            let pattern_matcher = ts_language
                .as_ref()
                .map(|lang| PatternMatcher::new(lang.clone()));

            Ok(InitState {
                rules,
                lang_info,
                rule_index,
                ts_language,
                pattern_matcher,
                language: self.mapping.language.clone(),
            })
        })
    }
}

impl LanguageParser for LazyDslParser {
    fn parse(&self, _filename: &str, content: &[u8]) -> Result<Node, ParseError> {
        let state = match self.init() {
            Ok(s) => s,
            Err(e) => return Err(e.clone()),
        };

        let lang = match state.ts_language.as_ref() {
            Some(lang) => lang,
            None => {
                return Err(ParseError::Other(format!(
                    "no tree-sitter grammar wired for language {} (grammar vendoring pending; see crate docs)",
                    self.language
                )))
            }
        };
        let pattern_matcher = state.pattern_matcher.as_ref().expect(
            "pattern_matcher is always Some when ts_language is Some (built together in init)",
        );

        // Go uses a pooled `*sitter.Parser`; a fresh parser per call is
        // behavior-identical (pooling never affects output bytes — DESIGN).
        let mut ts_parser = tree_sitter::Parser::new();
        ts_parser
            .set_language(lang)
            .map_err(|e| ParseError::Other(format!("dsl parser: set language: {e}")))?;

        let tree = ts_parser
            .parse(content, None)
            .ok_or_else(|| ParseError::Other("dsl parser: failed to parse".to_string()))?;

        let lowering = Lowering::new(
            content,
            &state.rules,
            &state.rule_index,
            pattern_matcher,
            &state.language,
            false,
        );

        // Go returns `errNoRootNode` only when the root is null, which
        // tree-sitter never produces for a successful parse. A root that lowers
        // to `None` (e.g. an empty source file) yields an empty `File` root in
        // Go's `outputNode` path is not reached — Go returns the canonical node
        // directly. Here `lower` returning `None` means the root collapsed; we
        // surface a default (empty) node to mirror Go's non-nil root for the
        // empty-input case.
        Ok(lowering.lower(&tree).unwrap_or_default())
    }

    fn language(&self) -> String {
        self.language.clone()
    }

    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
}

/// Returns the embedded precompiled mappings (Go `embeddedMappingsData`).
///
/// Derived once from [`cf_uast_uastmaps::embedded_mappings`] by parsing each
/// `.uastmap`'s DSL header for its language name and extensions. The list is
/// process-wide and built lazily.
fn embedded_mappings_data() -> &'static [PrecompiledMapping] {
    static DATA: OnceLock<Vec<PrecompiledMapping>> = OnceLock::new();
    DATA.get_or_init(|| {
        let mp = MappingParser::new();
        cf_uast_uastmaps::embedded_mappings()
            .iter()
            .map(|(&name, &content)| {
                // Parse the header (cheap) for extensions; full rule compilation
                // is deferred to first use by leaving `rules` empty.
                let extensions = mp
                    .parse_mapping(content)
                    .map(|(_, lang)| lang.extensions)
                    .unwrap_or_default();
                PrecompiledMapping {
                    language: name.to_string(),
                    extensions,
                    rules: Vec::new(),
                    uast: content.to_string(),
                }
            })
            .collect()
    })
}

/// Whether any embedded mappings are available (Go `embeddedMappingsAvailable`).
pub fn embedded_mappings_available() -> bool {
    !embedded_mappings_data().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_hashes_known_vector() {
        // Recompute the FNV-1a-variant hashes independently to pin the kernel.
        fn reference(s: &str) -> (usize, usize) {
            let mut h1: u64 = 14695981039346656037;
            let mut h2: u64 = 17316225907498340287;
            for &b in s.as_bytes() {
                h1 ^= b as u64;
                h1 = h1.wrapping_mul(1099511628211);
                h2 ^= b as u64;
                h2 = h2.wrapping_mul(1099511628211);
            }
            ((h1 % 512) as usize, (h2 % 512) as usize)
        }
        for ext in [".go", ".rs", ".py", ".java", ".unknownext", ""] {
            assert_eq!(bloom_hashes(ext), reference(ext), "ext={ext:?}");
        }
    }

    #[test]
    fn bloom_no_false_negatives_for_registered_extensions() {
        let loader = Loader::new();
        // Every registered extension must pass the bloom pre-check (a bloom
        // filter has no false negatives).
        for ext in loader.extensions.keys() {
            assert!(
                loader.bloom_may_contain(ext),
                "registered ext {ext:?} failed bloom pre-check"
            );
        }
    }

    #[test]
    fn embedded_mappings_present() {
        assert!(embedded_mappings_available());
        // The embedded set is the 68-language `.uastmap` table.
        assert_eq!(
            embedded_mappings_data().len(),
            cf_uast_uastmaps::len(),
            "precompiled mapping count must match the embedded table"
        );
    }

    #[test]
    fn known_extension_resolves_to_a_parser() {
        let loader = Loader::new();
        // `.go` is registered by the go mapping.
        let p = loader.language_parser(".go");
        assert!(p.is_some(), ".go should resolve to a parser");
        assert_eq!(p.unwrap().language(), "go");
    }

    #[test]
    fn unknown_extension_resolves_to_none() {
        let loader = Loader::new();
        assert!(loader.language_parser(".no_such_ext_zzz").is_none());
    }

    #[test]
    fn extension_lookup_is_case_insensitive() {
        let loader = Loader::new();
        assert!(loader.language_parser(".GO").is_some());
        assert!(loader.language_parser(".Go").is_some());
    }
}
