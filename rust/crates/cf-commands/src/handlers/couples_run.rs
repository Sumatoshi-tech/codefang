//! Real `history/couples` over the general history pipeline.
//!
//! Port of the Go streaming couples pipeline (run.go initHistoryPipeline /
//! initHeadOnly → Runner → couples `HistoryAnalyzer.Consume` (`processChange`,
//! seen-files Bloom merge dedup, oversized-changeset skip) → `Aggregator.Add`
//! → `ticksToReport` (`buildReport`: collect current files from the last
//! commit's tree, reduce to current files, byte-sorted file index,
//! per-file newline counts, people/files matrices) → `ComputeAllMetrics`).
//!
//! The tree diff base is always `parent(0)` (first parent), exactly as the Go
//! `TreeDiffAnalyzer` (`ensurePreviousTree` → `Parent(0)`); merge handling is
//! controlled by the `IsMerge` flag (`NumParents() > 1 && !--first-parent`).
//!
//! Output bytes route through `cf-gojson` (Go `encoding/json` parity); never
//! `serde_json`.

use cf_analyzers_plumbing::identity_detector::IdentityDetector;
use cf_couples::aggregator::Aggregator;
use cf_couples::tc::{CommitData, RenamePair};
use cf_couples::{compute_all_metrics, report};
use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
use std::collections::{BTreeMap, HashSet};

/// Expected unique file count for the seen-files Bloom filter
/// (Go: `seenFilesBloomExpected`).
const SEEN_FILES_BLOOM_EXPECTED: u64 = 100_000;
/// Target false-positive rate for the seen-files Bloom filter
/// (Go: `seenFilesBloomFP`).
const SEEN_FILES_BLOOM_FP: f64 = 0.01;
/// Maximum coupling-context size (Go: `CouplesMaximumMeaningfulContextSize`).
const MAX_MEANINGFUL_CONTEXT: usize = cf_couples::COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE;
/// 32 KB read buffer for newline counting (Go: `readBufferSize`).

/// `identity.AuthorMissing = (1 << 18) - 1`.
const AUTHOR_MISSING: i64 = (1 << 18) - 1;

/// `gitlib.Hash{}.String()` — the zero hash Go stamps on TCs whose Consume
/// early-returned without setting `CommitHash` (oversized couples changesets).
const ZERO_HASH_HEX: &str = "0000000000000000000000000000000000000000";

/// Builds the `history/couples` report value (Go `ComputedMetrics`, the single
/// value behind `ToJSON`/`ToYAML`) for either the HEAD commit (`--head`) or the
/// oldest `--limit` commits (streaming Reverse walk). Returns `None` if the
/// repository cannot be opened/walked. The caller serializes this one value
/// across json/yaml/bin uniformly (`serialize_history_metrics`), so every
/// format follows from the same report value (no per-format branch).
pub fn couples_run_value(sub: &clap::ArgMatches) -> Option<cf_gojson::GoValue> {
    couples_run(sub).map(|r| r.report_value)
}

/// The TYPED `ComputedMetrics` behind [`couples_run_value`] (same walk, same
/// `compute_all_metrics` product) — the text serializer reads struct fields
/// directly (Go couples/text.go `generateText` calls `ComputeAllMetrics` on
/// the report), so it must see the identical metrics the json/yaml bytes
/// encode.
pub fn couples_run_metrics(sub: &clap::ArgMatches) -> Option<cf_couples::ComputedMetrics> {
    couples_run(sub).map(|r| r.metrics)
}

