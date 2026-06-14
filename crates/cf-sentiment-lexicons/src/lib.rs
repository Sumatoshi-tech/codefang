//! Embedded multilingual sentiment lexicon for code-comment sentiment analysis.
//!
//! Rust port of the Go package
//! `internal/analyzers/sentiment/lexicons`. It is a tier-0 data crate (see
//! `specs/rust-rewrite/DESIGN.md` §1) used only by the sentiment analyzer
//! (`cf-sentiment`). It exposes lookup tables only and performs **no** machine
//! report serialization, so it does not depend on the `cf-gojson` / `cf-goyaml`
//! go-compat encoders; the consumer is responsible for routing report bytes
//! through them (DESIGN §2.6).
//!
//! # Data provenance
//!
//! Lexicon data is sourced from Chen & Skiena (2014) "Building Sentiment
//! Lexicons for All Major Languages" (ACL 2014,
//! <https://aclanthology.org/P14-2063/>). The dataset covers 136 languages; we
//! embed the 32 most common in software projects. Each entry maps a word to a
//! valence score on the VADER scale: `+1.5` for positive words, `-1.5` for
//! negative words.
//!
//! The data is vendored verbatim from the Go generated artifact
//! (`lexicon_data.gen.go`) into `data/lexicon_data.tsv` and regenerated into
//! Rust by `build.rs` at compile time, preserving both per-language grouping and
//! intra-language ordering so [`all`] and [`for_language`] return slices
//! byte-identical to the Go implementation. This data-parity is what keeps the
//! downstream VADER/govader sentiment scores byte-identical (DESIGN §2.6).
//!
//! # Example
//!
//! ```
//! // Every supported language is present.
//! assert!(cf_sentiment_lexicons::language_count() >= 30);
//!
//! // Look up a single language by ISO 639-1 code.
//! let russian = cf_sentiment_lexicons::for_language(cf_sentiment_lexicons::LANG_RUSSIAN).unwrap();
//! assert!(russian.len() >= 2000);
//!
//! // Unknown languages return `None`.
//! assert!(cf_sentiment_lexicons::for_language("xx").is_none());
//! ```

#![forbid(unsafe_code)]

/// A single lexicon entry: a word and its valence on the VADER scale `[-4, +4]`.
///
/// In the embedded dataset every valence is exactly `+1.5` (positive) or `-1.5`
/// (negative), mirroring the Go `Entry` struct.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// The lexicon word (lower-cased, as produced by the source generator).
    pub word: &'static str,
    /// Valence score on the VADER scale; `+1.5` positive, `-1.5` negative.
    pub valence: f64,
}

// Pull in the build-time-generated per-language tables and `REGISTRY`. The
// generated module references the `Entry` type from this scope.
include!(concat!(env!("OUT_DIR"), "/lexicon_data.rs"));

/// A supported lexicon language, identified by its ISO 639-1 code.
///
/// This mirrors the Go `Language` string type. Use the `LANG_*` constants for
/// the supported set.
pub type Language = &'static str;

