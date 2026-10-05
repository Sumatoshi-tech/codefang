//! File classifier.
//!
//! [`Classifier::classify_file`] is a fixed-priority cascade over enry-style
//! predicates plus generated-file checks:
//!
//! ```text
//! // first match wins:
//! if content is non-empty and contains a NUL byte:      Binary
//! if the path has an image extension:                   Image
//! if the path matches a vendoring convention:           Vendor
//! if the path matches a generated-file rule:            Generated
//! if non-empty content carries a generated marker:      Generated
//! if the path matches a documentation convention
//!    or its extension is a prose one (.md, .rst, …):   Documentation
//! if the extension maps to a configuration language:    Configuration
//! if the base name is a dotfile:                        DotFile
//! if enry recognizes a language (name, extension,
//!    shebang or modeline):                              Source
//! else:                                                 Other
//! ```
//!
//! The branch ORDER is load-bearing: a file matching several predicates is
//! classified by the first matching branch. Paths must be **relative to the
//! analyzed root** — the documentation and vendor matchers anchor on path
//! prefixes such as `docs/`.
//!
//! The vendor table ([`cf_pathfilter::is_vendor`]) and the language tables
//! ([`cf_langpath`]) are the vendored enry v2.1.0 data; the documentation
//! path matchers reproduce enry's regex table exactly without a regex
//! dependency.

use crate::category::{Category, FileClass};

/// Extensions of the linguist *prose* languages (AsciiDoc, Creole,
/// Markdown, MediaWiki, Org, Pod, RDoc, RMarkdown, Rich Text Format,
/// Textile, Wikitext, reStructuredText, Text): a file with one of these
/// extensions is documentation even outside a documentation directory. The
/// list is pinned here because enry's extension table resolves an extension
/// to its FIRST language (`.md` → "GCC Machine Description"), which hides
/// the prose type.
const PROSE_EXTENSIONS: [&str; 24] = [
    ".adoc",
    ".asc",
    ".asciidoc",
    ".creole",
    ".markdown",
    ".md",
    ".mdown",
    ".mediawiki",
    ".mkd",
    ".mkdn",
    ".mkdown",
    ".org",
    ".pod",
    ".rdoc",
    ".rest",
    ".rmd",
    ".ron",
    ".rst",
    ".rtf",
    ".text",
    ".textile",
    ".txt",
    ".wiki",
    ".wikitext",
];

/// enry's configuration-language set.
const CONFIGURATION_LANGUAGES: [&str; 6] = ["XML", "JSON", "TOML", "YAML", "INI", "SQL"];

/// Classifies files into categories using enry heuristics.
///
/// Zero-sized; all state lives in the static rule tables.
#[derive(Debug, Default, Clone, Copy)]
pub struct Classifier;

impl Classifier {
    /// Creates a new file classifier.
    #[must_use]
    pub fn new() -> Self {
        Classifier
    }

    /// Determines the composition class of a file from its root-relative
    /// path and (header) content — the cascade in the module docs.
    ///
    /// ```
    /// use cf_composition::{Classifier, Category, FileClass};
    ///
    /// let c = Classifier::new();
    /// assert_eq!(c.classify_file("pkg/main.go", b"package main\n"), Category::Source.into());
    /// assert_eq!(c.classify_file("vendor/foo/bar.go", b"package bar\n"), Category::Vendor.into());
    /// assert_eq!(c.classify_file("docs/guide.md", b"# Hi\n"), Category::Documentation.into());
    /// assert_eq!(c.classify_file("logo.png", &[]), Category::Image.into());
    /// // The binary check requires non-empty content.
    /// assert_eq!(c.classify_file("data.bin", &[0, 1, 2, 0xFF, 0, 0]), Category::Binary.into());
    /// // No rule matches an unknown extension.
    /// assert_eq!(c.classify_file("data.xyz", b"hello\n"), FileClass::Other);
    /// ```
    #[must_use]
    pub fn classify_file(&self, path: &str, content: &[u8]) -> FileClass {
        let category = if !content.is_empty() && enry::is_binary(content) {
            Category::Binary
        } else if enry::is_image(path) {
            Category::Image
        } else if enry::is_vendor(path) {
            Category::Vendor
        } else if pathfilter::is_generated_path(path)
            || (!content.is_empty() && pathfilter::is_generated_content(content))
        {
            Category::Generated
        } else if enry::is_documentation(path) || is_prose(path) {
            Category::Documentation
        } else if enry::is_configuration(path) {
            Category::Configuration
        } else if enry::is_dotfile(path) {
            Category::DotFile
        } else if self.language(path, content).is_some() {
            Category::Source
        } else {
            return FileClass::Other;
        };
        FileClass::Category(category)
    }