/// One walked commit's couples products (Go `couples.Consume` TC + runner
/// stamps). `data` is `None` for a dedup-skipped merge (Go returns an EMPTY
/// `CommitData` with a zero commit hash, which the aggregator ignores).
pub(crate) struct CouplesCommit {
    /// Full hex hash.
    pub hash: String,
    /// The per-commit TC payload; `None` ⇔ dedup-skipped merge.
    pub data: Option<CommitData>,
    /// Runner-stamped tick (TicksSinceStart).
    pub tick: i64,
    /// Loose-identity author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The full products of one couples walk: the aggregated report value plus the
/// per-commit TC stream (one entry per walked commit, walk order).
pub(crate) struct CouplesRun {
    /// The aggregated `ComputedMetrics` GoValue (json/yaml/bin source).
    pub report_value: cf_gojson::GoValue,
    /// The typed metrics `report_value` was rendered from (text source).
    pub metrics: cf_couples::ComputedMetrics,
    /// Per-commit TC products, walk order.
    pub commits: Vec<CouplesCommit>,
    /// Bounded store-path file-coupling records (Go `writeFileCoupling`:
    /// `computeSparseCoupling` over the reduced sparse map, min edge weight 2,
    /// co-change-descending, top 100) — the `file_coupling` store kind the plot
    /// sections consume.
    pub store_file_coupling: Vec<cf_couples::FileCouplingData>,
}

/// The data the `history/couples` plot sections consume — the Rust analogue of
/// the analyzer's structured store kinds (Go `WriteToStoreFromAggregator`).
pub struct CouplesPlotData {
    /// `file_coupling` records (bounded sparse pairs, co-change-descending).
    pub file_coupling: Vec<cf_couples::FileCouplingData>,
    /// `dev_matrix` names after `FilterTopDevs` — EMPTY on every `run`
    /// pipeline: the aggregator's `reversedNames` is populated only from a
    /// preloaded people dict, which run streaming never configures, so the
    /// dev-coupling heatmap section is always skipped (matches the live Go
    /// pages).
    pub dev_names: Vec<String>,
    /// `ownership` records, `filesSequence` order (identical inputs to the
    /// dense `FileOwnershipMetric.Compute`, so the metric product is reused).
    pub ownership: Vec<cf_couples::FileOwnershipData>,
}

/// Builds the `history/couples` plot-section data over the shared couples walk.
pub fn couples_plot_data(sub: &clap::ArgMatches) -> Option<CouplesPlotData> {
    let run = couples_run(sub)?;
    Some(CouplesPlotData {
        file_coupling: run.store_file_coupling,
        dev_names: Vec::new(),
        ownership: run.metrics.file_ownership,
    })
}

pub(crate) fn couples_run(sub: &clap::ArgMatches) -> Option<CouplesRun> {
    let path = crate::handlers::run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let head_only = sub.get_flag("head");
    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    // Commit window: HEAD-only loads the single HEAD commit; streaming selects
    // the `limit` NEWEST commits (Go `gitlib.loadHistoryCommits`: newest-first
    // walk, CollectN, then slices.Reverse to oldest-first) — NOT the `limit`
    // oldest. With `limit <= 0` this is the full oldest-first history.
    let hashes: Vec<cf_gitlib::Hash> = if head_only {
        vec![repo.head().ok()?]
    } else {
        let v = crate::handlers::load_history_commit_hashes(&repo, limit, first_parent)?;
        // Go consume order at `--workers 1`: the streaming pipeline (commit
        // streamer + blob/diff/uast prefetch) preserves input order end-to-end,
        // so the couples leaf consumes commits in the oldest-first revwalk order
        // they are fed (see `crate::handlers::pipeline_consume_order` for the
        // per-stage source citation). This is the order in which the couples
        // seen-files Bloom is populated, so a merge commit's merge-mode coupling
        // gate (`!seenFiles.Test(name)`) depends on it. The additive
        // coupling/people matrices are order-independent, but the Bloom gate and
        // loose-identity id assignment are not, so we reproduce this exact order.
        crate::handlers::pipeline_consume_order(v)
    };

    let opts = PathPolicyOptions::default();
    // Loose identity detection (run streaming never preloads a people dict).
    // Identity is a CORE (plumbing) analyzer: it runs sequentially on the main
    // goroutine in oldest-first order BEFORE the leaf workers, and the resolved
    // AuthorID is handed to the forked leaves via the per-commit plumbing
    // snapshot. So author resolution is global oldest-first, NOT per-worker.
    let mut identity = IdentityDetector::new();

    // Go runs the couples leaf through `processCommitsHybrid` (runner.go): with a
    // single non-SequentialOnly leaf and CoreCount(8) < len(Analyzers)(9), the
    // leaf is FORKED across `LeafWorkers` workers, each with an INDEPENDENT
    // seen-files Bloom and merge-dedup tracker (couples `Fork`: fresh
    // `newSeenFilesFilter()` + `NewMergeTracker()`; `Merge` deliberately does NOT
    // combine them). Commits are dispatched round-robin by consume position
    // (`workers[commitIdx % numWorkers]`, hybridCommitLoop), so worker `w`
    // processes consume positions `p` with `p % numWorkers == w`, in oldest-first
    // order within that worker. The per-commit coupling/people data each worker
    // produces is then merged additively into the shared aggregator, so only the
    // merge-mode Bloom gate (`!seenFiles.Test(name)`) is worker-partition
    // sensitive. `LeafWorkers` is `max(NumCPU / 3, 4)` (coordinator default),
    // making the merge-mode coupling count CPU-count dependent — we reproduce the
    // live binary on THIS machine, which the oracle also runs on.
    let num_workers = crate::handlers::leaf_worker_count();
    // One forked seen-files Bloom + merge-dedup tracker per worker.
    let mut seen_files: Vec<cf_alg_bloom::Filter> = Vec::with_capacity(num_workers);
    for _ in 0..num_workers {
        seen_files.push(
            cf_alg_bloom::Filter::new_with_estimates(SEEN_FILES_BLOOM_EXPECTED, SEEN_FILES_BLOOM_FP)
                .ok()?,
        );
    }
    let mut seen_merges: Vec<HashSet<cf_gitlib::Hash>> = vec![HashSet::new(); num_workers];

    // The aggregator grows its people slices on demand (ensure_capacity), so the
    // initial PeopleNumber is 0 (loose detection discovers authors incrementally).
    let mut agg = Aggregator::new(0);

    let mut last_commit_hash: Option<cf_gitlib::Hash> = None;

    // ---- parallel pure-compute stage -----------------------------------------
    // ONLY the per-commit-independent work is parallelized — the tree diff (vs
    // parent(0)) + path-policy filter — which dominates the per-commit cost. Each
    // worker thread opens its OWN libgit2 Repository handle (the handle is !Send;
    // per-thread handles also avoid shared-ODB-cache contention). The author
    // signature is read here too (pure per commit) so the reduce needs no commit
    // lookup. The order-SENSITIVE reduce below is UNCHANGED: the per-worker
    // seen-files Bloom (partitioned by `pos % leaf_worker_count()`), the per-worker
    // merge-dedup tracker, the loose-identity consume order (oldest-first), and the
    // additive aggregator all run sequentially in the exact same order — only now
    // reading `prepared[pos]` instead of recomputing the diff inline. The Bloom
    // partition width (`num_workers`) is the modeled Go leaf-worker count and is
    // INDEPENDENT of the parallel-compute worker count; neither is changed here.
    struct CouplesPrepared {
        num_parents: usize,
        sig_name: String,
        sig_email: String,
        sig_when: i64,
        committer_when: i64,
        committer_offset: i32,
        changes: Vec<cf_gitlib::changes::Change>,
    }
    let compute_workers =
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let opts_ref = &opts;
    let prepared = crate::handlers::history::parallel_prepare(
        &path,
        &hashes,
        compute_workers,
        move |repo, hash| {
            let commit = repo.lookup_commit(hash).ok()?;
            let num_parents = commit.num_parents();
            let gsig = commit.author();

            // Tree diff against the first parent (root → full initial tree).
            let new_tree = commit.tree().ok()?;
            let raw_changes = if num_parents > 0 {
                let parent = commit.parent(0).ok()?;
                let old_tree = parent.tree().ok()?;
                tree_diff(repo, Some(&old_tree), Some(&new_tree)).ok()?
            } else {
                initial_tree_changes(repo, Some(&new_tree)).ok()?
            };

            // TreeDiffAnalyzer.shouldIncludeChange: path-policy exclusion (no blob
            // content, no language filter for the default all-languages case).
            let changes: Vec<cf_gitlib::changes::Change> = raw_changes
                .into_iter()
                .filter(|c| {
                    let name =
                        if c.action == ChangeAction::Delete { &c.from.name } else { &c.to.name };
                    !exclude(name, None, opts_ref)
                })
                .collect();

            let cw = commit.committer().when;
            Some(CouplesPrepared {
                num_parents,
                sig_name: gsig.name.clone(),
                sig_email: gsig.email.clone(),
                sig_when: gsig.when.seconds(),
                committer_when: cw.seconds(),
                committer_offset: cw.offset_minutes(),
                changes,
            })
        },
    )?;

    // ---- sequential ordered-reduce stage (UNCHANGED order) -------------------
    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;
    let mut commits: Vec<CouplesCommit> = Vec::with_capacity(hashes.len());

    for (pos, hash) in hashes.iter().enumerate() {
        let prep = &prepared[pos];
        // Worker that consumes this commit in Go's hybrid leaf dispatch.
        let worker = pos % num_workers;

        // Runner tick stamping (TicksSinceStart over the committer time).
        let base =
            *tick0.get_or_insert_with(|| crate::handlers::floor_tick_secs(prep.committer_when));
        let raw_tick = (prep.committer_when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Merge dedup: a merge commit (NumParents > 1) seen twice by the SAME
        // worker contributes an empty CommitData (Go: SeenOrAdd → return empty).
        // With unique hashes in a single window this never triggers, but mirror
        // Go faithfully — and the tracker is per-worker (Fork/Merge above).
        let is_multi_parent = prep.num_parents > 1;
        if is_multi_parent && !seen_merges[worker].insert(*hash) {
            // Already seen: empty CommitData with a ZERO commit hash in Go —
            // the aggregator ignores it; record no per-commit entry.
            commits.push(CouplesCommit {
                hash: hash.to_string(),
                data: None,
                tick,
                author_id: 0,
                when: prep.committer_when,
                offset_min: prep.committer_offset,
            });
            continue;
        }

        // IsMerge (Go runner.buildAnalyzeContext): NumParents > 1, unless
        // --first-parent forces single-parent semantics.
        let merge_mode = is_multi_parent && !first_parent;

        // lastCommit is set on every (non-dedup-skipped) consume.
        last_commit_hash = Some(*hash);

        // Identity: resolve this commit's author id (loose signature). Consumed in
        // oldest-first order (CORE analyzer), independent of the worker partition.
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: prep.sig_name.clone(),
            email: prep.sig_email.clone(),
            when_unix: prep.sig_when,
        });
        // Go: author = Identity.AuthorID; if AuthorMissing → PeopleNumber.
        // With loose detection author_id is always a real id, never AuthorMissing.
        let author = if author_id == AUTHOR_MISSING { 0 } else { author_id as usize };

        // The tree diff + path-policy filter for this commit was computed in the
        // parallel pre-pass; the reduce only reads it.
        let changes = &prep.changes;

        // Build this commit's CommitData (Go: Consume).
        let mut data = CommitData { commit_counted: true, ..CommitData::default() };

        // Oversized changeset: skip coupling/ownership extraction, but the commit
        // is still counted (CommitCounted = true) — Go returns `&data` early
        // WITHOUT setting TC.CommitHash, so the streamed ndjson line carries the
        // ZERO hash and the aggregator's `!tc.CommitHash.IsZero()` guard drops
        // the commit from commit_stats/commits_by_tick (no timeseries entry).
        let oversized = changes.len() > MAX_MEANINGFUL_CONTEXT;
        if !oversized {
            for change in changes {
                process_change(change, merge_mode, author, &mut data, &mut seen_files[worker]);
            }
        }

        agg.add(author, &data);
        commits.push(CouplesCommit {
            hash: if oversized { ZERO_HASH_HEX.to_string() } else { hash.to_string() },
            data: Some(data),
            tick,
            author_id,
            when: prep.committer_when,
            offset_min: prep.committer_offset,
        });
    }

