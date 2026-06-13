//! Sentinel errors for MCP tool input validation.
//!
//! The display strings are part of the MCP tool contract: the tool surfaces
//! them as `TextContent` in the `IsError` result, and the integration / unit
//! tests assert on substrings of those messages. Keep every message
//! byte-identical when refactoring.

/// Errors produced while validating MCP tool inputs or running a tool.
///
/// Each variant's `Display` output (the `#[error]` string) is a frozen,
/// caller-visible message, including the `: <detail>` suffixes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// The inline code input was empty.
    #[error("code parameter is required and must not be empty")]
    EmptyCode,
    /// The language identifier was empty.
    #[error("language parameter is required and must not be empty")]
    EmptyLanguage,
    /// The inline code input exceeded the byte limit.
    #[error("code input exceeds maximum size: {size} bytes (max {max})")]
    CodeTooLarge {
        /// Actual code length in bytes.
        size: usize,
        /// The configured maximum.
        max: usize,
    },
    /// The repository path was empty.
    #[error("repo_path parameter is required and must not be empty")]
    EmptyRepoPath,
    /// The repository path was relative.
    #[error("repo_path must be an absolute path")]
    RepoPathNotAbsolute,
    /// The repository path does not exist.
    #[error("repository path does not exist: {path}")]
    RepoNotFound {
        /// The offending path.
        path: String,
    },
    /// The repository path exists but is not a directory.
    #[error("repository path does not exist: {path} is not a directory")]
    RepoNotDirectory {
        /// The offending path.
        path: String,
    },
    /// The directory has no `.git` entry.
    #[error("path is not a git repository: {path}")]
    NotGitRepo {
        /// The offending path.
        path: String,
    },
    /// The language is not supported by the parser.
    #[error("unsupported language: {language}")]
    UnsupportedLanguage {
        /// The unsupported language identifier.
        language: String,
    },
    /// The requested history analyzer key is not in the known set.
    #[error("unknown history analyzer: {name}")]
    UnknownHistoryAnalyzer {
        /// The unrecognized analyzer key.
        name: String,
    },
    /// Wrapper carrying a `<prefix>: <inner>` message (e.g.
    /// `create parser: ...`, `run analyzers: ...`, `load repository: ...`).
    #[error("{prefix}: {message}")]
    Wrapped {
        /// The context prefix.
        prefix: String,
        /// The wrapped message.
        message: String,
    },
}

impl ToolError {
    /// Wraps an arbitrary error message under a `<prefix>: <msg>` chain.
    ///
    /// ```
    /// use cf_mcp::ToolError;
    ///
    /// let err = ToolError::wrap("create parser", "boom");
    /// assert_eq!(err.to_string(), "create parser: boom");
    /// ```
    pub fn wrap(prefix: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Wrapped {
            prefix: prefix.into(),
            message: message.into(),
        }
    }
}

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
    fn wrapped_chains_prefix_and_message() {
        let err = ToolError::wrap("create parser", "boom");
        assert_eq!(err.to_string(), "create parser: boom");
    }
}
