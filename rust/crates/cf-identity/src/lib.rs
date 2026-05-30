//! Author/committer identity resolution and merging.
//!
//! Ported from the Go package `internal/identity`. On this branch the Go package
//! exposes two things:
//!
//! * a set of identity constants and fact/config key names (`keys.go`), and
//! * [`split_identity`], which parses an identity string into a name and email
//!   (`split.go`).
//!
//! Used by plumbing and the history analyzers (burndown, couples, devs,
//! file_history, sentiment).
//!
//! The Go package performs no machine-format report serialization (no JSON,
//! YAML, NDJSON, or `bin` output), so nothing here is routed through the shared
//! `cf-gojson` / `cf-goyaml` serialization crates — there are no report bytes to
//! make byte-identical. Behavior is otherwise reproduced exactly, including the
//! deliberately-not-`(1 << 18) - 1` value of [`AUTHOR_MISSING`].

#![forbid(unsafe_code)]

/// Bit shift used to compute [`AUTHOR_MISSING`].
///
/// Mirrors the Go unexported constant `authorMissingShift`.
const AUTHOR_MISSING_SHIFT: u32 = 18;

/// The internal author index which denotes any unmatched identities
/// (`Detector.Consume()` in Go).
///
/// It is deliberately `(1 << 18) - 2` and **not** `(1 << 18) - 1`; see the Go
/// comment referencing `BurndownAnalysis.packPersonWithDay()`. Reproducing this
/// exact value matters because the index is packed into burndown person/day
/// values downstream.
pub const AUTHOR_MISSING: i32 = (1 << AUTHOR_MISSING_SHIFT) - 2;

/// The string name which corresponds to [`AUTHOR_MISSING`].
///
/// Mirrors the Go constant `AuthorMissingName`.
pub const AUTHOR_MISSING_NAME: &str = "<unmatched>";

/// Name of the fact inserted in `Detector.Configure()` corresponding to
/// `Detector.PeopleDict` — the mapping from signatures to author indices.
///
/// Mirrors the Go constant `FactIdentityDetectorPeopleDict`.
pub const FACT_IDENTITY_DETECTOR_PEOPLE_DICT: &str = "IdentityDetector.PeopleDict";

/// Name of the fact inserted in `Detector.Configure()` corresponding to
/// `Detector.ReversedPeopleDict` — the mapping from author indices to the main
/// signature.
///
/// Mirrors the Go constant `FactIdentityDetectorReversedPeopleDict`.
pub const FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT: &str =
    "IdentityDetector.ReversedPeopleDict";

/// Name of the configuration option (`Detector.Configure()`) which allows
/// setting the external `PeopleDict` mapping from a file.
///
/// Mirrors the Go constant `ConfigIdentityDetectorPeopleDictPath`.
pub const CONFIG_IDENTITY_DETECTOR_PEOPLE_DICT_PATH: &str = "IdentityDetector.PeopleDictPath";

/// Name of the configuration option (`Detector.Configure()`) which changes the
/// matching algorithm to exact signature (name + email) correspondence.
///
/// Mirrors the Go constant `ConfigIdentityDetectorExactSignatures`.
pub const CONFIG_IDENTITY_DETECTOR_EXACT_SIGNATURES: &str = "IdentityDetector.ExactSignatures";

/// Name of the fact inserted in `Detector.Configure()` equal to the overall
/// number of unique authors (the length of `ReversedPeopleDict`).
///
/// Mirrors the Go constant `FactIdentityDetectorPeopleCount`.
pub const FACT_IDENTITY_DETECTOR_PEOPLE_COUNT: &str = "IdentityDetector.PeopleCount";

/// Name of the dependency provided by `Detector`.
///
/// Mirrors the Go constant `DependencyAuthor`.
pub const DEPENDENCY_AUTHOR: &str = "author";

