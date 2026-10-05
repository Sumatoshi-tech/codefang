//! The single general `run` pipeline and its analyzer registry.
//!
//! This module replaces the per-`(analyzer, format)` dispatch ladder that the
//! `codefang` binary historically carried with **one** general flow:
//!
//!  1. [`RunContext::from_matches`] resolves the requested analyzer set + output
//!     format from the parsed clap args.
//!  2. [`run_pipeline`] validates the run (path, repository, flags, format)
//!     through the shared preflight, splits the selection into static and
//!     history analyzers, resolves the per-phase format, and dispatches **by
//!     analyzer id** through the [`Registry`] — for static analyzers the file
//!     set is parsed to UAST once and each requested static analyzer runs from
//!     its crate; for history analyzers a single git revwalk feeds per-commit
//!     data to each requested history analyzer from its crate.
//!  3. The resulting report bytes are produced by the analyzer crate's own
//!     serializer (cf-gojson / cf-goyaml / cf-reportutil per format) — this
//!     module never owns analyzer math or hardcodes report bytes.
//!
//! ## Why a registry, not a match ladder
//!
//! The dispatch is a single keyed lookup: `registry.lookup(id)` returns the
//! [`AnalyzerEntry`] whose [`AnalyzerEntry::run`] handler is the crate-owned
//! report builder. Adding an analyzer is one registry insertion; there is no
//! per-format `if` chain. The handler receives the [`RunContext`] (resolved
//! path + parsed options) and the already-resolved format string, and returns
//! the report bytes or a [`HandlerError`] naming the cause.

use std::collections::BTreeMap;

use clap::ArgMatches;

use crate::formats::{apply_ndjson_modifier, resolve_formats};

/// Whether an analyzer is driven by the static (folder → UAST) phase or the
/// history (git revwalk → per-commit) phase. The analyzer id prefix
/// (`static/` vs `history/`) selects the phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Folder-walk + per-file UAST analyzer.
    Static,
    /// Git-revwalk + per-commit analyzer.
    History,
}

/// Resolved run inputs shared by every analyzer handler (path, format, the
/// `--head`/`--limit`/`--ndjson` toggles, path policy).
///
/// Handlers read only what they need; the context is constructed once per `run`
/// from the parsed clap [`ArgMatches`] so the dispatch loop stays general.
pub struct RunContext<'a> {
    /// Folder / repository path to analyze: the positional `[path]` arg,
    /// falling back to `--path`/`-p`, default `.`.
    pub path: String,
    /// The parsed clap matches for the `run` subcommand, so history handlers can
    /// read the walk flags (`--head`, `--limit`, `--ndjson`, …) without this
    /// struct enumerating every one.
    pub matches: &'a ArgMatches,
}

impl<'a> RunContext<'a> {
    /// Builds the run context from the parsed `run` subcommand matches: the
    /// positional `[path]` argument wins, else `--path`/`-p` (default `.`).
    #[must_use]
    pub fn from_matches(matches: &'a ArgMatches) -> Self {
        let path = matches
            .get_one::<String>("path-positional")
            .filter(|p| !p.is_empty())
            .or_else(|| matches.get_one::<String>("path"))
            .map_or_else(|| ".".to_owned(), String::to_owned);
        RunContext { path, matches }
    }

    /// The requested analyzer ids, in user-supplied order (`-a/--analyzers`,
    /// comma-split by clap's value delimiter).
    ///
    /// An absent (or empty) `--analyzers` resolves to the full set (`*`):
    /// `codefang run` with no `-a` runs every static + history analyzer and
    /// renders the combined output. The glob is expanded downstream by the
    /// same `expand_combined_ids`/`expand_history_phase_ids` path an explicit
    /// `-a '*'` takes, so the resolved selection is identical.
    #[must_use]
    pub fn analyzer_ids(&self) -> Vec<String> {
        let ids: Vec<String> = self
            .matches
            .get_many::<String>("analyzers")
            .map(|vals| vals.cloned().collect())
            .unwrap_or_default();
        if ids.is_empty() {
            vec!["*".to_owned()]
        } else {
            ids
        }
    }

    /// The raw `--format` value (default `json`), before per-phase resolution.
    #[must_use]
    pub fn raw_format(&self) -> String {
        self.matches
            .get_one::<String>("format")
            .cloned()
            .unwrap_or_else(|| "json".to_owned())
    }

    /// Whether `--head` is set (history single-HEAD-commit window).
    #[must_use]
    pub fn head(&self) -> bool {
        self.matches.get_flag("head")
    }

