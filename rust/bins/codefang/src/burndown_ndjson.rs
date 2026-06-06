//! `run --analyzers history/burndown --format timeseries --ndjson` closed form.
//!
//! Reproduces the Go streaming burndown time-series NDJSON pipeline for the
//! oldest `--limit` commits as a deterministic, single-pass computation:
//!
//! - **commit set / order** — `repository.Log(Reverse=true, FirstParent=true)`
//!   (oldest-first `SortTime|SortTopological|SortReverse`; burndown forces
//!   `--first-parent` in `run.go`), truncated to `--limit` commits.
//! - **tick assignment** (`plumbing.TicksSinceStart`, 24 h default tick) —
//!   `tick0 = FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h),
//!   previousTick)` over the committer time. With no identity provider
//!   (`PeopleNumber == 0`) the burndown "packed person+tick" value collapses to
//!   the raw tick, so the treap time keys are plain ticks.
//! - **per-commit deltas** (`burndown.HistoryAnalyzer.Consume` →
//!   `computeCommitLineStats`): each tracked path is a [`File`] line-survival
//!   treap. Inserts create a `File` at the commit tick; deletions update the
//!   file to length 0; modifications drive `File::update` from the
//!   `cf-godiff` line diff via the same `diffApplier` pending/flush logic as Go.
//!   Every `File::update` notifies an updater that records `(curTick, prevTick,
//!   delta)` into the per-commit global deltas. `lines_added` sums positive
//!   deltas at `(tick, tick)`; `lines_removed` sums the absolute negative deltas
//!   in the commit-tick row — exactly `computeCommitLineStats(globalDeltas[tick])`.
//!   Merge commits apply pure deletions at tick 0 (`applyDeletionUpdate`'s
//!   `if isMerge && !isDeletion { tick = 0 }`), so those negative deltas land
//!   outside the commit-tick row and are excluded from `lines_removed`.
//! - **NDJSON framing** (`analyze.WriteTimeSeriesNDJSON`): one
//!   `MergedCommitData` per commit, compact `json.Encoder.Encode` (sorted map
//!   keys `author, burndown, hash, tick, timestamp`, newline-terminated). Author
//!   is `""` (no identity provider); timestamp is the committer time as Go
//!   `time.RFC3339` in the commit's original zone offset.
//!
//! All output bytes route through `cf-gojson` for Go `encoding/json` parity.

use cf_burndown_core::{File, Updater};
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::{initial_tree_changes, tree_diff, Change, ChangeAction};
use cf_gitlib::repository::LogOptions;
use cf_godiff::Op;
use cf_gojson::value::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::{floor_tick_secs, format_rfc3339_offset, run_repo_path};

/// 24-hour tick period in seconds (`plumbing.DefaultTicksSinceStartTickSize`).
const TICK_PERIOD: i64 = 86_400;

/// Shared sink the per-file treap updaters push `(current, previous, delta)`
/// reports into. Drained per commit to compute that commit's line stats.
type DeltaSink = Rc<RefCell<Vec<(i64, i64, i64)>>>;

