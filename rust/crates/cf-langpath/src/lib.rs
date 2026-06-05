//! `cf-langpath` — language token → file-path matchers (enry data inversion).
//!
//! Rust port of the Go package
//! `internal/analyzers/plumbing/langpath`. It converts user-supplied language
//! tokens (e.g. `"go"`, `"Python"`, `"dockerfile"`) into a deterministic,
//! sorted, deduplicated set of pathspec globs (`"*.go"`, `"Dockerfile"`, …),
//! backed by [`src-d/enry`](https://github.com/src-d/enry)'s Linguist data.
//!
//! ## Parity
//!
//! Per DESIGN §3.7 / port rule (7), enry language classification is
//! decision-parity-critical: the glob set selects which files an analysis
//! includes, which in turn selects which bytes appear in machine reports. We
//! therefore vendor the **same** three Linguist data tables that the Go
//! `codefang` binary links (`github.com/src-d/enry/v2@v2.1.0`), as a verbatim
//! TSV snapshot in `data/enry-v2.1.0.tsv` (see `data/README.md`), and reproduce
//! enry's lookup functions exactly:
//!
//! - [`enry::GetLanguageByAlias`] — normalizes the token via
//!   `convertToAliasKey` (substring before the first `,`, spaces → `_`,
//!   lowercase) and looks it up in `LanguageByAliasMap`.
//! - [`enry::GetLanguageExtensions`] — reads `ExtensionsByLanguage[lang]`.
//! - The inversion of `LanguagesByFilename` (filename → langs) into
//!   `lang → []filename`, built once at first use.
//!
//! [`enry::GetLanguageByAlias`]: https://pkg.go.dev/github.com/src-d/enry/v2#GetLanguageByAlias
//! [`enry::GetLanguageExtensions`]: https://pkg.go.dev/github.com/src-d/enry/v2#GetLanguageExtensions

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

/// Error returned when a user-supplied token does not resolve to any Linguist
/// language (including its aliases).
///
/// Mirrors the Go sentinel `langpath.ErrUnknownLanguage`. The
/// [`Display`](std::fmt::Display) form reproduces Go's wrapped message
/// `unknown language: "<raw>"` (the raw token is quoted with Go `%q`-style
/// double quotes), so callers that surface the error text stay byte-compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLanguage {
    /// The original, untrimmed token the caller supplied (Go formats the raw
    /// `%q` value, not the trimmed one).
    pub raw: String,
}

impl std::fmt::Display for UnknownLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Go: fmt.Errorf("%w: %q", ErrUnknownLanguage, raw)
        // ErrUnknownLanguage.Error() == "unknown language"; %q double-quotes.
        write!(f, "unknown language: {}", go_quote(&self.raw))
    }
}

impl std::error::Error for UnknownLanguage {}

/// The sentinel token meaning "do not restrict by language".
const ALL_TOKEN: &str = "all";
/// Prepended to every extension-derived glob.
const EXTENSION_GLOB_PREFIX: &str = "*";

/// Parsed, read-only view of the vendored enry tables.
struct EnryData {
    /// `alias_key` → canonical Linguist name (`LanguageByAliasMap`).
    alias_to_lang: HashMap<String, String>,
    /// canonical name → extensions, with leading dot (`ExtensionsByLanguage`).
    extensions_by_language: HashMap<String, Vec<String>>,
    /// canonical name → literal filenames, the inversion of
    /// `LanguagesByFilename`.
    filenames_by_language: HashMap<String, Vec<String>>,
}

/// Vendored enry v2.1.0 tables, verbatim. See `data/README.md`.
const ENRY_TSV: &str = include_str!("../data/enry-v2.1.0.tsv");

/// Loads and caches the parsed enry tables. Parsed once on first access;
/// read-only thereafter (mirrors the Go package-load-time inversion).
fn enry_data() -> &'static EnryData {
    static DATA: OnceLock<EnryData> = OnceLock::new();
    DATA.get_or_init(|| parse_enry_tsv(ENRY_TSV))
}

