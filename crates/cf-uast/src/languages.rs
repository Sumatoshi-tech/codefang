//! Language name → tree-sitter `Language` dispatch.
//!
//! Maps each supported language name to its vendored tree-sitter grammar's
//! entry point.
//!
//! # Grammar pinning
//!
//! Per DESIGN §5, node positions/types flow into machine output, so every
//! grammar is vendored at the exact `go-sitter-forest v1.9.x` revision the
//! reference build links (pinned by the differential gate). Grammars are
//! compiled from the vendored `parser.c`/`scanner.c` sources by `build.rs`.
//!
//! [`get_language`] recognizes the full supported-language set (so callers and
//! the loader behave correctly w.r.t. *which* languages exist) but returns
//! [`None`] for languages whose grammar is not vendored yet. Wiring a grammar
//! is a localized change: vendor the sources, list them in `build.rs`, add the
//! `extern` binding, and return `Some(lang)` here.

/// The complete set of supported language names.
///
/// This is the authoritative dispatch key set: 68 names, one per language in
/// the native `cf-uast-mappings` registry (the two sets are identical).
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
///
/// # Examples
///
/// ```
/// use cf_uast::languages::is_supported_language;
///
/// assert!(is_supported_language("go"));
/// assert!(is_supported_language("rust"));
/// assert!(!is_supported_language("cobol"));
/// ```
#[must_use]
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
        pub fn tree_sitter_powershell() -> tree_sitter::Language;
        pub fn tree_sitter_markdown_inline() -> tree_sitter::Language;
        pub fn tree_sitter_groovy() -> tree_sitter::Language;
        pub fn tree_sitter_properties() -> tree_sitter::Language;
        pub fn tree_sitter_ansible() -> tree_sitter::Language;
        pub fn tree_sitter_c_sharp() -> tree_sitter::Language;
        pub fn tree_sitter_clojure() -> tree_sitter::Language;
        pub fn tree_sitter_commonlisp() -> tree_sitter::Language;
        pub fn tree_sitter_crystal() -> tree_sitter::Language;
        pub fn tree_sitter_css() -> tree_sitter::Language;
        pub fn tree_sitter_csv() -> tree_sitter::Language;
        pub fn tree_sitter_dart() -> tree_sitter::Language;
        pub fn tree_sitter_dockerfile() -> tree_sitter::Language;
        // dotenv's C entry point is `tree_sitter_env` (the grammar's internal
        // name is `env`); the Go binding calls the same symbol.
        pub fn tree_sitter_env() -> tree_sitter::Language;
        pub fn tree_sitter_elixir() -> tree_sitter::Language;
        pub fn tree_sitter_elm() -> tree_sitter::Language;
        pub fn tree_sitter_fish() -> tree_sitter::Language;
        pub fn tree_sitter_fortran() -> tree_sitter::Language;
        pub fn tree_sitter_git_config() -> tree_sitter::Language;
        pub fn tree_sitter_gosum() -> tree_sitter::Language;
        pub fn tree_sitter_gotmpl() -> tree_sitter::Language;
        pub fn tree_sitter_gowork() -> tree_sitter::Language;
        pub fn tree_sitter_graphql() -> tree_sitter::Language;
        pub fn tree_sitter_haskell() -> tree_sitter::Language;
        pub fn tree_sitter_hcl() -> tree_sitter::Language;
        pub fn tree_sitter_helm() -> tree_sitter::Language;
        pub fn tree_sitter_ini() -> tree_sitter::Language;
        pub fn tree_sitter_kotlin() -> tree_sitter::Language;
        pub fn tree_sitter_latex() -> tree_sitter::Language;
        pub fn tree_sitter_lua() -> tree_sitter::Language;
        pub fn tree_sitter_make() -> tree_sitter::Language;
        pub fn tree_sitter_markdown() -> tree_sitter::Language;
        pub fn tree_sitter_nim() -> tree_sitter::Language;
        pub fn tree_sitter_nim_format_string() -> tree_sitter::Language;
        pub fn tree_sitter_php() -> tree_sitter::Language;
        pub fn tree_sitter_proxima() -> tree_sitter::Language;
        pub fn tree_sitter_prql() -> tree_sitter::Language;
        pub fn tree_sitter_psv() -> tree_sitter::Language;
        pub fn tree_sitter_r() -> tree_sitter::Language;
        pub fn tree_sitter_rego() -> tree_sitter::Language;
        pub fn tree_sitter_ruby() -> tree_sitter::Language;
        pub fn tree_sitter_rust_with_rstml() -> tree_sitter::Language;
        pub fn tree_sitter_scala() -> tree_sitter::Language;
        pub fn tree_sitter_sql() -> tree_sitter::Language;
        pub fn tree_sitter_ssh_config() -> tree_sitter::Language;
        pub fn tree_sitter_swift() -> tree_sitter::Language;
        pub fn tree_sitter_tcl() -> tree_sitter::Language;
        pub fn tree_sitter_zig() -> tree_sitter::Language;
    }
}

