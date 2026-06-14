//! Defensive file-reading and terminal-output utilities for user-supplied
//! paths and strings.
//!
//! Used by the `uast` library and the `uast` command to safely resolve and
//! read files named by untrusted input, and to sanitize arbitrary strings
//! before they are written to a terminal.
//!
//! # Behavior
//!
//! - [`resolve_path`] rejects empty/whitespace paths and paths containing a
//!   NUL byte, performs **lexical** path cleaning and absolutization (which
//!   deliberately does *not* resolve symlinks), then `stat`s the result and
//!   rejects directories. Crucially it does **not** call
//!   [`std::fs::canonicalize`], because that would resolve symlinks and
//!   change observable behavior.
//! - [`read_file`] resolves then reads, returning the content together with
//!   the resolved absolute path.
//! - [`sanitize_for_terminal`] applies HTML escaping and then replaces `\n`,
//!   `\r`, `\t` with spaces and drops all other control characters.
//!
//! Terminal output is a cosmetic (non-binding) machine target per the design,
//! but [`sanitize_for_terminal`] keeps its escaping byte-exact
//! (reference-implementation behavior) so callers comparing terminal output
//! stay aligned. The crate emits no machine-format report bytes, so it does
//! not route through the shared `cf-gojson` / `cf-goyaml` serialization
//! crates.

use std::path::{Path, PathBuf};

