//! File classifier.
//!
//! [`Classifier::classify`] is a fixed-priority cascade over enry-style
//! predicates plus generated-file checks:
//!
//! ```text
//! // first match wins:
//! if content is non-empty and looks binary:        Binary
//! if the path has an image extension:              Image
//! if the path matches a vendoring convention:      Vendor
//! if the path matches a generated-file rule:       Generated
//! if non-empty content carries a generated marker: Generated
//! if the path matches a documentation convention:  Documentation
//! if the extension maps to a config language:      Configuration
//! if the base name is a dotfile:                   DotFile
//! else:                                            Source
//! ```
//!
//! The branch ORDER is load-bearing (a file matching several predicates is
//! classified by the first matching branch) and is part of the pinned
//! classification behaviour — preserve the exact sequence.
//!
//! # Data parity
//!
//! The `is_vendor` / `is_documentation` predicates are driven by generated
//! regex tables from `src-d/enry` v2.1.0 (extracted from github/linguist), and
//! the generated-file rules match the ones `cf-pathfilter` vendors. For full
//! byte-identity on a real corpus those tables should be sourced from the
//! workspace's already-vendored copies — `cf-pathfilter` (enry vendor regexes +
//! generated rules) and `cf-langpath` (the enry v2.1.0 extension table). The
//! predicates below reproduce enry's algorithms and the high-frequency data
//! subset exercised by the unit tests; widening the embedded tables changes
//! only the data slices these functions consult, not their structure.
//!
//! The fully-data-exact predicates here are the ones enry implements *without*
//! a data table: [`enry::is_image`] (4 hard-coded extensions),
//! [`enry::is_dotfile`] (dot-prefixed cleaned base name), [`enry::is_binary`]
//! (8000-byte NUL sniff), and [`enry::is_configuration`] (extension -> one of
//! `{INI, JSON, TOML, YAML, XML, SQL}`).

use crate::category::Category;

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

    /// Determines the category of a file from its path and content.
    ///
    /// The cascade runs branch-for-branch in the pinned order, including the
    /// non-empty-content guards on the binary and generated-content checks (so
    /// empty content never triggers Binary or content-Generated).
    ///
    /// ```
    /// use cf_composition::{Classifier, Category};
    ///
    /// let c = Classifier::new();
    /// assert_eq!(c.classify("pkg/main.go", b"package main\n"), Category::Source);
    /// assert_eq!(c.classify("vendor/foo/bar.go", b"package bar\n"), Category::Vendor);
    /// assert_eq!(c.classify("docs/README.md", b"# Hi\n"), Category::Documentation);
    /// assert_eq!(c.classify("logo.png", &[]), Category::Image);
    /// // The binary check requires non-empty content.
    /// assert_eq!(c.classify("data.bin", &[0, 1, 2, 0xFF, 0, 0]), Category::Binary);
    /// ```
    #[must_use]
    // Two arms both yield `Category::Generated` (generated-by-path then
    // generated-by-content), which clippy flags but is load-bearing for the
    // pinned branch order and readability.
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

/// Returns the final path element of `path` after cleaning (trailing-slash
/// trimming), for the `/`-separated inputs this classifier sees on its Linux
/// golden target. An empty path yields `"."`; an all-slash path yields `"/"`.
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

/// Returns the file extension including the leading dot, **case-preserving**
/// (the suffix from the final dot in the base name). Empty when the base name
/// has no dot. Use this where the predicate compares the raw extension (e.g.
/// `enry::is_image`'s case-sensitive check), so an upper-case `.GIF` is not
/// misclassified the way a lower-cased compare would.
fn ext(path: &str) -> String {
    let base = base_clean(path);
    match base.rfind('.') {
        Some(idx) => base[idx..].to_string(),
        None => String::new(),
    }
}

/// The subset of `src-d/enry` v2.1.0 predicates used by the classifier.
mod enry {
    use super::base_clean;

    /// Content is detected as binary.
    ///
    /// Exact rule: read up to 8000 bytes and return true iff a NUL byte appears
    /// in that window (git's binary detection).
    pub fn is_binary(content: &[u8]) -> bool {
        const SNIFF_LEN: usize = 8000;
        let window = &content[..content.len().min(SNIFF_LEN)];
        window.contains(&0u8)
    }

    /// Path has an image extension: exactly `.png`, `.jpg`, `.jpeg`, or `.gif`.
    ///
    /// The raw extension is compared, so the match is case-sensitive: an
    /// upper-case `.GIF` (e.g. ioq3's `RADIANT3.GIF`) is NOT an image and falls
    /// through to `source`, which is what the reference binary reports.
    pub fn is_image(path: &str) -> bool {
        matches!(super::ext(path).as_str(), ".png" | ".jpg" | ".jpeg" | ".gif")
    }

    /// The cleaned base name starts with `.` and is not exactly `.`.
    pub fn is_dotfile(path: &str) -> bool {
        let base = base_clean(path);
        base.starts_with('.') && base != "."
    }