/// Builds the burndown timeseries NDJSON bytes for the oldest `--limit` commits,
/// or `None` if the repository cannot be opened/walked.
pub fn burndown_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Oldest-first walk (Reverse). Burndown forces --first-parent (run.go:
    // `if slices.Contains(analyzerKeys, "burndown") && !opts.FirstParent {
    // opts.FirstParent = true }`), so the walk follows only the first parent of
    // merge commits (simplify_first_parent). Truncated to --limit commits.
    let mut iter = repo
        .log(&LogOptions { reverse: true, first_parent: true, ..LogOptions::default() })
        .ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }
    drop(iter);

    let opts = PathPolicyOptions::default();
    let sink: DeltaSink = Rc::new(RefCell::new(Vec::new()));
    // Persistent burndown state: per-path line-survival treaps (shard.filesByID).
    let mut tracked: HashMap<String, File> = HashMap::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    let mut out: Vec<u8> = Vec::new();

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let new_tree = commit.tree().ok()?;

        // Tick assignment (committer time vs. tick0 floor, monotonic).
        let committer = commit.committer();
        let when = committer.when.seconds();
        let t0 = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw = (when - t0).div_euclid(TICK_PERIOD);
        let tick = raw.max(previous_tick);
        previous_tick = tick;

        // Tree diff vs. first parent (full initial tree for a root commit).
        let num_parents = commit.num_parents();
        let changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };
        let is_merge = num_parents > 1;

        sink.borrow_mut().clear();
        process_commit_changes(&repo, &changes, &opts, &mut tracked, &sink, tick, is_merge);
        let (added, removed) = commit_line_stats(&sink.borrow(), tick);

        // burndown ExtractCommitTimeSeries map: sorted keys lines_added,
        // lines_removed.
        let mut burndown = GoMap::new_map();
        burndown.insert("lines_added", GoValue::Int(added));
        burndown.insert("lines_removed", GoValue::Int(removed));

        let timestamp =
            format_rfc3339_offset(committer.when.seconds(), committer.when.offset_minutes());

        // MergedCommitData.MarshalJSON flat map (json.Marshal(map) → sorted keys:
        // author, burndown, hash, tick, timestamp).
        let mut commit_obj = GoMap::new_map();
        commit_obj.insert("author", GoValue::Str(String::new()));
        commit_obj.insert("burndown", GoValue::Map(burndown));
        commit_obj.insert("hash", GoValue::Str(hash.to_string()));
        commit_obj.insert("tick", GoValue::Int(tick));
        commit_obj.insert("timestamp", GoValue::Str(timestamp));

        // json.Encoder.Encode (compact, newline-terminated) per commit.
        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(commit_obj)));
        out.push(b'\n');
    }

    Some(out)
}

/// Builds the burndown **record** NDJSON bytes for the oldest `--limit` commits
/// (`run --analyzers history/burndown --format ndjson`, no `--timeseries`,
/// no `--head`), or `None` if the repository cannot be opened/walked.
///
/// Reproduces the Go streaming NDJSON sink (`analyze.StreamingSink.WriteTC`):
/// one `NDJSONLine{hash, tick, author_id, timestamp, analyzer, data}` per commit,
/// where `data` is the burndown `CommitResult` carried as `TC.Data`. Unlike the
/// time-series sibling this emits the full per-commit sparse `GlobalDeltas`
/// (`curTick -> prevTick -> lineCountDelta`) plus the derived `LinesAdded` /
/// `LinesRemoved`; the people/matrix/file/ownership fields are `null` because the
/// streaming pipeline runs with `PeopleNumber == 0` and no file tracking.
///
/// `author_id` is the identity assigned by `IdentityDetector` (default loose
/// signature matching, dict built incrementally in walk order): each commit's
/// lowercased author email/name resolves to (or registers) a sequential id.
/// `timestamp` is the committer time in the commit's original zone offset
/// (`time.RFC3339`). Tick assignment matches the time-series path
/// (`TicksSinceStart`, monotonic floor of committer time against `tick0`).
///
/// Both the top-level `NDJSONLine` object (`hash, tick, author_id, timestamp,
/// analyzer, data`) and the nested `data` `CommitResult` (`GlobalDeltas,
/// PeopleDeltas, MatrixDeltas, FileDeltas, FileOwnership, LinesAdded,
/// LinesRemoved`) preserve Go struct-declaration field order via struct-origin
/// maps, exactly as `json.Marshal` of those structs. All bytes route through
/// `cf-gojson`.
pub fn burndown_record_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    let mut iter = repo
        .log(&LogOptions { reverse: true, first_parent: true, ..LogOptions::default() })
        .ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }
    drop(iter);

    let opts = PathPolicyOptions::default();
    let sink: DeltaSink = Rc::new(RefCell::new(Vec::new()));
    let mut tracked: HashMap<String, File> = HashMap::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;
    let mut identity = LooseIdentity::default();

    let mut out: Vec<u8> = Vec::new();

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let new_tree = commit.tree().ok()?;

        // author_id: loose identity over the commit's author signature.
        let author = commit.author();
        let author_id = identity.resolve(&author.name, &author.email);

        let committer = commit.committer();
        let when = committer.when.seconds();
        let t0 = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw = (when - t0).div_euclid(TICK_PERIOD);
        let tick = raw.max(previous_tick);
        previous_tick = tick;

        let num_parents = commit.num_parents();
        let changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };
        let is_merge = num_parents > 1;

        sink.borrow_mut().clear();
        process_commit_changes(&repo, &changes, &opts, &mut tracked, &sink, tick, is_merge);
        let (global, added, removed) = commit_sparse_stats(&sink.borrow(), tick);

        // CommitResult struct (struct-declaration field order, GlobalDeltas full
        // sparse map; people/matrix/file/ownership null).
        let mut data = GoMap::new(MapOrigin::Struct);
        data.insert("GlobalDeltas", sparse_to_value(&global));
        data.insert("PeopleDeltas", GoValue::Null);
        data.insert("MatrixDeltas", GoValue::Null);
        data.insert("FileDeltas", GoValue::Null);
        data.insert("FileOwnership", GoValue::Null);
        data.insert("LinesAdded", GoValue::Int(added));
        data.insert("LinesRemoved", GoValue::Int(removed));

        let timestamp =
            format_rfc3339_offset(committer.when.seconds(), committer.when.offset_minutes());

        // NDJSONLine: json.Marshal of a struct → field-declaration order
        // (hash, tick, author_id, timestamp, analyzer, data).
        let mut line = GoMap::new(MapOrigin::Struct);
        line.insert("hash", GoValue::Str(hash.to_string()));
        line.insert("tick", GoValue::Int(tick));
        line.insert("author_id", GoValue::Int(author_id));
        line.insert("timestamp", GoValue::Str(timestamp));
        line.insert("analyzer", GoValue::Str("burndown".to_string()));
        line.insert("data", GoValue::Map(data));

        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(line)));
        out.push(b'\n');
    }

    Some(out)
}

