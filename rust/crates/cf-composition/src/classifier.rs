//! File classifier.
//!
//! Ported from Go `internal/analyzers/file_history/classify.go`, whose
//! `(*Classifier).Classify` is a fixed-priority cascade over `src-d/enry`
//! predicates plus `pkg/pathfilter` generated-file checks:
//!
//! ```text
//! // first match wins:
//! if len(content) > 0 && enry.IsBinary(content):    Binary
//! if enry.IsImage(path):                            Image
//! if enry.IsVendor(path):                           Vendor
//! if filter.IsGeneratedPath(path):                  Generated
//! if len(content) > 0 && filter.IsGeneratedContent: Generated
//! if enry.IsDocumentation(path):                    Documentation
//! if enry.IsConfiguration(path):                    Configuration
//! if enry.IsDotFile(path):                          DotFile
//! else:                                             Source
//! ```
//!
//! The branch ORDER is load-bearing (a file matching several predicates is
//! classified by the first matching branch), so the Rust port preserves the
//! exact sequence.
//!
//! # Data parity (DESIGN rule 7)
//!
//! enry's `IsVendor` / `IsDocumentation` predicates are driven by generated
//! regex tables (`src-d/enry/v2/data/{vendor,documentation}.go`), and the
//! generated-file rules come from `pkg/pathfilter`. For full byte-identity on a
//! real corpus those tables should be sourced from the workspace's already-
//! vendored copies — `cf-pathfilter` (which vendors enry's `vendor.go` regexes
//! via its `build.rs`, and ports the pathfilter generated rules) and
//! `cf-langpath` (which ships the enry v2.1.0 extension TSV). See the crate
//! `todos`. The predicates below reproduce enry's algorithms and the
//! high-frequency data subset exercised by the ported Go tests; widening the
//! embedded tables changes only the data slices these functions consult, not
//! their structure.
//!
//! The fully-data-exact predicates here are the ones enry implements *without* a
//! data table: [`enry::is_image`] (4 hard-coded extensions), [`enry::is_dotfile`]
//! (`filepath.Base(filepath.Clean(path))` dot-prefix), [`enry::is_binary`]
//! (8000-byte NUL sniff), and [`enry::is_configuration`] (extension → one of
//! `{INI, JSON, TOML, YAML, XML}`).

use crate::category::Category;

/// Classifies files into categories using enry heuristics.
///
/// Zero-sized, like the Go `Classifier` (whose only field is a `*pathfilter`).
#[derive(Debug, Default, Clone, Copy)]
pub struct Classifier;

impl Classifier {
    /// Creates a new file classifier.
    #[must_use]
    pub fn new() -> Self {
        Classifier
    }

    /// Determines the category of a file from its path and content.
    ///
    /// Mirrors Go `(*Classifier).Classify` branch-for-branch, including the
    /// `len(content) > 0` guards on the binary and generated-content checks
    /// (so an empty/`nil` content never triggers Binary or content-Generated).
    #[must_use]
    // The cascade is deliberately kept branch-for-branch identical to Go's
    // `(*Classifier).Classify`; two arms both yield `Category::Generated`
    // (generated-by-path then generated-by-content), which clippy flags but is
    // load-bearing for parity and readability.
    #[allow(clippy::if_same_then_else)]
    pub fn classify(&self, path: &str, content: &[u8]) -> Category {
        if !content.is_empty() && enry::is_binary(content) {
            Category::Binary
        } else if enry::is_image(path) {
            Category::Image
        } else if enry::is_vendor(path) {
            Category::Vendor
        } else if pathfilter::is_generated_path(path) {
            Category::Generated
        } else if !content.is_empty() && pathfilter::is_generated_content(content) {
            Category::Generated
        } else if enry::is_documentation(path) {
            Category::Documentation
        } else if enry::is_configuration(path) {
            Category::Configuration
        } else if enry::is_dotfile(path) {
            Category::DotFile
        } else {
            Category::Source
        }
    }
}

