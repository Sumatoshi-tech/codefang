//! Analyzer report handlers + the default registry wiring.
//!
//! Each submodule is the crate-owned orchestration for one analyzer family
//! (static folder walk + per-file UAST analysis, or one git revwalk + per-commit
//! analysis), moved verbatim out of the `codefang` binary's `main.rs` where it
//! had previously been reached through a 31-arm per-`(analyzer, format)` `if`
//! ladder. The analyzer MATH stays in the cf-* analyzer crates these call
//! (cf-complexity, cf-halstead, cf-comments, cf-imports, cf-couples,
//! cf-shotness, cf-analyzer-burndown, …); this module owns only the
//! pipeline-tier orchestration + serialization (cf-gojson / cf-goyaml /
//! cf-reportutil), exactly as Go `internal/framework` + `run.go` do.
//!
//! [`default_registry`] builds the single [`crate::pipeline::Registry`] mapping
//! every analyzer id to ONE [`crate::pipeline::RunHandler`]. Each handler owns
//! its own `match format` (mirroring an analyzer's `FormatReport*` family in
//! Go); dispatch in [`crate::pipeline::run_pipeline`] is a keyed lookup by id,
//! NOT a per-format branch ladder.

pub mod burndown_ndjson;
pub mod couples_run;
pub mod go_sort;
pub mod history;
pub mod history_formats;
pub mod history_text;
pub mod plot;
pub mod plot_sections;
pub mod section_render;
pub mod shotness_run;
pub mod static_clones;
pub mod static_cohesion;
pub mod static_comments;
pub mod static_complexity;
pub mod static_complexity_bin;
pub mod static_complexity_yaml;
pub mod static_halstead;
pub mod static_imports;
pub mod static_json;

use crate::pipeline::{AnalyzerEntry, Mode, Registry, RunContext};

// ---------------------------------------------------------------------------
// Shared pipeline helpers (path resolution, tick floor, RFC3339 formatting).
// These mirror the Go run.go / plumbing helpers and are shared by the static
// and history handlers.
// ---------------------------------------------------------------------------

/// Resolves the repository path from `run`'s positional arg or `-p/--path`
/// (Go run.go: the positional wins when present, else `--path`, default `.`).
#[must_use]
pub fn run_repo_path(sub: &clap::ArgMatches) -> String {
    if let Some(p) = sub.get_one::<String>("path-positional") {
        if !p.is_empty() {
            return p.clone();
        }
    }
    sub.get_one::<String>("path").cloned().unwrap_or_else(|| ".".to_string())
}

/// The effective first-parent mode for the shared history revwalk, mirroring Go
/// `run.go` `initHistoryPipeline`:
///
/// ```go
/// if slices.Contains(analyzerKeys, "burndown") && !opts.FirstParent {
///     opts.FirstParent = true
/// }
/// ```
///
/// Go forces first-parent for the WHOLE history run (the single shared revwalk
/// that feeds every selected history analyzer) whenever `history/burndown` is in
/// the resolved leaf set, regardless of the `--first-parent` flag. Because every
/// history analyzer in one `run` shares that revwalk, the window selection — and
/// therefore each analyzer's tick assignment and commit set — must observe the
/// same forced flag. A handler that read only `--first-parent` would diverge from
/// Go whenever burndown is co-selected (e.g. `--analyzers history/devs,history/burndown`,
/// `history/*`, or `*`), even though it is not the burndown handler.
///
/// The burndown membership is computed over the RESOLVED leaf set, so literal ids
/// and globs (`history/*`, `*`) that select burndown all force the flag, exactly
/// as Go's `slices.Contains(analyzerKeys, "burndown")` does after glob expansion.
#[must_use]
pub fn effective_first_parent(sub: &clap::ArgMatches) -> bool {
    if sub.get_flag("first-parent") {
        return true;
    }
    let patterns: Vec<String> = sub
        .get_many::<String>("analyzers")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let pats: Vec<&str> = patterns.iter().map(String::as_str).collect();
    let (_static_ids, history_ids) = expand_combined_ids(&pats);
    history_ids.iter().any(|id| id == "history/burndown")
}

/// Replicates Go `run.go` `initHistoryPipeline` (the iterator path the real
/// `run` command uses — NOT `gitlib.LoadCommits`): walks history oldest-first
/// (`SortTime|SortTopological|SortReverse`) and feeds the analyzer the FIRST
/// `commitCount = min(limit, total)` commits. That selects the N OLDEST
/// reachable commits, oldest-first (oracle-verified against the live Go binary —
/// `--limit 20` on hercules yields the repo's first 20 commits, with ascending
/// composition ticks). `limit <= 0` returns the full oldest-first history.
#[must_use]
pub fn load_history_commit_hashes(
    repo: &cf_gitlib::Repository,
    limit: i64,
    first_parent: bool,
) -> Option<Vec<cf_gitlib::Hash>> {
    use cf_gitlib::repository::LogOptions;
    // ORACLE-VERIFIED window selection. The real `run` command uses
    // `run.go::initStreamingIterator`, which sets `logOpts.Reverse = true`
    // (oldest-first walk) and then streams the FIRST `commitCount =
    // min(limit, total)` commits — i.e. the `limit` OLDEST reachable commits,
    // oldest-first. (NOT `gitlib.loadHistoryCommits`'s newest-N+reverse: the live
    // Go binary at `--limit 2` on hercules emits the repo's first two commits —
    // analyser.go/LICENSE — proving the OLDEST set is selected, even though the
    // repo has 1006 commits.) Do NOT switch to `reverse: false` + post-reverse.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }
    Some(hashes)
}