    /// Whether `--ndjson` is set (NDJSON streaming modifier).
    #[must_use]
    pub fn ndjson(&self) -> bool {
        self.matches.get_flag("ndjson")
    }
}

/// Why a handler (or the dispatch around it) could not produce a report.
///
/// The `Display` form is the text after the CLI's `Error: ` prefix — the
/// user-facing contract (`site/guide/cli-reference.md`, "Errors").
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandlerError {
    /// The analyze path does not exist.
    #[error("path not found: {0}")]
    PathNotFound(String),
    /// The path exists but is not a git repository (history selection).
    #[error("cannot open repository: {0}")]
    CannotOpenRepository(String),
    /// The `--format` value is not a format codefang knows at all.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    /// A known format that this analyzer (or phase) cannot render.
    #[error("format {format} is not supported for {analyzer}")]
    FormatNotSupported {
        /// The `--format` value as supplied.
        format: String,
        /// The analyzer id (or selection description) that rejects it.
        analyzer: String,
    },
    /// A flag value is invalid (`--limit=-5`, `--tick-size 0`, an
    /// unresolvable `--since`, an unknown `--languages` token, an unreadable
    /// `--people-dict`); carries the full message.
    #[error("{0}")]
    InvalidFlag(String),
    /// The handler ran but produced no report for this input — the generic
    /// fallback for a `None` from a report builder after preflight passed.
    #[error("no report produced for {analyzer} --format {format}")]
    NoReport {
        /// The analyzer id.
        analyzer: String,
        /// The resolved format.
        format: String,
    },
}

