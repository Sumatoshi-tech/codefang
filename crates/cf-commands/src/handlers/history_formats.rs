//! Centralized history-phase output for the whole-selection formats — `text`,
//! `compact`, `ndjson`, `timeseries` and `timeseries --ndjson`.
//!
//! Unlike json/yaml/bin (per-analyzer encodings dispatched through the
//! registry), these formats are rendered over the whole selected analyzer
//! set:
//!
//! - `text`: the version header once, then per analyzer `"<id>:\n"` + the
//!   analyzer's human summary ([`super::history_text`]).
//! - `compact`: one line per analyzer — its id, `Info` and its key figures —
//!   rendered by the compact renderer the static analyzers use.
//! - `ndjson`: one compact JSON line per per-commit record
//!   (`{"hash","tick","author_id","timestamp","analyzer","data"}`), every
//!   analyzer's records merged by commit position (selection order within a
//!   commit).
//! - `timeseries`: a merged `codefang.timeseries.v1` document of the
//!   per-commit summaries of every analyzer, in commit order; with `--ndjson`
//!   its `commits` array, one compact line per commit.
//!
//! Every renderer builds the WHOLE output before returning it: an analyzer
//! that produces no report is a [`HandlerError`] and nothing reaches stdout.

use std::collections::{BTreeSet, HashMap, HashSet};

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::formats::{
    FORMAT_COMPACT, FORMAT_JSON, FORMAT_NDJSON, FORMAT_TEXT, FORMAT_TIMESERIES,
    FORMAT_TIMESERIES_NDJSON,
};
use crate::pipeline::{HandlerError, RunContext};

/// The [`HandlerError::NoReport`] for analyzer `id` in `format`.
fn no_report(id: &str, format: &str) -> HandlerError {
    HandlerError::NoReport {
        analyzer: id.to_string(),
        format: format.to_string(),
    }
}

/// Computes `render` for every selected analyzer — concurrently, after the
/// ONE shared UAST walk of the co-selected heavy analyzers is warmed — and
/// returns the results in selection order, or the first analyzer's
/// [`HandlerError::NoReport`].
fn render_each<T: Send>(
    ctx: &RunContext,
    ids: &[String],
    format: &str,
    render: impl Fn(&str, &RunContext) -> Option<T> + Sync,
) -> Result<Vec<T>, HandlerError> {
    super::uast_walk::prewarm(ctx.matches);
    super::run_concurrent(ids.len(), super::ANALYZER_CONCURRENCY, |i| {
        render(&ids[i], ctx)
    })
    .into_iter()
    .zip(ids)
    .map(|(result, id)| result.ok_or_else(|| no_report(id, format)))
    .collect()
}

// ---------------------------------------------------------------------------
// text
// ---------------------------------------------------------------------------

/// Appends the `codefang (v2):` / version / hash header.
fn print_header(out: &mut Vec<u8>) {
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
}

/// The text body of one analyzer ([`super::history_text`]).
fn leaf_text(id: &str, ctx: &RunContext) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    match id {
        "history/burndown" => super::history_text::burndown_text(sub),
        "history/devs" => super::history_text::devs_text(sub),
        "history/couples" => super::history_text::couples_text(sub),
        "history/file-history" => super::history_text::file_history_text(sub),
        "history/shotness" => super::history_text::shotness_text(sub),
        "history/quality" => super::history_text::quality_text(sub),
        "history/sentiment" => super::history_text::sentiment_text(sub),
        "history/typos" => super::history_text::typos_text(sub),
        "history/anomaly" => super::history_text::anomaly_text(sub),
        "history/imports" => super::history_text::imports_text(sub),
        _ => None,
    }
}