/// Splits a pipe-delimited or exact-format identity string into a canonical name
/// and email.
///
/// Ported from Go `identity.SplitIdentity`. The three recognized forms are:
///
/// * **Exact format** — `"name <email>"` ⇒ trimmed `name` and the `email`
///   between `" <"` and a trailing `">"`.
/// * **Pipe-delimited format** — `"name1|name2|email1|email2"` ⇒ the first part
///   without an `@` becomes the name and the first part with an `@` becomes the
///   email (see [`split_pipe_identity`]).
/// * **Plain name** — `"name"` ⇒ `name` with an empty email.
///
/// The empty string maps to two empty strings.
///
/// The return tuple is `(name, email)`.
///
/// # Examples
///
/// ```
/// use cf_identity::split_identity;
///
/// assert_eq!(split_identity("daniel smith <dbsmith@google.com>"),
///            ("daniel smith".to_string(), "dbsmith@google.com".to_string()));
/// assert_eq!(split_identity("daniel smith|dbsmith@google.com"),
///            ("daniel smith".to_string(), "dbsmith@google.com".to_string()));
/// assert_eq!(split_identity("daniel smith"),
///            ("daniel smith".to_string(), String::new()));
/// ```
pub fn split_identity(s: &str) -> (String, String) {
    if s.is_empty() {
        return (String::new(), String::new());
    }

    // Exact format: "name <email>".
    //
    // Go uses `strings.Index(s, " <") > 0` (strictly positive, so the marker
    // must not be at byte offset 0) together with a trailing ">". Byte offsets
    // are used for the slice bounds, matching Go's byte-based string slicing.
    if let Some(idx) = s.find(" <") {
        if idx > 0 && s.ends_with('>') {
            let name = s[..idx].trim().to_string();
            // Skip the two bytes of " <" and drop the trailing ">".
            let email = s[idx + 2..s.len() - 1].to_string();
            return (name, email);
        }
    }

    // Pipe-delimited format.
    if s.contains('|') {
        return split_pipe_identity(s);
    }

    // Plain name, no email.
    (s.to_string(), String::new())
}