/// `history/burndown --format json` over the general history pipeline (streaming,
/// e.g. `--limit N --workers 1`): the line-survival "burndown" report over the
/// oldest N commits.
///
/// This is the REAL port of the Go streaming pipeline
/// (`run.go initHistoryPipeline Reverse+FirstParent+Limit → RunStreaming →
/// burndown.HistoryAnalyzer.Consume → Aggregator.Add (MergeNestedAdditive of
/// per-commit GlobalDeltas) → ticksToReport (groupSparseHistory) →
/// ComputeAllMetrics`). The per-commit consume machinery is shared with the
/// NDJSON paths (`process_commit_changes`): every commit's filtered tree changes
/// drive the per-file burndown treaps, whose `(curTick, prevTick, delta)` updater
/// reports are folded into the running global sparse history `curTick -> prevTick
/// -> lineCountDelta` (additive, matching `mapx.MergeNestedAdditive`). At the end
/// the sparse history is densified with `groupSparseHistory` (Sampling =
/// Granularity = 30, both clamped equal) and `compute_global_metrics` runs the
/// global-survival + aggregate computation.
///
/// With the default config (`PeopleNumber == 0`, `TrackFiles == false`) the
/// report carries only `GlobalHistory`, so `file_survival` / `developer_survival`
/// are empty and `interactions` is nil — exactly the Go output shape. Bytes route
/// through `cf-gojson` (compact, no trailing newline) byte-identically to
/// `run/history_burndown.json`.
pub fn burndown_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Burndown defaults (Initialize): Granularity = Sampling = 30 (clamped equal
    // since Sampling is not > Granularity), TickSize = 24h. These config keys are
    // not exposed on the `run` subcommand, so the defaults always apply here.
    let granularity = 30i64;
    let sampling = 30i64;
    let tick_size_hours = 24i64;

    let mut iter = repo
        .log(&LogOptions { reverse: true, first_parent: true, ..LogOptions::default() })
        .ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }
    drop(iter);

    let opts = PathPolicyOptions::default();
    let sink: DeltaSink = Rc::new(RefCell::new(Vec::new()));
    let mut tracked: HashMap<String, File> = HashMap::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // Accumulated global sparse history (Aggregator.globalHistory) and the
    // maximum tick seen (Aggregator.lastTick → findLastTick).
    let mut global_history: cf_analyzer_burndown::SparseHistory = std::collections::BTreeMap::new();
    let mut last_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let new_tree = commit.tree().ok()?;

        let committer = commit.committer();
        let when = committer.when.seconds();
        let t0 = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw = (when - t0).div_euclid(TICK_PERIOD);
        let tick = raw.max(previous_tick);
        previous_tick = tick;

        let num_parents = commit.num_parents();
        let changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };
        let is_merge = num_parents > 1;

        sink.borrow_mut().clear();
        process_commit_changes(&repo, &changes, &opts, &mut tracked, &sink, tick, is_merge);
        let (global, _added, _removed) = commit_sparse_stats(&sink.borrow(), tick);

        // Aggregator.Add: MergeNestedAdditive(globalHistory, cr.GlobalDeltas).
        for (cur, row) in &global {
            let dst = global_history.entry(*cur).or_default();
            for (prev, delta) in row {
                *dst.entry(*prev).or_default() += *delta;
            }
        }

        if tick > last_tick {
            last_tick = tick;
        }
    }

    // ticksToReport: findLastTick scans the merged GlobalHistory tick keys, so
    // lastTick is the max curTick present (which equals the running max).
    let dense = cf_analyzer_burndown::group_sparse_history(&global_history, sampling, granularity, last_tick);
    let metrics = cf_analyzer_burndown::compute_global_metrics(&dense, sampling, tick_size_hours);

    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// `IdentityDetector` with default loose signature matching, built incrementally
