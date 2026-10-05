//! `history/anomaly` over the shared history walk.
//!
//! Per commit: the filtered tree diff against the first parent, the files it
//! names (a delete names the deleted file), line stats (skipped for merges),
//! per-blob language detection; then the shared identity/tick stamps feed
//! `cf_anomaly::build_report_data` → `compute_all_metrics` with the
//! validated [`cf_anomaly::config::Config`] (`--anomaly-threshold`,
//! `--anomaly-window`).

use std::collections::BTreeMap;

use cf_anomaly::config::{
    Config, DEFAULT_ANOMALY_THRESHOLD, DEFAULT_ANOMALY_WINDOW_SIZE, FLAG_ANOMALY_THRESHOLD,
    FLAG_ANOMALY_WINDOW, MIN_WINDOW_SIZE,
};
use cf_anomaly::metrics::{build_report_data, TickBounds};
use cf_anomaly::model::CommitAnomalyData;
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::ChangeAction;

use cf_units::{KIB, MIB};

use super::{
    compute_diff_line_stats, detect_language, gated_product, is_merge, walk_chunks, ChunkPlan,
    CommitStamp, Gated, HistoryRun, WalkCost,
};
use crate::pipeline::HandlerError;

/// The validated anomaly configuration from `--anomaly-threshold` and
/// `--anomaly-window` (each defaulting when absent).
///
/// # Errors
///
/// [`HandlerError::InvalidFlag`] for a window below 2 or a threshold that is
/// not strictly positive.
pub(crate) fn anomaly_config(sub: &clap::ArgMatches) -> Result<Config, HandlerError> {
    let window = super::flag_i64(sub, FLAG_ANOMALY_WINDOW, DEFAULT_ANOMALY_WINDOW_SIZE as i64);
    let threshold = super::flag_f64(
        sub,
        FLAG_ANOMALY_THRESHOLD,
        f64::from(DEFAULT_ANOMALY_THRESHOLD),
    );
    let window = usize::try_from(window).map_err(|_| {
        HandlerError::InvalidFlag(format!(
            "invalid --{FLAG_ANOMALY_WINDOW}: must be >= {MIN_WINDOW_SIZE}, got {window}"
        ))
    })?;
    Config::new(threshold as f32, window).map_err(|e| HandlerError::InvalidFlag(e.to_string()))
}

/// Preflight check of the anomaly flags (every history run validates them,
/// so a bad value is a named error before any walk).
pub(crate) fn validate_anomaly_flags(sub: &clap::ArgMatches) -> Result<(), HandlerError> {
    anomaly_config(sub).map(|_| ())
}

/// Runs the `history/anomaly` walk and returns the aggregated
/// [`cf_anomaly::model::ComputedMetrics`] — the single report value every
/// machine format (json/yaml/bin) encodes — or `None` if the repository
/// cannot be walked or the flags are invalid (preflight reports those).
#[must_use]
pub fn anomaly_run_metrics(sub: &clap::ArgMatches) -> Option<cf_anomaly::model::ComputedMetrics> {
    let config = anomaly_config(sub).ok()?;
    let walk = anomaly_walk(sub)?;
    let input = build_report_data(
        &walk.commit_metrics,
        &walk.commits_by_tick,
        walk.tick_bounds,
        config,
    );
    Some(cf_anomaly::metrics::compute_all_metrics(&input))
}

/// The finished `history/anomaly` walk, shared by the aggregated metrics
/// path and the per-commit ndjson/timeseries paths so every format encodes
/// the SAME walk.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AnomalyWalk {
    /// hex hash → per-commit anomaly data.
    pub commit_metrics: BTreeMap<String, CommitAnomalyData>,
    /// tick → hashes in walk order.
    pub commits_by_tick: BTreeMap<i64, Vec<String>>,
    /// tick → RFC3339-UTC committer-time bounds.
    pub tick_bounds: BTreeMap<i64, TickBounds>,
    /// hex hash → tick.
    pub tick_by_hash: BTreeMap<String, i64>,
    /// hex hash → (committer seconds, committer UTC-offset minutes).
    pub when_by_hash: BTreeMap<String, (i64, i32)>,
    /// hex hash → position in the ORIGINAL window (the key the merged ndjson
    /// stream orders on; an oversized-dropped commit leaves a gap).
    pub pos_by_hash: BTreeMap<String, usize>,
}

