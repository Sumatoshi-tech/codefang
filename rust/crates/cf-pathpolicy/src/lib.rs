//! Path inclusion policy for codefang analyzers.
//!
//! Decides whether a file path should be excluded from analysis based on
//! user-visible options that mirror the CLI flags (`--include-vendored`,
//! `--include-generated`, `--extra-excluded-prefixes`). Pure, stateless,
//! cross-phase. The decision selects which files feed machine reports, whose
//! bytes are pinned against the reference binary by `rust/tests/compat`.
//!
//! Exclusion-rule precedence (part of that contract):
//!
//! 1. an extra-excluded prefix match always excludes (even when the include
//!    flags below are set);
//! 2. otherwise, a vendored path is excluded unless `include_vendored`;
//! 3. otherwise, a generated path/content is excluded unless `include_generated`.
//!
//! # Classification boundary ([`Classifier`])
//!
//! The policy needs two classification collaborators:
//!
//! * vendor classification from the Linguist data tables
//!   (`cf_pathfilter::is_vendor`);
//! * the built-in generated-file heuristics (`cf_pathfilter::Filter`'s
//!   `is_generated_path` / `is_generated_content`).
//!
//! Those live in crates carrying data-parity-critical tables (DESIGN §2.6).
//! The policy is expressed against a small [`Classifier`] trait so the
//! exclusion *logic* can be tested in isolation; the concrete wiring to the
//! dependency crates lives behind the `default-deps` Cargo feature
//! ([`DefaultClassifier`] and the [`exclude`] convenience function).
//!
//! # Examples
//!
//! ```
//! use cf_pathpolicy::{exclude_with, Classifier, Options};
//!
//! // A classifier that knows only the corpus it is given.
//! struct Demo;
//! impl Classifier for Demo {
//!     fn is_vendor(&self, path: &str) -> bool {
//!         path.starts_with("vendor/")
//!     }
//!     fn is_generated_path(&self, path: &str) -> bool {
//!         path.ends_with(".pb.go")
//!     }
//!     fn is_generated_content(&self, _content: &[u8]) -> bool {
//!         false
//!     }
//! }
//!
//! let opts = Options::default();
//! assert!(exclude_with(&Demo, "vendor/x/y.go", None, &opts));
//! assert!(exclude_with(&Demo, "pkg/api/foo.pb.go", None, &opts));
//! assert!(!exclude_with(&Demo, "pkg/foo/bar.go", None, &opts));
//! ```

#![forbid(unsafe_code)]

/// User-visible configuration for the inclusion policy.
///
/// The default value (`Options::default()`) excludes vendor, generated, and
/// nothing else — the CLI's default behavior.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// When `true`, vendored paths are kept in analysis.
    pub include_vendored: bool,
    /// When `true`, generated paths/content are kept in analysis.
    pub include_generated: bool,
    /// Additional path prefixes whose matches are always excluded, regardless
    /// of the include flags above. Empty entries are ignored.
    pub extra_excluded_prefixes: Vec<String>,
}

/// Abstracts the two classification collaborators so the exclusion logic can
/// be tested without depending on the production data crates.
///
/// Production implementors must reproduce, respectively,
/// `cf_pathfilter::is_vendor`, `cf_pathfilter::Filter::is_generated_path`, and
/// `cf_pathfilter::Filter::is_generated_content`.
pub trait Classifier {
    /// Vendor / third-party classification (enry's Linguist-derived table).
    fn is_vendor(&self, path: &str) -> bool;
    /// Generated-file classification from the path alone.
    fn is_generated_path(&self, path: &str) -> bool;
    /// Generated-file classification from header content markers.
    fn is_generated_content(&self, content: &[u8]) -> bool;
}

/// Reports whether `path` should be skipped, using `classifier` for the
/// vendor/generated decisions.
///
/// `content` may be `None`; when provided, content-based heuristics may refine
/// the generated-file classification.
///
/// The short-circuit precedence and the non-empty-content / non-empty-prefix
/// guards are part of the compatibility contract (pinned by the differential
/// gate) — preserve them exactly.
#[must_use]
pub fn exclude_with<C: Classifier + ?Sized>(
    classifier: &C,
    path: &str,
    content: Option<&[u8]>,
    opts: &Options,
) -> bool {
    if matches_any_prefix(path, &opts.extra_excluded_prefixes) {
        return true;
    }

    if !opts.include_vendored && classifier.is_vendor(path) {
        return true;
    }

    if !opts.include_generated && is_generated(classifier, path, content) {
        return true;
    }

    false
}

