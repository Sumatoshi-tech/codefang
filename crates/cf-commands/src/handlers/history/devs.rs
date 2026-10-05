//! `history/devs` over the shared history walk.
//!
//! Per commit: the filtered tree diff against the first parent (commits whose
//! RAW diff exceeds `--max-changes-per-commit` are dropped before any
//! analyzer sees them), per-change line stats (non-merge commits only) and
//! per-blob language attribution. A commit whose filtered diff is empty
//! (a trivial merge, an `--allow-empty` commit, a commit touching only
//! excluded paths) is counted only with `--empty-commits`. The shared
//! identity/tick stamps key the per-commit `CommitDevData` and
//! `commits_by_tick`; `--anonymize` / `--people-dict` / `--exact-signatures`
//! shape the developer names.

use std::collections::{BTreeMap, HashMap};

use cf_devs::{parse_tick_data_with_bounds, CommitDevData, LineStats, MetricOptions, TickBounds};
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::ChangeAction;

use cf_units::{KIB, MIB};

use super::{
    compute_diff_line_stats, detect_language, gated_product, walk_chunks, ChunkPlan, CommitStamp,
    HistoryRun, StampSummary, WalkCost,
};

/// `history/devs --format json` bytes (compact, no trailing newline).
#[must_use]
pub fn devs_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_run_metrics(sub)?;
    Some(cf_gojson::marshal(
        &cf_devs::serialize::computed_metrics_to_go(&metrics),
    ))
}

/// `history/devs --format yaml` bytes: the `codefang (v2)` header, the
/// `history/devs:` section name and the metrics.
#[must_use]
pub fn devs_run_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_run_metrics(sub)?;
    let mut out = Vec::new();
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
    out.extend_from_slice(b"history/devs:\n");
    let body = cf_goyaml::marshal(&cf_devs::serialize::computed_metrics_to_go_yaml(&metrics));
    out.extend_from_slice(&body);
    Some(out)
}

/// `history/devs --format bin` bytes: the CFB1 envelope over the json value.
#[must_use]
pub fn devs_run_report_bin(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_run_metrics(sub)?;
    let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
    cf_reportutil::encode_binary_envelope(&payload).ok()
}

/// Runs the devs walk once and returns the aggregated
/// [`cf_devs::ComputedMetrics`], so every output format is an encoding of the
/// SAME report value.
#[must_use]
pub fn devs_run_metrics(sub: &clap::ArgMatches) -> Option<cf_devs::ComputedMetrics> {
    let walk = devs_walk(sub)?;
    let input = parse_tick_data_with_bounds(
        &walk.commit_dev_data,
        &walk.commits_by_tick,
        walk.names,
        walk.tick_size_nanos,
        walk.tick_bounds,
    );
    Some(cf_devs::compute_all_metrics(
        &input,
        &MetricOptions::default(),
    ))
}

/// The raw products of one `history/devs` walk, shared by the aggregated
/// metrics path and the per-commit time-series (NDJSON) path.
struct DevsWalk {
    /// hex hash → per-commit dev data.
    commit_dev_data: BTreeMap<String, CommitDevData>,
    /// tick → hashes in walk order; drives commit order.
    commits_by_tick: BTreeMap<i64, Vec<String>>,
    /// tick → RFC3339-UTC committer-time bounds over CDD commits.
    tick_bounds: BTreeMap<i64, TickBounds>,
    /// Every kept commit in walk (= tick) order: `(hex, tick, committer
    /// seconds, committer UTC-offset minutes)`.
    commit_meta: Vec<(String, i64, i64, i32)>,
    /// The window position of each `commit_meta` entry (the key every
    /// analyzer's ndjson records merge on; a capped commit leaves a gap).
    commit_pos: Vec<usize>,
    /// Finalized reversed people dictionary (index = author id).
    names: Vec<String>,
    /// The run's tick size in nanoseconds (the devs activity window unit).
    tick_size_nanos: i64,
}

/// Nanoseconds per hour (the devs tick-size unit).
const NANOS_PER_HOUR: i64 = 3_600 * 1_000_000_000;