/// Arabic (`ar`).
pub const LANG_ARABIC: Language = "ar";
/// Bulgarian (`bg`).
pub const LANG_BULGARIAN: Language = "bg";
/// Chinese (`zh`).
pub const LANG_CHINESE: Language = "zh";
/// Croatian (`hr`).
pub const LANG_CROATIAN: Language = "hr";
/// Czech (`cs`).
pub const LANG_CZECH: Language = "cs";
/// Danish (`da`).
pub const LANG_DANISH: Language = "da";
/// Dutch (`nl`).
pub const LANG_DUTCH: Language = "nl";
/// Finnish (`fi`).
pub const LANG_FINNISH: Language = "fi";
/// French (`fr`).
pub const LANG_FRENCH: Language = "fr";
/// German (`de`).
pub const LANG_GERMAN: Language = "de";
/// Greek (`el`).
pub const LANG_GREEK: Language = "el";
/// Hebrew (`he`).
pub const LANG_HEBREW: Language = "he";
/// Hindi (`hi`).
pub const LANG_HINDI: Language = "hi";
/// Hungarian (`hu`).
pub const LANG_HUNGARIAN: Language = "hu";
/// Indonesian (`id`).
pub const LANG_INDONESIAN: Language = "id";
/// Italian (`it`).
pub const LANG_ITALIAN: Language = "it";
/// Japanese (`ja`).
pub const LANG_JAPANESE: Language = "ja";
/// Korean (`ko`).
pub const LANG_KOREAN: Language = "ko";
/// Malay (`ms`).
pub const LANG_MALAY: Language = "ms";
/// Norwegian (`no`).
pub const LANG_NORWEGIAN: Language = "no";
/// Persian (`fa`).
pub const LANG_PERSIAN: Language = "fa";
/// Polish (`pl`).
pub const LANG_POLISH: Language = "pl";
/// Portuguese (`pt`).
pub const LANG_PORTUGUESE: Language = "pt";
/// Romanian (`ro`).
pub const LANG_ROMANIAN: Language = "ro";
/// Russian (`ru`).
pub const LANG_RUSSIAN: Language = "ru";
/// Slovak (`sk`).
pub const LANG_SLOVAK: Language = "sk";
/// Spanish (`es`).
pub const LANG_SPANISH: Language = "es";
/// Swedish (`sv`).
pub const LANG_SWEDISH: Language = "sv";
/// Thai (`th`).
pub const LANG_THAI: Language = "th";
/// Turkish (`tr`).
pub const LANG_TURKISH: Language = "tr";
/// Ukrainian (`uk`).
pub const LANG_UKRAINIAN: Language = "uk";
/// Vietnamese (`vi`).
pub const LANG_VIETNAMESE: Language = "vi";

/// Returns the ISO 639-1 codes of all supported lexicon languages.
///
/// The order is deterministic (ISO-code byte order). Port of the Go
/// test-helper `AllLanguages`.
pub fn all_languages() -> Vec<Language> {
    REGISTRY.iter().map(|(code, _, _)| *code).collect()
}

/// Returns the number of supported lexicon languages.
///
/// Port of the Go test-helper `LanguageCount`.
pub fn language_count() -> usize {
    REGISTRY.len()
}

/// Returns the English display name for a language code.
///
/// For an unknown code the code itself is returned, exactly as the Go
/// `LanguageName` helper does.
///
/// ```
/// use cf_sentiment_lexicons::{language_name, LANG_RUSSIAN};
///
/// assert_eq!(language_name(LANG_RUSSIAN), "Russian");
/// // Unknown codes echo back the input.
/// assert_eq!(language_name("xx"), "xx");
/// ```
pub fn language_name(lang: Language) -> String {
    REGISTRY
        .iter()
        .find(|(code, _, _)| *code == lang)
        .map(|(_, name, _)| (*name).to_string())
        .unwrap_or_else(|| lang.to_string())
}

/// Returns the lexicon entries for a single language, or `None` for an
/// unsupported code.
///
/// Port of the Go `ForLanguage`. The returned slice preserves the source
/// ordering of the embedded data.
pub fn for_language(lang: Language) -> Option<&'static [Entry]> {
    REGISTRY
        .iter()
        .find(|(code, _, _)| *code == lang)
        .map(|(_, _, entries)| *entries)
}

/// Returns combined lexicon entries from all supported languages.
///
/// Port of the Go `All`. Entries are concatenated in registry (ISO-code) order;
/// each language's internal ordering is preserved.
///
/// ```
/// use cf_sentiment_lexicons::{all, entry_count};
///
/// // The flattened view holds exactly `entry_count()` entries...
/// assert_eq!(all().len(), entry_count());
/// // ...and every embedded valence is +1.5 or -1.5.
/// assert!(all().iter().all(|e| e.valence == 1.5 || e.valence == -1.5));
/// ```
pub fn all() -> Vec<Entry> {
    let mut out = Vec::with_capacity(entry_count());
    for (_, _, entries) in REGISTRY {
        out.extend_from_slice(entries);
    }
    out
}