/// in commit-walk order (`registerLooseIdentity`). Maps lowercased author
/// email/name to a sequential id, registering new identities on first sight.
#[derive(Default)]
struct LooseIdentity {
    dict: HashMap<String, i64>,
    size: i64,
}

impl LooseIdentity {
    fn resolve(&mut self, name: &str, email: &str) -> i64 {
        let email = email.to_lowercase();
        let name = name.to_lowercase();

        if let Some(&id) = self.dict.get(&email) {
            self.dict.entry(name).or_insert(id);
            return id;
        }
        if let Some(&id) = self.dict.get(&name) {
            self.dict.insert(email, id);
            return id;
        }
        let id = self.size;
        self.dict.insert(email, id);
        self.dict.insert(name, id);
        self.size += 1;
        id
    }
}

/// `computeCommitLineStats` plus the full sparse `globalDeltas[cur][prev]` map.
/// Reports `(cur, prev, delta)` aggregate into numerically-sorted nested
/// `BTreeMap`s (so canceling `(tick,tick,+n)`/`(tick,tick,-n)` cells vanish, and
/// the int map keys emit in Go's numeric order). `lines_added` sums positive
/// `(tick, tick)` cells; `lines_removed` sums the magnitudes of negative cells in
/// the commit-tick row.
fn commit_sparse_stats(
    reports: &[(i64, i64, i64)],
    tick: i64,
) -> (BTreeMap<i64, BTreeMap<i64, i64>>, i64, i64) {
    let mut global: BTreeMap<i64, BTreeMap<i64, i64>> = BTreeMap::new();
    for &(cur, prev, delta) in reports {
        *global.entry(cur).or_default().entry(prev).or_default() += delta;
    }
    let mut added = 0i64;
    let mut removed = 0i64;
    if let Some(row) = global.get(&tick) {
        for (&prev, &delta) in row {
            if prev == tick && delta > 0 {
                added += delta;
            } else if delta < 0 {
                removed += -delta;
            }
        }
    }
    (global, added, removed)
}

/// Serializes the sparse `curTick -> prevTick -> delta` map as a struct-origin
/// `GoValue` so the numerically-sorted `BTreeMap` insertion order is preserved on
/// encode (matching Go's `json.Marshal` of `map[int]map[int]int64`, which sorts
/// integer keys numerically and renders them as strings).
fn sparse_to_value(global: &BTreeMap<i64, BTreeMap<i64, i64>>) -> GoValue {
    let mut outer = GoMap::new(MapOrigin::Struct);
    for (cur, row) in global {
        let mut inner = GoMap::new(MapOrigin::Struct);
        for (prev, delta) in row {
            inner.insert(prev.to_string(), GoValue::Int(*delta));
        }
        outer.insert(cur.to_string(), GoValue::Map(inner));
    }
    GoValue::Map(outer)
}