/// The file a change names: the After name for an Insert/Modify, the Before
/// name for a Delete (whose After side is empty).
fn changed_file_name(change: &cf_gitlib::changes::Change) -> &str {
    match change.action {
        ChangeAction::Delete => &change.from.name,
        ChangeAction::Insert | ChangeAction::Modify => &change.to.name,
    }
}

/// The per-commit, order-independent half of the anomaly walk: everything in
/// [`CommitAnomalyData`] except the order-assigned `author_id`.
fn anomaly_commit_product(
    run_is_merge: bool,
    repo: &cf_gitlib::Repository,
    changes: &[cf_gitlib::changes::Change],
) -> CommitAnomalyData {
    let mut cm = CommitAnomalyData::default();
    for change in changes {
        let name = changed_file_name(change).to_string();
        if !cm.files.contains(&name) {
            cm.files.push(name);
        }
    }
    cm.files_changed = cm.files.len() as i64;

    // Line stats are skipped for merge commits.
    if !run_is_merge {
        let (added, removed) = line_stats(repo, changes);
        cm.lines_added = added;
        cm.lines_removed = removed;
    }

    // Blob-hash → language map (Insert/Delete one side, Modify both sides),
    // then count each non-empty value into the commit's language map.
    let mut by_blob: BTreeMap<cf_gitlib::Hash, String> = BTreeMap::new();
    let mut detect = |entry: &cf_gitlib::changes::ChangeEntry| {
        let data = CachedBlob::from_repo(repo, entry.hash)
            .map(|b| b.data)
            .unwrap_or_default();
        by_blob.insert(entry.hash, detect_language(&entry.name, &data));
    };
    for change in changes {
        match change.action {
            ChangeAction::Insert => detect(&change.to),
            ChangeAction::Delete => detect(&change.from),
            ChangeAction::Modify => {
                detect(&change.to);
                detect(&change.from);
            }
        }
    }
    for lang in by_blob.values() {
        if !lang.is_empty() {
            *cm.languages.entry(lang.clone()).or_insert(0) += 1;
        }
    }

    cm.net_churn = cm.lines_added - cm.lines_removed;
    cm
}

/// `(added, removed)` over a commit's changes: Insert ⇒ the new blob's lines;
/// Delete ⇒ the old blob's lines; Modify ⇒ the line diff, skipping binary and
/// identical-content blobs.
fn line_stats(repo: &cf_gitlib::Repository, changes: &[cf_gitlib::changes::Change]) -> (i64, i64) {
    let mut added = 0i64;
    let mut removed = 0i64;
    for change in changes {
        match change.action {
            ChangeAction::Insert => {
                if let Ok(blob) = CachedBlob::from_repo(repo, change.to.hash) {
                    if let Ok(n) = blob.count_lines() {
                        added += n as i64;
                    }
                }
            }
            ChangeAction::Delete => {
                if let Ok(blob) = CachedBlob::from_repo(repo, change.from.hash) {
                    if let Ok(n) = blob.count_lines() {
                        removed += n as i64;
                    }
                }
            }
            ChangeAction::Modify => {
                let (Ok(blob_from), Ok(blob_to)) = (
                    CachedBlob::from_repo(repo, change.from.hash),
                    CachedBlob::from_repo(repo, change.to.hash),
                ) else {
                    continue;
                };
                if change.from.hash == change.to.hash
                    || blob_from.is_binary()
                    || blob_to.is_binary()
                {
                    continue;
                }
                let old_lines = blob_from.count_lines().map_or(0, |n| n as i64);
                let (a, r, _changed) =
                    compute_diff_line_stats(repo, change.from.hash, change.to.hash, old_lines);
                added += a;
                removed += r;
            }
        }
    }
    (added, removed)
}