/// Returns Go's streaming-pipeline *consume* order for an oldest-first commit
/// window: the IDENTITY (oldest-first revwalk order).
///
/// At `--workers 1` (the only config the differential gate exercises) every
/// stage of the Go coordinator pipeline preserves input order, so the leaf
/// analyzers consume commits in exactly the oldest-first order they are fed:
///
/// * `framework/commit_streamer.go` `Stream` emits contiguous oldest-first
///   batches (`commits[i:end]`) from a single goroutine.
/// * `framework/blob_pipeline.go` and `framework/diff_pipeline.go` use
///   `pipeline.RunPC` (`pkg/pipeline/runpc.go`): a single producer emits jobs
///   in input order onto one FIFO channel, and a single consumer reads and
///   emits them in that same order — the parallel blob/diff prefetch only
///   shares one batched worker request, it never resequences commits.
/// * `framework/uast_pipeline.go` `Process` is explicitly order-preserving
///   ("Output order matches input order via a slot-based approach"): the
///   `emit` goroutine waits on each slot's `done` in dispatch order.
/// * `framework/coordinator.go` `Process` and `pkg/pipeline/drain.go`
///   `SignalOnDrain` each forward items one-for-one from a single goroutine.
/// * `framework/runner.go` `processCommitsSerial`/`hybridCommitLoop` range over
///   the coordinator's `dataChan` in arrival order, and the per-commit `Index`
///   carried through is the plain oldest-first revwalk index
///   (`blob_pipeline.go`: `batch.StartIndex + job.index`).
///
/// So the order in which the COORDINATOR pipeline yields commits is the identity
/// of the oldest-first window. Earlier code reordered into round-robin
/// `PIPELINE_CHUNK` blocks here; that was incorrect — no pipeline stage performs
/// that reordering.
///
/// This is the order the CORE (plumbing) analyzers consume — notably the
/// `IdentityDetector`, which assigns loose author ids strictly oldest-first. It
/// is NOT necessarily the order in which a LEAF analyzer's order-sensitive state
/// is updated: at the default `LeafWorkers = max(NumCPU / 3, 4)` (which
/// `--workers` does NOT override — that flag only sets the blob/diff `Workers`
/// pool), Go's hybrid leaf path (`runner.go` `processCommitsHybrid`) forks the
/// leaf across workers, dispatching consume position `p` to worker `p % W`. The
/// effect is leaf-specific and handled at the leaf consumer, not here:
///   - couples: each fork has an INDEPENDENT seen-files Bloom; commits stay in
///     oldest-first order WITHIN a worker (see `couples_run`).
///   - file-history: forked TCs are drained worker-by-worker into one aggregator
///     whose `applyInsert` resets a path's hash list, so its add-order is the
///     commits stably reordered by `(p % W, p)` (see `file_history_run`).
/// Both use [`leaf_worker_count`] for `W`, reproducing the live binary on this
/// machine.
#[must_use]
pub fn pipeline_consume_order(hashes: Vec<cf_gitlib::Hash>) -> Vec<cf_gitlib::Hash> {
    hashes
}

/// Go leaf-worker divisor (`framework` `leafWorkerDivisor`): `LeafWorkers =
/// NumCPU / divisor`.
const LEAF_WORKER_DIVISOR: usize = 3;
/// Go minimum leaf-worker count (`framework` `minLeafWorkers`).
const MIN_LEAF_WORKERS: usize = 4;

/// Number of forked leaf-analyzer workers Go dispatches commits across, mirroring
/// `framework.DefaultCoordinatorConfig`: `max(NumCPU / 3, 4)`, where `NumCPU` is
/// the machine's logical CPU count (Go `runtime.NumCPU`).
///
/// Go's hybrid leaf path (`runner.go` `processCommitsHybrid`, taken for a single
/// non-`SequentialOnly` leaf when `0 < CoreCount < len(Analyzers)`) forks the
/// leaf across this many workers and dispatches consume position `p` to worker
/// `p % count`, each worker holding INDEPENDENT analyzer state (e.g. couples'
/// seen-files Bloom, file-history's per-path map). That makes the order-sensitive
/// parts of those analyzers depend on this count, so a byte-exact port must use
/// the same value as the live binary on this machine (which the oracle also runs
/// on). The `--workers` flag only overrides `Workers` (the blob/diff pool), never
/// `LeafWorkers`, so this is unaffected by `--workers 1`.
#[must_use]
pub fn leaf_worker_count() -> usize {
    let num_cpu = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (num_cpu / LEAF_WORKER_DIVISOR).max(MIN_LEAF_WORKERS)
}

/// Rounds Unix `secs` down to the start of its 24-hour tick (Go
/// `plumbing.FloorTime(when, 24h)`). `time.Round` rounds half away from zero;
/// the post-round correction yields the floor.
#[must_use]
pub fn floor_tick_secs(secs: i64) -> i64 {
    const PERIOD: i64 = 86_400;
    let rounded = ((secs + PERIOD / 2).div_euclid(PERIOD)) * PERIOD;
    if rounded > secs {
        rounded - PERIOD
    } else {
        rounded
    }
}