/// The per-commit, order-independent product of one commit's diff (behind
/// the shared drop gate, [`Gated`]).
#[derive(Default)]
struct DevsProduct {
    /// Post-filter change count, for the empty-commit gate.
    filtered_count: usize,
    /// `(language, line-stats)` per attributed change (non-merge only; empty
    /// for merges / oversized / empty commits).
    attributions: Vec<(String, LineStats)>,
}

/// Blob-hash → language over a commit's changes (Insert → To, Delete → From,
/// Modify → both sides), later changes overwriting earlier ones for the same
/// blob.
fn language_by_blob(
    repo: &cf_gitlib::Repository,
    changes: &[cf_gitlib::changes::Change],
) -> HashMap<cf_gitlib::Hash, String> {
    let mut langs: HashMap<cf_gitlib::Hash, String> = HashMap::new();
    let mut detect_into = |name: &str, h: cf_gitlib::Hash| {
        let lang = match CachedBlob::from_repo(repo, h) {
            Ok(b) => detect_language(name, &b.data),
            Err(_) => String::new(),
        };
        langs.insert(h, lang);
    };
    for change in changes {
        match change.action {
            ChangeAction::Insert => detect_into(&change.to.name, change.to.hash),
            ChangeAction::Delete => detect_into(&change.from.name, change.from.hash),
            ChangeAction::Modify => {
                detect_into(&change.to.name, change.to.hash);
                detect_into(&change.from.name, change.from.hash);
            }
        }
    }
    langs
}

/// Line stats of one change: Insert ⇒ the new blob's lines; Delete ⇒ the old
/// blob's lines; Modify ⇒ the shared libgit2 line diff
/// ([`compute_diff_line_stats`], so an empty blob on either side is an
/// ordinary 0-line text side); a same-blob modify, e.g. a pure rename or a
/// mode change, is a 0/0/0 entry that still creates the per-language key.
/// `None` for binary / unreadable blobs.
fn change_line_stats(
    repo: &cf_gitlib::Repository,
    change: &cf_gitlib::changes::Change,
) -> Option<LineStats> {
    match change.action {
        ChangeAction::Insert => {
            let lines = CachedBlob::from_repo(repo, change.to.hash)
                .ok()?
                .count_lines()
                .ok()?;
            Some(LineStats {
                added: lines as i64,
                removed: 0,
                changed: 0,
            })
        }
        ChangeAction::Delete => {
            let lines = CachedBlob::from_repo(repo, change.from.hash)
                .ok()?
                .count_lines()
                .ok()?;
            Some(LineStats {
                added: 0,
                removed: lines as i64,
                changed: 0,
            })
        }
        ChangeAction::Modify => {
            let old_lines = CachedBlob::from_repo(repo, change.from.hash)
                .ok()?
                .count_lines()
                .ok()?;
            CachedBlob::from_repo(repo, change.to.hash)
                .ok()?
                .count_lines()
                .ok()?;
            if change.from.hash == change.to.hash {
                return Some(LineStats::default());
            }
            let (added, removed, changed) =
                compute_diff_line_stats(repo, change.from.hash, change.to.hash, old_lines as i64);
            Some(LineStats {
                added,
                removed,
                changed,
            })
        }
    }
}

/// The per-commit pure product over the FILTERED changes: the filtered count
/// and the `(language, line-stats)` attributions (non-merge commits only).
fn devs_commit_product(
    repo: &cf_gitlib::Repository,
    commit: &cf_gitlib::commit::Commit<'_>,
    changes: &[cf_gitlib::changes::Change],
    first_parent: bool,
) -> DevsProduct {
    let mut attributions: Vec<(String, LineStats)> = Vec::new();
    if !super::is_merge(commit.num_parents(), first_parent) && !changes.is_empty() {
        let langs = language_by_blob(repo, changes);
        for change in changes {
            let Some(stats) = change_line_stats(repo, change) else {
                continue;
            };
            let data_hash = match change.action {
                ChangeAction::Delete => change.from.hash,
                _ => change.to.hash,
            };
            let lang = langs.get(&data_hash).cloned().unwrap_or_default();
            attributions.push((lang, stats));
        }
    }
    DevsProduct {
        filtered_count: changes.len(),
        attributions,
    }
}

/// `--empty-commits`: count commits whose filtered diff is empty.
fn empty_commits_flag(sub: &clap::ArgMatches) -> bool {
    sub.try_get_one::<bool>("empty-commits")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
}

