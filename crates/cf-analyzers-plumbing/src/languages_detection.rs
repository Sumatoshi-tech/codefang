//! `LanguagesDetection` provider.
//!
//! Produces a map from blob [`struct@Hash`] to detected language name.
//! Detection runs in a frozen order (language names flow into report output):
//! 1. a binary guard (binary blob -> `""`);
//! 2. a fast-path extension table ([`language_by_extension`]);
//! 3. a content-analysis fallback for unmatched extensions.
//!
//! The fallback must match the enry classifier byte-for-byte (DESIGN §2.6
//! mandates carrying enry's own data tables rather than swapping detectors);
//! it is injected via the [`EnryClassifier`] trait.

use std::collections::HashMap;

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::blob_cache::{is_binary, CachedBlob};
use crate::git_model::{Action, Changes, Hash};

/// Content-analysis language classifier boundary.
///
/// Takes the base file name and the file bytes and returns the language name,
/// or an empty string when undetermined. For byte-identity the implementation
/// must carry the enry data tables (DESIGN §2.6); it is injected.
pub trait EnryClassifier {
    /// Detect the language for a file given its base name and content.
    fn get_language(&self, base_name: &str, content: &[u8]) -> String;
}

/// `LanguagesDetection` provider.
pub struct LanguagesDetection<C: EnryClassifier> {
    classifier: C,
}

impl<C: EnryClassifier> LanguagesDetection<C> {
    /// Construct with the given content-analysis classifier.
    pub const fn new(classifier: C) -> Self {
        Self { classifier }
    }

    /// Detect the language for a single blob.
    ///
    /// Returns `""` for a binary blob, then tries the extension fast path,
    /// then falls back to content analysis.
    pub fn detect_language(&self, name: &str, blob: Option<&CachedBlob>) -> String {
        let Some(blob) = blob else {
            return String::new();
        };
        // Binary guard: a binary blob yields the empty language.
        if is_binary(&blob.data) {
            return String::new();
        }
        // Fast path: extension lookup.
        let lang = language_by_extension(name);
        if !lang.is_empty() {
            return lang.to_string();
        }
        // Slow path: content analysis.
        self.classifier.get_language(base_name(name), &blob.data)
    }

    /// Build the blob-hash -> language map for one commit: inserts key the
    /// `to` side, deletes the `from` side, and modifies key both sides.
    pub fn build(
        &self,
        changes: &Changes,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> HashMap<Hash, String> {
        let mut result: HashMap<Hash, String> = HashMap::new();
        for change in changes {
            match change.action() {
                Some(Action::Insert) => {
                    result.insert(
                        change.to.hash,
                        self.detect_language(&change.to.name, cache.get(&change.to.hash)),
                    );
                }
                Some(Action::Delete) => {
                    result.insert(
                        change.from.hash,
                        self.detect_language(&change.from.name, cache.get(&change.from.hash)),
                    );
                }
                Some(Action::Modify) => {
                    result.insert(
                        change.to.hash,
                        self.detect_language(&change.to.name, cache.get(&change.to.hash)),
                    );
                    result.insert(
                        change.from.hash,
                        self.detect_language(&change.from.name, cache.get(&change.from.hash)),
                    );
                }
                None => {}
            }
        }
        result
    }
}

impl<C: EnryClassifier> Analyzer for LanguagesDetection<C> {
    fn name(&self) -> &'static str {
        "LanguagesDetection"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["languages"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec!["changes", "blob_cache"]
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let cache = dep::<HashMap<Hash, CachedBlob>>(deps, "blob_cache")?;
        let changes = dep::<Changes>(deps, "changes")?;
        let result = self.build(changes, cache);
        let mut out = ValueMap::new();
        out.insert("languages".to_string(), Box::new(result));
        Ok(out)
    }
}

/// Base file name (libgit2 paths always use `/`).
fn base_name(path: &str) -> &str {
    path.rfind('/').map_or(path, |i| &path[i + 1..])
}

/// Programming language for a filename by extension. The extension is
/// lowercased before lookup; the table keys are all lowercase. Returns `""`
/// when there is no extension or no match.
///
/// The table is frozen (language-detection contract).
#[must_use]
pub fn language_by_extension(filename: &str) -> &'static str {
    let ext = match filename.rfind('.') {
        // The extension starts at the last dot; a dotfile with no other dot
        // yields an "extension" as well (reference-implementation behavior),
        // so keep the raw last-dot rule, which matches for the cases here.
        Some(i) => &filename[i..],
        None => return "",
    };
    let ext = ext.to_ascii_lowercase();
    EXTENSION_TO_LANGUAGE
        .iter()
        .find(|(k, _)| *k == ext)
        .map_or("", |(_, v)| *v)
}