    /// The path's extension-only language is a configuration language.
    ///
    /// Resolves the language via enry's real extension table (vendored in
    /// `cf-langpath`) through [`cf_langpath::language_by_extension`], then
    /// tests membership in the exact configuration-language set
    /// `{XML, JSON, TOML, YAML, INI, SQL}`. This replaced an earlier
    /// hand-curated extension list, which both missed entries (`SQL`, and the
    /// long XML-family tail) and added wrong ones, so files now bucket exactly
    /// as the reference binary does.
    pub fn is_configuration(path: &str) -> bool {
        const CONFIGURATION_LANGUAGES: [&str; 6] =
            ["XML", "JSON", "TOML", "YAML", "INI", "SQL"];
        match cf_langpath::language_by_extension(path) {
            Some(lang) => CONFIGURATION_LANGUAGES.contains(&lang.as_str()),
            None => false,
        }
    }

    /// Path matches a vendoring convention.
    ///
    /// enry tests the path against its compiled vendor regex table. The
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

    /// Path matches a documentation convention — an exact reproduction of
    /// enry's documentation matchers.
    ///
    /// The matcher is an OR of unanchored regex patterns (verbatim from
    /// `src-d/enry/v2@v2.1.0`, extracted from github/linguist):
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
    /// These are PATH-only (no prose extensions such as `.md`/`.rst` — an
    /// earlier version wrongly added those, over-classifying source files as
    /// documentation). The patterns are reproduced exactly below without a
    /// regex dependency.
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
        // Filename matchers: `(^|/)NAME(\.|$)` — NAME anchored at a component
        // start, followed by a literal `.` or by the END OF THE PATH. The pred
        // receives the exact candidate name (no extension); `file_component`
        // owns the `(\.|$)` anchoring.
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
        // `(^|/)CHANGE(S|LOG)?(\.|$)` — CHANGE / CHANGES / CHANGELOG.
        if file_component(path, |name| {
            matches!(name, "CHANGE" | "CHANGES" | "CHANGELOG")
        }) {
            return true;
        }
        // `(^|/)LICEN[CS]E(\.|$)` and `(^|/)[Ll]icen[cs]e(\.|$)`.
        file_component(path, |name| {
            matches!(
                name,
                "LICENCE" | "LICENSE" | "Licence" | "License" | "licence" | "license"
            )
        })
    }

    /// Tests `^(prefix)` for any of the given directory prefixes (each ending in
    /// `/`), matching the anchored `^[Xx].../` patterns.
    fn dir_prefix(path: &str, prefixes: &[&str]) -> bool {
        prefixes.iter().any(|p| path.starts_with(p))
    }

    /// Tests `(^|/)segment` for any of the given directory segments (each ending
    /// in `/`), matching the `(^|/)X/` patterns.
    fn dir_segment(path: &str, segments: &[&str]) -> bool {
        segments
            .iter()
            .any(|s| path.starts_with(s) || path.contains(&format!("/{s}")))
    }

    /// Tests `(^|/)NAME(\.|$)` exactly as the regex does: NAME anchored at a
    /// component start, immediately followed by a literal `.` (any component,
    /// even a directory like `NAME.d/`) **or by the end of the whole path** (the
    /// `$` — i.e. the final component equals NAME exactly).
    ///
    /// A bare directory component equal to NAME does NOT match: in the path
    /// string it is followed by `/`, which `(\.|$)` does not allow. (This was
    /// the kubernetes `CHANGELOG/CHANGELOG-1.x.md` divergence: enry classifies
    /// those as source — the `CHANGELOG` *directory* never matches — while a
    /// component-equality check wrongly made them documentation.)
    fn file_component(path: &str, pred: impl Fn(&str) -> bool) -> bool {
        let mut comps = path.split('/').peekable();
        while let Some(comp) = comps.next() {
            // `NAME$`: only the final component, exactly equal.
            if comps.peek().is_none() && pred(comp) {
                return true;
            }
            // `NAME.`: the component's prefix up to its first dot.
            if let Some(idx) = comp.find('.') {
                if pred(&comp[..idx]) {
                    return true;
                }
            }
        }
        false
    }
}

/// Generated-file checks, mirroring the rules `cf-pathfilter` vendors
/// (`DEFAULT_SUFFIXES`, `DEFAULT_FILENAME_PREFIXES`, `GENERATED_MARKERS`).
mod pathfilter {
    use super::base_clean;

    /// File suffixes indicating generated code (matches `cf-pathfilter`'s
    /// `DEFAULT_SUFFIXES`).
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
    /// code (matches `cf-pathfilter`'s `DEFAULT_FILENAME_PREFIXES`).
    const DEFAULT_FILENAME_PREFIXES: &[&str] = &["zz_generated", "mock_", "fake_", "wire_gen"];

    /// Byte markers found near the top of generated files (matches
    /// `cf-pathfilter`'s `GENERATED_MARKERS`).
    const GENERATED_MARKERS: &[&[u8]] = &[
        b"DO NOT EDIT",
        b"Code generated",
        b"AUTO-GENERATED",
        b"auto-generated",
        b"Autogenerated",
        b"@generated",
    ];

    /// How many header bytes to scan for a generated marker (matches
    /// `cf-pathfilter`'s `GENERATED_MARKER_SCAN_LIMIT`).
    const GENERATED_MARKER_SCAN_LIMIT: usize = 512;

    /// The path's name matches a generated suffix or filename prefix.
    pub fn is_generated_path(path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        if DEFAULT_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            return true;
        }
        let base = base_clean(path);
        DEFAULT_FILENAME_PREFIXES.iter().any(|p| base.starts_with(p))
    }

    /// A generated marker appears within the first
    /// [`GENERATED_MARKER_SCAN_LIMIT`] bytes of `content`.
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