/// Returns `true` if `path` begins with any non-empty entry of `prefixes`.
fn matches_any_prefix(path: &str, prefixes: &[String]) -> bool {
    prefixes
        .iter()
        .any(|prefix| !prefix.is_empty() && path.starts_with(prefix.as_str()))
}

/// Returns `true` if the `path` or header `content` identifies the file as
/// machine-generated per the classifier's heuristics.
///
/// A path match short-circuits; otherwise non-empty content is scanned for
/// generated markers.
fn is_generated<C: Classifier + ?Sized>(
    classifier: &C,
    path: &str,
    content: Option<&[u8]>,
) -> bool {
    if classifier.is_generated_path(path) {
        return true;
    }

    matches!(content, Some(c) if !c.is_empty() && classifier.is_generated_content(c))
}

#[cfg(feature = "default-deps")]
mod default_classifier {
    //! Concrete [`Classifier`] backed by the production data crates.

    use std::sync::LazyLock;

    use cf_pathfilter::{is_vendor, Filter};

    use super::{exclude_with, Classifier, Options};

    /// The built-in generated-file heuristics (filename suffixes, prefixes, and
    /// content markers) as they ship in [`cf_pathfilter`]. Reusing one
    /// immutable instance keeps allocation off the hot path.
    static DEFAULT_FILTER: LazyLock<Filter> = LazyLock::new(Filter::new);

    /// Production [`Classifier`]: vendor via [`cf_pathfilter::is_vendor`]
    /// (enry / Linguist data), generated via the shared [`DEFAULT_FILTER`].
    #[derive(Debug, Clone, Copy, Default)]
    pub struct DefaultClassifier;

    impl Classifier for DefaultClassifier {
        fn is_vendor(&self, path: &str) -> bool {
            is_vendor(path)
        }

        fn is_generated_path(&self, path: &str) -> bool {
            DEFAULT_FILTER.is_generated_path(path)
        }

        fn is_generated_content(&self, content: &[u8]) -> bool {
            DEFAULT_FILTER.is_generated_content(content)
        }
    }

    /// Reports whether `path` should be skipped under the production
    /// classifiers. This is the entry point analyzers call.
    #[must_use]
    pub fn exclude(path: &str, content: Option<&[u8]>, opts: &Options) -> bool {
        exclude_with(&DefaultClassifier, path, content, opts)
    }
}

