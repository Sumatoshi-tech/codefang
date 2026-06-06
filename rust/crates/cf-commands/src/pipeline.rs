//! The single general `run` pipeline and its analyzer registry.
//!
//! This module is the Rust analogue of Go `cmd/codefang/commands/run.go`
//! (`RunCommand.run` → `runDirect` → `runStaticPhase` / `runHistoryPhase`) plus
//! `internal/framework`'s streaming runner. It replaces the per-`(analyzer,
//! format)` dispatch ladder that the `codefang` binary historically carried
//! (the 31 `if analyzers == [..] && format == ".."` blocks) with **one**
//! general flow:
//!
//!  1. [`RunContext::from_matches`] resolves the requested analyzer set + output
//!     format from the parsed clap args (Go `RunCommand` field binding +
//!     `registry.SelectedIDs`).
//!  2. [`run_pipeline`] splits the selection into static and history analyzers
//!     (Go `registry.Split`), resolves the per-phase format (Go
//!     `analyze.ResolveFormats`), and dispatches **by analyzer id** through the
//!     [`Registry`] — for static analyzers the file set is parsed to UAST once
//!     and each requested static analyzer runs from its crate; for history
//!     analyzers a single git revwalk feeds per-commit data to each requested
//!     history analyzer from its crate (Go `runStaticPhase` /
//!     `runHistoryPhase`).
//!  3. The resulting report bytes are produced by the analyzer crate's own
//!     serializer (cf-gojson / cf-goyaml / cf-reportutil per format) — this
//!     module never owns analyzer math or hardcodes report bytes.
//!
//! ## Why a registry, not a match ladder
//!
//! The dispatch is a single keyed lookup: `registry.lookup(id)` returns the
//! [`AnalyzerEntry`] whose [`AnalyzerEntry::run`] handler is the crate-owned
//! report builder. Adding an analyzer is one registry insertion; there is no
//! per-format `if` chain. The handler closure receives the [`RunContext`]
//! (resolved path + parsed options) and the already-resolved format string, and
//! returns the report bytes the crate produces. The dispatch loop in
//! [`run_pipeline`] is format-agnostic: format selection is the handler's
//! concern, exactly as each analyzer crate's `FormatReport*` family is in Go.

use std::collections::BTreeMap;

use clap::ArgMatches;

use crate::formats::{apply_ndjson_modifier, resolve_formats};

/// Whether an analyzer is driven by the static (folder → UAST) phase or the
/// history (git revwalk → per-commit) phase. Mirrors Go `analyze.AnalyzerMode`
/// (`ModeStatic` / `ModeHistory`); the analyzer id prefix (`static/` vs
/// `history/`) selects the phase, exactly as Go `registry.Split` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Folder-walk + per-file UAST analyzer (Go `ModeStatic`).
    Static,
    /// Git-revwalk + per-commit analyzer (Go `ModeHistory`).
    History,
}

/// Resolved run inputs shared by every analyzer handler — the Rust analogue of
/// the per-run state Go threads through `runStaticPhase` / `runHistoryPhase`
/// (path, format, the `--head`/`--limit`/`--ndjson` toggles, path policy).
///
/// Handlers read only what they need; the context is constructed once per `run`
/// from the parsed clap [`ArgMatches`] so the dispatch loop stays general.
pub struct RunContext<'a> {
    /// Folder / repository path to analyze (Go `RunCommand.resolvePath`: the
    /// positional `[path]` arg, falling back to `--path`/`-p`, default `.`).
    pub path: String,
    /// The parsed clap matches for the `run` subcommand, so history handlers can
    /// read the streaming flags (`--head`, `--limit`, `--ndjson`, `--workers`,
    /// …) without this struct enumerating every one (Go reads them off
    /// `RunCommand` / `HistoryRunOptions`).
    pub matches: &'a ArgMatches,
}

impl<'a> RunContext<'a> {
    /// Builds the run context from the parsed `run` subcommand matches,
    /// resolving the analyze path the way Go `RunCommand.resolvePath` does: the
    /// positional `[path]` argument wins, else `--path`/`-p` (default `.`).
    #[must_use]
    pub fn from_matches(matches: &'a ArgMatches) -> Self {
        let path = matches
            .get_one::<String>("path-positional")
            .filter(|p| !p.is_empty())
            .or_else(|| matches.get_one::<String>("path"))
            .map(String::to_owned)
            .unwrap_or_else(|| ".".to_owned());
        RunContext { path, matches }
    }