    /// The 8-way classification shared with file-history analysis: the same
    /// cascade as [`Classifier::classify_file`], with
    /// [`FileClass::Other`] folded into [`Category::Source`].
    ///
    /// ```
    /// use cf_composition::{Classifier, Category};
    ///
    /// let c = Classifier::new();
    /// assert_eq!(c.classify("pkg/main.go", b"package main\n"), Category::Source);
    /// assert_eq!(c.classify("data.xyz", b"hello\n"), Category::Source);
    /// ```
    #[must_use]
    pub fn classify(&self, path: &str, content: &[u8]) -> Category {
        match self.classify_file(path, content) {
            FileClass::Category(c) => c,
            FileClass::Other => Category::Source,
        }
    }

    /// The enry language of a file (by file name, extension, shebang or
    /// modeline), if any.
    ///
    /// ```
    /// use cf_composition::Classifier;
    ///
    /// let c = Classifier::new();
    /// assert_eq!(c.language("main.go", b"").as_deref(), Some("Go"));
    /// assert_eq!(c.language("run", b"#!/usr/bin/env python3\n").as_deref(), Some("Python"));
    /// assert_eq!(c.language("data.xyz", b"hello\n"), None);
    /// ```
    #[must_use]
    #[allow(clippy::unused_self)] // instance method: the classifier owns language resolution
    pub fn language(&self, path: &str, content: &[u8]) -> Option<String> {
        cf_langpath::language_by_path_with_content(path, content)
    }
}

/// The path's extension (case-insensitive) belongs to a prose language.
fn is_prose(path: &str) -> bool {
    let extension = ext(path).to_ascii_lowercase();
    PROSE_EXTENSIONS.contains(&extension.as_str())
}

/// Returns the final path element of `path` after trimming trailing slashes.
/// An empty path yields `"."`; an all-slash path yields `"/"`.
fn base_clean(path: &str) -> &str {
    if path.is_empty() {
        return ".";
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(idx) => &trimmed[idx + 1..],
        None => trimmed,
    }
}

/// Returns the file extension including the leading dot, case-preserving
/// (the suffix from the final dot in the base name). Empty when the base
/// name has no dot.
fn ext(path: &str) -> &str {
    let base = base_clean(path);
    match base.rfind('.') {
        Some(idx) => &base[idx..],
        None => "",
    }
}

/// The subset of `src-d/enry` v2.1.0 predicates used by the classifier.
mod enry {
    use super::{base_clean, CONFIGURATION_LANGUAGES};

    /// Content is detected as binary: a NUL byte within the first 8000
    /// bytes (git's binary detection).
    pub fn is_binary(content: &[u8]) -> bool {
        const SNIFF_LEN: usize = 8000;
        let window = &content[..content.len().min(SNIFF_LEN)];
        window.contains(&0u8)
    }

    /// Path has an image extension: exactly `.png`, `.jpg`, `.jpeg`, or
    /// `.gif` (case-sensitive, as enry compares the raw extension).
    pub fn is_image(path: &str) -> bool {
        matches!(super::ext(path), ".png" | ".jpg" | ".jpeg" | ".gif")
    }

    /// The cleaned base name starts with `.` and is not exactly `.`.
    pub fn is_dotfile(path: &str) -> bool {
        let base = base_clean(path);
        base.starts_with('.') && base != "."
    }

    /// The path's extension-only language is a configuration language
    /// (`XML`, `JSON`, `TOML`, `YAML`, `INI`, `SQL`).
    pub fn is_configuration(path: &str) -> bool {
        cf_langpath::language_by_extension(path)
            .is_some_and(|lang| CONFIGURATION_LANGUAGES.contains(&lang.as_str()))
    }

    /// Path matches a vendoring convention (enry's full `VendorMatchers`
    /// table via [`cf_pathfilter::is_vendor`]).
    pub fn is_vendor(path: &str) -> bool {
        cf_pathfilter::is_vendor(path)
    }