#[cfg(feature = "default-deps")]
pub use default_classifier::{exclude, DefaultClassifier};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test classifier reproducing the exact decisions enry / pathfilter make
    /// for the paths exercised by the ported reference tests. This isolates
    /// the exclusion *logic* (the part this crate owns) from the data tables
    /// (owned by cf-langpath / cf-pathfilter, asserted by their own goldens).
    struct Fixture;

    impl Classifier for Fixture {
        fn is_vendor(&self, path: &str) -> bool {
            // Vendor matchers exercised by TestExclude_VendorPath_*.
            path.starts_with("vendor/")
                || path.starts_with("node_modules/")
                || path.starts_with("third_party/")
                || path.contains("/testdata/")
                || path.starts_with("testdata/")
                || path.ends_with(".min.js")
        }

        fn is_generated_path(&self, path: &str) -> bool {
            // Generated-path cases exercised by the reference tests.
            path.ends_with(".pb.go")
                || path.contains("zz_generated")
                || path.ends_with("_pb2.py")
                || path.starts_with("mock_")
                || path.contains("/mock_")
        }

        fn is_generated_content(&self, content: &[u8]) -> bool {
            // The "Code generated ... DO NOT EDIT" marker.
            let s = String::from_utf8_lossy(content);
            s.contains("Code generated") && s.contains("DO NOT EDIT")
        }
    }

    fn exclude(path: &str, content: Option<&[u8]>, opts: &Options) -> bool {
        exclude_with(&Fixture, path, content, opts)
    }

    // Ports reference test `TestExclude_PlainPath_Included`.
    #[test]
    fn plain_path_included() {
        let got = exclude("pkg/foo/bar.go", None, &Options::default());
        assert!(
            !got,
            "a non-vendor non-generated path must not be excluded under default options"
        );
    }

    // Ports reference test `TestExclude_VendorPath_ExcludedByDefault`.
    #[test]
    fn vendor_path_excluded_by_default() {
        let cases = [
            ("go vendor", "vendor/github.com/pkg/errors/errors.go"),
            ("node_modules", "node_modules/left-pad/index.js"),
            ("third-party", "third_party/boringssl/src.c"),
            ("testdata", "pkg/foo/testdata/sample.json"),
            ("minified js", "static/jquery.min.js"),
        ];
        for (name, path) in cases {
            let got = exclude(path, None, &Options::default());
            assert!(
                got,
                "Linguist-vendored path must be excluded under default options: {name}: {path}"
            );
        }
    }

    // Ports reference test `TestExclude_GeneratedPath_ExcludedByDefault`.
    #[test]
    fn generated_path_excluded_by_default() {
        let cases = [
            ("go protobuf", "pkg/api/foo.pb.go"),
            ("k8s zz_generated", "pkg/apis/core/v1/zz_generated_deepcopy.go"),
            ("python protobuf", "pkg/api/foo_pb2.py"),
            ("mockgen", "mocks/mock_service.go"),
        ];
        for (name, path) in cases {
            let got = exclude(path, None, &Options::default());
            assert!(
                got,
                "generated-looking path must be excluded under default options: {name}: {path}"
            );
        }
    }

    // Ports reference test `TestExclude_ExtraExcludedPrefixes_ExcludesMatches`.
    #[test]
    fn extra_excluded_prefixes_excludes_matches() {
        let opts = Options {
            extra_excluded_prefixes: vec![".venv/".to_string(), "docs/".to_string()],
            ..Options::default()
        };

        assert!(
            exclude(".venv/lib/foo.py", None, &opts),
            ".venv/ prefix must exclude python virtualenv content"
        );
        assert!(
            exclude("docs/README.md", None, &opts),
            "docs/ prefix must exclude documentation"
        );
        assert!(
            !exclude("pkg/foo.go", None, &opts),
            "a non-matching path must not be excluded"
        );
    }

    // Ports reference test `TestExclude_ExtraExcludedPrefixes_BypassIncludeOverrides`.
    #[test]
    fn extra_excluded_prefixes_bypass_include_overrides() {
        let opts = Options {
            include_vendored: true,
            include_generated: true,
            extra_excluded_prefixes: vec!["vendor/".to_string()],
        };

        assert!(
            exclude("vendor/foo.go", None, &opts),
            "extra_excluded_prefixes must still apply even when include flags are set"
        );
    }

    // Ports reference test `TestExclude_GeneratedContentMarker_ExcludedByDefault`.
    #[test]
    fn generated_content_marker_excluded_by_default() {
        let content = b"// Code generated by protoc-gen-go. DO NOT EDIT.\npackage foo\n";

        let got = exclude("pkg/foo/ordinary.go", Some(content), &Options::default());
        assert!(
            got,
            "content starting with a generated-file marker must be excluded under default options"
        );
    }

    // Ports reference test `TestExclude_IncludeGenerated_KeepsContentMarker`.
    #[test]
    fn include_generated_keeps_content_marker() {
        let content = b"// Code generated by protoc-gen-go. DO NOT EDIT.\npackage foo\n";
        let opts = Options {
            include_generated: true,
            ..Options::default()
        };

        let got = exclude("pkg/foo/ordinary.go", Some(content), &opts);
        assert!(
            !got,
            "include_generated=true must keep a generated-content file in analysis"
        );
    }

    // Ports reference test `TestExclude_IncludeGenerated_KeepsGenerated`.
    #[test]
    fn include_generated_keeps_generated() {
        let opts = Options {
            include_generated: true,
            ..Options::default()
        };

        let got = exclude("pkg/api/foo.pb.go", None, &opts);
        assert!(
            !got,
            "include_generated=true must keep generated paths in analysis"
        );
    }

    // Ports reference test `TestExclude_IncludeVendored_KeepsVendor`.
    #[test]
    fn include_vendored_keeps_vendor() {
        let opts = Options {
            include_vendored: true,
            ..Options::default()
        };

        let got = exclude("vendor/github.com/pkg/errors/errors.go", None, &opts);
        assert!(
            !got,
            "include_vendored=true must keep vendor paths in analysis"
        );
    }

    // matches_any_prefix ignores empty entries (the non-empty-prefix guard).
    #[test]
    fn empty_prefix_entries_are_ignored() {
        let opts = Options {
            extra_excluded_prefixes: vec![String::new()],
            ..Options::default()
        };
        assert!(
            !exclude("pkg/foo.go", None, &opts),
            "an empty prefix entry must never match"
        );
    }

    // is_generated must not treat empty content as a marker (the non-empty-content guard).
    #[test]
    fn empty_content_is_not_generated() {
        let got = exclude("pkg/foo/ordinary.go", Some(&[]), &Options::default());
        assert!(
            !got,
            "empty content must not trigger generated-content classification"
        );
    }
}
