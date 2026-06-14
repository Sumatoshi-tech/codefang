//! Error types for the checkpoint subsystem.
//!
//! Checkpoint validation distinguishes three sentinel conditions — repo path
//! mismatch, analyzer mismatch, and version mismatch — as dedicated variants of
//! [`CheckpointError`]. The `Display` text of the base sentinels is part of the
//! CLI compatibility contract (callers compare message prefixes, and the golden
//! CLI harness pins them); the validation variants additionally carry the
//! wrapped detail appended after `: `.

/// Result alias for checkpoint operations.
pub type Result<T> = std::result::Result<T, CheckpointError>;

/// Errors produced by the checkpoint subsystem.
///
/// The three `*Mismatch` variants are sentinel conditions and can be matched
/// directly:
///
/// ```
/// use cf_checkpoint::CheckpointError;
/// fn is_repo_mismatch(e: &CheckpointError) -> bool {
///     matches!(e, CheckpointError::RepoPathMismatch { .. })
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// The checkpoint was created for a different repository path.
    ///
    /// Sentinel "repo path mismatch". `want` is the path recorded in the
    /// checkpoint metadata, `got` the path the caller supplied. The quoted
    /// rendering (`{:?}`) matches the reference wording for ASCII paths.
    #[error("repo path mismatch: checkpoint has {want:?}, got {got:?}")]
    RepoPathMismatch {
        /// Repository path stored in the checkpoint.
        want: String,
        /// Repository path supplied by the caller.
        got: String,
    },

    /// The checkpoint was created for a different analyzer set.
    ///
    /// Sentinel "analyzer mismatch". The lists render space-separated in
    /// square brackets (see [`format_string_list`]).
    #[error(
        "analyzer mismatch: checkpoint has {}, got {}",
        format_string_list(.want),
        format_string_list(.got)
    )]
    AnalyzerMismatch {
        /// Analyzer list stored in the checkpoint.
        want: Vec<String>,
        /// Analyzer list supplied by the caller.
        got: Vec<String>,
    },

    /// The checkpoint metadata version does not match the current format.
    ///
    /// Sentinel "checkpoint version mismatch".
    #[error("checkpoint version mismatch: checkpoint has v{found}, current is v{current}")]
    VersionMismatch {
        /// Version recorded in the checkpoint.
        found: i64,
        /// Version the current build expects.
        current: i64,
    },

    /// An I/O error occurred (mkdir, open, read, write, rename, remove).
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// A serialization / deserialization error occurred in a [`crate::Codec`].
    #[error("{0}")]
    Codec(String),
}

impl CheckpointError {
    /// Returns the base sentinel message for the validation variants. Useful
    /// for prefix comparisons and for asserting which sentinel fired without
    /// inspecting the suffix.
    #[must_use]
    pub const fn sentinel_message(&self) -> Option<&'static str> {
        match self {
            Self::RepoPathMismatch { .. } => Some("repo path mismatch"),
            Self::AnalyzerMismatch { .. } => Some("analyzer mismatch"),
            Self::VersionMismatch { .. } => Some("checkpoint version mismatch"),
            _ => None,
        }
    }
}

/// Formats a string list as space-separated items wrapped in square brackets,
/// with no quoting of elements — e.g. `["a", "b"]` becomes `[a b]`. This exact
/// rendering is part of the error-message compatibility contract.
fn format_string_list(items: &[String]) -> String {
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
    fn analyzer_mismatch_renders_bracketed_list() {
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
    fn format_string_list_layout() {
        assert_eq!(format_string_list(&[]), "[]");
        assert_eq!(format_string_list(&["x".into()]), "[x]");
        assert_eq!(format_string_list(&["a".into(), "b".into()]), "[a b]");
    }
}