/// Formats Unix seconds as Go `time.RFC3339` in the zone given by
/// `offset_minutes`; a zero offset prints the literal `Z`.
#[must_use]
pub fn format_rfc3339_offset(unix_secs: i64, offset_minutes: i32) -> String {
    let local = unix_secs + i64::from(offset_minutes) * 60;
    let days = local.div_euclid(86400);
    let secs_of_day = local.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    let date = format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}");
    if offset_minutes == 0 {
        format!("{date}Z")
    } else {
        let sign = if offset_minutes < 0 { '-' } else { '+' };
        let abs = offset_minutes.unsigned_abs();
        format!("{date}{sign}{:02}:{:02}", abs / 60, abs % 60)
    }
}

/// Serializes a history analyzer's report value across the json/yaml/bin
/// machine formats, mirroring Go `OutputHistoryResults` +
/// `BaseHistoryAnalyzer.Serialize` (`writeMetricsToFormat`): the *same* report
/// value is encoded each way, so a handler computes the value once and routes
/// it here rather than re-deriving per format.
///
/// - `json` (a "raw" format): `json.Marshal(metrics.ToJSON())` — `json_value`,
///   no header, no trailing newline (cf-gojson `marshal`).
/// - `binary` (a "raw" format): `reportutil.EncodeBinaryEnvelope(metrics)` — a
///   CFB1 envelope wrapping the same `json_value` bytes (no header).
/// - `yaml` (non-raw): `PrintHeader` (`codefang (v2):` / version / hash) then
///   `<analyzer_name>:\n` then `yaml.Marshal(metrics.ToYAML())` — `yaml_value`.
///
/// `analyzer_name` is the history analyzer's `Name()` (the YAML section header,
/// e.g. `ImportsPerDeveloper`). Returns `None` for any non-machine format, so
/// the caller surfaces the same dispatch error Go does.
#[must_use]
pub fn serialize_history_metrics(
    format: &str,
    analyzer_name: &str,
    json_value: &cf_gojson::GoValue,
    yaml_value: &cf_gojson::GoValue,
) -> Option<Vec<u8>> {
    match format {
        // Raw formats: no version header, no per-analyzer section name.
        "json" => Some(cf_gojson::marshal(json_value)),
        "binary" | "bin" => cf_reportutil::encode_binary_envelope(json_value).ok(),
        // Non-raw: PrintHeader + "<Name>:\n" + yaml body.
        "yaml" => {
            let mut out = Vec::new();
            out.extend_from_slice(b"codefang (v2):\n");
            out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
            out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
            out.extend_from_slice(format!("{analyzer_name}:\n").as_bytes());
            out.extend_from_slice(&cf_goyaml::marshal(yaml_value));
            Some(out)
        }
        _ => None,
    }
}

/// Civil date from a day count since the Unix epoch (Howard Hinnant's algorithm).
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// Static analyzer glob/bin helpers (registry-ordered multi-analyzer bin output).
// ---------------------------------------------------------------------------

/// The static analyzers in registry order (Go `defaultUASTAnalyzers ++
/// defaultRawFileAnalyzers`). `bin_ported` is true for the analyzers whose
/// `--format bin` payload is reproduced byte-for-byte; cohesion is not yet
/// ported. clones IS ported: its bin payload is the CFB1 envelope of
/// `computeMetricsFromReport` over the cross-file aggregate report.
pub const STATIC_BIN_ANALYZERS: &[(&str, bool)] = &[
    ("static/clones", true),
    ("static/complexity", true),
    ("static/comments", true),
    ("static/halstead", true),
    ("static/cohesion", true),
    ("static/imports", true),
    ("static/composition", true),
];

/// True when `pat` is a literal static analyzer ID or a glob that could match
/// static IDs (and no history ID).
#[must_use]
pub fn is_static_id_or_glob(pat: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        let any_static = STATIC_BIN_ANALYZERS.iter().any(|(id, _)| go_path_match(pat, id));
        any_static && !history_glob_matches(pat)
    } else {
        STATIC_BIN_ANALYZERS.iter().any(|(id, _)| *id == pat)
    }
}

/// True when the glob matches any known history analyzer ID.
#[must_use]
pub fn history_glob_matches(pat: &str) -> bool {
    const HISTORY_IDS: &[&str] = &[
        "history/burndown",
        "history/couples",
        "history/devs",
        "history/file-history",
        "history/imports",
        "history/shotness",
        "history/typos",
        "history/sentiment",
        "history/quality",
        "history/anomaly",
    ];
    HISTORY_IDS.iter().any(|id| go_path_match(pat, id))
}