    /// Path matches a documentation convention — enry's documentation
    /// matchers, an OR of unanchored regex patterns reproduced exactly:
    ///
    /// ```text
    /// ^[Dd]ocs?/                  (^|/)[Dd]ocumentation/   (^|/)[Gg]roovydoc/
    /// (^|/)[Jj]avadoc/            ^[Mm]an/                 ^[Ee]xamples/
    /// ^[Dd]emos?/                 (^|/)inst/doc/
    /// (^|/)CHANGE(S|LOG)?(\.|$)   (^|/)CONTRIBUTING(\.|$)  (^|/)COPYING(\.|$)
    /// (^|/)INSTALL(\.|$)          (^|/)LICEN[CS]E(\.|$)    (^|/)[Ll]icen[cs]e(\.|$)
    /// (^|/)README(\.|$)          (^|/)[Rr]eadme(\.|$)      ^[Ss]amples?/
    /// ```
    pub fn is_documentation(path: &str) -> bool {
        const FILE_NAMES: &[&str] = &[
            "CONTRIBUTING",
            "COPYING",
            "INSTALL",
            "README",
            "Readme",
            "readme",
        ];
        dir_prefix(path, &["Doc/", "Docs/", "doc/", "docs/"])
            || dir_segment(path, &["Documentation/", "documentation/"])
            || dir_segment(path, &["groovydoc/", "Groovydoc/"])
            || dir_segment(path, &["javadoc/", "Javadoc/"])
            || dir_prefix(path, &["man/", "Man/"])
            || dir_prefix(path, &["examples/", "Examples/"])
            || dir_prefix(path, &["demo/", "demos/", "Demo/", "Demos/"])
            || dir_segment(path, &["inst/doc/"])
            || dir_prefix(path, &["sample/", "samples/", "Sample/", "Samples/"])
            || file_component(path, |name| FILE_NAMES.contains(&name))
            || file_component(path, |name| {
                matches!(name, "CHANGE" | "CHANGES" | "CHANGELOG")
            })
            || file_component(path, |name| {
                matches!(
                    name,
                    "LICENCE" | "LICENSE" | "Licence" | "License" | "licence" | "license"
                )
            })
    }

    /// Tests `^(prefix)` for any of the given directory prefixes (each ending
    /// in `/`).
    fn dir_prefix(path: &str, prefixes: &[&str]) -> bool {
        prefixes.iter().any(|p| path.starts_with(p))
    }

    /// Tests `(^|/)segment` for any of the given directory segments (each
    /// ending in `/`).
    fn dir_segment(path: &str, segments: &[&str]) -> bool {
        segments
            .iter()
            .any(|s| path.starts_with(s) || path.contains(&format!("/{s}")))
    }

    /// Tests `(^|/)NAME(\.|$)`: NAME anchored at a component start,
    /// immediately followed by a literal `.` (any component, even a
    /// directory like `NAME.d/`) or by the end of the whole path. A bare
    /// directory component equal to NAME does NOT match (it is followed by
    /// `/`), so `CHANGELOG/CHANGELOG-1.x.md` is not documentation.
    fn file_component(path: &str, pred: impl Fn(&str) -> bool) -> bool {
        let mut comps = path.split('/').peekable();
        while let Some(comp) = comps.next() {
            if comps.peek().is_none() && pred(comp) {
                return true;
            }
            if let Some(idx) = comp.find('.') {
                if pred(&comp[..idx]) {
                    return true;
                }
            }
        }
        false
    }
}

/// Generated-file checks, mirroring the rules `cf-pathfilter` vendors.
mod pathfilter {
    use super::base_clean;

    /// File suffixes indicating generated code.
    const DEFAULT_SUFFIXES: &[&str] = &[
        ".pb.go",
        ".pb.gw.go",
        ".generated.go",
        ".deepcopy.go",
        "_string.go",
        "_enumer.go",
        "_easyjson.go",
        "_pb2.py",
        "_pb2_grpc.py",
        ".pb.cc",
        ".pb.h",
        ".grpc.pb.cc",
        ".grpc.pb.h",
        ".min.js",
        ".min.css",
        ".bundle.js",
    ];

    /// Filename prefixes (matched against the base name) indicating generated
    /// code.
    const DEFAULT_FILENAME_PREFIXES: &[&str] = &["zz_generated", "mock_", "fake_", "wire_gen"];

    /// Byte markers found near the top of generated files.
    const GENERATED_MARKERS: &[&[u8]] = &[
        b"DO NOT EDIT",
        b"Code generated",
        b"AUTO-GENERATED",
        b"auto-generated",
        b"Autogenerated",
        b"@generated",
    ];

    /// How many header bytes to scan for a generated marker.
    const GENERATED_MARKER_SCAN_LIMIT: usize = 512;