/// The gated anomaly pre-pass over `run`'s change filter and flags: the
/// pure per-commit product a walk computes in parallel. Captures only the
/// shareable pieces of the run (its repository handle is not `Sync`).
fn anomaly_prepare(
    run: &HistoryRun,
) -> impl Fn(&cf_gitlib::Repository, cf_gitlib::Hash) -> Option<Gated<CommitAnomalyData>> + Sync + '_
{
    let filter = &run.filter;
    let first_parent = run.first_parent;
    let max_changes = run.max_changes;
    move |repo, hash| {
        gated_product(repo, hash, filter, max_changes, |commit, changes| {
            anomaly_commit_product(is_merge(commit.num_parents(), first_parent), repo, changes)
        })
    }
}

/// The anomaly reduce state. A kept commit's product IS its per-commit
/// record (every anomaly format reads per-commit data), so the fold keys it
/// by hash next to the commit's stamp facts; the gate wrapper is dropped.
#[derive(Default)]
struct AnomalyFold {
    /// hex hash → per-commit anomaly data.
    commit_metrics: BTreeMap<String, CommitAnomalyData>,
    /// tick → hashes in walk order.
    commits_by_tick: BTreeMap<i64, Vec<String>>,
    /// tick → `(earliest, latest)` committer seconds.
    tick_when: BTreeMap<i64, (i64, i64)>,
    /// hex hash → tick.
    tick_by_hash: BTreeMap<String, i64>,
    /// hex hash → (committer seconds, committer UTC-offset minutes).
    when_by_hash: BTreeMap<String, (i64, i32)>,
    /// hex hash → window position.
    pos_by_hash: BTreeMap<String, usize>,
}

impl AnomalyFold {
    /// Folds one kept commit, in window order: its product becomes the
    /// commit's record under the stamp's author id.
    fn consume(&mut self, stamp: &CommitStamp, mut product: CommitAnomalyData) {
        self.tick_when
            .entry(stamp.tick)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(stamp.when);
                *hi = (*hi).max(stamp.when);
            })
            .or_insert((stamp.when, stamp.when));
        self.commits_by_tick
            .entry(stamp.tick)
            .or_default()
            .push(stamp.hex.clone());
        self.tick_by_hash
            .entry(stamp.hex.clone())
            .or_insert(stamp.tick);
        self.when_by_hash
            .entry(stamp.hex.clone())
            .or_insert((stamp.when, stamp.offset_min));
        self.pos_by_hash
            .entry(stamp.hex.clone())
            .or_insert(stamp.pos);
        product.author_id = stamp.author_id;
        self.commit_metrics.insert(stamp.hex.clone(), product);
    }

    /// The finished walk: the folded records and the RFC3339 tick bounds.
    fn finish(self) -> AnomalyWalk {
        let tick_bounds = self
            .tick_when
            .iter()
            .map(|(&tick, &(lo, hi))| {
                (
                    tick,
                    TickBounds {
                        start_time: cf_analyze::metadata::format_rfc3339_utc(lo),
                        end_time: cf_analyze::metadata::format_rfc3339_utc(hi),
                    },
                )
            })
            .collect();
        AnomalyWalk {
            commit_metrics: self.commit_metrics,
            commits_by_tick: self.commits_by_tick,
            tick_bounds,
            tick_by_hash: self.tick_by_hash,
            when_by_hash: self.when_by_hash,
            pos_by_hash: self.pos_by_hash,
        }
    }
}

/// The anomaly walk's measured cost. Kubernetes, 31,923 commits: one worker
/// peaks at 858 MB, 24 workers at 1429 MB (24 MiB per added worker); the
/// whole window holds 99 bytes more per commit than 50-commit chunks.
const ANOMALY_COST: WalkCost = WalkCost {
    worker_bytes: 32 * MIB,
    product_bytes: KIB,
};

