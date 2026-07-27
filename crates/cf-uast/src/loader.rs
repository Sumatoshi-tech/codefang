//! The lazy language [`Loader`] and its extension bloom filter.
//!
//! The loader registers one [`LazyDslParser`] per embedded `.uastmap` mapping,
//! deferring tree-sitter language initialization until the first `parse`
//! call. A small fixed-size bloom filter over the registered extensions
//! provides a fast negative membership check.
//!
//! # Bloom-filter kernel
//!
//! The generic [`cf_alg_bloom::Filter`] uses FNV-128a double hashing with a
//! dynamically-sized bit array. The loader instead keeps its own kernel: a
//! **fixed 512-bit** array with **two FNV-1a-variant hashes** seeded from two
//! different offset bases — the reference-implementation hashing, kept
//! bit-identical because `Loader::language_parser` consults the bloom filter
//! before the map (and the loader tests assert membership). This is internal
//! behavior only and never appears in machine output.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use cf_uast_mapping::{LanguageInfo, Parser as MappingParser, PatternMatcher, Rule};
use cf_uast_node::Node;

use crate::lowering::{resolve_rules, Lowering, ResolvedRule};
use crate::types::{LanguageParser, ParseError};

/// The bit-array length for the extension bloom filter.
/// Per-file tree-sitter parse watchdog (30s). See the comment at the call
/// site in [`LoadedParser::parse`].
const PARSE_TIMEOUT_MICROS: u64 = 30_000_000;

const BLOOM_SIZE: usize = 512;

/// The number of bits per word in the bloom bit-array.
const BLOOM_WORD: usize = 64;

/// The number of `u64` words backing the bloom filter.
const BLOOM_WORDS: usize = BLOOM_SIZE / BLOOM_WORD;

/// Pre-compiled mapping data for one language.
///
/// For embedded languages it points at the language's static table in
/// [`cf_uast_mappings`] (the native mapping registry), whose `to_rules()`
/// output is equality-gated against the DSL parser — so the lazy init below
/// feeds the lowering the exact rules the DSL pipeline produced.
#[derive(Debug, Clone, Default)]
pub struct PrecompiledMapping {
    /// Language name.
    pub language: String,
    /// Supported file extensions.
    pub extensions: Vec<String>,
    /// Parsed mapping rules.
    pub rules: Vec<Rule>,
    /// The raw `.uastmap` DSL text the rules were parsed from. Kept for
    /// CUSTOM (user-supplied) mappings; empty for the embedded languages,
    /// which use [`PrecompiledMapping::table`] instead.
    pub uast: String,
    /// The language's static mapping table, when it is one of the embedded
    /// languages. Converted lazily by the parser's first use.
    pub table: Option<&'static cf_uast_mapping::LanguageMapping>,
}

/// Loads UAST parsers for different languages.
pub struct Loader {
    parsers: HashMap<String, Arc<dyn LanguageParser + Send + Sync>>,
    extensions: HashMap<String, Arc<dyn LanguageParser + Send + Sync>>,
    ext_bloom: [u64; BLOOM_WORDS],
}