    // buildReport (Go: ticksToReport → buildReport → collectCurrentFiles).
    //
    // The two code paths behave differently because of libgit2 object lifetimes:
    //
    // * --head: the single HEAD commit object is still LIVE when buildReport runs,
    //   so `lastCommit.Tree()` succeeds. `collectCurrentFiles` returns the HEAD
    //   tree's files, the report is reduced to those, and
    //   `computeFilesLinesFromCommit` reads each blob and counts newlines (real
    //   `FilesLines`). This matches the `--head --limit 5` golden (real line
    //   counts, files restricted to the HEAD tree).
    //
    // * streaming (--limit N, no --head): the commit objects consumed during the
    //   walk are FREED once their chunk completes, so the analyzer's `lastCommit`
    //   points at a freed commit and `lastCommit.Tree()` FAILS. `collectCurrentFiles`
    //   takes its fallback branch (return *all* accumulated raw-file keys) and
    //   every `commit.File(name)` in `computeFilesLinesFromCommit` likewise fails,
    //   so every `FilesLines` entry is 0. We reproduce that observed behavior: keep
    //   all raw files (no tree reduction) and zero line counts.
    let (current_files, files_lines): (Option<HashSet<String>>, BTreeMap<String, i32>) =
        if head_only {
            collect_current_and_lines(&repo, last_commit_hash)
        } else {
            (None, BTreeMap::new())
        };