/// Returns the tree-sitter [`tree_sitter::Language`] for `name`, or [`None`].
///
/// Each call returns the grammar's static table via the vendored FFI entry
/// point, which is cheap (a pointer), so no cache is needed.
///
/// Languages whose grammar C sources are not yet vendored resolve to [`None`];
/// wiring one is a localized edit: vendor its `vendor/tree-sitter-<lang>/`
/// sources, list them in `build.rs`'s `GRAMMARS`, add an `ffi` extern, and a
/// match arm here.
#[must_use]
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
        // revisions the reference build pins. Each entry point returns the
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
        // forest cmake@v1.9.5, parser.c + scanner.c). It parses CMake files
        // into a UAST with Function nodes for `function()`/`macro()`
        // definitions, contributing per-file reports + function counts on
        // repos like ioq3.
        #[allow(unsafe_code)]
        "cmake" => Some(unsafe { ffi::tree_sitter_cmake() }),
        // Non-code corpus grammars (go-sitter-forest): xml@v1.9.5,
        // toml@v1.9.2, perl@v1.9.9 (.pl), gitignore@v1.9.0,
        // gitattributes@v1.9.1. They produce function-free UASTs, but each
        // parsed file is counted in the static aggregators' report-count
        // divisor, so they must be parsed for the averaged metrics to match
        // the reference reports.
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
        // PowerShell (.ps1/.psm1) — go-sitter-forest powershell@v1.9.5. Has
        // function definitions, so it contributes to the static aggregators'
        // function counts (e.g. kubernetes cluster/gce/windows).
        #[allow(unsafe_code)]
        "powershell" => Some(unsafe { ffi::tree_sitter_powershell() }),
        // markdown_inline is the grammar `.md`/`.markdown` resolves to (the
        // loader registers it after the block `markdown` mapping, so it wins
        // the shared extensions). It parses Markdown files into a
        // (function-free) UAST that contributes to the per-file report count.
        #[allow(unsafe_code)]
        "markdown_inline" => Some(unsafe { ffi::tree_sitter_markdown_inline() }),
        // Groovy (.groovy/.gradle, go-sitter-forest groovy@v1.9.4) and
        // java-properties (.properties, properties@v1.9.2): both carry
        // comment nodes the comments analyzer counts (Gradle build scripts,
        // gradle.properties), so they must parse for totals to match.
        #[allow(unsafe_code)]
        "groovy" => Some(unsafe { ffi::tree_sitter_groovy() }),
        #[allow(unsafe_code)]
        "properties" => Some(unsafe { ffi::tree_sitter_properties() }),
        // The remaining reference-linked grammars, wired identically: each
        // entry point returns the static `TSLanguage` of the vendored
        // go-sitter-forest grammar compiled by `build.rs`.
        #[allow(unsafe_code)]
        "ansible" => Some(unsafe { ffi::tree_sitter_ansible() }),
        #[allow(unsafe_code)]
        "c_sharp" => Some(unsafe { ffi::tree_sitter_c_sharp() }),
        #[allow(unsafe_code)]
        "clojure" => Some(unsafe { ffi::tree_sitter_clojure() }),
        #[allow(unsafe_code)]
        "commonlisp" => Some(unsafe { ffi::tree_sitter_commonlisp() }),
        #[allow(unsafe_code)]
        "crystal" => Some(unsafe { ffi::tree_sitter_crystal() }),
        #[allow(unsafe_code)]
        "css" => Some(unsafe { ffi::tree_sitter_css() }),
        #[allow(unsafe_code)]
        "csv" => Some(unsafe { ffi::tree_sitter_csv() }),
        #[allow(unsafe_code)]
        "dart" => Some(unsafe { ffi::tree_sitter_dart() }),
        #[allow(unsafe_code)]
        "dockerfile" => Some(unsafe { ffi::tree_sitter_dockerfile() }),
        #[allow(unsafe_code)]
        "dotenv" => Some(unsafe { ffi::tree_sitter_env() }),
        #[allow(unsafe_code)]
        "elixir" => Some(unsafe { ffi::tree_sitter_elixir() }),
        #[allow(unsafe_code)]
        "elm" => Some(unsafe { ffi::tree_sitter_elm() }),
        #[allow(unsafe_code)]
        "fish" => Some(unsafe { ffi::tree_sitter_fish() }),
        #[allow(unsafe_code)]
        "fortran" => Some(unsafe { ffi::tree_sitter_fortran() }),
        #[allow(unsafe_code)]
        "git_config" => Some(unsafe { ffi::tree_sitter_git_config() }),
        #[allow(unsafe_code)]
        "gosum" => Some(unsafe { ffi::tree_sitter_gosum() }),
        #[allow(unsafe_code)]
        "gotmpl" => Some(unsafe { ffi::tree_sitter_gotmpl() }),
        #[allow(unsafe_code)]
        "gowork" => Some(unsafe { ffi::tree_sitter_gowork() }),
        #[allow(unsafe_code)]
        "graphql" => Some(unsafe { ffi::tree_sitter_graphql() }),
        #[allow(unsafe_code)]
        "haskell" => Some(unsafe { ffi::tree_sitter_haskell() }),
        #[allow(unsafe_code)]
        "hcl" => Some(unsafe { ffi::tree_sitter_hcl() }),
        #[allow(unsafe_code)]
        "helm" => Some(unsafe { ffi::tree_sitter_helm() }),
        #[allow(unsafe_code)]
        "ini" => Some(unsafe { ffi::tree_sitter_ini() }),
        #[allow(unsafe_code)]
        "kotlin" => Some(unsafe { ffi::tree_sitter_kotlin() }),
        #[allow(unsafe_code)]
        "latex" => Some(unsafe { ffi::tree_sitter_latex() }),
        #[allow(unsafe_code)]
        "lua" => Some(unsafe { ffi::tree_sitter_lua() }),
        #[allow(unsafe_code)]
        "make" => Some(unsafe { ffi::tree_sitter_make() }),
        #[allow(unsafe_code)]
        "markdown" => Some(unsafe { ffi::tree_sitter_markdown() }),
        #[allow(unsafe_code)]
        "nim" => Some(unsafe { ffi::tree_sitter_nim() }),
        #[allow(unsafe_code)]
        "nim_format_string" => Some(unsafe { ffi::tree_sitter_nim_format_string() }),
        #[allow(unsafe_code)]
        "php" => Some(unsafe { ffi::tree_sitter_php() }),
        #[allow(unsafe_code)]
        "proxima" => Some(unsafe { ffi::tree_sitter_proxima() }),
        #[allow(unsafe_code)]
        "prql" => Some(unsafe { ffi::tree_sitter_prql() }),
        #[allow(unsafe_code)]
        "psv" => Some(unsafe { ffi::tree_sitter_psv() }),
        #[allow(unsafe_code)]
        "r" => Some(unsafe { ffi::tree_sitter_r() }),
        #[allow(unsafe_code)]
        "rego" => Some(unsafe { ffi::tree_sitter_rego() }),
        #[allow(unsafe_code)]
        "ruby" => Some(unsafe { ffi::tree_sitter_ruby() }),
        #[allow(unsafe_code)]
        "rust_with_rstml" => Some(unsafe { ffi::tree_sitter_rust_with_rstml() }),
        #[allow(unsafe_code)]
        "scala" => Some(unsafe { ffi::tree_sitter_scala() }),
        #[allow(unsafe_code)]
        "sql" => Some(unsafe { ffi::tree_sitter_sql() }),
        #[allow(unsafe_code)]
        "ssh_config" => Some(unsafe { ffi::tree_sitter_ssh_config() }),
        #[allow(unsafe_code)]
        "swift" => Some(unsafe { ffi::tree_sitter_swift() }),
        #[allow(unsafe_code)]
        "tcl" => Some(unsafe { ffi::tree_sitter_tcl() }),
        #[allow(unsafe_code)]
        "zig" => Some(unsafe { ffi::tree_sitter_zig() }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_set_has_68_entries() {
        // One dispatch key per embedded `.uastmap` data file.
        assert_eq!(SUPPORTED_LANGUAGES.len(), 68);
    }

    #[test]
    fn supported_set_matches_embedded_table() {
        // Every registry language must be a recognized dispatch key.
        for lang in cf_uast_mappings::supported_languages() {
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