    /// The requested analyzer ids, in user-supplied order (Go `-a/--analyzers`,
    /// comma-split by clap's value delimiter). Empty when the flag is absent.
    #[must_use]
    pub fn analyzer_ids(&self) -> Vec<String> {
        self.matches
            .get_many::<String>("analyzers")
            .map(|vals| vals.cloned().collect())
            .unwrap_or_default()
    }

    /// The raw `--format` value (default `json`), before per-phase resolution.
    #[must_use]
    pub fn raw_format(&self) -> String {
        self.matches
            .get_one::<String>("format")
            .cloned()
            .unwrap_or_else(|| "json".to_owned())
    }

    /// Whether `--head` is set (history single-HEAD-commit mode).
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

/// The crate-owned report builder for one analyzer. Receives the resolved
/// [`RunContext`] and the already-resolved, ndjson-modified format string, and
/// returns the serialized report bytes (`None` when the analyzer cannot run for
/// this input — e.g. the repo cannot be walked — matching the Go path that
/// surfaces an error). The bytes MUST come from the analyzer crate's own
/// serializer (cf-gojson / cf-goyaml / cf-reportutil); this signature never
/// returns a model this module would re-encode, keeping all byte-shaping in the
/// owning crate.
pub type RunHandler = fn(ctx: &RunContext, format: &str) -> Option<Vec<u8>>;

/// One registry entry: an analyzer's id, its phase, and its crate-owned report
/// builder. The `formats` set is advisory documentation of which output formats
/// the handler supports; dispatch does not branch on it (the handler owns format
/// selection, returning `None` for an unsupported combination so the caller can
/// report the same "unsupported format" error Go does).
pub struct AnalyzerEntry {
    /// Canonical analyzer id, e.g. `static/complexity`, `history/burndown`.
    pub id: &'static str,
    /// Static (folder) or history (revwalk) phase.
    pub mode: Mode,
    /// The crate-owned report builder.
    pub run: RunHandler,
}

/// The analyzer registry: the single source of truth mapping analyzer id →
/// [`AnalyzerEntry`]. This is the Rust analogue of Go `analyze.Registry`
/// (`defaultUASTAnalyzers ++ defaultRawFileAnalyzers ++ defaultHistoryLeaves`),
/// built once and queried by id. Dispatch is a keyed lookup, NOT a per-format
/// match ladder.
pub struct Registry {
    entries: BTreeMap<&'static str, AnalyzerEntry>,
}

impl Registry {
    /// Builds an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Registry { entries: BTreeMap::new() }
    }

    /// Registers one analyzer entry, keyed by its id (Go `Registry.register`).
    pub fn register(&mut self, entry: AnalyzerEntry) {
        self.entries.insert(entry.id, entry);
    }

    /// Looks up an analyzer by id (Go `Registry.Descriptor`).
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&AnalyzerEntry> {
        self.entries.get(id)
    }

    /// Every registered id, sorted (Go `Registry.All` / `IDsByMode` feed
    /// `--list-analyzers`). Used by the binary's `--list-analyzers` path.
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.entries.keys().copied().collect()
    }

    /// Splits a requested id selection into (static, history) id lists,
    /// preserving the requested order within each phase. Mirrors Go
    /// `registry.Split`: the phase is taken from the registered entry's
    /// [`Mode`]. Unknown ids are dropped here (the caller reports them).
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

/// Errors the general pipeline can surface, mirroring the Go `run.go` error set
/// (`ErrNoAnalyzersSelected`, `ErrUnknownAnalyzer`, the format errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    /// No analyzer ids were selected (Go `ErrNoAnalyzersSelected`).
    NoAnalyzersSelected,
    /// A requested id is not in the registry (Go `ErrUnknownAnalyzer`).
    UnknownAnalyzer(String),
    /// The requested format is not supported (Go `formats.go` errors).
    UnsupportedFormat(String),
    /// An analyzer ran but could not produce a report for this input (the repo
    /// or folder could not be walked); carries the analyzer id.
    AnalyzerFailed(String),
}