/// Runs the `history/anomaly` walk over the run's chunk plan: the gated
/// per-commit products are computed in parallel one chunk at a time,
/// oversized commits dropped, and folded in window order ([`walk_chunks`]).
/// `None` when the repository cannot be walked.
pub(crate) fn anomaly_walk(sub: &clap::ArgMatches) -> Option<AnomalyWalk> {
    let run = HistoryRun::open(sub)?;
    let plan = ChunkPlan::for_walk(sub, "history/anomaly", run.hashes.len(), ANOMALY_COST).ok()?;
    let max_changes = run.max_changes;
    let mut fold = AnomalyFold::default();
    walk_chunks(
        &run,
        plan,
        anomaly_prepare(&run),
        |gated| gated.oversized(max_changes),
        |stamp, gated| fold.consume(stamp, gated.product),
    )?;
    Some(fold.finish())
}

/// Serializes one [`CommitAnomalyData`] as the per-commit record struct
/// (declaration field order; `files`/`languages` omitted when empty).
fn anomaly_data_value(cm: &CommitAnomalyData) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};
    let mut data = GoMap::new_struct();
    data.insert("files_changed".to_string(), GoValue::Int(cm.files_changed));
    data.insert("lines_added".to_string(), GoValue::Int(cm.lines_added));
    data.insert("lines_removed".to_string(), GoValue::Int(cm.lines_removed));
    data.insert("net_churn".to_string(), GoValue::Int(cm.net_churn));
    if !cm.files.is_empty() {
        data.insert(
            "files".to_string(),
            GoValue::Array(cm.files.iter().map(|f| GoValue::Str(f.clone())).collect()),
        );
    }
    if !cm.languages.is_empty() {
        let mut langs = GoMap::new_map();
        for (lang, n) in &cm.languages {
            langs.insert(lang.clone(), GoValue::Int(*n));
        }
        data.insert("languages".to_string(), GoValue::Object(langs));
    }
    data.insert("author_id".to_string(), GoValue::Int(cm.author_id));
    cf_gojson::GoValue::Object(data)
}

/// Per-commit anomaly NDJSON records: every walked commit emits a line.
#[must_use]
pub fn anomaly_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    let walk = anomaly_walk(sub)?;
    let mut records = Vec::new();
    for hashes in walk.commits_by_tick.values() {
        for hex in hashes {
            let Some(cm) = walk.commit_metrics.get(hex) else {
                continue;
            };
            let (secs, off) = walk.when_by_hash.get(hex).copied().unwrap_or((0, 0));
            records.push(super::super::history_formats::NdjsonRecord {
                pos: walk.pos_by_hash.get(hex).copied().unwrap_or(0),
                hash: hex.clone(),
                tick: walk.tick_by_hash.get(hex).copied().unwrap_or(0),
                author_id: cm.author_id,
                time_secs: secs,
                tz_offset_min: off,
                data: anomaly_data_value(cm),
            });
        }
    }
    Some(records)
}

/// The anomaly contribution to the merged `--format timeseries` document.
#[must_use]
pub fn anomaly_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    let walk = anomaly_walk(sub)?;
    let mut per_commit = Vec::new();
    for (hex, cm) in &walk.commit_metrics {
        per_commit.push((hex.clone(), anomaly_data_value(cm)));
    }
    let mut commit_meta = Vec::new();
    for hashes in walk.commits_by_tick.values() {
        for hex in hashes {
            let tick = walk.tick_by_hash.get(hex).copied().unwrap_or(0);
            let (secs, off) = walk.when_by_hash.get(hex).copied().unwrap_or((0, 0));
            commit_meta.push((
                hex.clone(),
                tick,
                crate::handlers::format_rfc3339_offset(secs, off),
                String::new(),
            ));
        }
    }
    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "anomaly",
        per_commit,
        commit_meta,
    })
}

#[cfg(test)]
mod tests {
    use super::super::fold_testutil::{
        assert_chunk_invariant, chunk_fixture, open_run, run_matches, OVERSIZED_POS,
    };
    use super::*;

    /// The analyzer id the fold tests run.
    const ID: &str = "history/anomaly";

