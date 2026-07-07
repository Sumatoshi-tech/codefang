//! `codefang_history` tool handler.
//!
//! Validates the repository path, selects the requested (or all) history
//! analyzers, runs the analysis pipeline over the commit range, and returns
//! the merged results as report-compatible pretty JSON.
//!
//! Pipeline execution (repository load → runner/coordinator → JSON-formatted
//! history results) is taken behind the [`HistoryAnalysisProvider`] trait so
//! this module does not depend on the concrete pipeline crates. The **input
//! validation, analyzer-key selection, default commit limit, and all error
//! wording live here and are unit-tested**; the provider supplies the merged
//! JSON-shaped [`JsonValue`].

use std::path::Path;

use crate::errors::ToolError;
#[cfg(test)]
use crate::gojson::JsonValue;
use crate::providers::{HistoryAnalysisProvider, HistoryRunOptions};
use crate::result::{ToolOutput, ToolResult};
use crate::tools::HistoryInput;

/// Default commit limit for the MCP history tool.
pub const DEFAULT_MCP_COMMIT_LIMIT: i64 = 1000;

/// The eight history analyzer keys, in their fixed registration order.
pub const ALL_HISTORY_KEYS: &[&str] = &[
    "burndown",
    "couples",
    "devs",
    "file-history",
    "imports",
    "sentiment",
    "shotness",
    "typos",
];

/// Returns the full history-analyzer key list as owned strings.
#[must_use]
pub fn all_history_keys() -> Vec<String> {
    ALL_HISTORY_KEYS.iter().map(|s| (*s).to_string()).collect()
}

/// Validates the history tool input parameters.
///
/// The check order and message wording are part of the tool contract:
/// - empty path → [`ToolError::EmptyRepoPath`];
/// - relative path → [`ToolError::RepoPathNotAbsolute`];
/// - missing path → [`ToolError::RepoNotFound`];
/// - non-directory → [`ToolError::RepoNotDirectory`];
/// - missing `.git` → [`ToolError::NotGitRepo`].
///
/// # Errors
/// Returns the first failing constraint.
pub fn validate_history_input(input: &HistoryInput) -> Result<(), ToolError> {
    if input.repo_path.is_empty() {
        return Err(ToolError::EmptyRepoPath);
    }

    let path = Path::new(&input.repo_path);
    if !path.is_absolute() {
        return Err(ToolError::RepoPathNotAbsolute);
    }

    let Ok(meta) = std::fs::metadata(path) else {
        return Err(ToolError::RepoNotFound {
            path: input.repo_path.clone(),
        });
    };

    if !meta.is_dir() {
        return Err(ToolError::RepoNotDirectory {
            path: input.repo_path.clone(),
        });
    }

    let git_dir = path.join(".git");
    if std::fs::metadata(&git_dir).is_err() {
        return Err(ToolError::NotGitRepo {
            path: input.repo_path.clone(),
        });
    }

    Ok(())
}

/// Selects the analyzer keys for the requested names, erroring on any unknown
/// key.
///
/// Validates each requested key against the known set, preserving the
/// requested order. Returns [`ToolError::UnknownHistoryAnalyzer`]
/// (`unknown history analyzer: <name>`) on the first unknown key.
///
/// # Errors
/// Returns [`ToolError::UnknownHistoryAnalyzer`] for the first unknown key.
pub fn select_history_keys(keys: &[String]) -> Result<Vec<String>, ToolError> {
    let mut selected = Vec::with_capacity(keys.len());
    for name in keys {
        if !ALL_HISTORY_KEYS.contains(&name.as_str()) {
            return Err(ToolError::UnknownHistoryAnalyzer { name: name.clone() });
        }
        selected.push(name.clone());
    }
    Ok(selected)
}