/// The crate-owned report builder for one analyzer. Receives the resolved
/// [`RunContext`] and the already-resolved, ndjson-modified format string, and
/// returns the serialized report bytes or the [`HandlerError`] naming why none
/// could be produced. The bytes MUST come from the analyzer crate's own
/// serializer (cf-gojson / cf-goyaml / cf-reportutil); this signature never
/// returns a model this module would re-encode.
pub type RunHandler = fn(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError>;

/// One registry entry: an analyzer's id, its phase, and its crate-owned report
/// builder. The handler owns format selection, returning
/// [`HandlerError::FormatNotSupported`] for a combination it cannot render.
pub struct AnalyzerEntry {
    /// Canonical analyzer id, e.g. `static/complexity`, `history/burndown`.
    pub id: &'static str,
    /// Static (folder) or history (revwalk) phase.
    pub mode: Mode,
    /// The crate-owned report builder.
    pub run: RunHandler,
}

/// The analyzer registry: the single source of truth mapping analyzer id →
/// [`AnalyzerEntry`], built once and queried by id. Dispatch is a keyed
/// lookup, NOT a per-format match ladder.
pub struct Registry {
    entries: BTreeMap<&'static str, AnalyzerEntry>,
}

impl Registry {
    /// Builds an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Registers one analyzer entry, keyed by its id.
    pub fn register(&mut self, entry: AnalyzerEntry) {
        self.entries.insert(entry.id, entry);
    }

    /// Looks up an analyzer by id.
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&AnalyzerEntry> {
        self.entries.get(id)
    }

    /// Every registered id, sorted (feeds `--list-analyzers`).
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.entries.keys().copied().collect()
    }

    /// Splits a requested id selection into (static, history) id lists,
    /// preserving the requested order within each phase. The phase is taken
    /// from the registered entry's [`Mode`]. Unknown ids are dropped here (the
    /// caller reports them).
    #[must_use]
    pub fn split<'s>(&self, ids: &'s [String]) -> (Vec<&'s String>, Vec<&'s String>) {
        let mut statics = Vec::new();
        let mut history = Vec::new();
        for id in ids {
            match self.lookup(id).map(|e| e.mode) {
                Some(Mode::Static) => statics.push(id),
                Some(Mode::History) => history.push(id),
                None => {}
            }
        }
        (statics, history)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of dispatching one analyzer through the registry: the serialized
/// report bytes plus the id that produced them (for diagnostics / combined
/// output ordering).
#[derive(Debug)]
pub struct PhaseOutput {
    /// The analyzer id that produced these bytes.
    pub id: String,
    /// The serialized report bytes (already in the requested format).
    pub bytes: Vec<u8>,
}

/// Errors the general pipeline can surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    /// No analyzer ids were selected.
    #[error("no analyzers selected. Use -a flag, e.g.: -a burndown,couples")]
    NoAnalyzersSelected,
    /// A requested id is not in the registry.
    #[error("unknown analyzer id: {0}")]
    UnknownAnalyzer(String),
    /// A handler (or the shared preflight) rejected the run.
    #[error(transparent)]
    Handler(#[from] HandlerError),
}

/// The single general run pipeline. Validates the run through the shared
/// preflight, resolves the analyzer set + per-phase format, then dispatches
/// every requested analyzer through `registry` by id, returning each phase's
/// serialized report bytes in selection order.
///
/// Static analyzers are dispatched first, then history analyzers; within each
/// phase the handlers run in the user-requested order.
///
/// # Errors
///
/// Returns [`PipelineError`] when the selection is empty, references an unknown
/// id, the preflight rejects the path / repository / flags / format, or a
/// handler cannot produce its report.
pub fn run_pipeline(
    registry: &Registry,
    ctx: &RunContext,
    ids: &[String],
) -> Result<Vec<PhaseOutput>, PipelineError> {
    if ids.is_empty() {
        return Err(PipelineError::NoAnalyzersSelected);
    }

    for id in ids {
        if registry.lookup(id).is_none() {
            return Err(PipelineError::UnknownAnalyzer(id.clone()));
        }
    }

    let (static_ids, history_ids) = registry.split(ids);
    let owned = |v: &[&String]| -> Vec<String> { v.iter().map(|s| (*s).clone()).collect() };
    crate::handlers::preflight(ctx, &owned(&static_ids), &owned(&history_ids))?;

    let (static_format, history_format) = resolve_formats(
        &ctx.raw_format(),
        !static_ids.is_empty(),
        !history_ids.is_empty(),
    )
    .map_err(|_| HandlerError::UnsupportedFormat(ctx.raw_format()))?;

    let mut outputs = Vec::with_capacity(ids.len());

    // Static phase: the per-analyzer reports are independent (every id was
    // validated against the registry above), so they are COMPUTED concurrently
    // and emitted in the selection order; the first failed analyzer in that
    // order surfaces its error.
    let static_reports: Vec<Result<Vec<u8>, HandlerError>> = crate::handlers::run_concurrent(
        static_ids.len(),
        crate::handlers::ANALYZER_CONCURRENCY,
        |i| dispatch(registry, static_ids[i], ctx, &static_format),
    );
    for (id, bytes) in static_ids.iter().zip(static_reports) {
        outputs.push(PhaseOutput {
            id: (*id).clone(),
            bytes: bytes?,
        });
    }

    // History phase: one revwalk feeds each history analyzer. The `--ndjson`
    // modifier turns `timeseries` into `timeseries+ndjson`. The shared UAST
    // walk for the co-selected heavy analyzers is pre-computed as one task,
    // then the per-analyzer reports are computed concurrently and emitted in
    // the selection order.
    let history_format = apply_ndjson_modifier(&history_format, ctx.ndjson());
    crate::handlers::uast_walk::prewarm(ctx.matches);
    let history_reports: Vec<Result<Vec<u8>, HandlerError>> = crate::handlers::run_concurrent(
        history_ids.len(),
        crate::handlers::ANALYZER_CONCURRENCY,
        |i| dispatch(registry, history_ids[i], ctx, &history_format),
    );
    for (id, bytes) in history_ids.iter().zip(history_reports) {
        outputs.push(PhaseOutput {
            id: (*id).clone(),
            bytes: bytes?,
        });
    }

    Ok(outputs)
}

/// Runs one registered handler (the id was validated by the caller).
fn dispatch(
    registry: &Registry,
    id: &str,
    ctx: &RunContext,
    format: &str,
) -> Result<Vec<u8>, HandlerError> {
    let entry = registry.lookup(id).ok_or_else(|| HandlerError::NoReport {
        analyzer: id.to_string(),
        format: format.to_string(),
    })?;
    (entry.run)(ctx, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_static(_ctx: &RunContext, _fmt: &str) -> Result<Vec<u8>, HandlerError> {
        Ok(b"static-report".to_vec())
    }

    fn stub_history(_ctx: &RunContext, fmt: &str) -> Result<Vec<u8>, HandlerError> {
        Ok(format!("history:{fmt}").into_bytes())
    }

    fn test_registry() -> Registry {
        let mut r = Registry::new();
        r.register(AnalyzerEntry {
            id: "static/complexity",
            mode: Mode::Static,
            run: stub_static,
        });
        r.register(AnalyzerEntry {
            id: "history/burndown",
            mode: Mode::History,
            run: stub_history,
        });
        r
    }

    /// A real (empty) git repository so the shared preflight passes for the
    /// stubbed history analyzer.
    fn repo_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git2::Repository::init(dir.path()).expect("git init");
        dir
    }

    fn run_matches(args: &[&str]) -> ArgMatches {
        crate::flags::build_run_command()
            .no_binary_name(true)
            .try_get_matches_from(args)
            .expect("parse run args")
    }

    #[test]
    fn split_routes_by_mode() {
        let r = test_registry();
        let ids = vec![
            "history/burndown".to_string(),
            "static/complexity".to_string(),
        ];
        let (s, h) = r.split(&ids);
        assert_eq!(s, vec![&"static/complexity".to_string()]);
        assert_eq!(h, vec![&"history/burndown".to_string()]);
    }

    #[test]
    fn absent_analyzers_flag_defaults_to_all() {
        let m = run_matches(&["."]);
        let ctx = RunContext::from_matches(&m);
        assert_eq!(ctx.analyzer_ids(), vec!["*".to_string()]);
    }

    #[test]
    fn explicit_analyzers_flag_is_preserved_in_order() {
        let m = run_matches(&["-a", "history/burndown,static/complexity"]);
        let ctx = RunContext::from_matches(&m);
        assert_eq!(
            ctx.analyzer_ids(),
            vec![
                "history/burndown".to_string(),
                "static/complexity".to_string()
            ]
        );
    }

    #[test]
    fn empty_selection_errors() {
        let r = test_registry();
        let m = run_matches(&["-a", "static/complexity"]);
        let ctx = RunContext::from_matches(&m);
        let err = run_pipeline(&r, &ctx, &[]).unwrap_err();
        assert_eq!(err, PipelineError::NoAnalyzersSelected);
    }

    #[test]
    fn unknown_analyzer_errors() {
        let r = test_registry();
        let m = run_matches(&["-a", "static/nope"]);
        let ctx = RunContext::from_matches(&m);
        let ids = vec!["static/nope".to_string()];
        let err = run_pipeline(&r, &ctx, &ids).unwrap_err();
        assert_eq!(err, PipelineError::UnknownAnalyzer("static/nope".into()));
    }

    #[test]
    fn dispatches_static_then_history_in_order() {
        let r = test_registry();
        let dir = repo_dir();
        let m = run_matches(&[
            "-a",
            "history/burndown,static/complexity",
            "--format",
            "json",
            dir.path().to_str().unwrap(),
        ]);
        let ctx = RunContext::from_matches(&m);
        let ids = vec![
            "history/burndown".to_string(),
            "static/complexity".to_string(),
        ];
        let out = run_pipeline(&r, &ctx, &ids).unwrap();
        assert_eq!(out.len(), 2);
        // Static is dispatched first regardless of request order.
        assert_eq!(out[0].id, "static/complexity");
        assert_eq!(out[1].id, "history/burndown");
    }

    #[test]
    fn ndjson_modifier_applies_to_history_format() {
        let r = test_registry();
        let dir = repo_dir();
        let m = run_matches(&[
            "-a",
            "history/burndown",
            "--format",
            "timeseries",
            "--ndjson",
            dir.path().to_str().unwrap(),
        ]);
        let ctx = RunContext::from_matches(&m);
        let ids = vec!["history/burndown".to_string()];
        let out = run_pipeline(&r, &ctx, &ids).unwrap();
        assert_eq!(out[0].bytes, b"history:timeseries+ndjson");
    }

    #[test]
    fn context_resolves_positional_path_over_flag() {
        let m = run_matches(&["/some/repo", "-a", "static/complexity", "-p", "/other"]);
        let ctx = RunContext::from_matches(&m);
        assert_eq!(ctx.path, "/some/repo");
    }

    /// The preflight runs inside the pipeline too, so library callers get the
    /// same named errors the CLI prints.
    #[test]
    fn pipeline_surfaces_preflight_errors() {
        let r = test_registry();
        let m = run_matches(&["-a", "static/complexity", "/definitely/not/here"]);
        let ctx = RunContext::from_matches(&m);
        let err = run_pipeline(&r, &ctx, &["static/complexity".to_string()]).unwrap_err();
        assert_eq!(err.to_string(), "path not found: /definitely/not/here");

        let dir = tempfile::tempdir().expect("tempdir");
        let m = run_matches(&["-a", "history/burndown", dir.path().to_str().unwrap()]);
        let ctx = RunContext::from_matches(&m);
        let err = run_pipeline(&r, &ctx, &["history/burndown".to_string()]).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("cannot open repository: {}", dir.path().display())
        );
    }
}