impl core::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PipelineError::NoAnalyzersSelected => write!(
                f,
                "no analyzers selected. Use -a flag, e.g.: -a burndown,couples"
            ),
            PipelineError::UnknownAnalyzer(id) => write!(f, "unknown analyzer: {id}"),
            PipelineError::UnsupportedFormat(fmt) => write!(f, "unsupported format: {fmt}"),
            PipelineError::AnalyzerFailed(id) => write!(f, "analyzer {id} produced no report"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// The single general run pipeline. Resolves the analyzer set + per-phase
/// format, then dispatches every requested analyzer through `registry` by id,
/// returning each phase's serialized report bytes in selection order.
///
/// This is the one place dispatch happens; there is no per-`(analyzer, format)`
/// branching. Static analyzers are dispatched first (Go `runStaticPhase`), then
/// history analyzers (Go `runHistoryPhase`); within each phase the handlers run
/// in the user-requested order. Format resolution mirrors Go
/// `analyze.ResolveFormats` + the `--ndjson`/`timeseries` modifier.
///
/// # Errors
///
/// Returns [`PipelineError`] when the selection is empty, references an unknown
/// id, the format is unsupported, or an analyzer cannot produce a report.
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

    let (static_format, history_format) =
        resolve_formats(&ctx.raw_format(), !static_ids.is_empty(), !history_ids.is_empty())
            .map_err(|_| PipelineError::UnsupportedFormat(ctx.raw_format()))?;

    let mut outputs = Vec::with_capacity(ids.len());

    // Static phase: parse the folder once (each handler walks the same root);
    // dispatch each static analyzer by id (Go runStaticPhase).
    for id in &static_ids {
        let entry = registry
            .lookup(id)
            .ok_or_else(|| PipelineError::UnknownAnalyzer((*id).clone()))?;
        let bytes = (entry.run)(ctx, &static_format)
            .ok_or_else(|| PipelineError::AnalyzerFailed((*id).clone()))?;
        outputs.push(PhaseOutput { id: (*id).clone(), bytes });
    }

    // History phase: one revwalk feeds each history analyzer (Go
    // runHistoryPhase). The `--ndjson` modifier turns `timeseries` into
    // `timeseries+ndjson` exactly as Go `applyNDJSONModifier` does.
    let history_format = apply_ndjson_modifier(&history_format, ctx.ndjson());
    for id in &history_ids {
        let entry = registry
            .lookup(id)
            .ok_or_else(|| PipelineError::UnknownAnalyzer((*id).clone()))?;
        let bytes = (entry.run)(ctx, &history_format)
            .ok_or_else(|| PipelineError::AnalyzerFailed((*id).clone()))?;
        outputs.push(PhaseOutput { id: (*id).clone(), bytes });
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_static(_ctx: &RunContext, _fmt: &str) -> Option<Vec<u8>> {
        Some(b"static-report".to_vec())
    }

    fn stub_history(_ctx: &RunContext, fmt: &str) -> Option<Vec<u8>> {
        Some(format!("history:{fmt}").into_bytes())
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

    fn run_matches(args: &[&str]) -> ArgMatches {
        crate::flags::build_run_command()
            .no_binary_name(true)
            .try_get_matches_from(args)
            .expect("parse run args")
    }

    #[test]
    fn split_routes_by_mode() {
        let r = test_registry();
        let ids = vec!["history/burndown".to_string(), "static/complexity".to_string()];
        let (s, h) = r.split(&ids);
        assert_eq!(s, vec![&"static/complexity".to_string()]);
        assert_eq!(h, vec![&"history/burndown".to_string()]);
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
        let m = run_matches(&["-a", "history/burndown,static/complexity", "--format", "json"]);
        let ctx = RunContext::from_matches(&m);
        let ids = vec!["history/burndown".to_string(), "static/complexity".to_string()];
        let out = run_pipeline(&r, &ctx, &ids).unwrap();
        assert_eq!(out.len(), 2);
        // Static is dispatched first regardless of request order.
        assert_eq!(out[0].id, "static/complexity");
        assert_eq!(out[1].id, "history/burndown");
    }

    #[test]
    fn ndjson_modifier_applies_to_history_format() {
        let r = test_registry();
        let m = run_matches(&[
            "-a",
            "history/burndown",
            "--format",
            "timeseries",
            "--ndjson",
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
}
