//! Error types for the gitlib layer.
//!
//! [`GitError`] is a flat set of sentinel errors plus `"<context>: <cause>"`
//! wrappers over libgit2 errors. Every `Display` string is part of the CLI
//! error-message contract and must stay byte-identical (pinned by the
//! differential gate in `rust/tests/compat`).

use std::fmt;

/// A cloneable wrapper around a libgit2 [`git2::Error`].
///
/// `git2::Error` is not `Clone`, but several gitlib result types (e.g.
/// [`crate::worker::BlobResult`]) carry an error by value and need to be
/// `Clone`. `GitCause` captures the libgit2 error's rendered message and
/// reproduces its [`std::fmt::Display`] **byte-for-byte** (message, plus the
/// `; class=…`/`; code=…` suffix git2 appends for non-`None` classes/codes), so
/// wrapping it in a `GitError` variant preserves the exact error-chain text.
#[derive(Debug, Clone)]
pub struct GitCause(String);

impl GitCause {
    /// The rendered cause string (identical to the source `git2::Error`'s
    /// `Display`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<git2::Error> for GitCause {
    fn from(e: git2::Error) -> Self {
        // git2::Error's Display is `<message>[; class=…][; code=…]`; capture the
        // fully-rendered form so our Display matches it exactly.
        GitCause(e.to_string())
    }
}

impl From<&git2::Error> for GitCause {
    fn from(e: &git2::Error) -> Self {
        GitCause(e.to_string())
    }
}