/// Returns the final path element of `path` after cleaning, equivalent to Go's
/// `filepath.Base(filepath.Clean(path))` for the `/`-separated inputs this
/// classifier sees on its Linux golden target.
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

/// Returns the file extension including the leading dot, **case-preserving**,
/// equivalent to Go's `filepath.Ext` (the suffix from the final dot in the base
/// name). Empty when the base name has no dot. Use this where the Go predicate
/// compares the raw extension (e.g. `enry.IsImage`'s case-sensitive check), so an
/// upper-case `.GIF` is not misclassified the way a lower-cased compare would.
fn ext(path: &str) -> String {
    let base = base_clean(path);
    match base.rfind('.') {
        Some(idx) => base[idx..].to_string(),
        None => String::new(),
    }
}

/// Port of the subset of `src-d/enry` v2.1.0 predicates used by the classifier.
mod enry {
    use super::base_clean;

    /// `enry.IsBinary` — content is detected as binary.
    ///
    /// Exact port: read up to `sniffLen` (8000) bytes and return true iff a NUL
    /// byte appears in that window (git's binary detection).
    pub fn is_binary(content: &[u8]) -> bool {
        const SNIFF_LEN: usize = 8000;
        let window = &content[..content.len().min(SNIFF_LEN)];
        window.contains(&0u8)
    }

    /// `enry.IsImage` — exact port: extension is exactly `.png`, `.jpg`,
    /// `.jpeg`, or `.gif`. Go compares the **raw** `filepath.Ext`, so the match
    /// is case-sensitive: an upper-case `.GIF` (e.g. ioq3's `RADIANT3.GIF`) is
    /// NOT an image and falls through to `source`, which is what Go reports.
    pub fn is_image(path: &str) -> bool {
        matches!(super::ext(path).as_str(), ".png" | ".jpg" | ".jpeg" | ".gif")
    }

    /// `enry.IsDotFile` — exact port: `filepath.Base(filepath.Clean(path))`
    /// starts with `.` and is not exactly `.`.
    pub fn is_dotfile(path: &str) -> bool {
        let base = base_clean(path);
        base.starts_with('.') && base != "."
    }

    /// `enry.IsConfiguration` — exact port:
    ///
    /// ```go
    /// func IsConfiguration(path string) bool {
    ///     language, _ := GetLanguageByExtension(path)
    ///     _, is := configurationLanguages[language]
    ///     return is
    /// }
    /// var configurationLanguages = map[string]bool{
    ///     "XML": true, "JSON": true, "TOML": true, "YAML": true, "INI": true, "SQL": true,
    /// }
    /// ```
    ///
    /// We resolve the extension-only language via enry's real
    /// `data.LanguagesByExtension` table (vendored in `cf-langpath`) through
    /// [`cf_langpath::language_by_extension`], then test membership in the exact
    /// `configurationLanguages` set. This replaces the previous hand-curated
    /// extension list, which both missed entries (`SQL`, and the long XML-family
    /// tail) and added wrong ones, so files now bucket exactly as Go does.
    pub fn is_configuration(path: &str) -> bool {
        const CONFIGURATION_LANGUAGES: [&str; 6] =
            ["XML", "JSON", "TOML", "YAML", "INI", "SQL"];
        match cf_langpath::language_by_extension(path) {
            Some(lang) => CONFIGURATION_LANGUAGES.contains(&lang.as_str()),
            None => false,
        }
    }

    /// `enry.IsVendor` — path matches a vendoring convention.
    ///
    /// enry tests the path against `data.VendorMatchers` (compiled regexes). The
    /// high-frequency conventions are reproduced here; the full regex list is a
    /// vendoring follow-up that should reuse `cf-pathfilter::is_vendor`.
    pub fn is_vendor(path: &str) -> bool {
        const VENDOR_PREFIXES: [&str; 8] = [
            "vendor/",
            "node_modules/",
            "third_party/",
            "third-party/",
            "Godeps/",
            "bower_components/",
            ".git/",
            "extern/",
        ];
        const VENDOR_SEGMENTS: [&str; 8] = [
            "/vendor/",
            "/node_modules/",
            "/third_party/",
            "/third-party/",
            "/Godeps/",
            "/bower_components/",
            "/.git/",
            "/extern/",
        ];
        if VENDOR_PREFIXES.iter().any(|p| path.starts_with(p)) {
            return true;
        }
        if VENDOR_SEGMENTS.iter().any(|s| path.contains(s)) {
            return true;
        }
        path == "vendor" || path == "node_modules"
    }

