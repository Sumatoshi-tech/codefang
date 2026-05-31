//! Sentinel errors for MCP tool input validation.
//!
//! Ported verbatim from the Go `internal/mcp/tools.go` (`ErrEmptyCode`, …) and
//! `internal/mcp/tools_history.go` (`ErrUnknownHistoryAnalyzer`). The display
//! strings are byte-for-byte identical to the Go `errors.New(...)` messages
//! because the tool surfaces them as `TextContent` in the `IsError` result, and
//! the integration / unit tests assert on substrings of those messages.

use std::fmt;

/// Errors produced while validating MCP tool inputs or running a tool.
///
/// The [`fmt::Display`] output of each variant matches the corresponding Go
/// sentinel error message exactly (including the wrapped `: <detail>` suffixes
/// produced by Go's `fmt.Errorf("%w: ...", err, ...)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// `code parameter is required and must not be empty`
    EmptyCode,
    /// `language parameter is required and must not be empty`
    EmptyLanguage,
    /// `code input exceeds maximum size: <n> bytes (max <m>)`
    CodeTooLarge {
        /// Actual code length in bytes.
        size: usize,
        /// The configured maximum.
        max: usize,
    },
    /// `repo_path parameter is required and must not be empty`
    EmptyRepoPath,
    /// `repo_path must be an absolute path`
    RepoPathNotAbsolute,
    /// `repository path does not exist: <path>`
    RepoNotFound {
        /// The offending path.
        path: String,
    },
    /// `repository path does not exist: <path> is not a directory`
    RepoNotDirectory {
        /// The offending path.
        path: String,
    },
    /// `path is not a git repository: <path>`
    NotGitRepo {
        /// The offending path.
        path: String,
    },
    /// `unsupported language: <language>`
    UnsupportedLanguage {
        /// The unsupported language identifier.
        language: String,
    },
    /// `unknown history analyzer: <name>`
    UnknownHistoryAnalyzer {
        /// The unrecognized analyzer key.
        name: String,
    },
    /// Wrapper carrying a `<prefix>: <inner>` message, reproducing Go's
    /// `fmt.Errorf("<prefix>: %w", err)` chaining (e.g. `create parser: ...`,
    /// `run analyzers: ...`, `load repository: ...`).
    Wrapped {
        /// The context prefix.
        prefix: String,
        /// The wrapped message.
        message: String,
    },
}

impl ToolError {
    /// Wraps an arbitrary error message under a Go-style `<prefix>: <msg>` chain.
    ///
    /// Mirrors `fmt.Errorf("<prefix>: %w", err)`.
    pub fn wrap(prefix: impl Into<String>, message: impl Into<String>) -> Self {
        ToolError::Wrapped {
            prefix: prefix.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::EmptyCode => {
                write!(f, "code parameter is required and must not be empty")
            }
            ToolError::EmptyLanguage => {
                write!(f, "language parameter is required and must not be empty")
            }
            ToolError::CodeTooLarge { size, max } => {
                // Matches: fmt.Errorf("%w: %d bytes (max %d)", ErrCodeTooLarge, len, max)
                write!(f, "code input exceeds maximum size: {size} bytes (max {max})")
            }
            ToolError::EmptyRepoPath => {
                write!(f, "repo_path parameter is required and must not be empty")
            }
            ToolError::RepoPathNotAbsolute => {
                write!(f, "repo_path must be an absolute path")
            }
            ToolError::RepoNotFound { path } => {
                // Matches: fmt.Errorf("%w: %s", ErrRepoNotFound, path)
                write!(f, "repository path does not exist: {path}")
            }
            ToolError::RepoNotDirectory { path } => {
                // Matches: fmt.Errorf("%w: %s is not a directory", ErrRepoNotFound, path)
                write!(f, "repository path does not exist: {path} is not a directory")
            }
            ToolError::NotGitRepo { path } => {
                // Matches: fmt.Errorf("%w: %s", ErrNotGitRepo, path)
                write!(f, "path is not a git repository: {path}")
            }
            ToolError::UnsupportedLanguage { language } => {
                // Matches: fmt.Errorf("%w: %s", ErrUnsupportedLanguage, language)
                write!(f, "unsupported language: {language}")
            }
            ToolError::UnknownHistoryAnalyzer { name } => {
                // Matches: fmt.Errorf("%w: %s", ErrUnknownHistoryAnalyzer, name)
                write!(f, "unknown history analyzer: {name}")
            }
            ToolError::Wrapped { prefix, message } => {
                write!(f, "{prefix}: {message}")
            }
        }
    }
}

impl std::error::Error for ToolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_code_message() {
        assert_eq!(
            ToolError::EmptyCode.to_string(),
            "code parameter is required and must not be empty"
        );
    }

    #[test]
    fn empty_language_message() {
        assert_eq!(
            ToolError::EmptyLanguage.to_string(),
            "language parameter is required and must not be empty"
        );
    }

    #[test]
    fn code_too_large_message_format() {
        let err = ToolError::CodeTooLarge {
            size: 1_048_577,
            max: 1_048_576,
        };
        assert!(err.to_string().contains("exceeds maximum size"));
        assert_eq!(
            err.to_string(),
            "code input exceeds maximum size: 1048577 bytes (max 1048576)"
        );
    }

    #[test]
    fn repo_path_messages() {
        assert!(ToolError::EmptyRepoPath
            .to_string()
            .contains("repo_path parameter is required"));
        assert!(ToolError::RepoPathNotAbsolute
            .to_string()
            .contains("absolute path"));
        assert!(ToolError::RepoNotFound { path: "/x".into() }
            .to_string()
            .contains("does not exist"));
        assert!(ToolError::RepoNotDirectory { path: "/x".into() }
            .to_string()
            .contains("is not a directory"));
        assert!(ToolError::NotGitRepo { path: "/x".into() }
            .to_string()
            .contains("not a git repository"));
    }

    #[test]
    fn unsupported_language_message() {
        let err = ToolError::UnsupportedLanguage {
            language: "brainfuck".into(),
        };
        assert!(err.to_string().contains("unsupported language"));
        assert_eq!(err.to_string(), "unsupported language: brainfuck");
    }

    #[test]
    fn unknown_history_analyzer_message() {
        let err = ToolError::UnknownHistoryAnalyzer { name: "bogus".into() };
        assert_eq!(err.to_string(), "unknown history analyzer: bogus");
    }

    #[test]
    fn wrapped_chains_like_go() {
        let err = ToolError::wrap("create parser", "boom");
        assert_eq!(err.to_string(), "create parser: boom");
    }
}