/// Expands the requested patterns over the registry-ordered static analyzers and
/// concatenates each selected analyzer's CFB1 bin envelope (Go
/// `FormatPerAnalyzer(FormatBinary)`). Returns `None` if any selected analyzer
/// is not ported (clones/cohesion) or any folder walk fails.
#[must_use]
pub fn static_multi_bin(patterns: &[&str], path: &str) -> Option<Vec<u8>> {
    let mut selected: Vec<(&str, bool)> = Vec::new();
    for &(id, ported) in STATIC_BIN_ANALYZERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if matched {
            selected.push((id, ported));
        }
    }
    if selected.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (id, ported) in selected {
        if !ported {
            return None;
        }
        let env = static_single_bin(id, path)?;
        out.extend_from_slice(&env);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Static analyzer multi-analyzer JSON merge (Go `renderer.SectionsToJSON` over
// several analyzers). For `codefang run --format json` with more than one static
// analyzer (or a glob that selects several), Go renders ONE `renderer.JSONReport`
// whose `sections` are the per-analyzer sections in REGISTRY order and whose
// `overall_score` is the executive-summary average of the scored sections
// (info-only sections, score < 0, are excluded; all-info ⇒ overall is -1 / Info).
// Each analyzer's section value comes from its own crate-owned report builder
// (the same GoValue the single-analyzer JSON path serializes), so the merge owns
// no analyzer math and every format follows the same report value.
// ---------------------------------------------------------------------------

/// Registry-ordered (Go `defaultUASTAnalyzers ++ defaultRawFileAnalyzers`) map of
/// static analyzer id → the crate-owned builder of that analyzer's single-section
/// `renderer.JSONReport` GoValue. Used by [`static_multi_json`] to merge several
/// analyzers' sections; the merge never branches per format — the same GoValue
/// feeds the serializer.
type ReportValueFn = fn(&str) -> Option<GoValue>;

use cf_gojson::{GoMap, GoValue, MapOrigin};

const STATIC_JSON_VALUE_BUILDERS: &[(&str, ReportValueFn)] = &[
    ("static/clones", static_clones::clones_report_value),
    ("static/complexity", static_complexity::complexity_report_value),
    ("static/comments", static_comments::comments_report_value),
    ("static/halstead", static_halstead::halstead_report_value),
    ("static/cohesion", static_cohesion::cohesion_report_value),
    ("static/imports", static_imports::imports_report_value),
    ("static/composition", static_json::composition_report_value),
];

/// Pulls the `sections` array and `overall_score` out of a single-analyzer
/// `renderer.JSONReport` GoValue (`{overall_score_label, sections, overall_score}`).
/// Returns the section GoValues and the contained `overall_score` (each section's
/// own `score` field is what the merge re-averages, but the single-analyzer
/// `overall_score` equals that section's score for one section, so we read the
/// per-section `score` directly for robustness against future multi-section
/// analyzers).
fn extract_sections(report: &GoValue) -> Vec<GoValue> {
    report
        .as_map()
        .and_then(|m| m.get("sections"))
        .and_then(|s| match s {
            GoValue::Array(items) => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Reads a section's numeric `score` field (Go `JSONSection.Score`), defaulting
/// to the info-only sentinel `-1.0` when absent.
fn section_score(section: &GoValue) -> f64 {
    match section.as_map().and_then(|m| m.get("score")) {
        Some(GoValue::Float(f)) => *f,
        Some(GoValue::Int(i)) => *i as f64,
        _ => -1.0,
    }
}

/// Go `terminal.FormatScore`: `round(score*10)/10` → `"N/10"`; a negative score
/// (info-only) renders `"Info"` (Go `ExecutiveSummary.OverallScoreLabel`).
fn overall_score_label(score: f64) -> String {
    if score < 0.0 {
        return "Info".to_string();
    }
    let n = (score * 10.0).round() as i64;
    format!("{n}/10")
}

/// Expands `patterns` over the registry-ordered static analyzers and renders ONE
/// merged `renderer.JSONReport` (Go `renderer.SectionsToJSON`): sections in
/// registry order, `overall_score` the average of the scored (`score >= 0`)
/// sections (or `-1` when none are scored). `None` if no static analyzer is
/// selected or any selected analyzer cannot produce a report (the caller then
/// falls through to the same error path Go takes).
#[must_use]
pub fn static_multi_json(patterns: &[&str], path: &str) -> Option<Vec<u8>> {
    let mut sections: Vec<GoValue> = Vec::new();
    let mut score_total = 0.0_f64;
    let mut score_count = 0_usize;

    for &(id, build) in STATIC_JSON_VALUE_BUILDERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if !matched {
            continue;
        }
        let report = build(path)?;
        for section in extract_sections(&report) {
            let s = section_score(&section);
            if s >= 0.0 {
                score_total += s;
                score_count += 1;
            }
            sections.push(section);
        }
    }

    if sections.is_empty() {
        return None;
    }

    let overall = if score_count == 0 {
        -1.0
    } else {
        score_total / score_count as f64
    };

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(overall_score_label(overall)));
    root.push("sections", GoValue::Array(sections));
    root.push("overall_score", GoValue::Float(overall));

    let bytes = cf_gojson::Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&GoValue::Map(root));
    Some(bytes)
}

/// True when `patterns` select MORE THAN ONE static analyzer (a literal multi-id
/// list or a glob matching several), so the JSON path must merge sections rather
/// than emit a single-analyzer document.
#[must_use]
pub fn static_json_selects_multiple(patterns: &[&str]) -> bool {
    let mut matched = 0usize;
    for &(id, _) in STATIC_JSON_VALUE_BUILDERS {
        let hit = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if hit {
            matched += 1;
            if matched > 1 {
                return true;
            }
        }
    }
    false
}

/// Produces a single static analyzer's CFB1 bin envelope.
#[must_use]
pub fn static_single_bin(id: &str, path: &str) -> Option<Vec<u8>> {
    match id {
        "static/clones" => static_clones::clones_report_bin(path),
        "static/complexity" => static_complexity_bin::complexity_report_bin(path),
        "static/comments" => static_comments::comments_report_bin(path),
        "static/halstead" => static_halstead::halstead_bin_report(path),
        "static/cohesion" => static_cohesion::cohesion_report_bin(path),
        "static/imports" => static_imports::imports_report_bin(path),
        "static/composition" => static_json::composition_bin(path),
        _ => None,
    }
}

/// The history analyzers in Go phase/registry order (Go `defaultHistoryLeaves`
/// as emitted by the combined unified-model path). Used to expand `*`/globs and
/// to order the history phase of the combined static+history render.
pub const HISTORY_COMBINED_ORDER: &[&str] = &[
    "history/typos",
    "history/file-history",
    "history/imports",
    "history/shotness",
    "history/anomaly",
    "history/burndown",
    "history/couples",
    "history/devs",
    "history/quality",
    "history/sentiment",
];

/// The history analyzers in Go's *separate-phase* per-analyzer emit order — the
/// order `runHistoryPhase` writes each leaf's standalone report when the run is
/// NOT a mixed static+history combined render (i.e. a history-only selection,
/// literal list or glob, in a machine format). This is the pipeline leaf order
/// (`pl.Leaves` → `selectLeaves`), which differs from both the registry id sort
/// and [`HISTORY_COMBINED_ORDER`]. Verified against the live Go binary
/// (`--analyzers history/* --format json`): the concatenated per-analyzer reports
/// appear in exactly this sequence.
pub const HISTORY_PHASE_EMIT_ORDER: &[&str] = &[
    "history/quality",
    "history/sentiment",
    "history/shotness",
    "history/couples",
    "history/imports",
    "history/typos",
    "history/anomaly",
    "history/burndown",
    "history/devs",
    "history/file-history",
];

/// Expands a requested pattern list into the concrete history leaf ids it
/// selects, in Go's separate-phase emit order ([`HISTORY_PHASE_EMIT_ORDER`]).
/// Literal ids match exactly; globs use Go `path.Match` semantics. Used by the
/// history-only-glob per-analyzer concatenation path (Go `runHistoryPhase` over a
/// glob-expanded selection), so a `history/*` or multi-id history selection emits
/// each leaf's standalone report in the same order Go does.
/// Whether any requested pattern selects the analyzer `id`, mirroring Go
/// `Registry.resolvePattern` (registry.go): a bare `*` matches EVERY id (Go
/// special-cases `pattern == "*"` to `allIDs()` BEFORE `path.Match`, because
/// `path.Match("*", "history/typos")` is false — `*` does not cross `/`); other
/// globs use Go `path.Match` semantics ([`go_path_match`]); a literal id matches
/// exactly. Without the `*` special case, `--analyzers '*'` would select nothing,
/// while `--analyzers 'history/*'` would still work (the literal `history/`
/// prefix anchors the match).
#[must_use]
fn pattern_selects_id(patterns: &[&str], id: &str) -> bool {
    let is_glob = |p: &str| p.contains(['*', '?', '[']);
    patterns.iter().any(|p| {
        if *p == "*" {
            true
        } else if is_glob(p) {
            go_path_match(p, id)
        } else {
            *p == id
        }
    })
}

/// Expands a requested pattern list into the concrete history leaf ids it
/// selects, in Go's separate-phase emit order ([`HISTORY_PHASE_EMIT_ORDER`]).
#[must_use]
pub fn expand_history_phase_ids(patterns: &[&str]) -> Vec<String> {
    let selected = |id: &str| pattern_selects_id(patterns, id);
    HISTORY_PHASE_EMIT_ORDER
        .iter()
        .filter(|id| selected(id))
        .map(|id| (*id).to_string())
        .collect()
}

/// Expands the requested analyzer patterns into concrete (static, history) id
/// lists in Go combined-model order: static analyzers in [`STATIC_BIN_ANALYZERS`]
/// registry order, then history analyzers in [`HISTORY_COMBINED_ORDER`]. Literal
/// (non-glob) ids are matched exactly; globs use Go `path.Match` semantics. This
/// mirrors Go `registry.Split` + `combinedIDsAndModes` ordering used by the
/// combined render.
#[must_use]
pub fn expand_combined_ids(patterns: &[&str]) -> (Vec<String>, Vec<String>) {
    let matches = |id: &str| pattern_selects_id(patterns, id);
    let statics: Vec<String> = STATIC_BIN_ANALYZERS
        .iter()
        .filter(|(id, _)| matches(id))
        .map(|(id, _)| (*id).to_string())
        .collect();
    let history: Vec<String> = HISTORY_COMBINED_ORDER
        .iter()
        .filter(|id| matches(id))
        .map(|id| (*id).to_string())
        .collect();
    (statics, history)
}

/// Renders the combined static+history run as the single `codefang.run.v1`
/// unified-model envelope, the Rust analogue of Go `renderCombinedDirect`
/// (run.go:678). Each selected analyzer is dispatched through its registry
/// handler with `--format bin`, producing a CFB1 envelope whose payload is the
/// analyzer's raw report JSON. The concatenated envelopes are decoded into a
/// [`cf_analyze::conversion::UnifiedModel`] (Go `DecodeCombinedBinaryReports`),
/// stamped with run metadata (Go `NewAnalysisMetadata`), and re-serialized in the
/// requested `output_format` via [`cf_analyze::conversion::write_converted_output`]
/// so every machine format (json/yaml/bin/ndjson/timeseries) follows from the
/// one model value.
///
/// Returns `None` if any selected analyzer cannot produce its bin payload (e.g.
/// an unported history analyzer), so the caller can fall back to the per-analyzer
/// pipeline rather than emit a partial envelope.
#[must_use]
pub fn render_combined(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
    output_format: &str,
) -> Option<Vec<u8>> {
    let registry = default_registry();
    let mut raw: Vec<u8> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut modes: Vec<cf_analyze::AnalyzerMode> = Vec::new();

    // Static phase, then history phase — Go renderCombinedDirect order. Each
    // handler is dispatched with the literal "bin" format; its CFB1 envelope is
    // appended to the combined buffer (Go staticExec/historyExec into &raw).
    // Each leaf's raw report is gathered via its CFB1 bin envelope. Handlers
    // match on the NORMALIZED format name (Go `ValidateFormat` maps the `bin`
    // alias to `binary`), so pass the normalized name here — passing the bare
    // `bin` alias would miss any handler that only accepts `binary` (e.g.
    // static/halstead), aborting the whole combined render.
    for id in static_ids {
        let entry = registry.lookup(id)?;
        let env = (entry.run)(ctx, "binary")?;
        raw.extend_from_slice(&env);
        ids.push(id.clone());
        modes.push(cf_analyze::AnalyzerMode::static_mode());
    }
    for id in history_ids {
        let entry = registry.lookup(id)?;
        let env = (entry.run)(ctx, "binary")?;
        raw.extend_from_slice(&env);
        ids.push(id.clone());
        modes.push(cf_analyze::AnalyzerMode::history());
    }

    let mut model = cf_analyze::conversion::decode_combined_binary_reports(&raw, &ids, &modes).ok()?;
    model.metadata = Some(cf_analyze::metadata::new_analysis_metadata(&ctx.path));

    // Normalize the requested format to the canonical name the conversion
    // serializer matches on (Go ValidateUniversalFormat: "bin" -> "binary",
    // case-folded), then apply the --ndjson modifier on timeseries exactly as
    // Go renderCombinedDirect does.
    let normalized = crate::formats::normalize_format(output_format);
    let render_format = if ctx.ndjson() && normalized == "timeseries" {
        "timeseries+ndjson".to_string()
    } else {
        normalized
    };

    let mut out: Vec<u8> = Vec::new();
    cf_analyze::conversion::write_converted_output(&model, &render_format, &mut out, None).ok()?;
    Some(out)
}

/// Go `path.Match` semantics over an analyzer ID (`*`, `?`, `[...]`).
#[must_use]
pub fn go_path_match(pattern: &str, name: &str) -> bool {
    go_path_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn go_path_match_inner(mut pat: &[u8], mut name: &[u8]) -> bool {
    while !pat.is_empty() {
        match pat[0] {
            b'*' => {
                while !pat.is_empty() && pat[0] == b'*' {
                    pat = &pat[1..];
                }
                if pat.is_empty() {
                    return !name.contains(&b'/');
                }
                let mut i = 0;
                loop {
                    if go_path_match_inner(pat, &name[i..]) {
                        return true;
                    }
                    if i >= name.len() || name[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            b'?' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
            b'[' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                let (matched, rest) = match_class(&pat[1..], name[0]);
                if !matched {
                    return false;
                }
                pat = rest;
                name = &name[1..];
            }
            c => {
                if name.is_empty() || name[0] != c {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
        }
    }
    name.is_empty()
}

fn match_class(pat: &[u8], ch: u8) -> (bool, &[u8]) {
    let mut i = 0;
    let mut negate = false;
    if i < pat.len() && (pat[i] == b'^' || pat[i] == b'!') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    while i < pat.len() && pat[i] != b']' {
        let lo = pat[i];
        i += 1;
        if i + 1 < pat.len() && pat[i] == b'-' && pat[i + 1] != b']' {
            let hi = pat[i + 1];
            i += 2;
            if lo <= ch && ch <= hi {
                matched = true;
            }
        } else if lo == ch {
            matched = true;
        }
    }
    let rest = if i < pat.len() { &pat[i + 1..] } else { &pat[i..] };
    (matched ^ negate, rest)
}

// ---------------------------------------------------------------------------
// Per-analyzer registry handlers. Each owns its own `match format`; this is the
// ONE place that knows how to format a given analyzer (Go: each analyzer's
// FormatReport* family). One handler per analyzer id — NOT one per (id,format).
// ---------------------------------------------------------------------------

fn h_static_clones(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized value: `--format bin` arrives here as
    // `"binary"` (Go `ValidateFormat` maps the `bin` alias to `binary`); accept
    // both spellings for robustness.
    match format {
        "json" => static_clones::clones_report_json(path),
        "yaml" => static_clones::clones_report_yaml(path),
        "binary" | "bin" => static_clones::clones_report_bin(path),
        "compact" => static_clones::clones_report_compact(path),
        "text" => Some(section_render::render_text_report(
            &static_clones::clones_report_value(path)?,
        )),
        _ => None,
    }
}

/// The shared static-phase path-policy options from the run flags
/// (Go `run.go pathPolicyFromFlags`: `--include-vendored` /
/// `--include-generated`; `--extra-excluded-prefixes` is not exposed on `run`).
pub(crate) fn static_path_policy(ctx: &RunContext) -> cf_pathpolicy::Options {
    cf_pathpolicy::Options {
        include_vendored: ctx.matches.get_flag("include-vendored"),
        include_generated: ctx.matches.get_flag("include-generated"),
        ..cf_pathpolicy::Options::default()
    }
}

fn h_static_complexity(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    match format {
        "json" => static_complexity::complexity_report_flags(
            path,
            &static_path_policy(ctx),
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_complexity_yaml::complexity_report_yaml(path),
        "binary" | "bin" => static_complexity_bin::complexity_report_bin(path),
        "compact" => Some(section_render::render_compact_report(
            &static_complexity::complexity_report_value_summary(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_complexity::complexity_report_value_summary(path)?,
        )),
        _ => None,
    }
}

fn h_static_cohesion(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name (accept the raw alias too).
    match format {
        "json" => static_cohesion::cohesion_report_json(path),
        "yaml" => static_cohesion::cohesion_report_yaml(path),
        "binary" | "bin" => static_cohesion::cohesion_report_bin(path),
        "compact" => Some(section_render::render_compact_report(
            &static_cohesion::cohesion_report_value_summary(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_cohesion::cohesion_report_value_summary(path)?,
        )),
        _ => None,
    }
}

fn h_static_composition(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_json::composition_report(path),
        "yaml" => static_json::composition_yaml(path),
        // The pipeline resolves the `bin` alias to the canonical `binary`
        // (formats::normalize_format) before dispatch, so match that; accept the
        // raw alias too for direct callers.
        "binary" | "bin" => static_json::composition_bin(path),
        "compact" => Some(section_render::render_compact_report(
            &static_json::composition_report_value(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_json::composition_report_value(path)?,
        )),
        _ => None,
    }
}

fn h_static_halstead(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    match format {
        "json" => static_halstead::halstead_json_report(path),
        "yaml" => static_halstead::halstead_yaml_report(path),
        "binary" => static_halstead::halstead_bin_report(path),
        "compact" => Some(section_render::render_compact_report(
            &static_halstead::halstead_report_value_summary(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_halstead::halstead_report_value_summary(path)?,
        )),
        _ => None,
    }
}

fn h_static_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    match format {
        "json" => static_imports::imports_report_json(path),
        "yaml" => static_imports::imports_report_yaml(path),
        "binary" => static_imports::imports_report_bin(path),
        "compact" => Some(section_render::render_compact_report(
            &static_imports::imports_report_value(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_imports::imports_report_value(path)?,
        )),
        _ => None,
    }
}

fn h_static_comments(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_comments::comments_report_json(path),
        "yaml" => static_comments::comments_report_yaml(path),
        // The pipeline resolves the `bin` alias to the canonical `binary`
        // (formats::normalize_format) before dispatch, so match that; accept the
        // raw alias too for direct callers.
        "binary" | "bin" => static_comments::comments_report_bin(path),
        "compact" => Some(section_render::render_compact_report(
            &static_comments::comments_report_value_summary(path)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_comments::comments_report_value_summary(path)?,
        )),
        _ => None,
    }
}

fn h_history_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One report value, encoded per format by the shared history serializer
    // (Go BaseHistoryAnalyzer.Serialize). The YAML section header is the Go
    // analyzer Name() (`imports.HistoryAnalyzer.Name` == "ImportsPerDeveloper").
    let metrics = history::imports_run_metrics(ctx.matches)?;
    serialize_history_metrics(
        format,
        "ImportsPerDeveloper",
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_typos(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => history::typos_run_report(ctx.matches),
        "yaml" => history::typos_run_report_yaml(ctx.matches),
        "binary" => history::typos_run_report_bin(ctx.matches),
        _ => None,
    }
}

fn h_history_couples(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One report value (Go `ComputedMetrics`, behind ToJSON/ToYAML); every
    // machine format is a serializer over it. ToJSON == ToYAML for couples, so
    // json_value and yaml_value share the same tree. The YAML section name is
    // the analyzer's Go `Name()` ("Couples").
    let value = couples_run::couples_run_value(ctx.matches)?;
    serialize_history_metrics(format, "Couples", &value, &value)
}

fn h_history_shotness(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One report value (Go `ComputedMetrics`, the value behind ToJSON/ToYAML);
    // every machine format is just a serializer over it. ToJSON == ToYAML for
    // shotness, so json_value and yaml_value share `to_go_value()`. The YAML
    // section name is the analyzer's Go `Name()` ("Shotness").
    let metrics = shotness_run::shotness_run_metrics(ctx.matches)?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, "Shotness", &value, &value)
}

fn h_history_devs(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    match (format, head) {
        ("json", true) => history::devs_head_report(sub),
        ("json", false) => history::devs_run_report(sub),
        ("yaml", true) => history::devs_head_report_yaml(sub),
        ("yaml", false) => history::devs_run_report_yaml(sub),
        ("timeseries+ndjson", false) => history::devs_run_timeseries_ndjson(sub),
        // The pipeline resolves the `bin` alias to canonical `binary`
        // (formats::normalize_format) before dispatch; accept the raw alias too
        // for direct callers.
        ("binary" | "bin", false) => history::devs_run_report_bin(sub),
        ("binary" | "bin", true) => {
            let metrics = history::devs_head_metrics(sub)?;
            let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
            cf_reportutil::encode_binary_envelope(&payload).ok()
        }
        _ => None,
    }
}

fn h_history_anomaly(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    use cf_anomaly::model::ToGoValue;
    if ctx.head() {
        // Closed-form merge-HEAD path (analyzer's deterministic head case): ONE
        // report value, every machine format an encoding of it via the shared
        // serializer — so the combined `*` model can request `binary` here too.
        let metrics = history::anomaly_head_report(ctx.matches)?;
        let value = metrics.to_go_value();
        return serialize_history_metrics(format, "TemporalAnomaly", &value, &value);
    }
    // Full revwalk (no --head): one report value (Go ComputeAllMetrics →
    // ComputedMetrics), every machine format an encoding of it via the shared
    // history serializer. ToJSON == ToYAML for anomaly, so json/yaml share the
    // same GoValue. The YAML section name is the analyzer's Go `Name()`
    // ("TemporalAnomaly").
    let metrics = history::anomaly_run_metrics(ctx.matches)?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, "TemporalAnomaly", &value, &value)
}

fn h_history_quality(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // `--head` is handled inside `quality_metrics` (single HEAD-commit window),
    // so every format is the same encoding of one computed value — including the
    // `binary` payload the combined `*` model gathers.
    // One computed report value (Go ComputeAllMetrics), three encodings routed
    // through the shared serializer (Go FormatReportJSON/YAML/Binary): json/bin
    // marshal the encoding/json value tree; yaml wraps the same struct-origin
    // value tree in the `codefang (v2)` envelope under `history/quality:`.
    let metrics = history::quality_metrics(ctx.matches)?;
    let value = cf_quality::serialize::computed_metrics_value(&metrics);
    serialize_history_metrics(format, "history/quality", &value, &value)
}

fn h_history_sentiment(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    use cf_sentiment::ToGoValue;
    // `--head` is handled inside `sentiment_metrics` (single HEAD-commit window),
    // so every format (including the combined `*` model's `binary`) is one
    // encoding of the same computed value.
    // One computed report value, three encodings (Go ComputeAllMetrics →
    // FormatReportJSON/YAML/Binary): json/bin marshal the encoding/json value
    // tree (nil slice → null); yaml wraps the yaml.v3 value tree (nil → []) in
    // the `codefang (v2)` envelope. Routed through the shared serializer so every
    // format follows the one computation (same path as the other history leaves).
    let metrics = history::sentiment_metrics(ctx.matches)?;
    serialize_history_metrics(
        format,
        "history/sentiment",
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_file_history(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One computed report value (Go ComputeAllMetricsWithOptions), three machine
    // encodings (json/bin/yaml). The crate's `computed_metrics_to_go` is the
    // single `ToJSON`/`ToYAML` value tree (file_history's ToJSON == ToYAML);
    // route it through the shared history-metrics serializer so all formats are
    // encodings of THE SAME value (Go BaseHistoryAnalyzer.Serialize). The YAML
    // section header is the analyzer's Name(): `FileHistoryAnalysis`.
    let value = history::file_history_report_value(ctx.matches)?;
    serialize_history_metrics(format, "FileHistoryAnalysis", &value, &value)
}

fn h_history_burndown(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    let ndjson = ctx.ndjson();
    match (format, head, ndjson) {
        ("timeseries", true, false) => history::burndown_head_timeseries(sub),
        ("timeseries+ndjson", false, _) => burndown_ndjson::burndown_timeseries_ndjson(sub),
        ("ndjson", false, _) => burndown_ndjson::burndown_record_ndjson(sub),
        ("json", false, _) => burndown_ndjson::burndown_run_report(sub),
        ("yaml", false, _) => burndown_ndjson::burndown_run_report_yaml(sub),
        ("binary" | "bin", false, _) => burndown_ndjson::burndown_run_report_bin(sub),
        ("json" | "yaml" | "binary" | "bin", true, _) => {
            let metrics = history::burndown_head_metrics(sub)?;
            let bytes = match format {
                "json" => cf_gojson::marshal(&metrics.to_go_value()),
                "binary" | "bin" => cf_reportutil::encode_binary_envelope(&metrics.to_go_value()).ok()?,
                _ => {
                    let mut out = Vec::new();
                    out.extend_from_slice(b"codefang (v2):\n");
                    out.extend_from_slice(
                        format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
                    );
                    out.extend_from_slice(
                        format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes(),
                    );
                    out.extend_from_slice(b"history/burndown:\n");
                    out.extend_from_slice(&cf_goyaml::marshal(&metrics.to_go_value_yaml()));
                    out
                }
            };
            Some(bytes)
        }
        _ => None,
    }
}

/// Builds the single default analyzer [`Registry`] — the Rust analogue of Go
/// `defaultRegistry()` (`analyze.NewRegistry(defaultUASTAnalyzers,
/// defaultRawFileAnalyzers, defaultHistoryLeaves)`). One registry insertion per
/// analyzer; dispatch is a keyed lookup by id, not a per-format match ladder.
#[must_use]
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    let s = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::Static,
        run,
    };
    let h = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::History,
        run,
    };

    r.register(s("static/clones", h_static_clones));
    r.register(s("static/complexity", h_static_complexity));
    r.register(s("static/cohesion", h_static_cohesion));
    r.register(s("static/composition", h_static_composition));
    r.register(s("static/halstead", h_static_halstead));
    r.register(s("static/imports", h_static_imports));
    r.register(s("static/comments", h_static_comments));

    r.register(h("history/imports", h_history_imports));
    r.register(h("history/typos", h_history_typos));
    r.register(h("history/couples", h_history_couples));
    r.register(h("history/shotness", h_history_shotness));
    r.register(h("history/devs", h_history_devs));
    r.register(h("history/anomaly", h_history_anomaly));
    r.register(h("history/quality", h_history_quality));
    r.register(h("history/sentiment", h_history_sentiment));
    r.register(h("history/file-history", h_history_file_history));
    r.register(h("history/burndown", h_history_burndown));

    r
}

