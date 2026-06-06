//! Error types for the gitlib layer, ported from the Go sentinel errors spread
//! across `pkg/gitlib` (`cgo_bridge.go`, `cached_blob.go`, `commit.go`,
//! `helpers.go`, `testing.go`).
//!
//! The Go package exposes a flat set of `errors.New`/typed sentinel values plus
//! `fmt.Errorf("...: %w", err)` wrappers. Rust models the wrappers with
//! [`GitError`] (each variant records the libgit2 cause where Go used `%w`) and
//! the bare sentinels as the [`GitError`] unit-ish variants whose
//! [`std::fmt::Display`] strings match the Go `Error()` text byte-for-byte.

use std::fmt;

/// A cloneable wrapper around a libgit2 [`git2::Error`].
///
/// `git2::Error` is not `Clone`, but several gitlib result types (e.g.
/// [`crate::worker::BlobResult`]) carry an error by value and need to be
/// `Clone`. `GitCause` captures the libgit2 error's decomposed parts and
/// reproduces its [`std::fmt::Display`] **byte-for-byte** (message, plus the
/// `; class=…`/`; code=…` suffix git2 appends for non-`None` classes/codes), so
/// wrapping it in a `GitError` variant preserves the exact Go `%w` chain text.
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
/// Variants reproduce the Go sentinel/`%w`-wrapped errors. The `Display`
/// strings of the sentinel variants match Go's `Error()` output exactly so that
/// callers asserting on message text behave identically. The libgit2-cause
/// Lib variant stores a [`GitCause`] (a cloneable, Display-preserving capture
/// of a libgit2 error) for the rare paths that need a cloneable wrapper.
#[derive(Debug)]
pub enum GitError {
    /// A free-form wrapped message (Go `fmt.Errorf` without a sentinel).
    Message(String),
    /// A `"<context>: <cause>"` wrapper over a libgit2 error (Go
    /// `fmt.Errorf("<context>: %w", err)`); built via [`GitError::lib`].
    Lib { context: String, source: GitCause },
    /// `open repository: <cause>` — [`crate::Repository::open`] failure.
    OpenRepository(git2::Error),
    /// `get HEAD: <cause>`.
    GetHead(git2::Error),
    /// `lookup commit: <cause>`.
    LookupCommit(git2::Error),
    /// `lookup blob: <cause>`.
    LookupBlob(git2::Error),
    /// `lookup tree: <cause>`.
    LookupTree(git2::Error),
    /// `create revwalk: <cause>`.
    CreateRevwalk(git2::Error),
    /// `push HEAD to revwalk: <cause>`.
    PushHead(git2::Error),
    /// `push to revwalk: <cause>`.
    PushRevwalk(git2::Error),
    /// `revwalk next: <cause>`.
    RevwalkNext(git2::Error),
    /// `revwalk iterate: <cause>`.
    RevwalkIterate(git2::Error),
    /// `get diff options: <cause>`.
    DiffOptions(git2::Error),
    /// `diff trees: <cause>`.
    DiffTrees(git2::Error),
    /// `get num deltas: <cause>`.
    NumDeltas(git2::Error),
    /// `get delta: <cause>`.
    GetDelta(git2::Error),
    /// `diff foreach: <cause>`.
    DiffForEach(git2::Error),
    /// `get diff stats: <cause>`.
    DiffStats(git2::Error),
    /// `get commit tree: <cause>`.
    CommitTree(git2::Error),
    /// `entry by path: <cause>`.
    EntryByPath(git2::Error),
    /// `looking up blob <hash>: <cause>` — `NewCachedBlobFromRepo`.
    CachedBlobLookup { hash: String, source: git2::Error },

    /// `parent commit not found` (Go `ErrParentNotFound`).
    ParentNotFound,
    /// `get commit tree: test commit has no tree` (Go `errTestCommitNoTree`).
    TestCommitNoTree,
    /// `binary` (Go `ErrBinary`, returned by `CachedBlob::count_lines`).
    Binary,
    /// `mock: operation not implemented` (Go `ErrMockNotImplemented`).
    MockNotImplemented,
    /// `cannot parse time: <input>` (Go `ErrInvalidTimeFormat`).
    InvalidTimeFormat(String),
    /// `remote repositories not supported: <uri>` (Go `ErrRemoteNotSupported`).
    RemoteNotSupported(String),
    /// `<spec> is not a commit: <cause>` (helpers.go ResolveTime).
    NotACommit { spec: String, source: git2::Error },

    // --- batch / worker (cgo_bridge.go) sentinels ---
    /// `failed to get repository pointer` (Go `ErrRepositoryPointer`).
    RepositoryPointer,
    /// `blob lookup failed` (Go `ErrBlobLookup`).
    BlobLookup,
    /// `memory allocation failed for blob` (Go `ErrBlobMemory`).
    BlobMemory,
    /// `blob is binary` (Go `ErrBlobBinary`).
    BlobBinary,
    /// `diff blob lookup failed` (Go `ErrDiffLookup`).
    DiffLookup,
    /// `memory allocation failed for diff` (Go `ErrDiffMemory`).
    DiffMemory,
    /// `diff blob is binary` (Go `ErrDiffBinary`).
    DiffBinary,
    /// `diff computation failed` (Go `ErrDiffCompute`).
    DiffCompute,
    /// `arena full` (Go `ErrArenaFull`).
    ArenaFull,
    /// `cf_configure_memory failed` (Go `ErrConfigureMemory`).
    ConfigureMemory,
}

