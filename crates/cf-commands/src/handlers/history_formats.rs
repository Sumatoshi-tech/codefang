//! Centralized history-phase output for the streaming/derived formats —
//! `text`, `ndjson`, `timeseries` (and `timeseries+ndjson`) — the Rust port of
//! the reference `analyze.OutputHistoryResults` + `analyze.StreamingSink` +
//! `analyze.BuildMergedTimeSeriesDirect` trio.
//!
//! Unlike json/yaml/bin (per-analyzer encodings dispatched through the
//! registry), the reference implementation routes these three formats through ONE history-phase output
//! function over the whole selected leaf set:
//!
//! - `text`: `PrintHeader` once, then per leaf `"<Name>:\n"` +
//!   `leaf.Serialize(res, "text", w)`. Only the leaves wired with a
//!   `SerializeTextFn` hook support text (sentiment, shotness, burndown,
//!   couples, devs, file-history); the rest fall through
//!   `writeMetricsToFormat`'s default arm and the run FAILS with
//!   `serialization error for <Name>: unsupported format: text` AFTER the
//!   header + section name were already written to stdout (exit 1).
//! - `ndjson`: one compact JSON line per non-nil per-commit TC
//!   (`{"hash","tick","author_id","timestamp","analyzer","data"}`), written in
//!   walk order during the pipeline; the final report is NOT emitted.
//! - `timeseries`: a merged `codefang.timeseries.v1` document built from the
//!   per-commit summaries of every leaf implementing
//!   `CommitTimeSeriesProvider`, ordered by `commits_by_tick` + `commit_meta`.
//!
//! This module owns the per-format byte shaping; the per-analyzer per-commit
//! payloads come from the walk functions in [`super::history`] and friends.

use cf_gojson::GoValue;

use crate::pipeline::RunContext;

/// A history-phase failure that occurs AFTER bytes were already streamed to
/// stdout (the reference implementation writes the text header/section names before the per-leaf
/// `Serialize` call can fail). The caller must emit `partial` to stdout, the
/// `Error: <message>` line to stderr, and exit 1 — exactly cobra's error path.
pub struct PartialFailure {
    /// Bytes the reference implementation would have already written to stdout before the error.
    pub partial: Vec<u8>,
    /// The error text after the `Error: ` prefix.
    pub message: String,
}

/// Result of a centralized history-format run: full stdout bytes on success,
/// [`PartialFailure`] when the reference implementation errors mid-stream. `None` means this module has
/// no implementation for the selection yet — the caller falls through to the
/// per-id pipeline (and its dispatch-blocked diagnostic), preserving the
/// previous behavior for unported combinations.
pub type HistoryFormatResult = Option<Result<Vec<u8>, PartialFailure>>;

/// The reference `HistoryAnalyzer.Name()` for each leaf id (the text/yaml section header
/// and the error-message subject). Most leaves default to the descriptor id;
/// shotness/couples/file-history/imports override it.
#[must_use]
pub fn leaf_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "history/imports" => "ImportsPerDeveloper",
        "history/typos" => "history/typos",
        "history/couples" => "Couples",
        "history/shotness" => "Shotness",
        "history/devs" => "history/devs",
        "history/anomaly" => "TemporalAnomaly",
        "history/quality" => "history/quality",
        "history/sentiment" => "history/sentiment",
        "history/file-history" => "FileHistoryAnalysis",
        "history/burndown" => "history/burndown",
        _ => return None,
    })
}

/// Appends the reference `analyze.PrintHeader` (`codefang (v2):` / version / hash).
fn print_header(out: &mut Vec<u8>) {
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
}

/// Whether the leaf's report is non-nil for this run — the reference implementation only prints the
/// `"<Name>:\n"` section (and only then can fail serialization) when
/// `results[leaf] != nil`, which requires actually running the analysis.
/// Reuses the same walk the json/yaml formats use, so the text path performs
/// the identical work the reference implementation does before erroring.
fn leaf_report_exists(id: &str, ctx: &RunContext) -> bool {
    let sub = ctx.matches;
    match id {
        "history/imports" => super::history::imports_run_metrics(sub).is_some(),
        "history/typos" => super::history::typos_report_data(sub).is_some(),
        "history/anomaly" => super::history::anomaly_run_metrics(sub).is_some(),
        "history/quality" => super::history::quality_metrics(sub).is_some(),
        _ => false,
    }
}