    let mut report_data = agg.build_report(current_files.as_ref(), &files_lines);

    // Reversed people dict from loose identity detection (Go:
    // GetReversedPeopleDict()). FinalizeDict builds the reverse entries from the
    // incrementally-collected names/emails (the streaming pipeline finalizes the
    // loose dict before report rendering).
    identity.finalize_dict();
    report_data.reversed_people_dict = identity.reversed_people_dict.clone();

    // Store-path file coupling (Go store_writer.go `writeFileCoupling`):
    // sparse pairs over the SAME reduce the report uses (the live binary's
    // plot pipeline observably takes `collectUnfiltered` — couples' lastCommit
    // object is freed by store-finalize time, so the filtered prune/cap path
    // never runs), min edge weight 2 (`DefaultMinEdgeWeight`; the
    // `Couples.MinEdgeWeight` config key has no run flag), descending
    // co-changes, top 100 (`DefaultTopKPerFile`).
    let (reduced_files, _reduced_people) = agg.reduced(current_files.as_ref());
    let (files_sequence, files_index) = cf_couples::matrix::build_files_index(&reduced_files);
    let sparse_pairs = cf_couples::store::compute_sparse_coupling(
        &reduced_files,
        &files_sequence,
        &files_index,
        2,
    );
    let store_file_coupling = cf_couples::store::top_k_file_coupling(sparse_pairs, 100);