impl GitError {
    /// Builds a [`GitError::Lib`] from a context label and a libgit2 error,
    /// mirroring Go's `fmt.Errorf("<context>: %w", err)`.
    #[must_use]
    pub fn lib(context: impl Into<String>, source: impl Into<GitCause>) -> Self {
        GitError::Lib { context: context.into(), source: source.into() }
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::Message(m) => f.write_str(m),
            GitError::Lib { context, source } => write!(f, "{context}: {source}"),
            GitError::OpenRepository(e) => write!(f, "open repository: {e}"),
            GitError::GetHead(e) => write!(f, "get HEAD: {e}"),
            GitError::LookupCommit(e) => write!(f, "lookup commit: {e}"),
            GitError::LookupBlob(e) => write!(f, "lookup blob: {e}"),
            GitError::LookupTree(e) => write!(f, "lookup tree: {e}"),
            GitError::CreateRevwalk(e) => write!(f, "create revwalk: {e}"),
            GitError::PushHead(e) => write!(f, "push HEAD to revwalk: {e}"),
            GitError::PushRevwalk(e) => write!(f, "push to revwalk: {e}"),
            GitError::RevwalkNext(e) => write!(f, "revwalk next: {e}"),
            GitError::RevwalkIterate(e) => write!(f, "revwalk iterate: {e}"),
            GitError::DiffOptions(e) => write!(f, "get diff options: {e}"),
            GitError::DiffTrees(e) => write!(f, "diff trees: {e}"),
            GitError::NumDeltas(e) => write!(f, "get num deltas: {e}"),
            GitError::GetDelta(e) => write!(f, "get delta: {e}"),
            GitError::DiffForEach(e) => write!(f, "diff foreach: {e}"),
            GitError::DiffStats(e) => write!(f, "get diff stats: {e}"),
            GitError::CommitTree(e) => write!(f, "get commit tree: {e}"),
            GitError::EntryByPath(e) => write!(f, "entry by path: {e}"),
            GitError::CachedBlobLookup { hash, source } => {
                write!(f, "looking up blob {hash}: {source}")
            }
            GitError::ParentNotFound => write!(f, "parent commit not found"),
            GitError::TestCommitNoTree => write!(f, "get commit tree: test commit has no tree"),
            GitError::Binary => write!(f, "binary"),
            GitError::MockNotImplemented => write!(f, "mock: operation not implemented"),
            GitError::InvalidTimeFormat(s) => write!(f, "cannot parse time: {s}"),
            GitError::RemoteNotSupported(uri) => {
                write!(f, "remote repositories not supported: {uri}")
            }
            GitError::NotACommit { spec, source } => write!(f, "{spec} is not a commit: {source}"),
            GitError::RepositoryPointer => write!(f, "failed to get repository pointer"),
            GitError::BlobLookup => write!(f, "blob lookup failed"),
            GitError::BlobMemory => write!(f, "memory allocation failed for blob"),
            GitError::BlobBinary => write!(f, "blob is binary"),
            GitError::DiffLookup => write!(f, "diff blob lookup failed"),
            GitError::DiffMemory => write!(f, "memory allocation failed for diff"),
            GitError::DiffBinary => write!(f, "diff blob is binary"),
            GitError::DiffCompute => write!(f, "diff computation failed"),
            GitError::ArenaFull => write!(f, "arena full"),
            GitError::ConfigureMemory => write!(f, "cf_configure_memory failed"),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GitError::OpenRepository(e)
            | GitError::GetHead(e)
            | GitError::LookupCommit(e)
            | GitError::LookupBlob(e)
            | GitError::LookupTree(e)
            | GitError::CreateRevwalk(e)
            | GitError::PushHead(e)
            | GitError::PushRevwalk(e)
            | GitError::RevwalkNext(e)
            | GitError::RevwalkIterate(e)
            | GitError::DiffOptions(e)
            | GitError::DiffTrees(e)
            | GitError::NumDeltas(e)
            | GitError::GetDelta(e)
            | GitError::DiffForEach(e)
            | GitError::DiffStats(e)
            | GitError::CommitTree(e)
            | GitError::EntryByPath(e) => Some(e),
            GitError::CachedBlobLookup { source, .. } | GitError::NotACommit { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

/// Convenience result alias for fallible gitlib operations.
pub type Result<T> = std::result::Result<T, GitError>;

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from cached_blob_test.go::TestErrBinaryExists.
    #[test]
    fn err_binary_display() {
        assert_eq!(GitError::Binary.to_string(), "binary");
    }

    // Ported from file_test.go::TestErrParentNotFoundExists.
    #[test]
    fn err_parent_not_found_display() {
        assert_eq!(GitError::ParentNotFound.to_string(), "parent commit not found");
    }

    // Ported from testing_test.go::TestErrMockNotImplementedExists.
    #[test]
    fn err_mock_not_implemented_display() {
        assert_eq!(
            GitError::MockNotImplemented.to_string(),
            "mock: operation not implemented"
        );
    }

    // Ported from cgo_bridge_test.go::TestBlobResultError (the message table).
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
}