/// The frozen extension fast-path table (language-detection contract; pinned
/// by the differential gate). Stored as a slice: lookups are linear but the
/// table is small, and this keeps iteration order deterministic. Keys are
/// lowercase.
static EXTENSION_TO_LANGUAGE: &[(&str, &str)] = &[
    (".go", "Go"),
    (".py", "Python"),
    (".pyw", "Python"),
    (".pyi", "Python"),
    (".pyx", "Python"),
    (".pxd", "Python"),
    (".gyp", "Python"),
    (".gypi", "Python"),
    (".js", "JavaScript"),
    (".mjs", "JavaScript"),
    (".cjs", "JavaScript"),
    (".jsx", "JavaScript"),
    (".es6", "JavaScript"),
    (".es", "JavaScript"),
    (".jsm", "JavaScript"),
    (".vue", "Vue"),
    (".svelte", "Svelte"),
    (".ts", "TypeScript"),
    (".mts", "TypeScript"),
    (".cts", "TypeScript"),
    (".tsx", "TSX"),
    (".rs", "Rust"),
    (".java", "Java"),
    (".kt", "Kotlin"),
    (".kts", "Kotlin"),
    (".scala", "Scala"),
    (".sc", "Scala"),
    (".c", "C"),
    (".h", "C"),
    (".cpp", "C++"),
    (".hpp", "C++"),
    (".cc", "C++"),
    (".cxx", "C++"),
    (".hxx", "C++"),
    (".c++", "C++"),
    (".h++", "C++"),
    (".hh", "C++"),
    (".ipp", "C++"),
    (".inl", "C++"),
    (".tcc", "C++"),
    (".tpp", "C++"),
    (".cs", "C#"),
    (".csx", "C#"),
    (".rb", "Ruby"),
    (".rake", "Ruby"),
    (".gemspec", "Ruby"),
    (".rbw", "Ruby"),
    (".ru", "Ruby"),
    (".podspec", "Ruby"),
    (".thor", "Ruby"),
    (".jbuilder", "Ruby"),
    (".php", "PHP"),
    (".php3", "PHP"),
    (".php4", "PHP"),
    (".php5", "PHP"),
    (".php7", "PHP"),
    (".phps", "PHP"),
    (".phtml", "PHP"),
    (".sh", "Shell"),
    (".bash", "Shell"),
    (".zsh", "Shell"),
    (".ksh", "Shell"),
    (".csh", "Shell"),
    (".tcsh", "Shell"),
    (".fish", "Shell"),
    (".ps1", "PowerShell"),
    (".psm1", "PowerShell"),
    (".psd1", "PowerShell"),
    (".pl", "Perl"),
    (".pm", "Perl"),
    (".pod", "Perl"),
    (".t", "Perl"),
    (".lua", "Lua"),
    // The reference table has both ".r" and ".R" keys mapping to "R"; after
    // lowercasing the lookup, ".r" covers both. ".rmd"/".Rmd" likewise both
    // lowercase to ".rmd".
    (".r", "R"),
    (".rmd", "RMarkdown"),
    (".swift", "Swift"),
    (".m", "Objective-C"),
    (".mm", "Objective-C++"),
    (".dart", "Dart"),
    (".ex", "Elixir"),
    (".exs", "Elixir"),
    (".eex", "Elixir"),
    (".leex", "Elixir"),
    (".heex", "Elixir"),
    (".erl", "Erlang"),
    (".hrl", "Erlang"),
    (".hs", "Haskell"),
    (".lhs", "Haskell"),
    (".clj", "Clojure"),
    (".cljs", "ClojureScript"),
    (".cljc", "Clojure"),
    (".edn", "Clojure"),
    (".fs", "F#"),
    (".fsi", "F#"),
    (".fsx", "F#"),
    (".fsscript", "F#"),
    (".ml", "OCaml"),
    (".mli", "OCaml"),
    (".mll", "OCaml"),
    (".mly", "OCaml"),
    (".json", "JSON"),
    (".json5", "JSON5"),
    (".yaml", "YAML"),
    (".yml", "YAML"),
    (".toml", "TOML"),
    (".xml", "XML"),
    (".csv", "CSV"),
    (".tsv", "TSV"),
    (".ini", "INI"),
    (".cfg", "INI"),
    (".conf", "INI"),
    (".env", "Dotenv"),
    (".html", "HTML"),
    (".htm", "HTML"),
    (".xhtml", "HTML"),
    (".css", "CSS"),
    (".scss", "SCSS"),
    (".sass", "Sass"),
    (".less", "Less"),
    (".styl", "Stylus"),
    (".md", "Markdown"),
    (".markdown", "Markdown"),
    (".rst", "reStructuredText"),
    (".tex", "TeX"),
    (".latex", "TeX"),
    (".adoc", "AsciiDoc"),
    (".asciidoc", "AsciiDoc"),
    (".sql", "SQL"),
    (".psql", "SQL"),
    (".mysql", "SQL"),
    (".pgsql", "SQL"),
    (".graphql", "GraphQL"),
    (".gql", "GraphQL"),
    (".proto", "Protocol Buffer"),
    (".thrift", "Thrift"),
    (".wat", "WebAssembly"),
    (".wast", "WebAssembly"),
    (".asm", "Assembly"),
    // ".s" and ".S" both lowercase to ".s".
    (".s", "Assembly"),
    (".zig", "Zig"),
    (".nim", "Nim"),
    (".nims", "Nim"),
    (".nimble", "Nim"),
    (".jl", "Julia"),
    (".v", "V"),
    (".cr", "Crystal"),
    (".groovy", "Groovy"),
    (".gradle", "Groovy"),
    (".gvy", "Groovy"),
    (".dockerfile", "Dockerfile"),
    (".mk", "Makefile"),
    (".mak", "Makefile"),
    (".cmake", "CMake"),
    (".tf", "HCL"),
    (".tfvars", "HCL"),
    (".hcl", "HCL"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_model::{Change, ChangeEntry};

    /// Classifier that never matches, so tests exercise only the fast path /
    /// binary guard. The real enry-parity classifier is a todo.
    struct NoContent;
    impl EnryClassifier for NoContent {
        fn get_language(&self, _base: &str, _content: &[u8]) -> String {
            String::new()
        }
    }

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    #[test]
    fn extension_fast_path() {
        assert_eq!(language_by_extension("main.go"), "Go");
        assert_eq!(language_by_extension("a/b/main.rs"), "Rust");
        // Uppercase extension lowercases to match.
        assert_eq!(language_by_extension("Foo.PY"), "Python");
        assert_eq!(language_by_extension("data.unknownext"), "");
        assert_eq!(language_by_extension("Makefile"), "");
    }

    #[test]
    fn binary_blob_yields_empty() {
        let ld = LanguagesDetection::new(NoContent);
        let blob = CachedBlob::new(vec![0u8, 1, 2]);
        // Even with a known extension, a binary blob is "".
        assert_eq!(ld.detect_language("main.go", Some(&blob)), "");
    }

    #[test]
    fn nil_blob_yields_empty() {
        let ld = LanguagesDetection::new(NoContent);
        assert_eq!(ld.detect_language("main.go", None), "");
    }

    #[test]
    fn build_keys_both_sides_of_modify() {
        let ld = LanguagesDetection::new(NoContent);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"package main\n".to_vec()));
        cache.insert(h(2), CachedBlob::new(b"package main\n".to_vec()));
        let changes = vec![Change {
            from: ChangeEntry { name: "a.go".into(), hash: h(1) },
            to: ChangeEntry { name: "a.go".into(), hash: h(2) },
        }];
        let out = ld.build(&changes, &cache);
        assert_eq!(out.get(&h(1)).map(String::as_str), Some("Go"));
        assert_eq!(out.get(&h(2)).map(String::as_str), Some("Go"));
    }

    #[test]
    fn provider_metadata() {
        let ld = LanguagesDetection::new(NoContent);
        assert_eq!(ld.name(), "LanguagesDetection");
        assert_eq!(ld.provides(), vec!["languages"]);
        assert_eq!(ld.requires(), vec!["changes", "blob_cache"]);
    }
}