impl fmt::Display for GitCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Errors produced by the gitlib layer.
///
/// The `Display` strings are frozen: callers (and the CLI error surface)
/// assert on the exact message text. Wrapper variants record the libgit2 cause
/// and render it as `"<context>: <cause>"`. The [`GitError::Lib`] variant
/// stores a [`GitCause`] (a cloneable, Display-preserving capture of a libgit2
/// error) for the rare paths that need a cloneable wrapper; it deliberately
/// does not participate in [`std::error::Error::source`].
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// A free-form wrapped message.
    #[error("{0}")]
    Message(String),
    /// A `"<context>: <cause>"` wrapper over a captured libgit2 error; built
    /// via [`GitError::lib`].
    #[error("{context}: {cause}")]
    Lib { context: String, cause: GitCause },
    /// `open repository: <cause>` — [`crate::Repository::open`] failure.
    #[error("open repository: {0}")]
    OpenRepository(#[source] git2::Error),
    /// `get HEAD: <cause>`.
    #[error("get HEAD: {0}")]
    GetHead(#[source] git2::Error),
    /// `lookup commit: <cause>`.
    #[error("lookup commit: {0}")]
    LookupCommit(#[source] git2::Error),
    /// `lookup blob: <cause>`.
    #[error("lookup blob: {0}")]
    LookupBlob(#[source] git2::Error),
    /// `lookup tree: <cause>`.
    #[error("lookup tree: {0}")]
    LookupTree(#[source] git2::Error),
    /// `create revwalk: <cause>`.
    #[error("create revwalk: {0}")]
    CreateRevwalk(#[source] git2::Error),
    /// `push HEAD to revwalk: <cause>`.
    #[error("push HEAD to revwalk: {0}")]
    PushHead(#[source] git2::Error),
    /// `push to revwalk: <cause>`.
    #[error("push to revwalk: {0}")]
    PushRevwalk(#[source] git2::Error),
    /// `revwalk next: <cause>`.
    #[error("revwalk next: {0}")]
    RevwalkNext(#[source] git2::Error),
    /// `revwalk iterate: <cause>`.
    #[error("revwalk iterate: {0}")]
    RevwalkIterate(#[source] git2::Error),
    /// `get diff options: <cause>`.
    #[error("get diff options: {0}")]
    DiffOptions(#[source] git2::Error),
    /// `diff trees: <cause>`.
    #[error("diff trees: {0}")]
    DiffTrees(#[source] git2::Error),
    /// `get num deltas: <cause>`.
    #[error("get num deltas: {0}")]
    NumDeltas(#[source] git2::Error),
    /// `get delta: <cause>`.
    #[error("get delta: {0}")]
    GetDelta(#[source] git2::Error),
    /// `diff foreach: <cause>`.
    #[error("diff foreach: {0}")]
    DiffForEach(#[source] git2::Error),
    /// `get diff stats: <cause>`.
    #[error("get diff stats: {0}")]
    DiffStats(#[source] git2::Error),
    /// `get commit tree: <cause>`.
    #[error("get commit tree: {0}")]
    CommitTree(#[source] git2::Error),
    /// `entry by path: <cause>`.
    #[error("entry by path: {0}")]
    EntryByPath(#[source] git2::Error),
    /// `looking up blob <hash>: <cause>` — [`crate::blob::CachedBlob::from_repo`].
    #[error("looking up blob {hash}: {source}")]
    CachedBlobLookup { hash: String, source: git2::Error },

    /// `parent commit not found`.
    #[error("parent commit not found")]
    ParentNotFound,
    /// `get commit tree: test commit has no tree`.
    #[error("get commit tree: test commit has no tree")]
    TestCommitNoTree,
    /// `binary` — returned by [`crate::blob::CachedBlob::count_lines`].
    #[error("binary")]
    Binary,
    /// `mock: operation not implemented`.
    #[error("mock: operation not implemented")]
    MockNotImplemented,
    /// `cannot parse time: <input>`.
    #[error("cannot parse time: {0}")]
    InvalidTimeFormat(String),
    /// `remote repositories not supported: <uri>`.
    #[error("remote repositories not supported: {0}")]
    RemoteNotSupported(String),
    /// `<spec> is not a commit: <cause>` — [`crate::Repository::resolve_time`].
    #[error("{spec} is not a commit: {source}")]
    NotACommit { spec: String, source: git2::Error },

    // --- batch / worker sentinels ---
    /// `failed to get repository pointer`.
    #[error("failed to get repository pointer")]
    RepositoryPointer,
    /// `blob lookup failed`.
    #[error("blob lookup failed")]
    BlobLookup,
    /// `memory allocation failed for blob`.
    #[error("memory allocation failed for blob")]
    BlobMemory,
    /// `blob is binary`.
    #[error("blob is binary")]
    BlobBinary,
    /// `diff blob lookup failed`.
    #[error("diff blob lookup failed")]
    DiffLookup,
    /// `memory allocation failed for diff`.
    #[error("memory allocation failed for diff")]
    DiffMemory,
    /// `diff blob is binary`.
    #[error("diff blob is binary")]
    DiffBinary,
    /// `diff computation failed`.
    #[error("diff computation failed")]
    DiffCompute,
    /// `arena full`.
    #[error("arena full")]
    ArenaFull,
    /// `cf_configure_memory failed`.
    #[error("cf_configure_memory failed")]
    ConfigureMemory,
}

impl GitError {
    /// Builds a [`GitError::Lib`] from a context label and a libgit2 error,
    /// rendering as `"<context>: <cause>"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_gitlib::GitError;
    ///
    /// let err = GitError::lib("init repository", git2::Error::from_str("boom"));
    /// assert_eq!(err.to_string(), "init repository: boom");
    /// ```
    #[must_use]
    pub fn lib(context: impl Into<String>, cause: impl Into<GitCause>) -> Self {
        GitError::Lib { context: context.into(), cause: cause.into() }
    }
}

/// Convenience result alias for fallible gitlib operations.
pub type Result<T> = std::result::Result<T, GitError>;

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors reference test TestErrBinaryExists.
    #[test]
    fn err_binary_display() {
        assert_eq!(GitError::Binary.to_string(), "binary");
    }

    // Mirrors reference test TestErrParentNotFoundExists.
    #[test]
    fn err_parent_not_found_display() {
        assert_eq!(GitError::ParentNotFound.to_string(), "parent commit not found");
    }

    // Mirrors reference test TestErrMockNotImplementedExists.
    #[test]
    fn err_mock_not_implemented_display() {
        assert_eq!(
            GitError::MockNotImplemented.to_string(),
            "mock: operation not implemented"
        );
    }

    // Mirrors reference test TestBlobResultError (the message table).
    #[test]
    fn batch_error_messages() {
        let cases: &[(GitError, &str)] = &[
            (GitError::RepositoryPointer, "failed to get repository pointer"),
            (GitError::BlobLookup, "blob lookup failed"),
            (GitError::BlobMemory, "memory allocation failed for blob"),
            (GitError::BlobBinary, "blob is binary"),
            (GitError::DiffLookup, "diff blob lookup failed"),
            (GitError::DiffMemory, "memory allocation failed for diff"),
            (GitError::DiffBinary, "diff blob is binary"),
            (GitError::DiffCompute, "diff computation failed"),
        ];
        for (err, want) in cases {
            assert_eq!(err.to_string(), *want);
        }
    }

    // Wrapper variants render as "<context>: <cause>" with the libgit2 cause
    // exposed through Error::source (Lib deliberately excluded).
    #[test]
    fn wrapper_display_and_source() {
        use std::error::Error as _;
        let e = GitError::OpenRepository(git2::Error::from_str("boom"));
        assert!(e.to_string().starts_with("open repository: boom"));
        assert!(e.source().is_some());

        let lib = GitError::lib("init repository", git2::Error::from_str("boom"));
        assert!(lib.to_string().starts_with("init repository: boom"));
        assert!(lib.source().is_none());
    }
}