/// Errors that [`resolve_path`] and [`read_file`] can return.
///
/// The `Display` strings are part of the CLI/log compatibility contract and
/// must not change.
#[derive(Debug, thiserror::Error)]
pub enum IoSafetyError {
    /// The supplied path was empty or contained only whitespace.
    #[error("path is empty")]
    EmptyPath,
    /// The supplied path contained a NUL byte.
    ///
    /// The stored `String` is the offending path, rendered double-quoted with
    /// escapes in the `Display` output.
    #[error("path contains NUL byte: {}", quote(.0))]
    PathContainsNul(String),
    /// The resolved path pointed to a directory.
    ///
    /// The stored `String` is the resolved absolute path.
    #[error("path points to a directory: {0}")]
    DirectoryPath(String),
    /// Computing the absolute path failed.
    #[error("resolve absolute path for {}: {source}", quote(path))]
    ResolveAbsolute {
        /// The original (uncleaned) path argument.
        path: String,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// `stat`ing the resolved path failed (e.g. it does not exist).
    #[error("stat {path}: {source}")]
    Stat {
        /// The resolved absolute path that was `stat`ed.
        path: String,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// Reading the resolved file failed.
    #[error("read {path}: {source}")]
    Read {
        /// The resolved absolute path that was read.
        path: String,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

impl IoSafetyError {
    /// Returns `true` if this error is the empty-path variant.
    #[must_use]
    pub const fn is_empty_path(&self) -> bool {
        matches!(self, Self::EmptyPath)
    }

    /// Returns `true` if this error is the NUL-byte variant.
    #[must_use]
    pub const fn is_path_contains_nul(&self) -> bool {
        matches!(self, Self::PathContainsNul(_))
    }

    /// Returns `true` if this error is the directory variant.
    #[must_use]
    pub const fn is_directory_path(&self) -> bool {
        matches!(self, Self::DirectoryPath(_))
    }
}

/// Resolves, validates, and reads a user-supplied file path.
///
/// Returns the file content together with the resolved absolute path.
///
/// Delegates validation and resolution to [`resolve_path`] and then reads the
/// file. Resolution errors are returned as-is (the inner variant is preserved,
/// so [`IoSafetyError::is_empty_path`] etc. still report correctly).
///
/// # Errors
///
/// Returns [`IoSafetyError`] if the path is empty, contains a NUL byte, points
/// to a directory, cannot be resolved/`stat`ed, or cannot be read.
///
/// # Examples
///
/// ```
/// # use std::io::Write;
/// // Create a real file in a temp dir, then read it back.
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("greeting.txt");
/// std::fs::write(&path, b"hello").unwrap();
///
/// let (content, resolved) = cf_iosafety::read_file(&path).unwrap();
/// assert_eq!(content, b"hello");
/// // The returned path is absolute and lexically cleaned (symlinks untouched).
/// assert!(resolved.is_absolute());
/// ```
pub fn read_file<P: AsRef<Path>>(path: P) -> Result<(Vec<u8>, PathBuf), IoSafetyError> {
    let resolved = resolve_path(&path)?;

    match std::fs::read(&resolved) {
        Ok(content) => Ok((content, resolved)),
        Err(source) => Err(IoSafetyError::Read {
            path: resolved.to_string_lossy().into_owned(),
            source,
        }),
    }
}

/// Normalizes and validates a user-supplied file path.
///
/// Returns the absolute path after lexical cleaning, lexical absolutization, and
/// a `stat`-check. Returns an error for empty paths, NUL bytes, directories, or
/// `stat` failures.
///
/// The cleaning and absolutization are **purely lexical** — `a/b/../c`
/// collapses to `a/c` without touching the filesystem and symlinks are *not*
/// resolved. (Using [`std::fs::canonicalize`] here would resolve symlinks and
/// change observable behavior.)
///
/// # Errors
///
/// - [`IoSafetyError::EmptyPath`] if the path is empty or all-whitespace.
/// - [`IoSafetyError::PathContainsNul`] if the path contains a NUL byte.
/// - [`IoSafetyError::ResolveAbsolute`] if the current directory cannot be read.
/// - [`IoSafetyError::Stat`] if the resolved path cannot be `stat`ed.
/// - [`IoSafetyError::DirectoryPath`] if the resolved path is a directory.
///
/// # Examples
///
/// ```
/// use cf_iosafety::resolve_path;
///
/// // Empty/whitespace-only paths are rejected.
/// assert!(resolve_path("   ").unwrap_err().is_empty_path());
///
/// // NUL bytes are rejected.
/// assert!(resolve_path("a\0b").unwrap_err().is_path_contains_nul());
///
/// // A real file resolves to its absolute, lexically-cleaned path.
/// let dir = tempfile::tempdir().unwrap();
/// std::fs::write(dir.path().join("f.txt"), b"x").unwrap();
/// let resolved = resolve_path(dir.path().join("sub").join("..").join("f.txt")).unwrap();
/// assert!(resolved.is_absolute());
/// assert!(!resolved.to_string_lossy().contains(".."));
/// ```
pub fn resolve_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, IoSafetyError> {
    let raw = path.as_ref().to_string_lossy();

    if raw.trim().is_empty() {
        return Err(IoSafetyError::EmptyPath);
    }

    if raw.contains('\0') {
        return Err(IoSafetyError::PathContainsNul(raw.into_owned()));
    }

    let clean_path = clean(&raw);

    let abs_path = abs(&clean_path).map_err(|source| IoSafetyError::ResolveAbsolute {
        path: raw.into_owned(),
        source,
    })?;

    let info = std::fs::metadata(&abs_path).map_err(|source| IoSafetyError::Stat {
        path: abs_path.clone(),
        source,
    })?;

    if info.is_dir() {
        return Err(IoSafetyError::DirectoryPath(abs_path));
    }

    Ok(PathBuf::from(abs_path))
}

/// Strips control characters and HTML-escapes the input.
///
/// Newlines (`\n`), carriage returns (`\r`), and tabs (`\t`) are replaced
/// with a single space; all other control characters are removed. HTML
/// escaping is applied first.
///
/// Because HTML escaping runs before the control-character pass and only ever
/// emits ASCII letters, digits, and the punctuation `&#;`, the two passes do
/// not interact destructively: an HTML-escaped string never contains control
/// characters introduced by escaping, so the control pass only affects
/// characters that were already present in the input.
///
/// # Examples
///
/// ```
/// let out = cf_iosafety::sanitize_for_terminal("<b>hi</b>\tthere\n");
/// assert_eq!(out, "&lt;b&gt;hi&lt;/b&gt; there ");
/// ```
#[must_use]
pub fn sanitize_for_terminal(input: &str) -> String {
    let escaped = html_escape_string(input);

    let mut out = String::with_capacity(escaped.len());
    for ch in escaped.chars() {
        match ch {
            '\n' | '\r' | '\t' => out.push(' '),
            c if is_control_cc(c) => {} // dropped
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Internal helpers (frozen escaping/cleaning semantics)
// ---------------------------------------------------------------------------

/// Minimal HTML escaping (frozen contract).
///
/// Escapes exactly five characters: `&` → `&amp;`, `'` → `&#39;`, `<` →
/// `&lt;`, `>` → `&gt;`, `"` → `&#34;`. Every literal `&` becomes `&amp;`, so
/// already-escaped entities get double-escaped — this is deliberate.
fn html_escape_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            c => out.push(c),
        }
    }
    out
}

/// Reports whether `r` is a control character (Unicode `Cc` category, i.e.
/// `U+0000..=U+001F` and `U+007F..=U+009F`).
fn is_control_cc(r: char) -> bool {
    let c = r as u32;
    c <= 0x1F || (0x7F..=0x9F).contains(&c)
}

/// Renders a string double-quoted with escapes (frozen error-message
/// formatting, e.g. `path contains NUL byte: "file\x00name"`). Covers the
/// escapes that appear in the iosafety error paths (notably NUL → `\x00`).
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0007}' => out.push_str("\\a"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{000B}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7F => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Lexically cleans a path for the Unix separator (`/`).
///
/// Implements the classic Lexical File Name Simplification: repeatedly collapse
/// multiple slashes into one, eliminate `.` path elements, and eliminate `..`
/// elements together with the non-`..` element that precedes them, while never
/// letting `..` ascend past the root of a rooted path. The empty result is
/// normalized to `.`.
fn clean(path: &str) -> String {
    const SEP: u8 = b'/';

    if path.is_empty() {
        return ".".to_string();
    }

    let bytes = path.as_bytes();
    let rooted = bytes[0] == SEP;
    let n = bytes.len();

    // `out` is a growable buffer; `dotdot` is the index in `out` beyond which
    // `..` cannot back up.
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut dotdot = 0usize;

    if rooted {
        out.push(SEP);
        dotdot = 1;
    }

    let mut r = 0usize;
    while r < n {
        if bytes[r] == SEP {
            // empty path element
            r += 1;
        } else if bytes[r] == b'.' && (r + 1 == n || bytes[r + 1] == SEP) {
            // "." element
            r += 1;
        } else if bytes[r] == b'.'
            && bytes[r + 1] == b'.'
            && (r + 2 == n || bytes[r + 2] == SEP)
        {
            // ".." element: back up if possible
            r += 2;
            if out.len() > dotdot {
                // can backtrack
                let mut w = out.len() - 1;
                while w > dotdot && out[w] != SEP {
                    w -= 1;
                }
                out.truncate(w);
            } else if !rooted {
                // cannot backtrack but not rooted, so append ".."
                if !out.is_empty() {
                    out.push(SEP);
                }
                out.push(b'.');
                out.push(b'.');
                dotdot = out.len();
            }
        } else {
            // real path element; add slash if needed
            if (rooted && out.len() != 1) || (!rooted && !out.is_empty()) {
                out.push(SEP);
            }
            // copy element
            while r < n && bytes[r] != SEP {
                out.push(bytes[r]);
                r += 1;
            }
        }
    }

    if out.is_empty() {
        return ".".to_string();
    }

    // `out` is built from valid UTF-8 input split on ASCII `/`, so it is valid.
    String::from_utf8(out).unwrap_or_else(|_| ".".to_string())
}

/// Lexically absolutizes a path.
///
/// If the path is not already absolute, the current working directory is
/// prepended (via `current_dir`) and the join is lexically [`clean`]ed. If the
/// path is already absolute it is simply cleaned. No filesystem traversal or
/// symlink resolution occurs.
fn abs(path: &str) -> Result<String, std::io::Error> {
    if Path::new(path).is_absolute() {
        return Ok(clean(path));
    }

    let wd = std::env::current_dir()?;
    let wd = wd.to_string_lossy();
    let joined = format!("{wd}/{path}");
    Ok(clean(&joined))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    // -- resolve_path ------------------------------------------------------

    #[test]
    fn resolve_path_empty_path() {
        let err = resolve_path("").unwrap_err();
        assert!(err.is_empty_path(), "expected EmptyPath, got {err:?}");
    }

    #[test]
    fn resolve_path_whitespace_only_path() {
        let err = resolve_path("   ").unwrap_err();
        assert!(err.is_empty_path(), "expected EmptyPath, got {err:?}");
    }

    #[test]
    fn resolve_path_nul_byte() {
        let err = resolve_path("file\0name").unwrap_err();
        assert!(
            err.is_path_contains_nul(),
            "expected PathContainsNul, got {err:?}"
        );
    }

    #[test]
    fn resolve_path_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_path(dir.path()).unwrap_err();
        assert!(
            err.is_directory_path(),
            "expected DirectoryPath, got {err:?}"
        );
    }

    #[test]
    fn resolve_path_nonexistent_file() {
        let err = resolve_path("/nonexistent/path/file.txt").unwrap_err();
        assert!(
            !err.is_empty_path() && !err.is_directory_path() && !err.is_path_contains_nul(),
            "expected a stat error, got {err:?}"
        );
    }

    #[test]
    fn resolve_path_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, b"hello").unwrap();

        let resolved = resolve_path(&path).unwrap();
        assert!(resolved.is_absolute());
    }