/// The text serializer for the leaves wired with the reference implementation's `SerializeTextFn` hook
/// (the [`super::history_text`] renderers). `None` for the four leaves with no
/// reference hook, making [`history_text`] take the unsupported-format error path.
fn leaf_text(id: &str, ctx: &RunContext) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    match id {
        "history/sentiment" => super::history_text::sentiment_text(sub),
        "history/shotness" => super::history_text::shotness_text(sub),
        "history/burndown" => super::history_text::burndown_text(sub),
        "history/couples" => super::history_text::couples_text(sub),
        "history/devs" => super::history_text::devs_text(sub),
        "history/file-history" => super::history_text::file_history_text(sub),
        _ => None,
    }
}

/// Whether the reference implementation wires a `SerializeTextFn` for this leaf (text is a supported
/// format). The other four history leaves fall through `writeMetricsToFormat`
/// to `unsupported format: text`.
fn leaf_supports_text(id: &str) -> bool {
    matches!(
        id,
        "history/sentiment"
            | "history/shotness"
            | "history/burndown"
            | "history/couples"
            | "history/devs"
            | "history/file-history"
    )
}

/// The centralized `--format text` history path (the reference `OutputHistoryResults`
/// with `format == "text"`): header once, then per leaf the section name and
/// the leaf's text serialization. A leaf without a text hook aborts the run
/// with the reference implementation's exact `serialization error` message AFTER the header + its
/// section name were written.
#[must_use]
pub fn history_text(ctx: &RunContext, ids: &[String]) -> HistoryFormatResult {
    // Every selected leaf must be one this module can either render or
    // faithfully fail; otherwise fall through to the per-id pipeline. (The
    // co-selected heavy leaves share ONE memoized UAST walk: the first leaf's
    // walk call computes it, the rest re-read it.)
    for id in ids {
        leaf_name(id)?;
        if leaf_supports_text(id) {
            // Reference-supported leaf whose Rust text renderer is not ported yet:
            // fall through (dispatch-blocked) rather than mis-serialize.
            leaf_text(id, ctx)?;
        }
    }

    let mut out = Vec::new();
    print_header(&mut out);

    for id in ids {
        let name = leaf_name(id).expect("checked above");
        if leaf_supports_text(id) {
            let body = leaf_text(id, ctx).expect("checked above");
            out.extend_from_slice(format!("{name}:\n").as_bytes());
            out.extend_from_slice(&body);
        } else {
            // The reference implementation runs the full analysis; only a non-nil report prints the
            // section name and reaches the failing Serialize call.
            if !leaf_report_exists(id, ctx) {
                return Some(Err(PartialFailure {
                    partial: out,
                    message: format!("analyzer history phase failed for {id}"),
                }));
            }
            out.extend_from_slice(format!("{name}:\n").as_bytes());
            return Some(Err(PartialFailure {
                partial: out,
                message: format!("serialization error for {name}: unsupported format: text"),
            }));
        }
    }

    Some(Ok(out))
}

// ---------------------------------------------------------------------------
// timeseries
// ---------------------------------------------------------------------------

/// One leaf's contribution to the merged timeseries: its `Flag()` plus the
/// per-commit summary values (the reference `ExtractCommitTimeSeries`, keyed by hash) and
/// the ordering data its report carries (`commits_by_tick` + `commit_meta`).
pub struct TimeSeriesContribution {
    /// The reference `Flag()` — the analyzer key inside each merged commit object.
    pub flag: &'static str,
    /// Per-commit summary values keyed by full hex hash (reference: provider data).
    pub per_commit: Vec<(String, GoValue)>,
    /// Ordered commit metadata derived from the report's `commits_by_tick` +
    /// `commit_meta`: `(hash, tick, rfc3339_timestamp, author)` in tick order.
    /// Empty when the leaf's report has no `commits_by_tick` (e.g. devs).
    pub commit_meta: Vec<(String, i64, String, String)>,
}

/// Per-leaf timeseries data source. `None` body when the leaf is not ported
/// yet. A leaf that has NO provider in the reference implementation (typos) returns a contribution with
/// empty `per_commit`, mirroring `collectProviderData` skipping it.
fn leaf_timeseries(id: &str, ctx: &RunContext) -> Option<TimeSeriesContribution> {
    let sub = ctx.matches;
    match id {
        // typos: no CommitTimeSeriesProvider and no commits_by_tick — the
        // merged document is structurally empty, but the reference implementation still runs the walk.
        "history/typos" => {
            super::history::typos_report_data(sub)?;
            Some(TimeSeriesContribution {
                flag: "typos",
                per_commit: Vec::new(),
                commit_meta: Vec::new(),
            })
        }
        "history/devs" => super::history::devs_timeseries_contribution(sub),
        "history/anomaly" => super::history::anomaly_timeseries_contribution(sub),
        "history/imports" => super::history::imports_timeseries_contribution(sub),
        "history/quality" => super::history::quality_timeseries_contribution(sub),
        "history/sentiment" => super::history::sentiment_timeseries_contribution(sub),
        "history/shotness" => super::shotness_run::shotness_timeseries_contribution(sub),
        "history/couples" => super::couples_run::couples_timeseries_contribution(sub),
        "history/file-history" => super::history::file_history_timeseries_contribution(sub),
        "history/burndown" => super::burndown_ndjson::burndown_timeseries_contribution(sub),
        _ => None,
    }
}