/// `computeCommitLineStats(cr, curTick)`: the `(cur, prev, delta)` reports are
/// first aggregated into the sparse `globalDeltas[cur][prev]` cells (matching Go's
/// `incrementSparseHistory`), so a `(tick, tick, +n)` insert and a
/// `(tick, tick, -n)` delete in the same cell cancel. Then, over the current
/// tick's row, positive `(tick, tick)` cells sum into `lines_added` and negative
/// cells' magnitudes sum into `lines_removed`.
fn commit_line_stats(reports: &[(i64, i64, i64)], tick: i64) -> (i64, i64) {
    use std::collections::HashMap;
    // globalDeltas[cur][prev] += delta.
    let mut global: HashMap<i64, HashMap<i64, i64>> = HashMap::new();
    for &(cur, prev, delta) in reports {
        *global.entry(cur).or_default().entry(prev).or_default() += delta;
    }
    let mut added = 0i64;
    let mut removed = 0i64;
    if let Some(row) = global.get(&tick) {
        for (&prev, &delta) in row {
            if prev == tick && delta > 0 {
                added += delta;
            } else if delta < 0 {
                removed += -delta;
            }
        }
    }
    (added, removed)
}

/// Builds the treap updater for a newly tracked file: every `File::update`
/// report `(current, previous, delta)` is pushed to the shared sink. With no
/// identity provider the treap time keys are raw ticks, so `current`/`previous`
/// are already `curTick`/`prevTick`.
fn make_updater(sink: &DeltaSink) -> Vec<Updater> {
    let s = sink.clone();
    let updater: Updater = Box::new(move |cur, prev, delta| {
        s.borrow_mut().push((cur, prev, delta));
    });
    vec![updater]
}

/// Applies one commit's filtered changes to the per-path treaps, recording all
/// line deltas into `sink`.
#[allow(clippy::too_many_arguments)]
fn process_commit_changes(
    repo: &cf_gitlib::Repository,
    changes: &[Change],
    opts: &PathPolicyOptions,
    tracked: &mut HashMap<String, File>,
    sink: &DeltaSink,
    tick: i64,
    is_merge: bool,
) {
    for change in changes {
        // TreeDiff.filterChanges: exclude by the change's name (To for
        // insert/modify, From for delete) via the shared path policy.
        let filter_name = match change.action {
            ChangeAction::Delete => &change.from.name,
            _ => &change.to.name,
        };
        if exclude(filter_name, None, opts) {
            continue;
        }

        match change.action {
            ChangeAction::Insert => apply_insertion(repo, change, tracked, sink, tick),
            ChangeAction::Delete => apply_deletion(repo, change, tracked, tick, is_merge),
            ChangeAction::Modify => apply_modification(repo, change, tracked, sink, tick, is_merge),
        }
    }
}

/// `handleInsertion`: create a `File` at the commit tick of the To-blob's line
/// length (binaries skipped). `File::new` fires the updater with
/// `(tick, tick, +lines)`.
fn apply_insertion(
    repo: &cf_gitlib::Repository,
    change: &Change,
    tracked: &mut HashMap<String, File>,
    sink: &DeltaSink,
    tick: i64,
) {
    if change.to.hash.is_zero() {
        return;
    }
    let Ok(blob) = CachedBlob::from_repo(repo, change.to.hash) else {
        return;
    };
    let Ok(lines) = blob.count_lines() else {
        return; // binary: CountLines → ErrBinary; handleInsertion skips.
    };
    let file = File::new(tick, lines as i64, make_updater(sink));
    tracked.insert(change.to.name.clone(), file);
}

/// `handleDeletion`: when the tracked treap length matches the From-blob's line
/// count, update the file to length 0 (recording the survival debits), else
/// force-remove with no delta. On a merge the deletion time is tick 0 so its
/// negative deltas fall outside the commit-tick row. The file is always dropped.
fn apply_deletion(
    repo: &cf_gitlib::Repository,
    change: &Change,
    tracked: &mut HashMap<String, File>,
    tick: i64,
    is_merge: bool,
) {
    let name = if !change.to.hash.is_zero() {
        change.to.name.clone()
    } else {
        change.from.name.clone()
    };
    let Some(mut file) = tracked.remove(&name) else {
        return;
    };
    let Ok(blob) = CachedBlob::from_repo(repo, change.from.hash) else {
        return;
    };
    let Ok(lines) = blob.count_lines() else {
        return; // binary From: countDeletionLines errors (not in supported window).
    };
    if file.len() != lines as i64 {
        // forceRemoveFile: drop without recording a delta (already removed).
        return;
    }
    // applyDeletionUpdate: tick 0 on a merge (non-deletion), else the commit tick.
    let del_time = if is_merge { 0 } else { tick };
    let len = file.len();
    file.update(del_time, 0, 0, len);
    // file.Delete() drops tracking — already removed from the map.
}

