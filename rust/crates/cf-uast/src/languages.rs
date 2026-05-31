//! Language name → tree-sitter `Language` dispatch.
//!
//! Port of Go `pkg/uast/languages.go`, which maps each supported language name
//! to its `go-sitter-forest` `GetLanguage` function and caches the resulting
//! `*sitter.Language`. In Rust the equivalent is a per-language grammar crate
//! (`tree-sitter-<lang>`) whose `LANGUAGE`/`language()` returns a
//! [`tree_sitter::Language`].
//!
//! # Grammar integration is centralized (and pending)
//!
//! Per DESIGN §5, node positions/types flow into machine output, so every
//! grammar must be pinned to the exact `go-sitter-forest v1.9.x` commit the Go
//! build vendors. The per-language grammar crates (and, for grammars without a
//! Rust crate, vendored `parser.c`/`scanner.c` compiled via `cc`) are integrated
//! centrally in the workspace and surfaced to this crate behind Cargo features.
//!
//! Until that central integration lands, [`get_language`] recognizes the full
//! supported-language set (so callers and the loader behave correctly w.r.t.
//! *which* languages exist) but returns [`None`] for the actual
//! [`tree_sitter::Language`]. Wiring a grammar is then a localized change here:
//! add the feature + `extern` binding and return `Some(lang)`.

/// The complete set of language names with embedded `.uastmap` mappings, in the
/// same order as Go's `languageFuncs` map literal (`languages.go`).
///
/// This is the authoritative dispatch key set: the 68 keys of Go's
/// `languageFuncs` (`languages.go`), one per embedded `.uastmap` data file in
/// `cf-uast-uastmaps` (the two sets are identical).
pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "ansible",
    "bash",
    "c",
    "c_sharp",
    "clojure",
    "cmake",
    "commonlisp",
    "cpp",
    "crystal",
    "css",
    "csv",
    "dart",
    "dockerfile",
    "dotenv",
    "elixir",
    "elm",
    "fish",
    "fortran",
    "git_config",
    "gitattributes",
    "gitignore",
    "go",
    "gosum",
    "gotmpl",
    "gowork",
    "graphql",
    "groovy",
    "haskell",
    "hcl",
    "helm",
    "html",
    "ini",
    "java",
    "javascript",
    "json",
    "kotlin",
    "latex",
    "lua",
    "make",
    "markdown",
    "markdown_inline",
    "nim",
    "nim_format_string",
    "perl",
    "php",
    "powershell",
    "properties",
    "proto",
    "proxima",
    "prql",
    "psv",
    "python",
    "r",
    "rego",
    "ruby",
    "rust",
    "rust_with_rstml",
    "scala",
    "sql",
    "ssh_config",
    "swift",
    "tcl",
    "toml",
    "tsx",
    "typescript",
    "xml",
    "yaml",
    "zig",
];

/// Returns whether `name` is a supported language.
pub fn is_supported_language(name: &str) -> bool {
    SUPPORTED_LANGUAGES.contains(&name)
}

/// Returns the tree-sitter [`tree_sitter::Language`] for `name`, or [`None`].
///
/// Port of Go `GetLanguage`. The Go version caches the constructed language in a
/// `sync.Map`; the Rust grammar crates return a cheap `Language` handle, so no
/// cache is needed here.
///
/// Currently returns [`None`] for every recognized language pending central
/// grammar-crate integration (see the module docs and crate todos). Wiring a
/// language is a localized edit: gate a `tree-sitter-<lang>` dependency behind a
/// feature and return its `Language` for the matching name.
pub fn get_language(name: &str) -> Option<tree_sitter::Language> {
    // The match arms are intentionally exhaustive over SUPPORTED_LANGUAGES so a
    // wiring change is a one-line edit per language. Example of the eventual
    // shape (behind a feature):
    //
    //   #[cfg(feature = "lang-go")]
    //   "go" => Some(tree_sitter_go::LANGUAGE.into()),
    //
    // Until grammars are integrated, all recognized languages resolve to None.
    if is_supported_language(name) {
        None
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_set_has_68_entries() {
        // Go's `languageFuncs` map literal has exactly 68 keys, one per embedded
        // `.uastmap` data file.
        assert_eq!(SUPPORTED_LANGUAGES.len(), 68);
    }

    #[test]
    fn supported_set_matches_embedded_table() {
        // Every embedded `.uastmap` language must be a recognized dispatch key.
        for lang in cf_uast_uastmaps::supported_languages() {
            assert!(
                is_supported_language(lang),
                "embedded language {lang} missing from dispatch set"
            );
        }
    }

    #[test]
    fn common_languages_recognized() {
        for lang in ["go", "rust", "python", "java", "javascript", "typescript"] {
            assert!(is_supported_language(lang), "{lang} should be supported");
        }
    }

    #[test]
    fn unknown_language_not_supported() {
        assert!(!is_supported_language("cobol"));
        assert!(get_language("cobol").is_none());
    }
}