/// The centralized `--format timeseries` history path: collect provider data
/// from every leaf (flag-sorted, the reference `collectProviderData`), order commits by
/// the first report carrying `commits_by_tick`, and emit the merged
/// `codefang.timeseries.v1` document (2-space-indented JSON + trailing
/// newline, the reference `WriteMergedTimeSeries`). With `ndjson_lines` (the
/// `timeseries+ndjson` composition) each merged commit object is emitted as
/// one compact line instead.
#[must_use]
pub fn history_timeseries(
    ctx: &RunContext,
    ids: &[String],
    ndjson_lines: bool,
) -> HistoryFormatResult {
    // The co-selected heavy leaves share ONE memoized UAST walk: the first
    // leaf's contribution computes it, the rest re-read it.
    let mut contribs = Vec::with_capacity(ids.len());
    for id in ids {
        contribs.push(leaf_timeseries(id, ctx)?);
    }
    // The reference implementation sorts leaves by Flag() before collecting provider data.
    contribs.sort_by(|a, b| a.flag.cmp(b.flag));

    // Ordering comes from the FIRST leaf (selection order) whose report has
    // commits_by_tick; commit_meta enriches timestamps/authors.
    let commit_meta: &[(String, i64, String, String)] = contribs
        .iter()
        .find(|c| !c.commit_meta.is_empty())
        .map_or(&[], |c| c.commit_meta.as_slice());

    // active = leaves with non-empty provider data, flag-sorted.
    let active: Vec<&TimeSeriesContribution> = contribs
        .iter()
        .filter(|c| !c.per_commit.is_empty())
        .collect();

    let bytes = render_merged_timeseries(&active, commit_meta, ndjson_lines);
    Some(Ok(bytes))
}

/// Builds the merged-timeseries bytes (the reference `BuildMergedTimeSeriesDirect` +
/// `WriteMergedTimeSeries`/`WriteTimeSeriesNDJSON`).
fn render_merged_timeseries(
    active: &[&TimeSeriesContribution],
    commit_meta: &[(String, i64, String, String)],
    ndjson_lines: bool,
) -> Vec<u8> {
    use cf_gojson::{GoMap, MapOrigin};

    // The set of hashes any analyzer contributed.
    let mut in_set = std::collections::HashSet::new();
    for c in active {
        for (h, _) in &c.per_commit {
            in_set.insert(h.as_str());
        }
    }

    // Ordered commits: commit_meta order filtered to the contributed set.
    let mut commits: Vec<GoValue> = Vec::new();
    for (hash, tick, ts, author) in commit_meta {
        if !in_set.contains(hash.as_str()) {
            continue;
        }
        // The reference implementation marshals MergedCommitData through a map[string]any — keys are
        // SORTED by encoding/json (author, hash, <flags...>, tick, timestamp
        // interleave alphabetically).
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("hash", GoValue::Str(hash.clone()));
        m.push("timestamp", GoValue::Str(ts.clone()));
        m.push("author", GoValue::Str(author.clone()));
        m.push("tick", GoValue::Int(*tick));
        for c in active {
            if let Some((_, v)) = c.per_commit.iter().find(|(h, _)| h == hash) {
                m.push(c.flag, v.clone());
            }
        }
        commits.push(GoValue::Map(m));
    }

    // Top level is a STRUCT (field order fixed by the reference json tags).
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "version",
        GoValue::Str("codefang.timeseries.v1".to_string()),
    );
    root.push("tick_size_hours", GoValue::Int(24));
    root.push(
        "analyzers",
        GoValue::Array(
            active
                .iter()
                .map(|c| GoValue::Str(c.flag.to_string()))
                .collect(),
        ),
    );

    if ndjson_lines {
        // One compact line per merged commit object.
        let mut out = Vec::new();
        for c in commits {
            out.extend_from_slice(&cf_gojson::marshal(&c));
            out.push(b'\n');
        }
        return out;
    }

    root.push("commits", GoValue::Array(commits));
    let mut out = cf_gojson::Encoder::indented("  ").encode_to_vec(&GoValue::Map(root));
    // The reference `json.Encoder.Encode` appends a newline after the document.
    out.push(b'\n');
    out
}