    /// A reduced `run` tree with just the two anomaly flags (the threshold
    /// flag is read by name, so the handler works with any tree that
    /// registers it).
    fn matches(args: &[&str]) -> clap::ArgMatches {
        clap::Command::new("run")
            .arg(
                clap::Arg::new(FLAG_ANOMALY_WINDOW)
                    .long(FLAG_ANOMALY_WINDOW)
                    .value_parser(clap::value_parser!(i64)),
            )
            .arg(
                clap::Arg::new(FLAG_ANOMALY_THRESHOLD)
                    .long(FLAG_ANOMALY_THRESHOLD)
                    .value_parser(clap::value_parser!(f64)),
            )
            .try_get_matches_from(std::iter::once("run").chain(args.iter().copied()))
            .expect("args parse")
    }

    fn invalid_flag(result: Result<Config, HandlerError>) -> String {
        match result {
            Err(HandlerError::InvalidFlag(msg)) => msg,
            other => panic!("expected InvalidFlag, got {other:?}"),
        }
    }

    #[test]
    fn absent_flags_are_the_defaults() {
        assert_eq!(anomaly_config(&matches(&[])).unwrap(), Config::default());
    }

    #[test]
    fn both_flags_are_read() {
        let cfg = anomaly_config(&matches(&[
            "--anomaly-window",
            "5",
            "--anomaly-threshold",
            "3.5",
        ]))
        .unwrap();
        assert_eq!(cfg.window_size, 5);
        assert!((cfg.threshold - 3.5).abs() < 1e-6);
    }

    #[test]
    fn invalid_values_are_named_errors() {
        assert_eq!(
            invalid_flag(anomaly_config(&matches(&["--anomaly-window", "1"]))),
            "invalid --anomaly-window: must be >= 2, got 1"
        );
        assert_eq!(
            invalid_flag(anomaly_config(&matches(&["--anomaly-window=-3"]))),
            "invalid --anomaly-window: must be >= 2, got -3"
        );
        assert_eq!(
            invalid_flag(anomaly_config(&matches(&["--anomaly-threshold", "0"]))),
            "invalid --anomaly-threshold: must be > 0, got 0"
        );
    }

    /// The real command tree registers `--anomaly-window`; preflight rejects
    /// a bad value for ANY history run.
    #[test]
    fn preflight_rejects_a_bad_window_through_the_real_tree() {
        let argv = [
            "codefang",
            "run",
            "-a",
            "history/devs",
            "--anomaly-window",
            "1",
            ".",
        ];
        let parsed = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = parsed.subcommand_matches("run").expect("run");
        let msg = match super::super::validate_history_run(sub) {
            Err(HandlerError::InvalidFlag(msg)) => msg,
            other => panic!("expected InvalidFlag, got {other:?}"),
        };
        assert_eq!(msg, "invalid --anomaly-window: must be >= 2, got 1");
    }

    /// The anomaly walk folds per chunk: folding the chunk fixture in chunks
    /// of every size finishes to the one-pass walk (the oversized commit
    /// dropped), and that is what the walk returns for any `--chunk-size`.
    #[test]
    fn the_walk_folds_in_chunks_to_the_one_pass_value() {
        let test = chunk_fixture();
        let run = open_run(&test, ID, &[]);
        let max_changes = run.max_changes;
        let whole = assert_chunk_invariant(
            &run,
            anomaly_prepare(&run),
            |gated| gated.oversized(max_changes),
            AnomalyFold::default,
            |fold, stamp, gated| fold.consume(stamp, gated.product),
            |fold, _| fold.finish(),
        );
        let positions: Vec<usize> = whole.pos_by_hash.values().copied().collect();
        assert_eq!(positions.len(), run.hashes.len() - 1, "{positions:?}");
        assert!(!positions.contains(&OVERSIZED_POS), "{positions:?}");
        for size in ["1", "4"] {
            let chunked = anomaly_walk(&run_matches(&test, ID, &["--chunk-size", size]));
            assert_eq!(chunked.as_ref(), Some(&whole), "--chunk-size {size}");
        }
    }
}