    /// `enry.IsDocumentation` — exact port of `data.DocumentationMatchers`.
    ///
    /// enry's matcher is a `substring.Or` of unanchored `regexp.MatchString`
    /// patterns (verbatim from `src-d/enry/v2@v2.1.0/data/documentation.go`,
    /// extracted from github/linguist):
    ///
    /// ```text
    /// ^[Dd]ocs?/                  (^|/)[Dd]ocumentation/   (^|/)[Gg]roovydoc/
    /// (^|/)[Jj]avadoc/            ^[Mm]an/                 ^[Ee]xamples/
    /// ^[Dd]emos?/                 (^|/)inst/doc/
    /// (^|/)CHANGE(S|LOG)?(\.|$)   (^|/)CONTRIBUTING(\.|$)  (^|/)COPYING(\.|$)
    /// (^|/)INSTALL(\.|$)          (^|/)LICEN[CS]E(\.|$)    (^|/)[Ll]icen[cs]e(\.|$)
    /// (^|/)README(\.|$)          (^|/)[Rr]eadme(\.|$)      ^[Ss]amples?/
    /// ```
    ///
    /// These are PATH-only (no prose extensions such as `.md`/`.rst` — the
    /// previous port wrongly added those, over-classifying source files as
    /// documentation). The patterns are reproduced exactly below without a regex
    /// dependency.
    pub fn is_documentation(path: &str) -> bool {
        // `^[Dd]ocs?/` — starts with Doc/ Docs/ doc/ docs/.
        if dir_prefix(path, &["Doc/", "Docs/", "doc/", "docs/"]) {
            return true;
        }
        // `(^|/)X/` directory-segment matchers.
        if dir_segment(path, &["Documentation/", "documentation/"]) {
            return true;
        }
        if dir_segment(path, &["groovydoc/", "Groovydoc/"]) {
            return true;
        }
        if dir_segment(path, &["javadoc/", "Javadoc/"]) {
            return true;
        }
        // `^[Mm]an/`
        if dir_prefix(path, &["man/", "Man/"]) {
            return true;
        }
        // `^[Ee]xamples/`
        if dir_prefix(path, &["examples/", "Examples/"]) {
            return true;
        }
        // `^[Dd]emos?/`
        if dir_prefix(path, &["demo/", "demos/", "Demo/", "Demos/"]) {
            return true;
        }
        // `(^|/)inst/doc/`
        if dir_segment(path, &["inst/doc/"]) {
            return true;
        }
        // `^[Ss]amples?/`
        if dir_prefix(path, &["sample/", "samples/", "Sample/", "Samples/"]) {
            return true;
        }
        // Filename matchers: `(^|/)NAME(\.|$)` — the path component equals NAME
        // or starts with `NAME.`. enry's `(\.|$)` allows a trailing extension.
        const FILE_NAMES: &[&str] = &[
            "CONTRIBUTING",
            "COPYING",
            "INSTALL",
            "README",
            "Readme",
            "readme",
        ];
        if file_component(path, |name| FILE_NAMES.contains(&name)) {
            return true;
        }
        // `(^|/)CHANGE(S|LOG)?(\.|$)` — CHANGE / CHANGES / CHANGELOG, optionally
        // with an extension.
        if file_component(path, |name| {
            let stem = name.split('.').next().unwrap_or(name);
            matches!(stem, "CHANGE" | "CHANGES" | "CHANGELOG")
        }) {
            return true;
        }
        // `(^|/)LICEN[CS]E(\.|$)` and `(^|/)[Ll]icen[cs]e(\.|$)`.
        file_component(path, |name| {
            let stem = name.split('.').next().unwrap_or(name);
            matches!(
                stem,
                "LICENCE" | "LICENSE" | "Licence" | "License" | "licence" | "license"
            )
        })
    }