/// The centralized `--format text` history path: the header once, then per
/// analyzer its id as the section line and its text body.
///
/// # Errors
///
/// [`HandlerError::NoReport`] naming the first analyzer without a report;
/// nothing is rendered then.
pub fn history_text(ctx: &RunContext, ids: &[String]) -> Result<Vec<u8>, HandlerError> {
    let bodies = render_each(ctx, ids, FORMAT_TEXT, leaf_text)?;
    let mut out = Vec::new();
    print_header(&mut out);
    for (id, body) in ids.iter().zip(bodies) {
        out.extend_from_slice(format!("{id}:\n").as_bytes());
        out.extend_from_slice(&body);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// compact
// ---------------------------------------------------------------------------

/// The report key whose scalar entries become a history analyzer's compact
/// figures.
const COMPACT_AGGREGATE_KEY: &str = "aggregate";
/// The score every history compact section carries (info-only, rendered
/// `Info`): history reports are measurements, not graded sections.
const COMPACT_INFO_SCORE: f64 = -1.0;
/// Decimals a fractional compact figure keeps.
const COMPACT_DECIMALS: usize = 2;
/// Largest magnitude a float may have to print as an integer.
const COMPACT_MAX_INTEGRAL: f64 = 1e15;

/// One compact figure as a human reads it: integers as-is, a float with at
/// most [`COMPACT_DECIMALS`] decimals (trailing zeros dropped), booleans as
/// words. `None` for a non-scalar value.
fn compact_figure(v: &GoValue) -> Option<String> {
    Some(match v {
        GoValue::Int(n) => n.to_string(),
        GoValue::Uint(n) => n.to_string(),
        GoValue::Bool(b) => b.to_string(),
        GoValue::Float(f) if f.fract() == 0.0 && f.abs() < COMPACT_MAX_INTEGRAL => {
            format!("{f:.0}")
        }
        GoValue::Float(f) => {
            let fixed = format!("{f:.COMPACT_DECIMALS$}");
            fixed
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
        _ => return None,
    })
}

/// The scalar `(key, figure)` pairs of the report's `aggregate` object
/// (falling back to the top-level scalars).
fn aggregate_figures(report: &GoValue) -> Vec<(String, String)> {
    let scalars = |m: &GoMap| -> Vec<(String, String)> {
        m.iter()
            .filter_map(|(k, v)| Some((k.clone(), compact_figure(v)?)))
            .collect()
    };
    report
        .as_map()
        .map(|root| {
            root.get(COMPACT_AGGREGATE_KEY)
                .and_then(GoValue::as_map)
                .map_or_else(|| scalars(root), scalars)
        })
        .unwrap_or_default()
}

/// The compact figures of one analyzer: the `aggregate` scalars of its JSON
/// report; imports (whose report has no aggregate) summarizes its usage map.
fn leaf_compact_figures(id: &str, ctx: &RunContext) -> Option<Vec<(String, String)>> {
    if id == "history/imports" {
        let report = super::history::imports_run_metrics(ctx.matches)?;
        let summary = super::history_text::imports_summary(&report);
        return Some(vec![
            ("developers".to_string(), summary.developers.to_string()),
            ("languages".to_string(), summary.languages.to_string()),
            (
                "distinct_imports".to_string(),
                summary.distinct_imports.to_string(),
            ),
            ("usages".to_string(), summary.usages.to_string()),
        ]);
    }
    let registry = super::default_registry();
    let bytes = (registry.lookup(id)?.run)(ctx, FORMAT_JSON).ok()?;
    let report = cf_analyze::json_parse::parse(&bytes).ok()?;
    Some(aggregate_figures(&report))
}

/// The compact section of one analyzer: its id (padded to `title_width` so
/// the figures of a multi-analyzer run line up) as title, an info-only score
/// and the figures joined `key=value` as the status the compact line shows.
fn compact_section(id: &str, title_width: usize, figures: &[(String, String)]) -> GoValue {
    let status = figures
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(format!("{id:<title_width$}")));
    section.push("status", GoValue::Str(status));
    section.push("score", GoValue::Float(COMPACT_INFO_SCORE));
    GoValue::Map(section)
}

/// The centralized `--format compact` history path: one line per analyzer,
/// rendered by the shared compact renderer.
///
/// # Errors
///
/// [`HandlerError::NoReport`] naming the first analyzer without a report.
pub fn history_compact(ctx: &RunContext, ids: &[String]) -> Result<Vec<u8>, HandlerError> {
    let figures = render_each(ctx, ids, FORMAT_COMPACT, leaf_compact_figures)?;
    let title_width = ids.iter().map(|id| id.chars().count()).max().unwrap_or(0);
    let sections = ids
        .iter()
        .zip(&figures)
        .map(|(id, f)| compact_section(id, title_width, f))
        .collect();
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("sections", GoValue::Array(sections));
    Ok(super::section_render::render_compact_report(&GoValue::Map(
        root,
    )))
}

// ---------------------------------------------------------------------------
// timeseries
// ---------------------------------------------------------------------------

/// Ordered commit metadata of a contribution: `(hash, tick,
/// rfc3339_timestamp, author)`, oldest first.
pub type CommitMeta = (String, i64, String, String);

/// One analyzer's contribution to the merged timeseries: its flag plus the
/// per-commit summary values (keyed by hash) and the commit order of its walk.
pub struct TimeSeriesContribution {
    /// The analyzer key inside each merged commit object.
    pub flag: &'static str,
    /// Per-commit summary values keyed by full hex hash.
    pub per_commit: Vec<(String, GoValue)>,
    /// The walk's commits, oldest first (possibly only the data-bearing
    /// ones).
    pub commit_meta: Vec<CommitMeta>,
}

/// One compact JSON line per data-bearing commit of `contrib`: the merged
/// commit object `{author, hash, <flag>, tick, timestamp}` with the
/// analyzer's per-commit value under its flag.
#[must_use]
pub fn timeseries_ndjson_lines(contrib: &TimeSeriesContribution) -> Vec<u8> {
    commit_lines(&merged_commits(&[contrib]))
}

/// The `history/typos` contribution: per commit that fixed typos, the number
/// of fixes and of distinct files they were in.
fn typos_timeseries_contribution(sub: &clap::ArgMatches) -> Option<TimeSeriesContribution> {
    let walk = super::history::typos::typos_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in walk.iter().filter(|c| !c.typos.is_empty()) {
        let hash = c.typos[0].commit.string();
        let files: BTreeSet<&str> = c.typos.iter().map(|t| t.file.as_str()).collect();
        let mut entry = GoMap::new_map();
        entry.insert("fixes", GoValue::Int(c.typos.len() as i64));
        entry.insert("files", GoValue::Int(files.len() as i64));
        per_commit.push((hash.clone(), GoValue::Map(entry)));
        commit_meta.push((
            hash,
            c.tick,
            super::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(TimeSeriesContribution {
        flag: "typos",
        per_commit,
        commit_meta,
    })
}

/// Per-analyzer timeseries data source.
fn leaf_timeseries(id: &str, ctx: &RunContext) -> Option<TimeSeriesContribution> {
    let sub = ctx.matches;
    match id {
        "history/typos" => typos_timeseries_contribution(sub),
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

/// The centralized `--format timeseries` history path: every analyzer's
/// contribution (flag-sorted) merged per commit into the
/// `codefang.timeseries.v1` document (2-space-indented JSON + trailing
/// newline). With `ndjson_lines` (`--format timeseries --ndjson`) the
/// document's `commits` array is emitted instead, one compact line per
/// commit.
///
/// # Errors
///
/// An invalid `--tick-size`, or [`HandlerError::NoReport`] naming the first
/// analyzer without a report.
pub fn history_timeseries(
    ctx: &RunContext,
    ids: &[String],
    ndjson_lines: bool,
) -> Result<Vec<u8>, HandlerError> {
    let format = if ndjson_lines {
        FORMAT_TIMESERIES_NDJSON
    } else {
        FORMAT_TIMESERIES
    };
    let tick_size_hours =
        super::history::HistoryOptions::from_matches(ctx.matches)?.tick_size_hours;
    let mut contribs = render_each(ctx, ids, format, leaf_timeseries)?;
    contribs.sort_by(|a, b| a.flag.cmp(b.flag));
    let refs: Vec<&TimeSeriesContribution> = contribs.iter().collect();
    let commits = merged_commits(&refs);
    if ndjson_lines {
        return Ok(commit_lines(&commits));
    }

    // Top level is a STRUCT (fixed field order).
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "version",
        GoValue::Str("codefang.timeseries.v1".to_string()),
    );
    root.push("tick_size_hours", GoValue::Int(tick_size_hours));
    root.push(
        "analyzers",
        GoValue::Array(
            contribs
                .iter()
                .filter(|c| !c.per_commit.is_empty())
                .map(|c| GoValue::Str(c.flag.to_string()))
                .collect(),
        ),
    );
    root.push("commits", GoValue::Array(commits));
    let mut out = cf_gojson::Encoder::indented("  ").encode_to_vec(&GoValue::Map(root));
    out.push(b'\n');
    Ok(out)
}

/// One compact JSON line per merged commit object.
fn commit_lines(commits: &[GoValue]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in commits {
        out.extend_from_slice(&cf_gojson::marshal(c));
        out.push(b'\n');
    }
    out
}

/// The commit order of the merged document: the longest walk's commits
/// (ties → flag order), then every commit only a shorter walk lists (e.g. a
/// typo-fixing commit when only data-bearing walks are selected), each
/// placed after the last commit of its tick or earlier.
fn commit_order<'a>(contribs: &[&'a TimeSeriesContribution]) -> Vec<&'a CommitMeta> {
    let Some(base) = contribs
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| {
            a.commit_meta
                .len()
                .cmp(&b.commit_meta.len())
                .then_with(|| ib.cmp(ia))
        })
        .map(|(_, c)| c)
    else {
        return Vec::new();
    };
    let mut order: Vec<&CommitMeta> = base.commit_meta.iter().collect();
    let mut seen: HashSet<&str> = order.iter().map(|m| m.0.as_str()).collect();
    for c in contribs {
        for meta in &c.commit_meta {
            if seen.insert(meta.0.as_str()) {
                let at = order.partition_point(|m| m.1 <= meta.1);
                order.insert(at, meta);
            }
        }
    }
    order
}

/// The merged commit objects: per commit in [`commit_order`] that any
/// contribution has a value for, `{author, hash, <flag>…, tick, timestamp}`
/// (map-origin: keys byte-sorted on encode). Each contribution's values are
/// indexed by hash once, so the merge is linear in the commit count.
fn merged_commits(contribs: &[&TimeSeriesContribution]) -> Vec<GoValue> {
    let active: Vec<(&'static str, HashMap<&str, &GoValue>)> = contribs
        .iter()
        .filter(|c| !c.per_commit.is_empty())
        .map(|c| {
            let values = c.per_commit.iter().map(|(h, v)| (h.as_str(), v)).collect();
            (c.flag, values)
        })
        .collect();
    commit_order(contribs)
        .into_iter()
        .filter_map(|(hash, tick, ts, author)| {
            let mut values = active
                .iter()
                .filter_map(|(flag, by_hash)| Some((*flag, *by_hash.get(hash.as_str())?)))
                .peekable();
            values.peek()?;
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("hash", GoValue::Str(hash.clone()));
            m.push("timestamp", GoValue::Str(ts.clone()));
            m.push("author", GoValue::Str(author.clone()));
            m.push("tick", GoValue::Int(*tick));
            for (flag, v) in values {
                m.push(flag, v.clone());
            }
            Some(GoValue::Map(m))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ndjson
// ---------------------------------------------------------------------------

/// One per-commit record: the commit identity plus the analyzer-specific
/// `data` payload.
pub struct NdjsonRecord {
    /// The commit's position in the walk (0-based); drives the merged emit
    /// order across leaves.
    pub pos: usize,
    /// Full hex commit hash.
    pub hash: String,
    /// Tick index.
    pub tick: i64,
    /// Identity-detector author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub time_secs: i64,
    /// Committer timezone offset, minutes.
    pub tz_offset_min: i32,
    /// The analyzer-specific payload (`data`); records with no payload are
    /// never constructed.
    pub data: GoValue,
}

/// Per-leaf ndjson record source.
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
        "history/burndown" => super::burndown_ndjson::burndown_ndjson_records(sub),
        _ => None,
    }
}

/// The `"analyzer"` field of each ndjson line and the analyzer key in the
/// merged timeseries, per analyzer id.
fn leaf_flag(id: &str) -> Option<&'static str> {
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

/// The centralized `--format ndjson` history path: one compact JSON line per
/// record, fields `hash`/`tick`/`author_id`/`timestamp`/`analyzer`/`data` in
/// struct order, RFC3339 timestamp. Every leaf's records are merged by commit
/// position; within one commit the lines follow the selection order — a pure
/// function of the repository, the flags and the selection.
///
/// # Errors
///
/// [`HandlerError::NoReport`] naming the first analyzer without records.
pub fn history_ndjson(ctx: &RunContext, ids: &[String]) -> Result<Vec<u8>, HandlerError> {
    let serialize = |out: &mut Vec<u8>, flag: &str, r: NdjsonRecord| {
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
    };

    let records = render_each(ctx, ids, FORMAT_NDJSON, |id, ctx| {
        Some((leaf_flag(id)?, leaf_ndjson(id, ctx)?))
    })?;
    let mut streams: Vec<(
        &'static str,
        std::iter::Peekable<std::vec::IntoIter<NdjsonRecord>>,
    )> = records
        .into_iter()
        .map(|(flag, r)| (flag, r.into_iter().peekable()))
        .collect();

    let mut out = Vec::new();
    loop {
        // The stream whose head has the lowest position; ties resolve to the
        // first-selected leaf.
        let mut best: Option<(usize, usize)> = None;
        for (i, (_, it)) in streams.iter_mut().enumerate() {
            if let Some(head) = it.peek() {
                if best.map_or(true, |(_, bp)| head.pos < bp) {
                    best = Some((i, head.pos));
                }
            }
        }
        let Some((i, _)) = best else { break };
        let (flag, it) = &mut streams[i];
        let head = it.next().expect("peeked head exists");
        serialize(&mut out, flag, head);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Commits in the large-merge fixture (well above the 2000 the quadratic
    /// lookup made slow).
    const MANY_COMMITS: usize = 2_500;

    fn meta(i: usize, tick: i64) -> CommitMeta {
        (
            format!("{i:040x}"),
            tick,
            format!("2021-01-01T00:00:{:02}Z", i % 60),
            String::new(),
        )
    }

    fn value(n: usize) -> GoValue {
        let mut m = GoMap::new_map();
        m.insert("n", GoValue::Int(n as i64));
        GoValue::Map(m)
    }

    /// The merge as the quadratic implementation computed it: per commit of
    /// the base walk, a linear search of every contribution.
    fn naive_merge(base: &[CommitMeta], contribs: &[&TimeSeriesContribution]) -> Vec<GoValue> {
        base.iter()
            .filter_map(|(hash, tick, ts, author)| {
                let mut m = GoMap::new(MapOrigin::Map);
                m.push("hash", GoValue::Str(hash.clone()));
                m.push("timestamp", GoValue::Str(ts.clone()));
                m.push("author", GoValue::Str(author.clone()));
                m.push("tick", GoValue::Int(*tick));
                let mut any = false;
                for c in contribs {
                    if let Some((_, v)) = c.per_commit.iter().find(|(h, _)| h == hash) {
                        m.push(c.flag, v.clone());
                        any = true;
                    }
                }
                any.then_some(GoValue::Map(m))
            })
            .collect()
    }

    /// Two contributions over 2500 commits: one with a value on every
    /// commit (and the full commit order), one on every third commit (and
    /// only its own commits). The hash-indexed merge is byte-identical to the
    /// linear-search merge, for the document and for the ndjson lines.
    #[test]
    fn indexed_merge_equals_the_linear_search_merge_on_many_commits() {
        let all: Vec<CommitMeta> = (0..MANY_COMMITS).map(|i| meta(i, i as i64 / 7)).collect();
        let dense = TimeSeriesContribution {
            flag: "devs",
            per_commit: all
                .iter()
                .enumerate()
                .map(|(i, m)| (m.0.clone(), value(i)))
                .rev()
                .collect(),
            commit_meta: all.clone(),
        };
        let sparse = TimeSeriesContribution {
            flag: "typos",
            per_commit: all
                .iter()
                .step_by(3)
                .map(|m| (m.0.clone(), value(1)))
                .collect(),
            commit_meta: all.iter().step_by(3).cloned().collect(),
        };
        let contribs = [&dense, &sparse];
        let merged = merged_commits(&contribs);
        assert_eq!(merged.len(), MANY_COMMITS);
        assert_eq!(merged, naive_merge(&all, &contribs));
        assert_eq!(
            commit_lines(&merged),
            commit_lines(&naive_merge(&all, &contribs))
        );
        assert_eq!(
            timeseries_ndjson_lines(&sparse),
            commit_lines(&naive_merge(&sparse.commit_meta, &[&sparse]))
        );
    }

    /// A commit only a shorter walk lists is placed by its tick, after the
    /// longer walk's commits of that tick.
    #[test]
    fn commits_only_a_shorter_walk_lists_are_placed_by_tick() {
        let long = TimeSeriesContribution {
            flag: "imports-per-dev",
            per_commit: vec![(meta(0, 0).0, value(0)), (meta(2, 3).0, value(2))],
            commit_meta: vec![meta(0, 0), meta(2, 3)],
        };
        let short = TimeSeriesContribution {
            flag: "typos",
            per_commit: vec![(meta(1, 0).0, value(1))],
            commit_meta: vec![meta(1, 0)],
        };
        let hashes: Vec<String> = commit_order(&[&short, &long])
            .iter()
            .map(|m| m.0.clone())
            .collect();
        assert_eq!(hashes, [meta(0, 0).0, meta(1, 0).0, meta(2, 3).0]);
        assert!(commit_order(&[]).is_empty());
    }

    #[test]
    fn compact_figures_are_rounded_for_reading() {
        assert_eq!(compact_figure(&GoValue::Int(-3)).as_deref(), Some("-3"));
        assert_eq!(compact_figure(&GoValue::Uint(7)).as_deref(), Some("7"));
        assert_eq!(compact_figure(&GoValue::Float(2.0)).as_deref(), Some("2"));
        assert_eq!(
            compact_figure(&GoValue::Float(100.0 / 3.0)).as_deref(),
            Some("33.33")
        );
        assert_eq!(compact_figure(&GoValue::Float(0.5)).as_deref(), Some("0.5"));
        assert_eq!(
            compact_figure(&GoValue::Bool(true)).as_deref(),
            Some("true")
        );
        assert_eq!(compact_figure(&GoValue::Str("x".into())), None);
    }

    /// The compact titles of a multi-analyzer run are padded to one width.
    #[test]
    fn compact_titles_share_one_width() {
        let ids = ["history/devs", "history/file-history"];
        let lines = ids.map(|id| {
            let section = compact_section(id, 20, &[("n".to_string(), "1".to_string())]);
            let mut root = GoMap::new(MapOrigin::Struct);
            root.push("sections", GoValue::Array(vec![section]));
            String::from_utf8(super::super::section_render::render_compact_report(
                &GoValue::Map(root),
            ))
            .expect("utf-8")
        });
        let column = |l: &str| l.find('[').expect("bar");
        assert_eq!(column(&lines[0]), column(&lines[1]), "{lines:?}");
        assert!(lines[0].contains("Info  n=1"), "{lines:?}");
    }

    /// An analyzer without a report is a named error — never a partial
    /// document — for every whole-selection format.
    #[test]
    fn a_missing_report_is_a_named_error_with_no_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let argv = [
            "codefang",
            "run",
            "-a",
            "history/devs",
            dir.path().to_str().expect("utf-8"),
        ];
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run");
        let ctx = RunContext::from_matches(sub);
        let ids = vec!["history/devs".to_string()];
        let renders: [(&str, Result<Vec<u8>, HandlerError>); 5] = [
            (FORMAT_TEXT, history_text(&ctx, &ids)),
            (FORMAT_COMPACT, history_compact(&ctx, &ids)),
            (FORMAT_NDJSON, history_ndjson(&ctx, &ids)),
            (FORMAT_TIMESERIES, history_timeseries(&ctx, &ids, false)),
            (
                FORMAT_TIMESERIES_NDJSON,
                history_timeseries(&ctx, &ids, true),
            ),
        ];
        for (format, result) in renders {
            match result {
                Err(HandlerError::NoReport {
                    analyzer,
                    format: f,
                }) => {
                    assert_eq!((analyzer.as_str(), f.as_str()), ("history/devs", format));
                }
                other => panic!("{format}: expected NoReport, got {other:?}"),
            }
        }
    }
}