impl Loader {
    /// Creates a loader populated from the embedded `.uastmap` mappings, with
    /// every parser registered lazily.
    #[must_use]
    pub fn new() -> Self {
        let mut loader = Self {
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
    /// Used by [`crate::Parser::with_map`] to install custom mappings.
    pub(crate) fn register(&mut self, parser: Arc<dyn LanguageParser + Send + Sync>) {
        for ext in parser.extensions() {
            let lower = ext.to_lowercase();
            self.extensions.insert(lower.clone(), Arc::clone(&parser));
            self.bloom_add(&lower);
        }
        self.parsers.insert(parser.language(), parser);
    }

    /// Returns the parser registered for the given file extension, if any.
    ///
    /// The extension is lowercased, the bloom filter gives a fast negative
    /// pre-check, then the map is consulted.
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_uast::{Loader, LanguageParser};
    ///
    /// let loader = Loader::new();
    /// // Lookup is case-insensitive; `.go` is registered by the go mapping.
    /// assert_eq!(loader.language_parser(".GO").unwrap().language(), "go");
    /// assert!(loader.language_parser(".no_such_ext_zzz").is_none());
    /// ```
    #[must_use]
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

    /// Returns all loaded parsers keyed by language.
    #[must_use]
    pub fn get_parsers(&self) -> &HashMap<String, Arc<dyn LanguageParser + Send + Sync>> {
        &self.parsers
    }

    /// Sets the two bloom bits for `ext`.
    fn bloom_add(&mut self, ext: &str) {
        let (h1, h2) = bloom_hashes(ext);
        self.ext_bloom[h1 / BLOOM_WORD] |= 1u64 << (h1 % BLOOM_WORD);
        self.ext_bloom[h2 / BLOOM_WORD] |= 1u64 << (h2 % BLOOM_WORD);
    }

    /// Returns whether both bloom bits for `ext` are set.
    fn bloom_may_contain(&self, ext: &str) -> bool {
        let (h1, h2) = bloom_hashes(ext);
        self.ext_bloom[h1 / BLOOM_WORD] & (1u64 << (h1 % BLOOM_WORD)) != 0
            && self.ext_bloom[h2 / BLOOM_WORD] & (1u64 << (h2 % BLOOM_WORD)) != 0
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns two independent bloom bit positions for `s`.
///
/// Two FNV-1a-variant hashes are folded over the input bytes from two distinct
/// 64-bit offset bases with the standard 64-bit FNV prime; each result is
/// taken modulo [`BLOOM_SIZE`]. The arithmetic wraps at 64 bits
/// (`wrapping_*`), bit-identical to the reference implementation.
fn bloom_hashes(s: &str) -> (usize, usize) {
    const FNV_BASIS_1: u64 = 14_695_981_039_346_656_037;
    const FNV_BASIS_2: u64 = 17_316_225_907_498_340_287;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut h1 = FNV_BASIS_1;
    let mut h2 = FNV_BASIS_2;

    // Hash the raw bytes of the string, not chars.
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
/// initialization until the first `parse` call.
///
/// Initialization is performed at most once via a [`OnceLock`].
struct LazyDslParser {
    mapping: PrecompiledMapping,
    extensions: Vec<String>,
    language: String,
    inited: OnceLock<Result<InitState, ParseError>>,
}

/// The state produced when a lazy parser is initialized: a tree-sitter
/// [`tree_sitter::Language`] plus the parsed rules / derived lookup structures
/// needed to lower a concrete syntax tree to a UAST.
struct InitState {
    /// Rules with inheritance pre-merged and patterns pre-compiled once, so the
    /// lowering walk borrows them and never clones a `Rule` or compiles a
    /// pattern per node.
    resolved: Vec<ResolvedRule>,
    #[allow(dead_code)]
    lang_info: LanguageInfo,
    /// First-occurrence-wins rule index keyed by rule name (inheritance).
    rule_index: HashMap<String, usize>,
    /// Pattern-root-type → candidate rule indices in declaration order.
    rule_dispatch: HashMap<String, Vec<usize>>,
    /// The tree-sitter language, looked up via [`crate::languages::get_language`].
    /// `None` means no grammar is vendored for this language yet.
    ts_language: Option<tree_sitter::Language>,
    /// Compiled-pattern matcher bound to `ts_language`.
    /// `None` when no grammar is available.
    pattern_matcher: Option<PatternMatcher>,
    /// Language name.
    language: String,
}

impl LazyDslParser {
    fn new(pm: PrecompiledMapping) -> Self {
        let extensions = pm.extensions.clone();
        let language = pm.language.clone();
        Self {
            mapping: pm,
            extensions,
            language,
            inited: OnceLock::new(),
        }
    }

    /// Initializes the parser once.
    fn init(&self) -> &Result<InitState, ParseError> {
        self.inited.get_or_init(|| {
            // Obtain rules + language info: embedded languages convert their
            // static table (equality-gated against the DSL parser, so the
            // lowering sees identical inputs); custom mappings carry parsed
            // rules; raw DSL text is parsed as the last resort.
            let (rules, lang_info) = if let Some(table) = self.mapping.table {
                table.to_rules()
            } else if self.mapping.rules.is_empty() {
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

            // Build the O(1) rule lookup index (first occurrence wins — rule
            // order is observable).
            let mut rule_index: HashMap<String, usize> = HashMap::with_capacity(rules.len());
            for (i, r) in rules.iter().enumerate() {
                rule_index.entry(r.name.clone()).or_insert(i);
            }

            // Dispatch table keyed by each rule's PATTERN root node type
            // (fallback: rule name), candidates in declaration order — this
            // is what lets several conditioned rules share one node type
            // (see `Lowering::find_mapping_rule`).
            let mut rule_dispatch: HashMap<String, Vec<usize>> =
                HashMap::with_capacity(rules.len());
            for (i, r) in rules.iter().enumerate() {
                let key = crate::lowering::pattern_root_type(&r.pattern).unwrap_or(&r.name);
                rule_dispatch.entry(key.to_string()).or_default().push(i);
            }

            let ts_language = crate::languages::get_language(&self.mapping.language);
            let pattern_matcher = ts_language
                .as_ref()
                .map(|lang| PatternMatcher::new(lang.clone()));

            // Pre-resolve inheritance and pre-compile each rule's pattern once.
            // A pattern that fails to compile (or no matcher when no grammar is
            // vendored) stores `None`, reproducing the old lazy per-node
            // `compile_and_cache(..).ok()? -> None` fall-through exactly.
            let resolved = resolve_rules(&rules, &rule_index, pattern_matcher.as_ref());

            Ok(InitState {
                resolved,
                lang_info,
                rule_index,
                rule_dispatch,
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

        let Some(lang) = state.ts_language.as_ref() else {
            return Err(ParseError::Other(format!(
                "no tree-sitter grammar wired for language {} (grammar vendoring pending; see crate docs)",
                self.language
            )));
        };
        let pattern_matcher = state.pattern_matcher.as_ref().expect(
            "pattern_matcher is always Some when ts_language is Some (built together in init)",
        );

        // A fresh tree-sitter parser per call: pooling would be a perf-only
        // refinement and never affects output bytes (DESIGN).
        let mut ts_parser = tree_sitter::Parser::new();
        ts_parser
            .set_language(lang)
            .map_err(|e| ParseError::Other(format!("dsl parser: set language: {e}")))?;
        // Watchdog against pathological inputs: without a timeout a
        // degenerate parse hangs the process forever (SIGKILL-only). 30s is
        // orders of magnitude above any legitimate single-file parse; on
        // expiry `parse` returns None and the file is skipped like any other
        // parse failure.
        #[allow(deprecated)]
        ts_parser.set_timeout_micros(PARSE_TIMEOUT_MICROS);

        let tree = ts_parser
            .parse(content, None)
            .ok_or_else(|| ParseError::Other("dsl parser: failed to parse".to_string()))?;

        let lowering = Lowering::new(
            content,
            &state.resolved,
            &state.rule_dispatch,
            pattern_matcher,
            &state.language,
            false,
        );

        // `lower` returning `None` means the root collapsed (e.g. an empty
        // source file); surface a default (empty) node so callers always get a
        // non-null root (reference-implementation behavior).
        Ok(lowering.lower(&tree).unwrap_or_default())
    }

    fn language(&self) -> String {
        self.language.clone()
    }

    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
}

/// Returns the embedded precompiled mappings.
///
/// Derived once from [`cf_uast_mappings::ALL`] (the static registry) — no
/// DSL parsing and no embedded text; extensions come from the static tables.
/// The list is process-wide and built lazily.
fn embedded_mappings_data() -> &'static [PrecompiledMapping] {
    static DATA: OnceLock<Vec<PrecompiledMapping>> = OnceLock::new();
    DATA.get_or_init(|| {
        cf_uast_mappings::ALL
            .iter()
            .map(|&(stem, table)| PrecompiledMapping {
                // The registry stem is the embedded-map key (e.g. `c_sharp`),
                // which is what the loader registers parsers under; the
                // table's `name` field keeps the DSL header name (`csharp`).
                language: stem.to_string(),
                extensions: table.extensions.iter().map(|e| (*e).to_string()).collect(),
                rules: Vec::new(),
                uast: String::new(),
                table: Some(table),
            })
            .collect()
    })
}

/// Whether any embedded mappings are available.
///
/// # Examples
///
/// ```
/// // The embedded `.uastmap` table is compiled in, so this is always true.
/// assert!(cf_uast::embedded_mappings_available());
/// ```
#[must_use]
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
                h1 ^= u64::from(b);
                h1 = h1.wrapping_mul(1099511628211);
                h2 ^= u64::from(b);
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
        // The embedded set is the 68-language static registry.
        assert_eq!(
            embedded_mappings_data().len(),
            cf_uast_mappings::ALL.len(),
            "precompiled mapping count must match the static registry"
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