/// Returns the total number of lexicon entries across all languages.
///
/// Port of the Go `EntryCount`.
pub fn entry_count() -> usize {
    REGISTRY.iter().map(|(_, _, entries)| entries.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from lexicons_test.go::TestAllLanguages.
    #[test]
    fn all_languages_has_at_least_thirty() {
        assert!(all_languages().len() >= 30);
    }

    // Ported from lexicons_test.go::TestLanguageCount.
    #[test]
    fn language_count_at_least_thirty() {
        assert!(language_count() >= 30);
    }

    // Ported from lexicons_test.go::TestEntryCount.
    #[test]
    fn entry_count_over_fifty_thousand() {
        assert!(entry_count() > 50_000, "entry_count = {}", entry_count());
    }

    // Ported from lexicons_test.go::TestForLanguage_Supported.
    #[test]
    fn for_language_supported_minimums() {
        let cases: &[(Language, usize)] = &[
            (LANG_RUSSIAN, 2000),
            (LANG_CHINESE, 1000),
            (LANG_JAPANESE, 500),
            (LANG_KOREAN, 1500),
            (LANG_SPANISH, 3000),
            (LANG_FRENCH, 3000),
            (LANG_GERMAN, 3000),
            (LANG_PORTUGUESE, 3000),
        ];
        for (lang, min_size) in cases {
            let entries = for_language(lang).expect("supported language must be present");
            assert!(
                entries.len() >= *min_size,
                "{lang} should have at least {min_size} entries, has {}",
                entries.len()
            );
        }
    }

    // Ported from lexicons_test.go::TestForLanguage_Unsupported.
    #[test]
    fn for_language_unsupported_is_none() {
        assert!(for_language("xx").is_none());
    }

    // Ported from lexicons_test.go::TestLanguageName.
    #[test]
    fn language_name_lookup() {
        assert_eq!(language_name(LANG_RUSSIAN), "Russian");
        assert_eq!(language_name(LANG_CHINESE), "Chinese");
        assert_eq!(language_name("xx"), "xx");
    }

    // Ported from lexicons_test.go::TestAll.
    #[test]
    fn all_over_fifty_thousand() {
        assert!(all().len() > 50_000);
    }

    // Ported from lexicons_test.go::TestEntryValence.
    #[test]
    fn entry_valence_is_plus_or_minus_one_point_five() {
        for entry in all() {
            assert!(!entry.word.is_empty());
            assert!(
                entry.valence == 1.5 || entry.valence == -1.5,
                "entry {:?} has unexpected valence {}",
                entry.word,
                entry.valence
            );
        }
    }

    // Ported from lexicons_test.go::TestLanguagesHavePositiveAndNegative.
    #[test]
    fn languages_have_positive_and_negative() {
        for lang in all_languages() {
            let entries = for_language(lang).expect("registered language must be present");
            let mut pos = 0usize;
            let mut neg = 0usize;
            for e in entries {
                if e.valence > 0.0 {
                    pos += 1;
                } else {
                    neg += 1;
                }
            }
            assert!(pos > 0, "{lang} has no positive entries");
            assert!(neg > 0, "{lang} has no negative entries");
        }
    }

    // Extra invariant: registry codes are unique and sorted (deterministic order).
    #[test]
    fn registry_codes_unique_and_sorted() {
        let codes = all_languages();
        for w in codes.windows(2) {
            assert!(
                w[0].as_bytes() < w[1].as_bytes(),
                "registry not strictly sorted: {:?} !< {:?}",
                w[0],
                w[1]
            );
        }
    }
}