/// Splits a pipe-delimited identity string into a name and email.
///
/// Ported from Go `splitPipeIdentity`. Iterates the `|`-separated parts in order,
/// taking the first part **without** an `@` as the name and the first part
/// **with** an `@` as the email, stopping once both have been found.
///
/// The return tuple is `(name, email)`. Either component may be empty if no
/// matching part exists.
fn split_pipe_identity(s: &str) -> (String, String) {
    let mut name = String::new();
    let mut email = String::new();

    for part in s.split('|') {
        if name.is_empty() && !part.contains('@') {
            name = part.to_string();
        }

        if email.is_empty() && part.contains('@') {
            email = part.to_string();
        }

        if !name.is_empty() && !email.is_empty() {
            break;
        }
    }

    (name, email)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NAME: &str = "daniel smith";
    const TEST_EMAIL: &str = "dbsmith@google.com";

    // Ported from Go: TestSplitIdentity_PipeDelimited.
    #[test]
    fn split_identity_pipe_delimited() {
        let (name, email) = split_identity("daniel smith|dbsmith@google.com");
        assert_eq!(name, TEST_NAME);
        assert_eq!(email, TEST_EMAIL);
    }

    // Ported from Go: TestSplitIdentity_ExactFormat.
    #[test]
    fn split_identity_exact_format() {
        let (name, email) = split_identity("daniel smith <dbsmith@google.com>");
        assert_eq!(name, TEST_NAME);
        assert_eq!(email, TEST_EMAIL);
    }

    // Ported from Go: TestSplitIdentity_NameOnly.
    #[test]
    fn split_identity_name_only() {
        let (name, email) = split_identity("daniel smith");
        assert_eq!(name, TEST_NAME);
        assert!(email.is_empty());
    }

    // Ported from Go: TestSplitIdentity_Empty.
    #[test]
    fn split_identity_empty() {
        let (name, email) = split_identity("");
        assert!(name.is_empty());
        assert!(email.is_empty());
    }

    // Ported from Go: TestSplitIdentity_MultipleAliases.
    #[test]
    fn split_identity_multiple_aliases() {
        let (name, email) = split_identity("alice|bob|alice@example.com|bob@example.com");
        assert_eq!(name, "alice");
        assert_eq!(email, "alice@example.com");
    }

    // Ported from Go: TestSplitIdentity_UnmatchedAuthor.
    #[test]
    fn split_identity_unmatched_author() {
        let (name, email) = split_identity(AUTHOR_MISSING_NAME);
        assert_eq!(name, AUTHOR_MISSING_NAME);
        assert!(email.is_empty());
    }

    // --- Additional parity tests (behavior implied by the Go source) ---

    #[test]
    fn author_missing_is_not_one_less_than_pow() {
        // The Go comment warns this must NOT be (1 << 18) - 1.
        assert_eq!(AUTHOR_MISSING, (1 << 18) - 2);
        assert_eq!(AUTHOR_MISSING, 262_142);
        assert_ne!(AUTHOR_MISSING, (1 << 18) - 1);
    }

    #[test]
    fn constant_strings_match_go() {
        assert_eq!(AUTHOR_MISSING_NAME, "<unmatched>");
        assert_eq!(FACT_IDENTITY_DETECTOR_PEOPLE_DICT, "IdentityDetector.PeopleDict");
        assert_eq!(
            FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT,
            "IdentityDetector.ReversedPeopleDict"
        );
        assert_eq!(
            CONFIG_IDENTITY_DETECTOR_PEOPLE_DICT_PATH,
            "IdentityDetector.PeopleDictPath"
        );
        assert_eq!(
            CONFIG_IDENTITY_DETECTOR_EXACT_SIGNATURES,
            "IdentityDetector.ExactSignatures"
        );
        assert_eq!(
            FACT_IDENTITY_DETECTOR_PEOPLE_COUNT,
            "IdentityDetector.PeopleCount"
        );
        assert_eq!(DEPENDENCY_AUTHOR, "author");
    }

    #[test]
    fn exact_format_trims_name_only() {
        // Go trims the name with TrimSpace but takes the email verbatim.
        let (name, email) = split_identity("  spaced name   <  e@x.com  >");
        assert_eq!(name, "spaced name");
        assert_eq!(email, "  e@x.com  ");
    }

    #[test]
    fn exact_marker_at_offset_zero_is_not_exact_format() {
        // " <a@b>" has the " <" marker at index 0, so the strictly-positive
        // check fails; with no '|' it falls through to the plain-name branch.
        let (name, email) = split_identity(" <a@b>");
        assert_eq!(name, " <a@b>");
        assert!(email.is_empty());
    }

    #[test]
    fn exact_format_requires_trailing_angle() {
        // " <" present but no trailing '>' and no '|' -> plain name.
        let (name, email) = split_identity("name <notclosed");
        assert_eq!(name, "name <notclosed");
        assert!(email.is_empty());
    }

    #[test]
    fn pipe_email_first() {
        // Email part appears before any name part.
        let (name, email) = split_identity("a@example.com|bob");
        assert_eq!(name, "bob");
        assert_eq!(email, "a@example.com");
    }

    #[test]
    fn pipe_no_email_part() {
        let (name, email) = split_identity("alice|bob");
        assert_eq!(name, "alice");
        assert!(email.is_empty());
    }

    #[test]
    fn pipe_no_name_part() {
        let (name, email) = split_identity("a@x.com|b@y.com");
        assert_eq!(name, String::new());
        assert_eq!(email, "a@x.com");
    }

    #[test]
    fn split_pipe_identity_direct() {
        let (name, email) = split_pipe_identity("n1|n2|e1@x|e2@y");
        assert_eq!(name, "n1");
        assert_eq!(email, "e1@x");
    }
}
