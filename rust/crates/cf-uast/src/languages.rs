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

/// FFI entry points for the vendored grammar C sources compiled by `build.rs`.
///
/// Each `tree_sitter_<lang>()` returns the grammar's static [`tree_sitter::Language`]
/// table. `Language` is `#[repr(transparent)]` over the `*const TSLanguage`
/// pointer the C function returns, so declaring the extern with a `Language`
/// return type is ABI-correct; `ts_language_delete` is a no-op for native
/// (non-wasm) static languages, so the resulting `Drop` is safe.
#[allow(unsafe_code)]
mod ffi {
    extern "C" {
        pub fn tree_sitter_go() -> tree_sitter::Language;
        pub fn tree_sitter_html() -> tree_sitter::Language;
        pub fn tree_sitter_python() -> tree_sitter::Language;
        pub fn tree_sitter_c() -> tree_sitter::Language;
        pub fn tree_sitter_rust() -> tree_sitter::Language;
        pub fn tree_sitter_typescript() -> tree_sitter::Language;
        pub fn tree_sitter_tsx() -> tree_sitter::Language;
        pub fn tree_sitter_javascript() -> tree_sitter::Language;
        pub fn tree_sitter_json() -> tree_sitter::Language;
        pub fn tree_sitter_yaml() -> tree_sitter::Language;
        pub fn tree_sitter_cpp() -> tree_sitter::Language;
        pub fn tree_sitter_bash() -> tree_sitter::Language;
        pub fn tree_sitter_proto() -> tree_sitter::Language;
        pub fn tree_sitter_java() -> tree_sitter::Language;
        pub fn tree_sitter_cmake() -> tree_sitter::Language;
        pub fn tree_sitter_xml() -> tree_sitter::Language;
        pub fn tree_sitter_toml() -> tree_sitter::Language;
        pub fn tree_sitter_perl() -> tree_sitter::Language;
        pub fn tree_sitter_gitignore() -> tree_sitter::Language;
        pub fn tree_sitter_gitattributes() -> tree_sitter::Language;
        pub fn tree_sitter_markdown_inline() -> tree_sitter::Language;
    }
}

/// Returns the tree-sitter [`tree_sitter::Language`] for `name`, or [`None`].
///
/// Port of Go `GetLanguage`. The Go version caches the constructed language in a
/// `sync.Map`; here each call returns the grammar's static table via the
/// vendored FFI entry point, which is cheap (a pointer), so no cache is needed.
///
/// Languages whose grammar C sources are not yet vendored resolve to [`None`];
/// wiring one is a localized edit: vendor its `vendor/tree-sitter-<lang>/`
/// sources, list them in `build.rs`'s `GRAMMARS`, add an `ffi` extern, and a
/// match arm here.
pub fn get_language(name: &str) -> Option<tree_sitter::Language> {
    match name {
        // SAFETY: `tree_sitter_go` is the entry point of the vendored
        // go-sitter-forest go@v1.9.4 grammar (ABI 14), compiled into this crate
        // by `build.rs`. It returns the grammar's static `TSLanguage` table.
        #[allow(unsafe_code)]
        "go" => Some(unsafe { ffi::tree_sitter_go() }),
        // SAFETY: `tree_sitter_html` is the entry point of the vendored
        // go-sitter-forest html@v1.9.1 grammar, compiled into this crate by
        // `build.rs` (parser.c + scanner.c). Returns its static `TSLanguage`.
        #[allow(unsafe_code)]
        "html" => Some(unsafe { ffi::tree_sitter_html() }),
        // The compat-corpus grammars, vendored at the exact go-sitter-forest
        // revisions the Go build's go.mod pins. Each entry point returns the
        // grammar's static `TSLanguage` table compiled by `build.rs`.
        #[allow(unsafe_code)]
        "python" => Some(unsafe { ffi::tree_sitter_python() }),
        #[allow(unsafe_code)]
        "c" => Some(unsafe { ffi::tree_sitter_c() }),
        #[allow(unsafe_code)]
        "rust" => Some(unsafe { ffi::tree_sitter_rust() }),
        #[allow(unsafe_code)]
        "typescript" => Some(unsafe { ffi::tree_sitter_typescript() }),
        #[allow(unsafe_code)]
        "tsx" => Some(unsafe { ffi::tree_sitter_tsx() }),
        #[allow(unsafe_code)]
        "javascript" => Some(unsafe { ffi::tree_sitter_javascript() }),
        #[allow(unsafe_code)]
        "json" => Some(unsafe { ffi::tree_sitter_json() }),
        #[allow(unsafe_code)]
        "yaml" => Some(unsafe { ffi::tree_sitter_yaml() }),
        #[allow(unsafe_code)]
        "cpp" => Some(unsafe { ffi::tree_sitter_cpp() }),
        #[allow(unsafe_code)]
        "bash" => Some(unsafe { ffi::tree_sitter_bash() }),
        #[allow(unsafe_code)]
        "proto" => Some(unsafe { ffi::tree_sitter_proto() }),
        #[allow(unsafe_code)]
        "java" => Some(unsafe { ffi::tree_sitter_java() }),
        // cmake is the grammar `.cmake`/`CMakeLists.txt` resolves to (go-sitter-
        // forest cmake@v1.9.5, parser.c + scanner.c). Wiring it lets Rust parse
        // CMake files into a UAST with Function nodes for `function()`/`macro()`
        // definitions, matching Go's per-file report + function counts on repos
        // like ioq3.
        #[allow(unsafe_code)]
        "cmake" => Some(unsafe { ffi::tree_sitter_cmake() }),
        // Non-code corpus grammars Go links (go-sitter-forest): xml@v1.9.5,
        // toml@v1.9.2, perl@v1.9.9 (.pl), gitignore@v1.9.0, gitattributes@v1.9.1.
        // They produce function-free UASTs, but each parsed file is counted in the
        // static aggregators' reportCount divisor, so Rust must parse them too to
        // match Go's averaged metrics.
        #[allow(unsafe_code)]
        "xml" => Some(unsafe { ffi::tree_sitter_xml() }),
        #[allow(unsafe_code)]
        "toml" => Some(unsafe { ffi::tree_sitter_toml() }),
        #[allow(unsafe_code)]
        "perl" => Some(unsafe { ffi::tree_sitter_perl() }),
        #[allow(unsafe_code)]
        "gitignore" => Some(unsafe { ffi::tree_sitter_gitignore() }),
        #[allow(unsafe_code)]
        "gitattributes" => Some(unsafe { ffi::tree_sitter_gitattributes() }),
        // markdown_inline is the grammar `.md`/`.markdown` resolves to (Go's
        // loader registers it after the block `markdown` mapping, so it wins the
        // shared extensions). Wiring it lets Rust parse Markdown files into a
        // (function-free) UAST, matching Go's per-file report count.
        #[allow(unsafe_code)]
        "markdown_inline" => Some(unsafe { ffi::tree_sitter_markdown_inline() }),
        _ => None,
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