/// Parses the vendored TSV into the three lookup tables.
///
/// Record formats (one per line, tab-separated):
/// - `A<TAB>alias_key<TAB>canonical`
/// - `E<TAB>canonical<TAB>ext1<TAB>ext2…`
/// - `F<TAB>filename<TAB>lang1<TAB>lang2…` (inverted into lang → filename)
fn parse_enry_tsv(tsv: &str) -> EnryData {
    let mut alias_to_lang = HashMap::new();
    let mut extensions_by_language = HashMap::new();
    let mut filenames_by_language: HashMap<String, Vec<String>> = HashMap::new();

    for line in tsv.lines() {
        if line.is_empty() {
            continue;
        }
        let mut cols = line.split('\t');
        let tag = cols.next().unwrap_or("");
        match tag {
            "A" => {
                if let (Some(key), Some(lang)) = (cols.next(), cols.next()) {
                    alias_to_lang.insert(key.to_string(), lang.to_string());
                }
            }
            "E" => {
                if let Some(lang) = cols.next() {
                    let exts: Vec<String> = cols.map(str::to_string).collect();
                    extensions_by_language.insert(lang.to_string(), exts);
                }
            }
            "F" => {
                if let Some(filename) = cols.next() {
                    // Invert: each lang in this filename's list gains `filename`.
                    // Preserve enry's per-language filename ordering by appending
                    // in the order filenames appear in the (sorted) TSV.
                    for lang in cols {
                        filenames_by_language
                            .entry(lang.to_string())
                            .or_default()
                            .push(filename.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    EnryData {
        alias_to_lang,
        extensions_by_language,
        filenames_by_language,
    }
}

/// Reproduces enry's `convertToAliasKey`: take the substring before the first
/// comma, replace ASCII spaces with underscores, then lowercase.
///
/// Matches `data.convertToAliasKey` byte-for-byte:
/// `strings.SplitN(s, ",", 2)[0]`, `strings.Replace(_, " ", "_", -1)`,
/// `strings.ToLower`.
fn convert_to_alias_key(lang_name: &str) -> String {
    // SplitN(",", 2)[0]: everything up to (not including) the first comma.
    let before_comma = match lang_name.find(',') {
        Some(idx) => &lang_name[..idx],
        None => lang_name,
    };
    // Replace ASCII space with underscore (Go replaces the byte ' ' only).
    let underscored = before_comma.replace(' ', "_");
    // Go strings.ToLower is full-Unicode lowercasing.
    underscored.to_lowercase()
}

/// Reproduces `enry.GetLanguageByAlias`: returns the canonical language for a
/// token, or `None` when unrecognized.
fn get_language_by_alias(token: &str) -> Option<&'static str> {
    let key = convert_to_alias_key(token);
    enry_data()
        .alias_to_lang
        .get(&key)
        .map(String::as_str)
}

/// Reproduces `enry.GetLanguageExtensions`: extensions (with leading dot) for a
/// canonical language, or an empty slice when none are registered.
fn get_language_extensions(language: &str) -> &'static [String] {
    enry_data()
        .extensions_by_language
        .get(language)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Result of [`globs`]: the sorted/deduplicated glob set and the `wants_all`
/// flag.
///
/// This is the Rust shape of the Go `(globs []string, wantsAll bool, err error)`
/// triple. When `wants_all` is `true`, `globs` is empty and callers should skip
/// pathspec push-down entirely.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Globs {
    /// Sorted, deduplicated pathspec globs (`"*.go"`, `"Dockerfile"`, …). Empty
    /// when `wants_all` is `true`.
    pub globs: Vec<String>,
    /// `true` when the caller did not restrict languages — empty input or the
    /// literal `"all"` token. Callers should skip pathspec push-down.
    pub wants_all: bool,
}

/// Converts a list of user-supplied language tokens into a sorted, deduplicated
/// set of pathspec globs.
///
/// Mirrors the Go `langpath.Globs`:
/// - Empty input → `wants_all = true`, empty globs, no error.
/// - Any token that case-insensitively equals `"all"` (after trimming) →
///   `wants_all = true`, empty globs, no error.
/// - Each remaining token is trimmed, normalized via enry's alias key, and
///   resolved to a canonical language. The language's extensions become
///   `"*<ext>"` globs and its literal filenames become bare-filename globs.
/// - An unrecognized token returns [`UnknownLanguage`] carrying the original
///   (untrimmed) token, matching Go's `%q` of the raw value.
///
/// The returned `globs` are sorted by raw byte order (Rust `BTreeSet<String>`
/// orders by `[u8]` lexicographically, equivalent to Go `slices.Sort` over
/// strings) and deduplicated. A fresh `Vec` is returned per call (callers may
/// mutate it freely).
///
/// # Examples
///
/// ```
/// use cf_langpath::globs;
///
/// let r = globs(&["go"]).unwrap();
/// assert!(!r.wants_all);
/// assert_eq!(r.globs, vec!["*.go".to_string()]);
///
/// let all = globs(&["all"]).unwrap();
/// assert!(all.wants_all);
/// assert!(all.globs.is_empty());
///
/// assert!(globs(&["notalang"]).is_err());
/// ```
///
/// # Errors
///
/// Returns [`UnknownLanguage`] when any token does not resolve to a Linguist
/// language (including its aliases).
pub fn globs<S: AsRef<str>>(langs: &[S]) -> Result<Globs, UnknownLanguage> {
    if langs.is_empty() {
        return Ok(Globs {
            globs: Vec::new(),
            wants_all: true,
        });
    }

    // BTreeSet<String> orders by raw byte (`[u8]`) comparison, matching Go's
    // `slices.Sort` over UTF-8 strings, and deduplicates.
    let mut set: BTreeSet<String> = BTreeSet::new();

    let data = enry_data();

    for raw in langs {
        let raw = raw.as_ref();
        let token = raw.trim();
        if token.eq_ignore_ascii_case(ALL_TOKEN) {
            return Ok(Globs {
                globs: Vec::new(),
                wants_all: true,
            });
        }

        let canonical = match get_language_by_alias(token) {
            Some(lang) => lang,
            None => {
                return Err(UnknownLanguage {
                    raw: raw.to_string(),
                })
            }
        };

        for ext in get_language_extensions(canonical) {
            set.insert(format!("{EXTENSION_GLOB_PREFIX}{ext}"));
        }

        if let Some(names) = data.filenames_by_language.get(canonical) {
            for name in names {
                set.insert(name.clone());
            }
        }
    }

    Ok(Globs {
        globs: set.into_iter().collect(),
        wants_all: false,
    })
}

/// Quotes a string the way Go's `fmt` `%q` verb does for the error message:
/// double quotes with Go-style escaping of the common control/quote characters.
///
/// langpath only ever feeds user tokens here; the escape set covers what those
/// tokens can realistically contain while staying compatible with Go's `%q`.
fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper mirroring the Go test's `mapset` dedup check.
    fn is_unique(xs: &[String]) -> bool {
        let set: BTreeSet<&String> = xs.iter().collect();
        set.len() == xs.len()
    }

    #[test]
    fn all_token_yields_wants_all() {
        // Go: TestGlobs_AllToken_YieldsWantsAll
        let r = globs(&["all"]).unwrap();
        assert!(r.wants_all, "all token must set wants_all");
        assert!(r.globs.is_empty(), "wants_all must return empty globs");
    }

    #[test]
    fn returns_fresh_slice_per_call() {
        // Go: TestGlobs_ReturnsFreshSlicePerCall
        let mut a = globs(&["go"]).unwrap();
        assert!(!a.globs.is_empty());
        let b = globs(&["go"]).unwrap();
        assert!(!b.globs.is_empty());

        a.globs[0] = "tampered".to_string();
        assert_ne!(
            a.globs[0], b.globs[0],
            "mutating one call's result must not affect a subsequent call's result"
        );
    }

    #[test]
    fn dockerfile_includes_basename_glob() {
        // Go: TestGlobs_Dockerfile_IncludesBasenameGlob
        let r = globs(&["dockerfile"]).unwrap();
        assert!(!r.wants_all);
        assert!(
            r.globs.contains(&"Dockerfile".to_string()),
            "filename-only languages must emit a literal-filename glob; got {:?}",
            r.globs
        );
    }

    #[test]
    fn multiple_languages_sorted_and_deduplicated() {
        // Go: TestGlobs_MultipleLanguages_SortedAndDeduplicated
        let r = globs(&["python", "go", "python"]).unwrap();
        assert!(!r.wants_all);
        assert!(!r.globs.is_empty());
        assert!(r.globs.windows(2).all(|w| w[0] <= w[1]), "globs must be sorted");
        assert!(
            r.globs.contains(&"*.go".to_string()),
            "go extension must be present"
        );
        assert!(
            r.globs.contains(&"*.py".to_string()),
            "python extension must be present"
        );
        assert!(is_unique(&r.globs), "globs must be deduplicated");
    }

    #[test]
    fn unknown_token_returns_err_unknown_language() {
        // Go: TestGlobs_UnknownToken_ReturnsErrUnknownLanguage
        for input in [
            vec!["notalang"],
            vec!["go", "notalang"],
            vec!["notalang", "go"],
        ] {
            let err = globs(&input).unwrap_err();
            assert!(
                err.to_string().contains("notalang"),
                "error must mention the raw token, got {err}"
            );
            assert!(
                err.to_string().contains("unknown language"),
                "error must wrap unknown language sentinel, got {err}"
            );
        }
    }

    #[test]
    fn go_token_yields_star_dot_go() {
        // Go: TestGlobs_GoToken_YieldsStarDotGo
        for input in ["go", "Go", "GO", "  go  ", "golang"] {
            let r = globs(&[input]).unwrap();
            assert!(!r.wants_all, "input {input:?} must not set wants_all");
            assert_eq!(
                r.globs,
                vec!["*.go".to_string()],
                "input {input:?} must yield exactly [\"*.go\"]"
            );
        }
    }

    #[test]
    fn empty_input_yields_wants_all() {
        // Go: TestGlobs_EmptyInput_YieldsWantsAll
        let empty: [&str; 0] = [];
        let r = globs(&empty).unwrap();
        assert!(r.wants_all);
        assert!(r.globs.is_empty());
    }

    #[test]
    fn convert_to_alias_key_matches_enry() {
        // enry convertToAliasKey: before first comma, space→underscore, lower.
        assert_eq!(convert_to_alias_key("Go"), "go");
        assert_eq!(convert_to_alias_key("  go  "), "__go__"); // no trimming here
        assert_eq!(convert_to_alias_key("Visual Basic"), "visual_basic");
        assert_eq!(convert_to_alias_key("Foo, Bar"), "foo");
        assert_eq!(convert_to_alias_key("F#"), "f#");
    }

    #[test]
    fn vendored_tables_loaded() {
        // Pins the cardinalities documented in data/README.md so an accidental
        // data swap is caught. enry v2.1.0: 750 aliases, 504 extension-langs,
        // 234 filename records.
        let d = enry_data();
        assert_eq!(d.alias_to_lang.len(), 750, "alias count");
        assert_eq!(d.extensions_by_language.len(), 504, "extension-language count");
        // 234 F-records invert into a (smaller) set of distinct languages; just
        // assert it is non-empty and that a known mapping survived inversion.
        assert!(!d.filenames_by_language.is_empty());
        assert!(d
            .filenames_by_language
            .get("Dockerfile")
            .map(|v| v.contains(&"Dockerfile".to_string()))
            .unwrap_or(false));
    }

    #[test]
    fn unknown_language_quotes_raw_token() {
        // Go formats the *raw* (untrimmed) token with %q.
        let err = globs(&["  notalang  "]).unwrap_err();
        assert_eq!(err.to_string(), "unknown language: \"  notalang  \"");
    }
}