/// The devs reduce state: each kept commit's product is folded into the
/// per-commit records the report and the per-commit formats read, then
/// dropped — the attributions never outlive their chunk.
struct DevsFold {
    /// `--empty-commits`.
    count_empty: bool,
    /// hex hash → per-commit dev data.
    commit_dev_data: BTreeMap<String, CommitDevData>,
    /// tick → hashes in walk order.
    commits_by_tick: BTreeMap<i64, Vec<String>>,
    /// tick → `(earliest, latest)` committer seconds over CDD commits.
    tick_when: BTreeMap<i64, (i64, i64)>,
    /// Every kept commit: `(hex, tick, committer seconds, UTC-offset minutes)`.
    commit_meta: Vec<(String, i64, i64, i32)>,
    /// The window position of each `commit_meta` entry.
    commit_pos: Vec<usize>,
}

impl DevsFold {
    /// An empty fold; `count_empty` is `--empty-commits`.
    fn new(count_empty: bool) -> Self {
        Self {
            count_empty,
            commit_dev_data: BTreeMap::new(),
            commits_by_tick: BTreeMap::new(),
            tick_when: BTreeMap::new(),
            commit_meta: Vec::new(),
            commit_pos: Vec::new(),
        }
    }

    /// Folds one kept commit, in window order.
    fn consume(&mut self, stamp: &CommitStamp, product: DevsProduct) {
        let hex = &stamp.hex;

        // commits_by_tick and commit_meta record EVERY kept commit (the
        // window visits each hash once).
        self.commits_by_tick
            .entry(stamp.tick)
            .or_default()
            .push(hex.clone());
        self.commit_meta
            .push((hex.clone(), stamp.tick, stamp.when, stamp.offset_min));
        self.commit_pos.push(stamp.pos);

        // Empty-commit gate: no record when the FILTERED tree diff is empty,
        // unless --empty-commits asks for those commits to be counted.
        if product.filtered_count == 0 && !self.count_empty {
            return;
        }

        let mut cdd = CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id: stamp.author_id,
            languages: BTreeMap::new(),
        };
        for (lang, stats) in product.attributions {
            cdd.added += stats.added;
            cdd.removed += stats.removed;
            cdd.changed += stats.changed;
            let ls = cdd.languages.entry(lang).or_default();
            *ls = ls.plus(stats);
        }
        self.commit_dev_data.insert(hex.clone(), cdd);

        self.tick_when
            .entry(stamp.tick)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(stamp.when);
                *hi = (*hi).max(stamp.when);
            })
            .or_insert((stamp.when, stamp.when));
    }

    /// The finished walk: the folded records, the RFC3339 tick bounds and
    /// the developer names.
    fn finish(self, summary: StampSummary, tick_size_nanos: i64) -> DevsWalk {
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
        DevsWalk {
            commit_dev_data: self.commit_dev_data,
            commits_by_tick: self.commits_by_tick,
            tick_bounds,
            commit_meta: self.commit_meta,
            commit_pos: self.commit_pos,
            names: summary.names,
            tick_size_nanos,
        }
    }
}

/// The devs walk's measured cost. Kubernetes, 31,923 commits: one worker
/// peaks at 825 MB, 24 workers at 1416 MB (25 MiB per added worker); the
/// whole window and 50-commit chunks peak alike (the per-commit product is a
/// few line-stat records).
const DEVS_COST: WalkCost = WalkCost {
    worker_bytes: 32 * MIB,
    product_bytes: KIB,
};

/// Runs the devs walk over the run's chunk plan: the gated per-commit
/// products are computed in parallel one chunk at a time and folded in
/// window order ([`walk_chunks`]).
fn devs_walk(sub: &clap::ArgMatches) -> Option<DevsWalk> {
    let run = HistoryRun::open(sub)?;
    let plan = ChunkPlan::for_walk(sub, "history/devs", run.hashes.len(), DEVS_COST).ok()?;
    let max_changes = run.max_changes;
    let filter = &run.filter;
    let first_parent = run.first_parent;
    let mut fold = DevsFold::new(empty_commits_flag(sub));
    let summary = walk_chunks(
        &run,
        plan,
        move |repo, hash| {
            gated_product(repo, hash, filter, max_changes, |commit, changes| {
                devs_commit_product(repo, commit, changes, first_parent)
            })
        },
        |gated| gated.oversized(max_changes),
        |stamp, gated| fold.consume(stamp, gated.product),
    )?;
    Some(fold.finish(summary, run.tick_size_hours() * NANOS_PER_HOUR))
}