// ---------------------------------------------------------------------------
// ndjson
// ---------------------------------------------------------------------------

/// One per-commit TC record (the reference `analyze.TC` after runner stamping): the
/// commit identity plus the analyzer-specific `data` payload.
pub struct NdjsonRecord {
    /// The commit's consume position in the walk (0-based). For leaves the reference implementation
    /// forks across workers this drives the drain order; sequential leaves
    /// emit in walk order regardless.
    pub pos: usize,
    /// Full hex commit hash.
    pub hash: String,
    /// Tick index (TicksSinceStart).
    pub tick: i64,
    /// Identity-detector author id.
    pub author_id: i64,
    /// Commit author time, Unix seconds.
    pub time_secs: i64,
    /// Commit author timezone offset, minutes.
    pub tz_offset_min: i32,
    /// The analyzer-specific payload (`data`); records with no payload are
    /// never constructed (the reference implementation skips nil-Data TCs).
    pub data: GoValue,
}

/// Whether the reference implementation forks this leaf across `LeafWorkers` (declared `Sequential:
/// false` and `analyze.Parallelizable`). Forked leaves' TCs reach the NDJSON
/// sink drained worker-by-worker: consume position `p` goes to worker `p % W`
/// (`W` = [`super::leaf_worker_count`]), so the emitted line order is the
/// commits stably reordered by `(p % W, p)` — oracle-verified on
/// anomaly@hercules. `burndown` and `devs` declare `Sequential: true` and emit
/// in plain walk order.
fn leaf_forked(id: &str) -> bool {
    !matches!(id, "history/burndown" | "history/devs")
}

/// Per-leaf ndjson record source. `None` when the leaf is not ported yet.
fn leaf_ndjson(id: &str, ctx: &RunContext) -> Option<Vec<NdjsonRecord>> {
    let sub = ctx.matches;
    match id {
        "history/devs" => super::history::devs_ndjson_records(sub),
        "history/anomaly" => super::history::anomaly_ndjson_records(sub),
        "history/imports" => super::history::imports_ndjson_records(sub),
        "history/quality" => super::history::quality_ndjson_records(sub),
        "history/typos" => super::history::typos_ndjson_records(sub),
        "history/sentiment" => super::history::sentiment_ndjson_records(sub),
        "history/shotness" => super::shotness_run::shotness_ndjson_records(sub),
        "history/couples" => super::couples_run::couples_ndjson_records(sub),
        "history/file-history" => super::history::file_history_ndjson_records(sub),
        _ => None,
    }
}

/// The reference `Flag()` per leaf id (the `"analyzer"` field of each ndjson line and the
/// analyzer key in the merged timeseries).
#[must_use]
pub fn leaf_flag(id: &str) -> Option<&'static str> {
    Some(match id {
        "history/imports" => "imports-per-dev",
        "history/typos" => "typos",
        "history/couples" => "couples",
        "history/shotness" => "shotness",
        "history/devs" => "devs",
        "history/anomaly" => "anomaly",
        "history/quality" => "quality",
        "history/sentiment" => "sentiment",
        "history/file-history" => "file-history",
        "history/burndown" => "burndown",
        _ => return None,
    })
}

/// The centralized `--format ndjson` history path: one
/// compact JSON line per non-nil TC, fields `hash`/`tick`/`author_id`/
/// `timestamp`/`analyzer`/`data` in struct order, RFC3339 timestamp.
#[must_use]
pub fn history_ndjson(ctx: &RunContext, ids: &[String]) -> HistoryFormatResult {
    use cf_gojson::{GoMap, MapOrigin};

    // The co-selected heavy leaves share ONE memoized UAST walk: the first
    // leaf's records compute it, the rest re-read it.
    let mut out = Vec::new();
    for id in ids {
        let flag = leaf_flag(id)?;
        let mut records = leaf_ndjson(id, ctx)?;
        if leaf_forked(id) {
            let w = super::leaf_worker_count();
            records.sort_by_key(|r| (r.pos % w, r.pos));
        }
        for r in records {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("hash", GoValue::Str(r.hash));
            m.push("tick", GoValue::Int(r.tick));
            m.push("author_id", GoValue::Int(r.author_id));
            m.push(
                "timestamp",
                GoValue::Str(super::format_rfc3339_offset(r.time_secs, r.tz_offset_min)),
            );
            m.push("analyzer", GoValue::Str(flag.to_string()));
            m.push("data", r.data);
            out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(m)));
            out.push(b'\n');
        }
    }
    Some(Ok(out))
}