    /// The path's name matches a generated suffix or filename prefix.
    pub fn is_generated_path(path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        if DEFAULT_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            return true;
        }
        let base = base_clean(path);
        DEFAULT_FILENAME_PREFIXES
            .iter()
            .any(|p| base.starts_with(p))
    }

    /// A generated marker appears within the first
    /// [`GENERATED_MARKER_SCAN_LIMIT`] bytes of `content`.
    pub fn is_generated_content(content: &[u8]) -> bool {
        let window = &content[..content.len().min(GENERATED_MARKER_SCAN_LIMIT)];
        GENERATED_MARKERS
            .iter()
            .any(|m| window.windows(m.len()).any(|w| w == *m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(path: &str, content: &[u8]) -> FileClass {
        Classifier::new().classify_file(path, content)
    }

    fn cat(c: Category) -> FileClass {
        FileClass::Category(c)
    }

    #[test]
    fn every_class_is_reachable() {
        assert_eq!(class("main.go", b"package main\n"), cat(Category::Source));
        assert_eq!(class("Makefile", b"all:\n"), cat(Category::Source));
        assert_eq!(class("README.md", b"# T\n"), cat(Category::Documentation));
        assert_eq!(
            class("docs/guide.md", b"# G\n"),
            cat(Category::Documentation)
        );
        assert_eq!(class("notes.txt", b"plain\n"), cat(Category::Documentation));
        assert_eq!(
            class("pkg/notes.md", b"# n\n"),
            cat(Category::Documentation)
        );
        assert_eq!(class("pkg/NOTES.RST", b"n\n"), cat(Category::Documentation));
        assert_eq!(
            class("config.yaml", b"k: v\n"),
            cat(Category::Configuration)
        );
        // enry lists `.sql` under PLSQL first, so SQL files are source, not
        // configuration (the configuration set only matches when the
        // extension's FIRST language is one of its six).
        assert_eq!(
            cf_langpath::language_by_extension("schema.sql").as_deref(),
            Some("PLSQL")
        );
        assert_eq!(class("schema.sql", b"SELECT 1;\n"), cat(Category::Source));
        assert_eq!(class("settings.json", b"{}"), cat(Category::Configuration));
        assert_eq!(
            class("vendor/x/y.go", b"package x\n"),
            cat(Category::Vendor)
        );
        assert_eq!(
            class("gen/thing.pb.go", b"// Code generated. DO NOT EDIT.\n"),
            cat(Category::Generated)
        );
        assert_eq!(
            class(
                "api.go",
                b"// Code generated by tool. DO NOT EDIT.\npackage api\n"
            ),
            cat(Category::Generated)
        );
        assert_eq!(class(".editorconfig", b"[*]\n"), cat(Category::DotFile));
        assert_eq!(class("logo.png", &[]), cat(Category::Image));
        assert_eq!(class("data.bin", &[0, 1, 0xFF, 0]), cat(Category::Binary));
        assert_eq!(class("data.xyz", b"hello world\n"), FileClass::Other);
    }

    #[test]
    fn empty_content_never_triggers_binary_or_generated_content() {
        assert_eq!(class("empty.go", &[]), cat(Category::Source));
        assert_eq!(class("empty.txt", &[]), cat(Category::Documentation));
    }

    #[test]
    fn cascade_order_is_first_match_wins() {
        // vendor beats documentation and configuration
        assert_eq!(class("vendor/README.md", b"# v\n"), cat(Category::Vendor));
        // generated-by-path beats documentation
        assert_eq!(
            class("docs/x.pb.go", b"package x\n"),
            cat(Category::Generated)
        );
        // configuration beats dotfile for a YAML dotfile
        assert_eq!(
            class(".golangci.yml", b"linters:\n"),
            cat(Category::Configuration)
        );
        // binary content beats every path rule
        assert_eq!(class("docs/guide.md", &[0, 1, 0]), cat(Category::Binary));
    }

    #[test]
    fn shebang_scripts_are_source() {
        assert_eq!(
            class("run", b"#!/usr/bin/env python3\nprint(1)\n"),
            cat(Category::Source)
        );
    }

    #[test]
    fn classify_folds_other_into_source() {
        let c = Classifier::new();
        assert_eq!(c.classify("data.xyz", b"hello\n"), Category::Source);
        assert_eq!(c.classify("vendor/a.go", b""), Category::Vendor);
    }

    #[test]
    fn documentation_matchers_anchor_like_enry() {
        assert!(enry::is_documentation("docs/a.go"));
        assert!(!enry::is_documentation("src/docs/a.go"));
        assert!(enry::is_documentation("src/Documentation/a.go"));
        assert!(enry::is_documentation("LICENSE"));
        assert!(enry::is_documentation("pkg/CHANGELOG.md"));
        assert!(!enry::is_documentation("CHANGELOG/CHANGELOG-1.x.md"));
    }

    #[test]
    fn image_extension_is_case_sensitive() {
        assert!(enry::is_image("a/b.gif"));
        assert!(!enry::is_image("a/b.GIF"));
    }

    #[test]
    fn base_and_ext_helpers() {
        assert_eq!(base_clean(""), ".");
        assert_eq!(base_clean("///"), "/");
        assert_eq!(base_clean("a/b/c.go/"), "c.go");
        assert_eq!(ext("a/b.tar.gz"), ".gz");
        assert_eq!(ext("Makefile"), "");
    }
}