/// The per-commit devs entry as a key-sorted map (`commits`, `lines_added`,
/// `lines_removed`, `lines_changed`, `net_change`, `author_id`, `languages`).
fn devs_entry_map(cdd: &CommitDevData) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};
    let mut devs = GoMap::new_map();
    devs.insert("commits".to_string(), GoValue::Int(cdd.commits));
    devs.insert("lines_added".to_string(), GoValue::Int(cdd.added));
    devs.insert("lines_removed".to_string(), GoValue::Int(cdd.removed));
    devs.insert("lines_changed".to_string(), GoValue::Int(cdd.changed));
    devs.insert(
        "net_change".to_string(),
        GoValue::Int(cdd.added - cdd.removed),
    );
    devs.insert("author_id".to_string(), GoValue::Int(cdd.author_id));
    if !cdd.languages.is_empty() {
        devs.insert("languages".to_string(), languages_map(&cdd.languages));
    }
    GoValue::Object(devs)
}

/// `map[string]LineStats` (key-sorted by language, each in `added` / `removed`
/// / `changed` field order).
fn languages_map(languages: &BTreeMap<String, LineStats>) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};
    let mut langs = GoMap::new_map();
    for (lang, stats) in languages {
        let mut ls = GoMap::new_struct();
        ls.insert("added".to_string(), GoValue::Int(stats.added));
        ls.insert("removed".to_string(), GoValue::Int(stats.removed));
        ls.insert("changed".to_string(), GoValue::Int(stats.changed));
        langs.insert(lang.clone(), GoValue::Object(ls));
    }
    GoValue::Object(langs)
}

/// Per-commit devs NDJSON records: one record per data-bearing commit in walk
/// order; `data` is the per-commit struct (`commits`, `lines_added`,
/// `lines_removed`, `lines_changed`, `author_id`, `languages`).
#[must_use]
pub fn devs_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = devs_walk(sub)?;
    let mut records = Vec::new();
    // Records carry the commit's WINDOW position: the merge with the other
    // leaves in `history_ndjson` keys on it, so neither a data-less commit
    // nor one dropped by `--max-changes-per-commit` may shift it.
    for (&pos, (hex, tick, secs, off)) in walk.commit_pos.iter().zip(&walk.commit_meta) {
        let Some(cdd) = walk.commit_dev_data.get(hex) else {
            continue;
        };
        let mut data = GoMap::new_struct();
        data.insert("commits".to_string(), GoValue::Int(cdd.commits));
        data.insert("lines_added".to_string(), GoValue::Int(cdd.added));
        data.insert("lines_removed".to_string(), GoValue::Int(cdd.removed));
        data.insert("lines_changed".to_string(), GoValue::Int(cdd.changed));
        data.insert("author_id".to_string(), GoValue::Int(cdd.author_id));
        if !cdd.languages.is_empty() {
            data.insert("languages".to_string(), languages_map(&cdd.languages));
        }
        records.push(super::super::history_formats::NdjsonRecord {
            pos,
            hash: hex.clone(),
            tick: *tick,
            author_id: cdd.author_id,
            time_secs: *secs,
            tz_offset_min: *off,
            data: GoValue::Object(data),
        });
    }
    Some(records)
}

/// The devs contribution to the merged `--format timeseries` document: the
/// per-commit entries plus the walk's own commit order (every kept commit,
/// tick order), so a devs-only document lists its commits.
#[must_use]
pub fn devs_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    let walk = devs_walk(sub)?;
    let per_commit = walk
        .commit_dev_data
        .iter()
        .map(|(hex, cdd)| (hex.clone(), devs_entry_map(cdd)))
        .collect();
    let commit_meta = walk
        .commit_meta
        .iter()
        .map(|(hex, tick, secs, off)| {
            (
                hex.clone(),
                *tick,
                crate::handlers::format_rfc3339_offset(*secs, *off),
                String::new(),
            )
        })
        .collect();
    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "devs",
        per_commit,
        commit_meta,
    })
}