    #[test]
    fn resolve_path_returns_clean_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, b"hello").unwrap();

        // Pass a path with ".." components that must be lexically cleaned.
        let dirty = dir.path().join("subdir").join("..").join("test.txt");
        let resolved = resolve_path(&dirty).unwrap();

        // The lexical clean must be idempotent and the ".." must have
        // collapsed "subdir/..".
        let resolved_str = resolved.to_string_lossy();
        assert_eq!(clean(&resolved_str), resolved_str.as_ref());
        assert!(!resolved_str.contains(".."));
        assert!(resolved.is_absolute());
    }

    // -- read_file ---------------------------------------------------------

    #[test]
    fn read_file_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        let expected = b"file content";
        fs::write(&path, expected).unwrap();

        let (content, resolved) = read_file(&path).unwrap();
        assert_eq!(content, expected);
        assert!(resolved.is_absolute());
    }

    #[test]
    fn read_file_empty_path() {
        let err = read_file("").unwrap_err();
        assert!(err.is_empty_path(), "expected EmptyPath, got {err:?}");
    }

    #[test]
    fn read_file_nonexistent_file() {
        let err = read_file("/no/such/file.txt").unwrap_err();
        assert!(
            !err.is_empty_path() && !err.is_directory_path(),
            "expected a stat error, got {err:?}"
        );
    }

    #[test]
    fn read_file_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_file(dir.path()).unwrap_err();
        assert!(
            err.is_directory_path(),
            "expected DirectoryPath, got {err:?}"
        );
    }

    // -- sanitize_for_terminal --------------------------------------------

    #[test]
    fn sanitize_for_terminal_plain_text() {
        assert_eq!(sanitize_for_terminal("hello world"), "hello world");
    }

    #[test]
    fn sanitize_for_terminal_html_escaping() {
        let got = sanitize_for_terminal("<script>alert('xss')</script>");
        assert!(got.contains("&lt;script&gt;"), "got: {got}");
        assert!(!got.contains("<script>"), "got: {got}");
    }

    #[test]
    fn sanitize_for_terminal_control_characters() {
        let got = sanitize_for_terminal("hello\0world\u{0007}bell");
        assert!(!got.contains('\0'), "got: {got:?}");
        assert!(!got.contains('\u{0007}'), "got: {got:?}");
        assert!(got.contains("hello"), "got: {got}");
        assert!(got.contains("world"), "got: {got}");
    }

    #[test]
    fn sanitize_for_terminal_whitespace_replacement() {
        let got = sanitize_for_terminal("line1\nline2\ttab\rcarriage");
        assert!(!got.contains('\n'));
        assert!(!got.contains('\t'));
        assert!(!got.contains('\r'));
        assert!(got.contains("line1 line2 tab carriage"), "got: {got}");
    }

    #[test]
    fn sanitize_for_terminal_empty_string() {
        assert!(sanitize_for_terminal("").is_empty());
    }

    // -- exact-output regression tests (byte-level parity contract) -------

    #[test]
    fn html_escape_exact() {
        // The escaper handles exactly these five characters.
        assert_eq!(html_escape_string("&"), "&amp;");
        assert_eq!(html_escape_string("'"), "&#39;");
        assert_eq!(html_escape_string("<"), "&lt;");
        assert_eq!(html_escape_string(">"), "&gt;");
        assert_eq!(html_escape_string("\""), "&#34;");
        assert_eq!(
            html_escape_string("a&b<c>d'e\"f"),
            "a&amp;b&lt;c&gt;d&#39;e&#34;f"
        );
    }

    #[test]
    fn sanitize_exact_output() {
        // Full pipeline: escape then control-strip.
        assert_eq!(
            sanitize_for_terminal("<b>hi</b>\tthere\n"),
            "&lt;b&gt;hi&lt;/b&gt; there "
        );
    }

    // -- lexical clean parity ----------------------------------------------

    #[test]
    fn clean_lexical_simplification_table() {
        // Cases lifted from the reference implementation's lexical-clean test
        // table (Unix subset).
        assert_eq!(clean(""), ".");
        assert_eq!(clean("abc"), "abc");
        assert_eq!(clean("abc/def"), "abc/def");
        assert_eq!(clean("a/b/c"), "a/b/c");
        assert_eq!(clean("."), ".");
        assert_eq!(clean(".."), "..");
        assert_eq!(clean("../.."), "../..");
        assert_eq!(clean("../../abc"), "../../abc");
        assert_eq!(clean("/abc"), "/abc");
        assert_eq!(clean("/"), "/");
        assert_eq!(clean("abc/"), "abc");
        assert_eq!(clean("abc//def//ghi"), "abc/def/ghi");
        assert_eq!(clean("//abc"), "/abc");
        assert_eq!(clean("///abc"), "/abc");
        assert_eq!(clean("abc/./def"), "abc/def");
        assert_eq!(clean("/./abc/def"), "/abc/def");
        assert_eq!(clean("abc/.."), ".");
        assert_eq!(clean("abc/def/.."), "abc");
        assert_eq!(clean("abc/def/../.."), ".");
        assert_eq!(clean("/abc/def/../.."), "/");
        assert_eq!(clean("abc/def/../../.."), "..");
        assert_eq!(clean("/abc/def/../../.."), "/");
        assert_eq!(clean("abc/def/../../../ghi/jkl/../../../mno"), "../../mno");
        assert_eq!(clean("/../abc"), "/abc");
        assert_eq!(clean("a/b/c/.."), "a/b");
        assert_eq!(clean("a/b/../c/../d"), "a/d");
        assert_eq!(clean("subdir/../test.txt"), "test.txt");
    }

    #[test]
    fn abs_is_lexical_for_absolute_inputs() {
        // For an already-absolute path, abs == clean and no symlink resolution.
        assert_eq!(abs("/a/b/../c").unwrap(), "/a/c");
        let p = abs("/tmp/x/./y").unwrap();
        assert_eq!(p, "/tmp/x/y");
        assert!(Path::new(&p).is_absolute());
    }

    #[test]
    fn quote_nul() {
        assert_eq!(quote("file\0name"), "\"file\\x00name\"");
    }
}