    let metrics = compute_all_metrics(&report_data);
    Some(CouplesRun {
        report_value: report::computed_metrics_to_value(&metrics),
        metrics,
        commits,
        store_file_coupling,
    })
}

/// Per-commit couples NDJSON records (forked leaf): every non-dedup-skipped
/// commit emits a line; `data` is Go's `*CommitData` struct — `CouplingFiles`
/// (initialized slice), `AuthorFiles` (map, key-sorted), `Renames` (initialized
/// slice of `{FromName, ToName}`), `CommitCounted` bool.
pub fn couples_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<crate::handlers::history_formats::NdjsonRecord>> {
    use cf_gojson::value::{GoMap, GoValue};

    let run = couples_run(sub)?;
    let mut records = Vec::new();
    for (pos, c) in run.commits.iter().enumerate() {
        let Some(cd) = &c.data else { continue };
        let mut data = GoMap::new_struct();
        data.insert(
            "CouplingFiles".to_string(),
            GoValue::Array(cd.coupling_files.iter().map(|f| GoValue::Str(f.clone())).collect()),
        );
        let mut authors = GoMap::new_map();
        for (f, n) in &cd.author_files {
            authors.insert(f.clone(), GoValue::Int(i64::from(*n)));
        }
        data.insert("AuthorFiles".to_string(), GoValue::Map(authors));
        data.insert(
            "Renames".to_string(),
            GoValue::Array(
                cd.renames
                    .iter()
                    .map(|r| {
                        let mut m = GoMap::new_struct();
                        m.insert("FromName".to_string(), GoValue::Str(r.from_name.clone()));
                        m.insert("ToName".to_string(), GoValue::Str(r.to_name.clone()));
                        GoValue::Map(m)
                    })
                    .collect(),
            ),
        );
        data.insert("CommitCounted".to_string(), GoValue::Bool(cd.commit_counted));
        records.push(crate::handlers::history_formats::NdjsonRecord {
            pos,
            hash: c.hash.clone(),
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: GoValue::Map(data),
        });
    }
    Some(records)
}