    /// Tests `^(prefix)` for any of the given directory prefixes (each ending in
    /// `/`), matching enry's anchored `^[Xx].../` patterns.
    fn dir_prefix(path: &str, prefixes: &[&str]) -> bool {
        prefixes.iter().any(|p| path.starts_with(p))
    }

    /// Tests `(^|/)segment` for any of the given directory segments (each ending
    /// in `/`), matching enry's `(^|/)X/` patterns.
    fn dir_segment(path: &str, segments: &[&str]) -> bool {
        segments
            .iter()
            .any(|s| path.starts_with(s) || path.contains(&format!("/{s}")))
    }

    /// Tests `(^|/)NAME(\.|$)`: splits `path` into `/`-separated components and
    /// returns true if any component's leading token (up to the first `.`)
    /// satisfies `pred`. The `(\.|$)` means the component may carry a trailing
    /// extension.
    fn file_component(path: &str, pred: impl Fn(&str) -> bool) -> bool {
        path.split('/').any(|comp| {
            // Exactly NAME, or NAME followed by `.<ext>`.
            if pred(comp) {
                return true;
            }
            match comp.find('.') {
                Some(idx) => pred(&comp[..idx]),
                None => false,
            }
        })
    }
}

/// Port of the `pkg/pathfilter` generated-file checks (`IsGeneratedPath` /
/// `IsGeneratedContent`), mirroring the rules `cf-pathfilter` already vendors
/// (`DEFAULT_SUFFIXES`, `DEFAULT_FILENAME_PREFIXES`, `GENERATED_MARKERS`).
mod pathfilter {
    use super::base_clean;

    /// File suffixes indicating generated code. Mirrors `cf-pathfilter`'s
    /// `DEFAULT_SUFFIXES` (Go `defaultSuffixes`).
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
    /// code. Mirrors `cf-pathfilter`'s `DEFAULT_FILENAME_PREFIXES`.
    const DEFAULT_FILENAME_PREFIXES: &[&str] = &["zz_generated", "mock_", "fake_", "wire_gen"];

    /// Byte markers found near the top of generated files. Mirrors
    /// `cf-pathfilter`'s `GENERATED_MARKERS`.
    const GENERATED_MARKERS: &[&[u8]] = &[
        b"DO NOT EDIT",
        b"Code generated",
        b"AUTO-GENERATED",
        b"auto-generated",
        b"Autogenerated",
        b"@generated",
    ];

    /// How many header bytes to scan for a generated marker. Mirrors
    /// `cf-pathfilter`'s `GENERATED_MARKER_SCAN_LIMIT`.
    const GENERATED_MARKER_SCAN_LIMIT: usize = 512;

    /// `pathfilter.IsGeneratedPath` — the path's name matches a generated suffix
    /// or filename prefix.
    pub fn is_generated_path(path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        if DEFAULT_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            return true;
        }
        let base = base_clean(path);
        DEFAULT_FILENAME_PREFIXES.iter().any(|p| base.starts_with(p))
    }

    /// `pathfilter.IsGeneratedContent` — a generated marker appears within the
    /// first [`GENERATED_MARKER_SCAN_LIMIT`] bytes of `content`.
    pub fn is_generated_content(content: &[u8]) -> bool {
        let window = &content[..content.len().min(GENERATED_MARKER_SCAN_LIMIT)];
        GENERATED_MARKERS.iter().any(|m| contains_subslice(window, m))
    }

    /// Returns whether `haystack` contains `needle` as a contiguous subslice.
    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() {
            return true;
        }
        if needle.len() > haystack.len() {
            return false;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}