/// `handleModification`: tracked text file → drive `File::update` from the line
/// diff; untracked, binary-from, or length-mismatched → fall back to insertion.
fn apply_modification(
    repo: &cf_gitlib::Repository,
    change: &Change,
    tracked: &mut HashMap<String, File>,
    sink: &DeltaSink,
    tick: i64,
    is_merge: bool,
) {
    // Not currently tracked → handleInsertion.
    if !tracked.contains_key(&change.from.name) {
        apply_insertion(repo, change, tracked, sink, tick);
        return;
    }

    let (Ok(blob_from), Ok(blob_to)) = (
        CachedBlob::from_repo(repo, change.from.hash),
        CachedBlob::from_repo(repo, change.to.hash),
    ) else {
        return;
    };

    let from_binary = blob_from.is_binary();
    let to_binary = blob_to.is_binary();

    // classifyBlobErrors: from binary,to text → insertion; from text,to binary →
    // deletion; both binary → skip.
    if from_binary && to_binary {
        return;
    }
    if from_binary && !to_binary {
        apply_insertion(repo, change, tracked, sink, tick);
        return;
    }
    if !from_binary && to_binary {
        apply_deletion(repo, change, tracked, tick, is_merge);
        return;
    }

    // Both valid: file.Len() must equal the diff's OldLinesOfCode (= From blob
    // line count); on mismatch, resetAndReinsert (full insertion).
    let old_lines = blob_from.count_lines().unwrap_or(0) as i64;
    let tracked_len = tracked.get(&change.from.name).map(File::len).unwrap_or(0);
    if tracked_len != old_lines {
        tracked.remove(&change.from.name);
        apply_insertion(repo, change, tracked, sink, tick);
        return;
    }

    // applyDiffs: drive File::update from the line diff via the diffApplier
    // pending/flush logic. The file may move to change.To.Name (here From == To).
    let diffs = cf_godiff::line_diff(&blob_from.data, &blob_to.data, true);
    let mut file = tracked.remove(&change.from.name).unwrap();
    apply_diffs(&mut file, &diffs, tick);
    tracked.insert(change.to.name.clone(), file);
}

/// Port of burndown's `diffApplier` (`applyDiffs`): walks the line diff and
/// issues `File::update(packValue=tick, position, insLen, delLen)` calls. A
/// pending delete merges with a following insert into a single replace; equal
/// runs flush any pending delete and advance the position.
fn apply_diffs(file: &mut File, diffs: &[cf_godiff::Segment], tick: i64) {
    let mut position: i64 = 0;
    // pending mirrors Go's `pending diffmatchpatch.Diff` (its op + line length).
    // An empty pending is represented by `None`.
    let mut pending: Option<(Op, i64)> = None;

    // flushPending: apply a deferred edit (insert advances position; delete does
    // not) and clear it.
    fn flush(file: &mut File, pending: &mut Option<(Op, i64)>, position: &mut i64, tick: i64) {
        if let Some((op, len)) = pending.take() {
            if op == Op::Insert {
                file.update(tick, *position, len, 0);
                *position += len;
            } else {
                file.update(tick, *position, 0, len);
            }
        }
    }

    for seg in diffs {
        let len = seg.lines.len() as i64;
        match seg.op {
            Op::Equal => {
                flush(file, &mut pending, &mut position, tick);
                position += len;
            }
            Op::Insert => {
                // handleInsert: if a pending (delete) exists, emit a combined
                // replace and clear it; otherwise defer this insert as pending.
                if let Some((_, del_len)) = pending {
                    file.update(tick, position, len, del_len);
                    position += len;
                    pending = None;
                } else {
                    pending = Some((Op::Insert, len));
                }
            }
            Op::Delete => {
                // Go overwrites pending with the delete (no flush); after
                // DiffCleanupMerge a delete is always followed by its insert or
                // an equality, so a pending insert never precedes a delete.
                pending = Some((Op::Delete, len));
            }
        }
    }
    flush(file, &mut pending, &mut position, tick);
}