/// Processes a `codefang_history` tool call.
///
/// The step order is observable through the returned errors, so keep it:
/// 1. validate the history input.
/// 2. normalize the commit limit (`<= 0` → [`DEFAULT_MCP_COMMIT_LIMIT`]).
/// 3. default to all analyzer keys when none requested; validate the selection.
/// 4. run the pipeline via the provider.
/// 5. return the merged results as pretty JSON.
///
/// Note: the reference flow validates the key selection *after* loading the
/// repository, but selection is purely a function of the key set, so the
/// observable error (unknown analyzer) is identical. Path validation still
/// happens first, so the repo-load error class is unaffected.
#[must_use]
pub fn handle_history(
    provider: &dyn HistoryAnalysisProvider,
    input: &HistoryInput,
) -> (ToolResult, ToolOutput) {
    if let Err(err) = validate_history_input(input) {
        return (ToolResult::error(&err), ToolOutput::empty());
    }

    let limit = if input.limit <= 0 {
        DEFAULT_MCP_COMMIT_LIMIT
    } else {
        input.limit
    };

    let requested = if input.analyzers.is_empty() {
        all_history_keys()
    } else {
        input.analyzers.clone()
    };

    let selected = match select_history_keys(&requested) {
        Ok(s) => s,
        Err(err) => return (ToolResult::error(&err), ToolOutput::empty()),
    };

    let opts = HistoryRunOptions {
        repo_path: input.repo_path.clone(),
        analyzers: selected,
        limit,
        first_parent: input.first_parent,
        since: input.since.clone(),
    };

    match provider.run(&opts) {
        Ok(results) => (ToolResult::json(&results), ToolOutput::with_data(results)),
        Err(err) => (ToolResult::error(&err), ToolOutput::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider;
    impl HistoryAnalysisProvider for FakeProvider {
        fn run(&self, opts: &HistoryRunOptions) -> Result<JsonValue, ToolError> {
            let entries = opts
                .analyzers
                .iter()
                .map(|k| (k.clone(), JsonValue::Int(opts.limit)))
                .collect();
            Ok(JsonValue::sorted_object(entries))
        }
    }

    /// Creates a unique temp dir for a test; optionally with a `.git` subdir.
    fn temp_repo(tag: &str, with_git: bool) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "cf-mcp-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if with_git {
            std::fs::create_dir_all(dir.join(".git")).unwrap();
        } else {
            std::fs::create_dir_all(&dir).unwrap();
        }
        dir
    }

    #[test]
    fn empty_repo_path_is_error() {
        let input = HistoryInput::default();
        let (res, _) = handle_history(&FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("repo_path parameter is required"));
    }

    #[test]
    fn relative_path_is_error() {
        let input = HistoryInput {
            repo_path: "relative/path".into(),
            ..Default::default()
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("absolute path"));
    }

    #[test]
    fn nonexistent_path_is_error() {
        let input = HistoryInput {
            repo_path: "/nonexistent/path/to/repo".into(),
            ..Default::default()
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("does not exist"));
    }

    #[test]
    fn non_git_dir_is_error() {
        let tmp = temp_repo("nongit", false);
        let input = HistoryInput {
            repo_path: tmp.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(res.is_error);
        assert!(res.first_text().contains("not a git repository"));
    }

    #[test]
    fn valid_git_dir_runs_pipeline() {
        let tmp = temp_repo("valid", true);
        let input = HistoryInput {
            repo_path: tmp.to_string_lossy().into_owned(),
            analyzers: vec!["couples".into()],
            limit: 5,
            first_parent: true,
            ..Default::default()
        };
        let (res, out) = handle_history(&FakeProvider, &input);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!res.is_error, "unexpected error: {}", res.first_text());
        assert!(res.first_text().contains("couples"));
        assert!(out.data.is_some());
    }

    #[test]
    fn with_since_runs_pipeline() {
        let tmp = temp_repo("since", true);
        let input = HistoryInput {
            repo_path: tmp.to_string_lossy().into_owned(),
            analyzers: vec!["couples".into()],
            limit: 5,
            since: "24h".into(),
            first_parent: true,
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!res.is_error, "unexpected error: {}", res.first_text());
    }

    #[test]
    fn unknown_analyzer_key_is_error() {
        let tmp = temp_repo("unknown", true);
        let input = HistoryInput {
            repo_path: tmp.to_string_lossy().into_owned(),
            analyzers: vec!["does-not-exist".into()],
            ..Default::default()
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(res.is_error);
        assert!(res.first_text().contains("unknown history analyzer"));
    }

    #[test]
    fn default_limit_applied_when_unset() {
        let tmp = temp_repo("limit", true);
        let input = HistoryInput {
            repo_path: tmp.to_string_lossy().into_owned(),
            analyzers: vec!["burndown".into()],
            limit: 0,
            ..Default::default()
        };
        let (res, _) = handle_history(&FakeProvider, &input);
        let _ = std::fs::remove_dir_all(&tmp);
        // FakeProvider stores the effective limit as the value; default is 1000.
        assert!(res.first_text().contains("1000"));
    }

    #[test]
    fn all_history_keys_match_registration_order() {
        assert_eq!(
            ALL_HISTORY_KEYS,
            &[
                "burndown",
                "couples",
                "devs",
                "file-history",
                "imports",
                "sentiment",
                "shotness",
                "typos"
            ]
        );
    }

    #[test]
    fn select_preserves_requested_order() {
        let sel = select_history_keys(&["typos".into(), "burndown".into()]).unwrap();
        assert_eq!(sel, vec!["typos", "burndown"]);
    }
}
