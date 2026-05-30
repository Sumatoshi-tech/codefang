//! Error types for the checkpoint subsystem.
//!
//! The Go package exposes three sentinel errors used with `errors.Is` for
//! checkpoint validation (`manager.go`):
//!
//! ```go
//! var (
//!     ErrRepoPathMismatch = errors.New("repo path mismatch")
//!     ErrAnalyzerMismatch = errors.New("analyzer mismatch")
//!     ErrVersionMismatch  = errors.New("checkpoint version mismatch")
//! )
//! ```
//!
//! In Rust these become dedicated variants of [`CheckpointError`]. The
//! `Display` text of the base sentinels matches the Go strings byte-for-byte so
//! that callers comparing message prefixes (and the golden CLI harness) stay
//! aligned; the validation variants additionally carry the wrapped detail Go
//! appends via `fmt.Errorf("%w: ...")`.

use std::fmt;

/// Result alias for checkpoint operations.
pub type Result<T> = std::result::Result<T, CheckpointError>;

/// Errors produced by the checkpoint subsystem.
///
/// The three `*Mismatch` variants correspond to the Go sentinel errors and can
/// be matched directly (the Rust equivalent of `errors.Is`):
///
/// ```
/// use cf_checkpoint::CheckpointError;
/// fn is_repo_mismatch(e: &CheckpointError) -> bool {
///     matches!(e, CheckpointError::RepoPathMismatch { .. })
/// }
/// ```
#[derive(Debug)]
pub enum CheckpointError {
    /// The checkpoint was created for a different repository path.
    ///
    /// Mirrors `ErrRepoPathMismatch` ("repo path mismatch"). `want` is the path
    /// recorded in the checkpoint metadata, `got` the path the caller supplied,
    /// reproducing Go's `%w: checkpoint has %q, got %q`.
    RepoPathMismatch {
        /// Repository path stored in the checkpoint.
        want: String,
        /// Repository path supplied by the caller.
        got: String,
    },

    /// The checkpoint was created for a different analyzer set.
    ///
    /// Mirrors `ErrAnalyzerMismatch` ("analyzer mismatch"), reproducing Go's
    /// `%w: checkpoint has %v, got %v`.
    AnalyzerMismatch {
        /// Analyzer list stored in the checkpoint.
        want: Vec<String>,
        /// Analyzer list supplied by the caller.
        got: Vec<String>,
    },

    /// The checkpoint metadata version does not match the current format.
    ///
    /// Mirrors `ErrVersionMismatch` ("checkpoint version mismatch"),
    /// reproducing Go's `%w: checkpoint has v%d, current is v%d`.
    VersionMismatch {
        /// Version recorded in the checkpoint.
        found: i64,
        /// Version the current build expects.
        current: i64,
    },

    /// An I/O error occurred (mkdir, open, read, write, rename, remove).
    Io(std::io::Error),

    /// A serialization / deserialization error occurred in a [`crate::Codec`].
    Codec(String),
}

impl CheckpointError {
    /// Returns the base sentinel message for the validation variants, matching
    /// the Go `errors.New(...)` strings exactly. Useful for prefix comparisons
    /// and for asserting which sentinel fired without inspecting the suffix.
    pub fn sentinel_message(&self) -> Option<&'static str> {
        match self {
            CheckpointError::RepoPathMismatch { .. } => Some("repo path mismatch"),
            CheckpointError::AnalyzerMismatch { .. } => Some("analyzer mismatch"),
            CheckpointError::VersionMismatch { .. } => Some("checkpoint version mismatch"),
            _ => None,
        }
    }
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Go: fmt.Errorf("%w: checkpoint has %q, got %q", ErrRepoPathMismatch, want, got)
            // Go's %q quotes Go-style; for ASCII paths this matches Rust's {:?}.
            CheckpointError::RepoPathMismatch { want, got } => write!(
                f,
                "repo path mismatch: checkpoint has {want:?}, got {got:?}"
            ),
            // Go: fmt.Errorf("%w: checkpoint has %v, got %v", ErrAnalyzerMismatch, want, got)
            CheckpointError::AnalyzerMismatch { want, got } => write!(
                f,
                "analyzer mismatch: checkpoint has {}, got {}",
                format_go_slice(want),
                format_go_slice(got)
            ),
            // Go: fmt.Errorf("%w: checkpoint has v%d, current is v%d", ErrVersionMismatch, found, current)
            CheckpointError::VersionMismatch { found, current } => write!(
                f,
                "checkpoint version mismatch: checkpoint has v{found}, current is v{current}"
            ),
            CheckpointError::Io(e) => write!(f, "{e}"),
            CheckpointError::Codec(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CheckpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CheckpointError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CheckpointError {
    fn from(e: std::io::Error) -> Self {
        CheckpointError::Io(e)
    }
}

/// Formats a string slice the way Go's `%v` renders a `[]string`:
/// space-separated, wrapped in square brackets, with no quoting of elements.
/// For example `["a", "b"]` becomes `[a b]`.
fn format_go_slice(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(s);
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_mismatch_sentinel() {
        let e = CheckpointError::RepoPathMismatch {
            want: "/a".to_string(),
            got: "/b".to_string(),
        };
        assert_eq!(e.sentinel_message(), Some("repo path mismatch"));
        assert!(e.to_string().starts_with("repo path mismatch:"));
    }

    #[test]
    fn analyzer_mismatch_renders_go_slice() {
        let e = CheckpointError::AnalyzerMismatch {
            want: vec!["burndown".into()],
            got: vec!["devs".into()],
        };
        assert_eq!(e.sentinel_message(), Some("analyzer mismatch"));
        assert_eq!(
            e.to_string(),
            "analyzer mismatch: checkpoint has [burndown], got [devs]"
        );
    }

    #[test]
    fn version_mismatch_message() {
        let e = CheckpointError::VersionMismatch {
            found: 1,
            current: 2,
        };
        assert_eq!(e.sentinel_message(), Some("checkpoint version mismatch"));
        assert_eq!(
            e.to_string(),
            "checkpoint version mismatch: checkpoint has v1, current is v2"
        );
    }

    #[test]
    fn format_go_slice_matches_go_v_verb() {
        assert_eq!(format_go_slice(&[]), "[]");
        assert_eq!(format_go_slice(&["x".into()]), "[x]");
        assert_eq!(format_go_slice(&["a".into(), "b".into()]), "[a b]");
    }
}