/// The couples contribution to the merged `--format timeseries` document (Go
/// `couples.ExtractCommitTimeSeries` over `report["commit_stats"]`): per
/// commit `{"files_touched": len(CouplingFiles), "author_id": id}`. The couples
/// aggregator is NOT tick-bucketed — its single TICK carries tick index 0, so
/// every merged commit reports `"tick": 0`.
pub fn couples_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<crate::handlers::history_formats::TimeSeriesContribution> {
    use cf_gojson::value::{GoMap, GoValue};

    let run = couples_run(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &run.commits {
        let Some(cd) = &c.data else { continue };
        if c.hash == ZERO_HASH_HEX {
            // Aggregator.Add: `if !tc.CommitHash.IsZero()` — oversized commits
            // never reach commit_stats, so the merged timeseries omits them.
            continue;
        }
        let mut entry = GoMap::new_map();
        entry.insert("files_touched".to_string(), GoValue::Int(cd.coupling_files.len() as i64));
        entry.insert("author_id".to_string(), GoValue::Int(c.author_id));
        per_commit.push((c.hash.clone(), GoValue::Map(entry)));
        commit_meta.push((
            c.hash.clone(),
            0, // aggregator TICK.Tick is 0 (single un-bucketed TICK).
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(crate::handlers::history_formats::TimeSeriesContribution {
        flag: "couples",
        per_commit,
        commit_meta,
    })
}

/// Collects the current-file set and per-file newline counts from the live
/// commit at `last_commit_hash` (Go: `collectCurrentFiles` +
/// `computeFilesLinesFromCommit`, used on the `--head` path where the commit
/// object is still live). Returns `(None, empty)` if the tree cannot be read,
/// matching Go's fallback (all raw files, zero lines).
fn collect_current_and_lines(
    repo: &cf_gitlib::Repository,
    last_commit_hash: Option<cf_gitlib::Hash>,
) -> (Option<HashSet<String>>, BTreeMap<String, i32>) {
    let Some(lc) = last_commit_hash else {
        return (None, BTreeMap::new());
    };
    let Ok(commit) = repo.lookup_commit(lc) else {
        return (None, BTreeMap::new());
    };
    let Ok(tree) = commit.tree() else {
        return (None, BTreeMap::new());
    };

    let mut set = HashSet::new();
    let mut lines: BTreeMap<String, i32> = BTreeMap::new();
    let mut iter = tree.files();
    while let Some(f) = iter.next_file() {
        // newline count of the blob (Go: countFileLinesAt over the file blob).
        let n = match f.contents() {
            Ok(data) => data.iter().filter(|&&b| b == b'\n').count() as i32,
            Err(_) => 0,
        };
        lines.insert(f.name.clone(), n);
        set.insert(f.name);
    }
    (Some(set), lines)
}

/// Per-commit change processing (Go: `HistoryAnalyzer.processChange`).
fn process_change(
    change: &cf_gitlib::changes::Change,
    merge_mode: bool,
    author: usize,
    data: &mut CommitData,
    seen_files: &mut cf_alg_bloom::Filter,
) {
    let action = change.action;

    let mut name = if action == ChangeAction::Delete {
        change.from.name.clone()
    } else {
        change.to.name.clone()
    };

    if action == ChangeAction::Modify && change.to.name != change.from.name {
        data.renames.push(RenamePair {
            from_name: change.from.name.clone(),
            to_name: change.to.name.clone(),
        });
        name = change.to.name.clone();
    }

    if merge_mode && action == ChangeAction::Delete {
        return;
    }

    if !merge_mode {
        if action != ChangeAction::Delete {
            data.coupling_files.push(name.clone());
        }
        seen_files.add(name.as_bytes());
        // Go: if author != AuthorMissing { AuthorFiles[name] = 1 }.
        if author != AUTHOR_MISSING_IDX {
            data.author_files.insert(name, 1);
        }
        return;
    }

    // Merge mode (Go: HistoryAnalyzer.processChange). Only add the file to the
    // coupling context if it was NOT seen on the first-parent line already
    // (`!seenFiles.Test(name)`): a file changed on a merged-in branch must not
    // be double-counted against files it already coupled with on mainline.
    // The author touch is always recorded (coupling dedup != ownership dedup).
    if !seen_files.test(name.as_bytes()) {
        data.coupling_files.push(name.clone());
    }
    if author != AUTHOR_MISSING_IDX {
        data.author_files.insert(name, 1);
    }
}

/// `identity.AuthorMissing` as a `usize` index (Go uses it both as the sentinel
/// id and, when present, as the `PeopleNumber` slot). Under loose detection the
/// resolved author is never this sentinel.
const AUTHOR_MISSING_IDX: usize = (1 << 18) - 1;

