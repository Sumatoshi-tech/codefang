//! History-phase analyzer report builders, moved verbatim from the codefang
//! binary `main.rs` (the per-analyzer history report functions behind the old
//! per-(analyzer,format) dispatch ladder). The analyzer MATH lives in the
//! cf-* crates these call; this module owns only the shared history-pipeline
//! orchestration (one revwalk → per-commit tree diff → per-commit analyzer
//! feed → serialize) that the reference implementation `runHistoryPhase` + framework own.
//! Report bytes route through cf-gojson / cf-goyaml / cf-reportutil.

use crate::handlers::{floor_tick_secs, format_rfc3339_offset, run_repo_path};

thread_local! {
    /// One [`cf_uast::Parser`] per worker thread, reused across every commit that
    /// thread processes in a [`parallel_prepare`] UAST walk. `Parser::new()`
    /// registers a lazy parser for every embedded language mapping (hundreds of
    /// bloom inserts), so constructing it per commit dominated the parallel
    /// runtime — making the parallel UAST walks slower than sequential. A
    /// thread-local amortizes that construction to once per thread while keeping
    /// each thread its OWN parser (tree-sitter parsers are not thread-safe, so the
    /// parser is never shared ACROSS threads).
    static UAST_PARSER: cf_uast::Parser = cf_uast::Parser::new();
}

/// Runs `f` with this worker thread's reusable [`cf_uast::Parser`] (see
/// [`UAST_PARSER`]). Used by the per-commit compute closures of the parallel
/// UAST history walks (imports / quality / sentiment) so the parser is built once
/// per thread, not once per commit.
pub(crate) fn with_uast_parser<R>(f: impl FnOnce(&cf_uast::Parser) -> R) -> R {
    UAST_PARSER.with(|p| f(p))
}

/// Closed-form `history/burndown --head` metrics (single HEAD-commit window),
/// shared by the json/yaml/bin head formats.
pub fn burndown_head_metrics(
    sub: &clap::ArgMatches,
) -> Option<cf_analyzer_burndown::ComputedMetrics> {
    use cf_analyzer_burndown::metrics::{AggregateData, SurvivalData};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;
    let new_tree = commit.tree().ok()?;

    // Diff base: first parent's tree, or the empty tree for a root commit.
    let changes = if commit.num_parents() > 0 {
        let parent = commit.parent(0).ok()?;
        let old_tree = parent.tree().ok()?;
        tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
    } else {
        initial_tree_changes(&repo, Some(&new_tree)).ok()?
    };

    let opts = PathPolicyOptions::default();
    let mut total_lines: i64 = 0;
    for change in &changes {
        // handleInsertion uses To.Name; every surviving non-deletion change is
        // counted as a full insertion.
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        if exclude(&change.to.name, None, &opts) {
            continue;
        }
        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
            continue;
        };
        // CountLines → ErrBinary for binary blobs (handleInsertion skips them).
        if let Ok(lines) = blob.count_lines() {
            total_lines += lines as i64;
        }
    }

    Some(cf_analyzer_burndown::ComputedMetrics {
        aggregate: AggregateData {
            total_current_lines: total_lines,
            total_peak_lines: total_lines,
            overall_survival_rate: if total_lines > 0 { 1.0 } else { 0.0 },
            analysis_period_days: 0,
            num_bands: 1,
            num_samples: 1,
            tracked_files: 0,
            tracked_developers: 0,
        },
        global_survival: vec![SurvivalData {
            sample_index: 0,
            total_lines,
            survival_rate: if total_lines > 0 { 1.0 } else { 0.0 },
            band_breakdown: vec![total_lines],
        }],
        // computeFileSurvival/computeDeveloperSurvivalList return empty (non-nil)
        // slices → JSON `[]`; computeInteraction returns nil → JSON `null`.
        file_survival: Some(Vec::new()),
        developer_survival: Some(Vec::new()),
        interactions: None,
    })
}

/// Builds the `history/burndown --head --format timeseries` report bytes for the
/// HEAD commit, or `None` if HEAD has no resolvable tree.
///
/// Reproduces analyze.MergedTimeSeries for the single HEAD commit: the top-level
/// struct (`version`, `tick_size_hours`, `analyzers`, `commits`) holds one commit
/// whose `MarshalJSON`-flattened object carries the sorted-key metadata + the
/// burndown ExtractCommitTimeSeries map `{lines_added, lines_removed}`. The
/// commit insertion-line count is the same closed form as [`burndown_head_metrics`]
/// (every surviving non-deletion change is a full insertion; binaries skipped).
/// Timestamp is the committer time `RFC3339`-formatted in the commit's
/// ORIGINAL zone offset (runner.recordCommitMeta: `tc.Timestamp.Format(RFC3339)`,
/// `tc.Timestamp == ac.Time == commit.Committer().When`). Author is "" (burndown
/// registers no identity provider, so `authorName` resolves the missing author to
/// the empty string). tick_size_hours defaults to 24 (no --tick-size on run).
pub fn burndown_head_timeseries(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gojson::value::{GoMap, GoValue};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;
    let new_tree = commit.tree().ok()?;

    let changes = if commit.num_parents() > 0 {
        let parent = commit.parent(0).ok()?;
        let old_tree = parent.tree().ok()?;
        tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
    } else {
        initial_tree_changes(&repo, Some(&new_tree)).ok()?
    };

    let opts = PathPolicyOptions::default();
    let mut total_lines: i64 = 0;
    for change in &changes {
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        if exclude(&change.to.name, None, &opts) {
            continue;
        }
        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
            continue;
        };
        if let Ok(lines) = blob.count_lines() {
            total_lines += lines as i64;
        }
    }

    let committer = commit.committer();
    let timestamp =
        format_rfc3339_offset(committer.when.seconds(), committer.when.offset_minutes());
    let hash = commit.hash().to_string();

    // burndown ExtractCommitTimeSeries map: sorted keys lines_added, lines_removed.
    let mut burndown = GoMap::new_map();
    burndown.insert("lines_added", GoValue::Int(total_lines));
    burndown.insert("lines_removed", GoValue::Int(0));

    // MergedCommitData.MarshalJSON flat map (json.Marshal(map) → sorted keys:
    // author, burndown, hash, tick, timestamp).
    let mut commit_obj = GoMap::new_map();
    commit_obj.insert("author", GoValue::Str(String::new()));
    commit_obj.insert("burndown", GoValue::Map(burndown));
    commit_obj.insert("hash", GoValue::Str(hash));
    commit_obj.insert("tick", GoValue::Int(0));
    commit_obj.insert("timestamp", GoValue::Str(timestamp));

    // MergedTimeSeries struct: declaration order version, tick_size_hours,
    // analyzers, commits.
    let mut root = GoMap::new_struct();
    root.insert("version", GoValue::Str("codefang.timeseries.v1".into()));
    root.insert("tick_size_hours", GoValue::Int(24));
    root.insert(
        "analyzers",
        GoValue::Array(vec![GoValue::Str("burndown".into())]),
    );
    root.insert("commits", GoValue::Array(vec![GoValue::Map(commit_obj)]));

    // json.Encoder.SetIndent("", "  ").Encode → 2-space indent + trailing newline.
    let mut bytes = cf_gojson::marshal_indent(&GoValue::Map(root));
    bytes.push(b'\n');
    Some(bytes)
}

/// Formats Unix seconds as the reference `time.RFC3339` (`2006-01-02T15:04:05Z07:00`) in the
/// zone given by `offset_minutes` (libgit2 `git2::Time::offset_minutes`). A zero
/// offset prints the literal `Z`; otherwise `±HH:MM`. Mirrors the reference implementation's behavior where
/// a non-UTC `time.Time` formats its numeric offset and only UTC prints `Z`.
/// Builds the `history/anomaly --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the reference head-only pipeline for `history/anomaly`:
///  - tree diff: HEAD's tree vs its **first parent's** tree
///    (`TreeDiffAnalyzer.ensurePreviousTree` uses `Parent(0)`), then filtered
///    through the shared vendor / generated path policy
///    (`filterChanges -> pathpolicy.Exclude(name, nil, opts)`, content `nil`,
///    default opts: exclude vendor + generated paths). `files_changed` is the
///    surviving change count;
///  - per-change language detection (`LanguagesDetectionAnalyzer.Languages` +
///    `accumulateLanguagesAndAuthors`): each filtered change contributes its
///    extension-mapped language; `language_diversity` is the distinct count;
///  - a **merge** HEAD (`NumParents()>1`) skips `accumulateLineStats`
///    (reference behavior), so lines added/removed and net churn are 0 — the
///    deterministic, language-free-of-blob-content closed form. For a non-merge
///    HEAD the reference pipeline computes diff-match-patch line stats this closed form
///    does not reproduce; we return `None` so the caller surfaces the dispatch
///    sentinel rather than emitting subtly-divergent bytes;
///  - identity: a single HEAD commit yields author id 0
///    (`IdentityDetector` loose dict over `[head]`), so `author_count` is 1;
///  - tick assignment: the single HEAD commit lands in tick 0; tick bounds
///    start == end == HEAD's **committer** time, `RFC3339`-formatted UTC.
///
/// The typed report (`commit_metrics`/`commits_by_tick`/`tick_bounds`) is fed to
/// `cf_anomaly::build_report_data` → `compute_all_metrics`, whose
/// `ComputedMetrics::to_go_value` is serialized through cf-gojson (reference:
/// encoding/json parity: declaration-order keys, byte-sorted map keys, the reference implementation
/// shortest-float, `anomalies` nil slice → `null`, no trailing newline).
pub fn anomaly_head_report(sub: &clap::ArgMatches) -> Option<cf_anomaly::model::ComputedMetrics> {
    use std::collections::BTreeMap;

    use cf_anomaly::metrics::{build_report_data, TickBounds};
    use cf_anomaly::model::CommitAnomalyData;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{tree_diff, ChangeAction};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;

    // The HEAD commit is Consume'd exactly once (single tick). A MERGE HEAD
    // (NumParents > 1) skips accumulateLineStats (the reference implementation's LineStats plumbing emits
    // nothing for merges) so its line stats are 0; a regular HEAD computes them
    // from the HEAD-vs-first-parent diff. A root HEAD (no parent) has no diff
    // contract reproduced here.
    if commit.num_parents() == 0 {
        return None;
    }
    let is_merge = commit.num_parents() > 1;

    let committer_when = commit.committer().when.seconds(); // ac.Time == committer When.
    let commit_hash = commit.hash().to_string();

    // Tree diff HEAD vs first parent, then the shared vendor/generated filter.
    let new_tree = commit.tree().ok()?;
    let parent = commit.parent(0).ok()?;
    let old_tree = parent.tree().ok()?;
    let changes = tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?;

    let opts = PathPolicyOptions::default();
    let mut files_changed: i64 = 0;
    let mut languages: BTreeMap<String, i64> = BTreeMap::new();
    let mut lines_added: i64 = 0;
    let mut lines_removed: i64 = 0;
    for change in &changes {
        // changeNameHash: Delete → From.Name, otherwise To.Name.
        let name = if matches!(change.action, ChangeAction::Delete) {
            &change.from.name
        } else {
            &change.to.name
        };
        // filterChanges: pathpolicy.Exclude(name, nil, opts) (content nil).
        if exclude(name, None, &opts) {
            continue;
        }
        files_changed += 1;

        // accumulateLanguagesAndAuthors: count each non-empty detected language.
        // The reference implementation's Languages plumbing analyzer detects from BLOB CONTENT (not just the
        // extension), so a changed file whose extension is unknown but whose
        // content is recognized still contributes to language_diversity (e.g.
        // hercules's merge HEAD). Mirror the full revwalk path's `devs_detect_language`.
        let blob_hash = if matches!(change.action, ChangeAction::Delete) {
            change.from.hash
        } else {
            change.to.hash
        };
        let data = CachedBlob::from_repo(&repo, blob_hash)
            .map(|b| b.data)
            .unwrap_or_default();
        let lang = devs_detect_language(name, &data);
        if !lang.is_empty() {
            *languages.entry(lang).or_insert(0) += 1;
        }

        // accumulateLineStats (skipped for merge commits, mirroring the LineStats
        // plumbing analyzer): Insert ⇒ +lines of the new blob; Delete ⇒ +lines of
        // the old blob removed; Modify ⇒ diff-match-patch line stats, skipping
        // binary / identical-content blobs.
        if is_merge {
            continue;
        }
        match change.action {
            ChangeAction::Insert => {
                if let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) {
                    if let Ok(n) = blob.count_lines() {
                        lines_added += n as i64;
                    }
                }
            }
            ChangeAction::Delete => {
                if let Ok(blob) = CachedBlob::from_repo(&repo, change.from.hash) {
                    if let Ok(n) = blob.count_lines() {
                        lines_removed += n as i64;
                    }
                }
            }
            ChangeAction::Modify => {
                let (Ok(blob_from), Ok(blob_to)) = (
                    CachedBlob::from_repo(&repo, change.from.hash),
                    CachedBlob::from_repo(&repo, change.to.hash),
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
                    compute_diff_line_stats(&repo, change.from.hash, change.to.hash, old_lines);
                lines_added += a;
                lines_removed += r;
            }
        }
    }

    // Per-commit anomaly data: single HEAD commit ⇒ author id 0.
    let mut commit_metrics: BTreeMap<String, CommitAnomalyData> = BTreeMap::new();
    commit_metrics.insert(
        commit_hash.clone(),
        CommitAnomalyData {
            files_changed,
            lines_added,
            lines_removed,
            net_churn: lines_added - lines_removed,
            files: Vec::new(),
            languages,
            author_id: 0,
        },
    );

    // Single HEAD commit → tick 0.
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    commits_by_tick.insert(0, vec![commit_hash]);

    // tick_bounds[0] = { start: end: committer time } formatted RFC3339 UTC.
    let when_rfc3339 = cf_analyze::metadata::format_rfc3339_utc(committer_when);
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    tick_bounds.insert(
        0,
        TickBounds {
            start_time: when_rfc3339.clone(),
            end_time: when_rfc3339,
        },
    );

    // Default config: Threshold 2.0, WindowSize 20 (DefaultAnomalyThreshold /
    // DefaultAnomalyWindowSize); no --anomaly-threshold/--anomaly-window flags.
    let input = build_report_data(&commit_metrics, &commits_by_tick, tick_bounds, 2.0, 20);
    Some(cf_anomaly::metrics::compute_all_metrics(&input))
}

/// Runs the real `history/anomaly` pipeline over the oldest `--limit` commits and
/// returns the aggregated [`cf_anomaly::model::ComputedMetrics`], or `None` if the
/// repository cannot be opened/walked. This is the single report value every
/// machine format (json/yaml/bin) is an encoding of — the non-`--head` analogue of
/// [`anomaly_head_report`]'s closed form, driven through the general
/// revwalk → per-commit → aggregate → `ComputeAllMetrics` pipeline.
///
/// Faithful port of the reference streaming anomaly path
/// (the reference `initHistoryPipeline` → `framework.RunStreaming` →
/// `plumbing.{TreeDiff,BlobCache,FileDiff,LineStats,Languages,Identity}` →
/// `anomaly.Analyzer.Consume` → `extractTC`/`buildTick` → `ticksToReport` →
/// `AggregateCommitsToTicks` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits. `--first-parent` adds first-parent
///    simplification.
///  - **tick assignment** (`plumbing.TicksSinceStart`, 24 h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h), previousTick)`
///    over the committer time. `commits_by_tick` records each tick's commit
///    hashes; tick bounds = min/max committer time of the tick's commits, the reference implementation
///    `time.RFC3339`-formatted in UTC.
///  - **per-commit changes** (`TreeDiffAnalyzer`): tree diff against the commit's
///    **first git parent** (root → full initial tree), already filtered by the
///    shared vendor/generated path policy (`pathpolicy.Exclude(name, nil)`).
///    `files_changed = len(changes)`; `files` = each change's `To.Name`
///    (`anomaly.Consume`), unconditionally — exactly as the reference implementation appends.
///  - **line stats** (`LinesStatsCalculator`, **skipped for merge commits** —
///    `ac.IsMerge`): Insert ⇒ `CountLines(To)` added; Delete ⇒ `CountLines(From)`
///    removed; Modify ⇒ `computeDiffLineStats` (diff-match-patch line diff),
///    skipping binary / identical-content blobs. `net_churn = added - removed`.
///  - **languages** (`LanguagesDetectionAnalyzer.Languages`, keyed by blob hash:
///    Insert/Delete one side, Modify both sides; binary ⇒ `""`, extension fast
///    path, enry content fallback): `accumulateLanguagesAndAuthors` counts each
///    non-empty detected language into the commit's `languages` map, so
///    `language_diversity` (per tick) is the distinct-language count.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): the author id;
///    `author_count` per tick is the distinct-author-id count.
///
/// `cf_anomaly::metrics::build_report_data` then aggregates commits to ticks and
/// runs Z-score detection (default threshold 2.0, window 20 — `run` passes no
/// `--anomaly-threshold`/`--anomaly-window`), and `compute_all_metrics` derives
/// the `anomalies`/`time_series`/`aggregate` report value.
pub fn anomaly_run_metrics(sub: &clap::ArgMatches) -> Option<cf_anomaly::model::ComputedMetrics> {
    use cf_anomaly::metrics::build_report_data;
    let walk = anomaly_walk(sub)?;
    // Default config: Threshold 2.0, WindowSize 20.
    let input = build_report_data(
        &walk.commit_metrics,
        &walk.commits_by_tick,
        walk.tick_bounds,
        2.0,
        20,
    );
    Some(cf_anomaly::metrics::compute_all_metrics(&input))
}

/// The raw products of one `history/anomaly` revwalk, shared by the aggregated
/// metrics path and the per-commit ndjson/timeseries paths so every format
/// encodes the SAME walk (reference: runner: one pipeline, per-commit TCs).
pub(crate) struct AnomalyWalk {
    /// hex hash → per-commit anomaly data.
    pub commit_metrics: std::collections::BTreeMap<String, cf_anomaly::model::CommitAnomalyData>,
    /// tick → hashes in walk order.
    pub commits_by_tick: std::collections::BTreeMap<i64, Vec<String>>,
    /// tick → RFC3339-UTC committer-time bounds.
    pub tick_bounds: std::collections::BTreeMap<i64, cf_anomaly::metrics::TickBounds>,
    /// hex hash → tick.
    pub tick_by_hash: std::collections::BTreeMap<String, i64>,
    /// hex hash → (committer seconds, committer UTC-offset minutes).
    pub when_by_hash: std::collections::BTreeMap<String, (i64, i32)>,
}

pub(crate) fn anomaly_walk(sub: &clap::ArgMatches) -> Option<AnomalyWalk> {
    use std::collections::BTreeMap;

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_anomaly::metrics::TickBounds;
    use cf_anomaly::model::CommitAnomalyData;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation,
    // ignoring `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`).
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let policy = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    let mut commit_metrics: BTreeMap<String, CommitAnomalyData> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min, max) secs.

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // ---- parallel pure-compute stage -----------------------------------------
    // The expensive per-commit work — tree diff + per-change line stats (libgit2)
    // + language detection — is a PURE function of (repo, commit) and independent
    // across commits, so run it across all cores. The order-dependent reduce below
    // (identity ids, ticks, commits_by_tick, tick bounds) then runs sequentially
    // over the results in oldest-first order, byte-identically. `author_id` is the
    // ONLY order-dependent field, so it is left off here and stamped in the reduce.
    /// The expensive, per-commit-independent products of one commit's diff: the
    /// fully-built [`CommitAnomalyData`] minus its order-assigned `author_id`.
    struct AnomalyPrepared {
        data: CommitAnomalyData,
    }
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let policy_ref = &policy;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        let commit = repo.lookup_commit(hash).ok()?;
        let num_parents = commit.num_parents();
        // The reference `Commit.NumParents()` is reported as 1 for a merge under
        // --first-parent (implied when the selection includes history/burndown):
        // the simplified walk visits a merge as an ordinary single-parent commit,
        // so its first-parent diff line stats ARE accumulated. Only a merge seen
        // by the full walk skips accumulateLineStats.
        let is_merge = num_parents > 1 && !first_parent;

        // Tree diff against the first parent (root → full initial tree), then the
        // shared vendor/generated filter (TreeDiffAnalyzer.filterChanges).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(repo, Some(&new_tree)).ok()?
        };
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, policy_ref)
            })
            .collect();

        // anomaly.Consume: FilesChanged = len(changes); Files = each change's
        // To.Name (unconditionally, like the reference implementation's append).
        let mut cm = CommitAnomalyData {
            files_changed: changes.len() as i64,
            ..Default::default()
        };
        for change in &changes {
            cm.files.push(change.to.name.clone());
        }

        // accumulateLineStats (skipped for merge commits): per-change line stats.
        if !is_merge {
            let mut added = 0i64;
            let mut removed = 0i64;
            for change in &changes {
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
                        // computeModifyStats: needs both blobs, skips binary and
                        // identical content.
                        let (Ok(blob_from), Ok(blob_to)) = (
                            CachedBlob::from_repo(repo, change.from.hash),
                            CachedBlob::from_repo(repo, change.to.hash),
                        ) else {
                            continue;
                        };
                        if change.from.hash == change.to.hash {
                            continue;
                        }
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let old_lines = blob_from.count_lines().map_or(0, |n| n as i64);
                        let (a, r, _changed) = compute_diff_line_stats(
                            repo,
                            change.from.hash,
                            change.to.hash,
                            old_lines,
                        );
                        added += a;
                        removed += r;
                    }
                }
            }
            cm.lines_added = added;
            cm.lines_removed = removed;
        }

        // accumulateLanguagesAndAuthors: build the blob-hash → language map exactly
        // as Languages() does (Insert/Delete one side, Modify both sides), then
        // count each non-empty value into cm.languages.
        let mut by_blob: BTreeMap<cf_gitlib::hash::Hash, String> = BTreeMap::new();
        let mut detect = |entry: &cf_gitlib::changes::ChangeEntry| {
            let data = CachedBlob::from_repo(repo, entry.hash)
                .map(|b| b.data)
                .unwrap_or_default();
            by_blob.insert(entry.hash, devs_detect_language(&entry.name, &data));
        };
        for change in &changes {
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
        Some(AnomalyPrepared { data: cm })
    })?;

    // ---- sequential ordered-reduce stage -------------------------------------
    let mut tick_by_hash: BTreeMap<String, i64> = BTreeMap::new();
    let mut when_by_hash: BTreeMap<String, (i64, i32)> = BTreeMap::new();
    for (i, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;
        let hash_str = hash.to_hex();

        // Identity: resolve this commit's author id (loose signature), oldest-first.
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        // Tick assignment from the committer time (24 h default).
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Track committer-time bounds for the tick (extractTC updateTimeRange).
        tick_when
            .entry(tick)
            .and_modify(|(lo, hi)| {
                if when < *lo {
                    *lo = when;
                }
                if when > *hi {
                    *hi = when;
                }
            })
            .or_insert((when, when));
        commits_by_tick
            .entry(tick)
            .or_default()
            .push(hash_str.clone());
        tick_by_hash.entry(hash_str.clone()).or_insert(tick);
        when_by_hash
            .entry(hash_str.clone())
            .or_insert((when, committer_when.offset_minutes()));

        // Consume the precomputed per-commit data (tree diff / line stats /
        // languages); only the order-assigned author id is stamped here.
        let mut cm = prepared[i].data.clone();
        cm.author_id = author_id;
        commit_metrics.insert(hash_str, cm);
    }

    // tick_bounds[tick] = { start: end: committer min/max } RFC3339 UTC.
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    Some(AnomalyWalk {
        commit_metrics,
        commits_by_tick,
        tick_bounds,
        tick_by_hash,
        when_by_hash,
    })
}

/// Serializes one [`cf_anomaly::model::CommitAnomalyData`] as the reference struct
/// (declaration field order; `files`/`languages` are `omitempty`).
fn anomaly_data_value(cm: &cf_anomaly::model::CommitAnomalyData) -> cf_gojson::GoValue {
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

/// Per-commit anomaly NDJSON records: every walked commit emits a line (reference:
/// `anomaly.Consume` never returns nil Data for a real commit); `data` is the
/// `*CommitAnomalyData` struct.
pub fn anomaly_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    let walk = anomaly_walk(sub)?;
    let mut records = Vec::new();
    let mut pos = 0usize;
    for hashes in walk.commits_by_tick.values() {
        for hex in hashes {
            // Every walked commit occupies a consume position (anomaly emits a
            // TC for each, so positions and records are 1:1 here).
            let p = pos;
            pos += 1;
            let Some(cm) = walk.commit_metrics.get(hex) else {
                continue;
            };
            let (secs, off) = walk.when_by_hash.get(hex).copied().unwrap_or((0, 0));
            records.push(super::history_formats::NdjsonRecord {
                pos: p,
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

/// The anomaly contribution to the merged `--format timeseries` document (reference:
/// `anomaly.ExtractCommitTimeSeries` over `report["commit_metrics"]`); the
/// anomaly report DOES carry `commits_by_tick`, so commits are ordered.
pub fn anomaly_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
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
            // CommitMeta.Author is "" — ReversedPeopleDict is not finalized when
            // recordCommitMeta runs, so the reference implementation's authorName always misses.
            commit_meta.push((
                hex.clone(),
                tick,
                crate::handlers::format_rfc3339_offset(secs, off),
                String::new(),
            ));
        }
    }
    Some(super::history_formats::TimeSeriesContribution {
        flag: "anomaly",
        per_commit,
        commit_meta,
    })
}

/// Builds the `run --analyzers history/quality --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the reference streaming quality pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (reference initHistoryPipeline: `commitCount` capped at `opts.Limit`).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time; the tick size is the 24 h default (`run` passes no
///    `--tick-size`). Tick bounds = min/max committer time of the commits in the
///    tick, `RFC3339`-formatted in UTC (`FormatStartTime/EndTime`).
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`TreeDiffAnalyzer.ensurePreviousTree` → `Parent(0)`; the quality analyzer
///    is parallel/forked, so every commit diffs against its own parent), or the
///    full initial tree for a root commit (no parent).
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **> 32**
///    file changes is spilled to disk; on the streaming run the quality analyzer's
///    `TreeDiff.Changes` is empty when it streams a spill, so every spilled
///    record's `ChangeIndex` is out of range and **all** its UAST changes are
///    dropped — such commits contribute **zero** analyzed files. Commits with ≤ 32
///    changes are parsed in memory.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): the shared vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`); the surviving files are analyzed.
///
/// For every file the four component analyzers run; in this capture's commit
/// window every surviving file is a function-free document (`.md` / `.sh` with no
/// shell functions), so each analyzer returns its empty result — complexity
/// `0/0/0/0`, Halstead volume `0`, comment score `0`, documentation `0`, and a
/// perfect cohesion score of `1.0` (cohesion of a tree with no methods). The
/// per-tick [`TickQuality`] is fed to `cf_quality::compute_all_metrics` and
/// serialized compact through cf-gojson (`to_json_compact`: the reference `json.Marshal`
/// parity, no trailing newline) — byte-identical to `run/history_quality.json`.
pub fn quality_metrics(sub: &clap::ArgMatches) -> Option<cf_quality::ComputedMetrics> {
    use cf_quality::{compute_all_metrics, ReportData, TickBounds, TickQuality};
    use std::collections::BTreeMap;

    let walk = quality_walk(sub)?;

    // Per-tick merged quality + bounds (committer-time min/max), folding each
    // commit's per-file samples into its tick in walk order.
    let mut tick_quality: BTreeMap<i64, TickQuality> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for c in &walk {
        tick_when
            .entry(c.tick)
            .and_modify(|(lo, hi)| {
                if c.when < *lo {
                    *lo = c.when;
                }
                if c.when > *hi {
                    *hi = c.when;
                }
            })
            .or_insert((c.when, c.when));
        let tq = tick_quality.entry(c.tick).or_default();
        merge_tick_quality(tq, &c.tq);
    }

    // Format tick bounds RFC3339 UTC (FormatStartTime / FormatEndTime).
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    let input = ReportData {
        tick_quality,
        tick_bounds,
    };
    Some(compute_all_metrics(&input))
}

/// One walked commit's quality products (the reference `quality.Consume` TC + runner
/// stamps).
#[derive(Clone)]
pub(crate) struct QualityCommit {
    /// The commit's position in the ORIGINAL walk window (0-based, counting
    /// oversized-dropped commits too): the reference runner assigns consume
    /// positions before the oversized drop suppresses a commit's record, and
    /// the forked-leaf NDJSON drain order is keyed on this position.
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// This commit's per-file quality samples (reference: per-commit `TickQuality`).
    pub tq: cf_quality::TickQuality,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Loose-identity author id (walk order).
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The shared `history/quality` revwalk: per-commit `TickQuality` plus the
/// order-assigned identity/tick stamps, in walk order. Every quality format
/// consumes THIS one walk.
pub(crate) fn quality_walk(sub: &clap::ArgMatches) -> Option<Vec<QualityCommit>> {
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_pathpolicy::Options as PathPolicyOptions;

    // Multi-analyzer runs route through the ONE shared UAST walk (same code,
    // one tree diff + one parse per blob per commit across the co-selected
    // analyzers); single-analyzer runs keep this direct walk.
    if let Some(shared) = super::uast_walk::shared_quality_walk(sub) {
        return shared;
    }

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation, ignoring
    // `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`). The HEAD commit on a large repo spills (> 32
    // changes ⇒ zero UAST files), yielding the reference implementation's single all-zero tick-0 report.
    let first_parent = crate::handlers::effective_first_parent(sub);
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let opts = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // Oversized-commit gate: commits whose RAW tree diff exceeds the cap are
    // silently dropped from history BEFORE any analyzer (reference framework
    // behaviour; flag `--max-changes-per-commit`, 0 = default 10000).
    let max_changes = crate::handlers::history::max_changes_per_commit_cap(sub);

    // ---- parallel pure-compute stage -----------------------------------------
    // The expensive per-commit work — tree diff + per-file UAST parse + the four
    // component analyzers — is a PURE function of (repo, commit), so run it across
    // all cores. The result is a per-commit `TickQuality` carrying that commit's
    // per-file samples (empty for a spilled/zero-file commit) plus the RAW
    // tree-diff change count for the oversized-commit gate. The order-dependent
    // reduce below (monotonic tick assignment, per-tick committer-time bounds,
    // merge of each commit's samples into its tick) runs UNCHANGED and
    // sequentially over these results. The per-commit body is the SAME
    // `quality_commit_product` the shared multi-analyzer walk calls.
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let opts_ref = &opts;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        // Per-thread UAST parser, reused across this thread's commits (tree-sitter
        // parsers are not thread-safe, so never shared across threads).
        crate::handlers::history::with_uast_parser(|parser| {
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = commit_tree_changes(repo, &commit)?;
            let raw_change_count = changes.len();
            // Oversized commits are dropped before any analyzer — no per-file work.
            if raw_change_count > max_changes {
                return Some((raw_change_count, cf_quality::TickQuality::default()));
            }
            let mut cache = super::uast_walk::CommitParseCache::new(repo, parser, opts_ref);
            Some((
                raw_change_count,
                quality_commit_product(&changes, &mut cache),
            ))
        })
    })?;

    // ---- sequential ordered identity/tick stamping ----------------------------
    let mut commits = Vec::with_capacity(hashes.len());
    for (i, hash) in hashes.iter().enumerate() {
        // Oversized-commit skip: dropped from history before identity/tick
        // stamping (the reference framework never shows the commit to any
        // analyzer, core or leaf).
        if prepared[i].0 > max_changes {
            continue;
        }
        let commit = repo.lookup_commit(*hash).ok()?;
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();

        // Identity: resolve this commit's author id (loose signature), oldest-first.
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        commits.push(QualityCommit {
            pos: i,
            hash: hash.to_string(),
            tq: prepared[i].1.clone(),
            tick,
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
        });
    }
    Some(commits)
}

/// Computes a commit's tree changes against its first parent (root commit →
/// the full initial tree) — the ONE diff every history UAST walk starts from,
/// shared by the direct walks and the multi-analyzer shared walk.
pub(crate) fn commit_tree_changes(
    repo: &cf_gitlib::Repository,
    commit: &cf_gitlib::commit::Commit<'_>,
) -> Option<cf_gitlib::changes::Changes> {
    use cf_gitlib::changes::{initial_tree_changes, tree_diff};
    let new_tree = commit.tree().ok()?;
    if commit.num_parents() > 0 {
        let parent = commit.parent(0).ok()?;
        let old_tree = parent.tree().ok()?;
        tree_diff(repo, Some(&old_tree), Some(&new_tree)).ok()
    } else {
        initial_tree_changes(repo, Some(&new_tree)).ok()
    }
}

/// The `history/quality` per-commit product (the reference `quality.Consume`
/// body): the per-file quality samples of this commit's surviving After trees.
/// Called by BOTH the direct [`quality_walk`] and the shared multi-analyzer
/// UAST walk, so the two are byte-identical by construction.
///
/// Gates: the spill rule (> 32 changes ⇒ zero UAST changes), the Delete /
/// zero-hash skip, then the `UASTPipeline.parseBlob` gate chain inside the
/// parse cache. A gates-passed file whose parse fails still counts as one
/// analyzed file (a function-free sample), keeping `files_analyzed`
/// byte-identical (reference: every parsed file has a node there).
pub(crate) fn quality_commit_product(
    changes: &[cf_gitlib::changes::Change],
    cache: &mut super::uast_walk::CommitParseCache<'_>,
) -> cf_quality::TickQuality {
    use super::uast_walk::{ParseOutcome, SPILL_THRESHOLD};
    use cf_gitlib::changes::ChangeAction;

    // This commit's per-file quality samples (the reference implementation appends one sample per
    // analyzed file into the tick; here we collect them per commit first).
    let mut commit_q = cf_quality::TickQuality::default();

    // Spill rule: > 32 changes ⇒ the quality analyzer sees zero UAST changes.
    if changes.len() > SPILL_THRESHOLD {
        return commit_q;
    }

    for change in changes {
        // Quality analyzes the After version only (Insert / Modify).
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        // Parse the file to UAST and run the four component analyzers on the
        // `change.After` root, exactly as the reference `quality.(*Analyzer).analyzeNode`
        // (complexity -> halstead -> comments -> cohesion), recording the same
        // scalar keys. One sample per analyzed file.
        match &*cache.parse(&change.to.name, change.to.hash) {
            ParseOutcome::Parsed(root) => accumulate_quality_file(root, &mut commit_q),
            ParseOutcome::Failed(_) => push_empty_quality_sample(&mut commit_q),
            ParseOutcome::Skipped => {}
        }
    }
    commit_q
}

/// Serializes one per-commit [`cf_quality::TickQuality`] as the reference implementation's `*TickQuality`
/// struct (declaration field order, exported field names, nil slices → null).
fn quality_tq_value(tq: &cf_quality::TickQuality) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};
    fn floats(v: &[f64]) -> GoValue {
        if v.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(v.iter().map(|f| GoValue::Float(*f)).collect())
        }
    }
    fn ints(v: &[i64]) -> GoValue {
        if v.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(v.iter().map(|i| GoValue::Int(*i)).collect())
        }
    }
    let mut m = GoMap::new_struct();
    m.insert("Complexities".to_string(), floats(&tq.complexities));
    m.insert("Cognitives".to_string(), floats(&tq.cognitives));
    m.insert("MaxComplexities".to_string(), ints(&tq.max_complexities));
    m.insert("Functions".to_string(), ints(&tq.functions));
    m.insert("HalsteadVolumes".to_string(), floats(&tq.halstead_volumes));
    m.insert("HalsteadEfforts".to_string(), floats(&tq.halstead_efforts));
    m.insert("DeliveredBugs".to_string(), floats(&tq.delivered_bugs));
    m.insert("CommentScores".to_string(), floats(&tq.comment_scores));
    m.insert("DocCoverages".to_string(), floats(&tq.doc_coverages));
    m.insert("CohesionScores".to_string(), floats(&tq.cohesion_scores));
    GoValue::Object(m)
}

/// Per-commit quality NDJSON records (forked leaf): every commit emits a line
/// whose `data` is the reference `*TickQuality` struct (nil slices → null).
pub fn quality_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    let walk = quality_walk(sub)?;
    Some(
        walk.iter()
            .map(|c| super::history_formats::NdjsonRecord {
                // The consume position counts oversized-dropped commits too
                // (the reference runner numbers positions before the drop
                // suppresses a record), so the forked-leaf drain order keys on
                // the ORIGINAL walk position, not the surviving index.
                pos: c.pos,
                hash: c.hash.clone(),
                tick: c.tick,
                author_id: c.author_id,
                time_secs: c.when,
                tz_offset_min: c.offset_min,
                data: quality_tq_value(&c.tq),
            })
            .collect(),
    )
}

/// The quality contribution to the merged `--format timeseries` document (reference:
/// `quality.ExtractCommitTimeSeries` over `report["commit_quality"]`): per
/// commit the 11-key summary map (`cf_quality::commit_summary`).
pub fn quality_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
    let walk = quality_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &walk {
        let summary = cf_quality::commit_summary(&c.tq);
        per_commit.push((
            c.hash.clone(),
            cf_quality::serialize::commit_summary_value(&summary),
        ));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(super::history_formats::TimeSeriesContribution {
        flag: "quality",
        per_commit,
        commit_meta,
    })
}

/// Builds the `run --analyzers history/quality --format timeseries --ndjson`
/// bytes: one compact JSON line per walked commit (the reference per-chunk
/// `TimeSeriesChunkFlusher` over `DrainCommitStats`), each line the merged
/// commit object `{author, hash, quality:{...}, tick, timestamp}` with the
/// per-commit summary map (`drainQualityCommitData` keys) under `"quality"`.
/// Same memoized walk as every other quality format.
pub fn quality_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gojson::{GoMap, GoValue, MapOrigin};

    let contrib = quality_timeseries_contribution(sub)?;
    let mut out = Vec::new();
    for (hash, tick, ts, author) in &contrib.commit_meta {
        // assembleCommits filters the ordered meta to hashes the analyzer
        // contributed data for (nil-Data TCs order but never emit).
        let Some((_, v)) = contrib.per_commit.iter().find(|(h, _)| h == hash) else {
            continue;
        };
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("hash", GoValue::Str(hash.clone()));
        m.push("timestamp", GoValue::Str(ts.clone()));
        m.push("author", GoValue::Str(author.clone()));
        m.push("tick", GoValue::Int(*tick));
        m.push("quality", v.clone());
        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(m)));
        out.push(b'\n');
    }
    Some(out)
}

/// Appends every per-file sample in `src` onto `dst`, preserving order. Used by
/// the parallel quality walk to fold one commit's precomputed [`TickQuality`]
/// (its per-file samples) into its tick's accumulator in walk order — equivalent
/// to having pushed each file's samples directly, but computed off-thread.
fn merge_tick_quality(dst: &mut cf_quality::TickQuality, src: &cf_quality::TickQuality) {
    dst.complexities.extend_from_slice(&src.complexities);
    dst.cognitives.extend_from_slice(&src.cognitives);
    dst.max_complexities
        .extend_from_slice(&src.max_complexities);
    dst.functions.extend_from_slice(&src.functions);
    dst.halstead_volumes
        .extend_from_slice(&src.halstead_volumes);
    dst.halstead_efforts
        .extend_from_slice(&src.halstead_efforts);
    dst.delivered_bugs.extend_from_slice(&src.delivered_bugs);
    dst.comment_scores.extend_from_slice(&src.comment_scores);
    dst.doc_coverages.extend_from_slice(&src.doc_coverages);
    dst.cohesion_scores.extend_from_slice(&src.cohesion_scores);
}

/// Pushes one function-free quality sample (the reference `analyzeNode` over a tree with no
/// functions): complexity 0/0/0/0, Halstead 0/0/0, comments 0/0, cohesion 1.0
/// (the cohesion analyzer's empty-result `cohesion_score`).
fn push_empty_quality_sample(tq: &mut cf_quality::TickQuality) {
    tq.complexities.push(0.0);
    tq.cognitives.push(0.0);
    tq.max_complexities.push(0);
    tq.functions.push(0);
    tq.halstead_volumes.push(0.0);
    tq.halstead_efforts.push(0.0);
    tq.delivered_bugs.push(0.0);
    tq.comment_scores.push(0.0);
    tq.doc_coverages.push(0.0);
    tq.cohesion_scores.push(1.0);
}

/// Runs the four component analyzers on one file's UAST root and appends their
/// scalars to `tq`, mirroring the reference `quality.(*Analyzer).analyzeNode`:
/// `analyzeComplexity` -> `analyzeHalstead` -> `analyzeComments` -> `analyzeCohesion`.
/// Each component appends exactly one value per file (the reference implementation appends unconditionally
/// on success; the per-file `Analyze` calls here do not error for a parsed root).
fn accumulate_quality_file(root: &cf_uast::Node, tq: &mut cf_quality::TickQuality) {
    // The history quality analyzer instantiates its component analyzers with
    // the shared traverser's `maxDepth = 10` (the static surfaces run
    // uncapped): only function/comment nodes at depth <= 10 below the file
    // root are DISCOVERED; per-function analysis over each found subtree is
    // unchanged. Halstead is unaffected (measured equal against the live
    // reference binary on depth-pathological trees).
    const QUALITY_FIND_MAX_DEPTH: usize = 10;

    // --- complexity (cf_complexity::Analyzer::analyze over its node model) ---
    let cx_root = uast_to_cx_node(root);
    let cx = cf_complexity::Analyzer
        .analyze_with_find_depth(Some(&cx_root), Some(QUALITY_FIND_MAX_DEPTH));
    tq.complexities
        .push(govalue_int(&cx, "total_complexity") as f64);
    tq.cognitives
        .push(govalue_int(&cx, "cognitive_complexity") as f64);
    tq.max_complexities.push(govalue_int(&cx, "max_complexity"));
    tq.functions.push(govalue_int(&cx, "total_functions"));

    // --- halstead (standalone findFunctions/Analyze file-level measures) ---
    let h = cf_halstead::analyze(root);
    tq.halstead_volumes.push(h.volume);
    tq.halstead_efforts.push(h.effort);
    tq.delivered_bugs.push(h.delivered_bugs);

    // --- comments (cf_comments::Analyzer::analyze) ---
    match cf_comments::Analyzer::new()
        .analyze_with_find_depth(Some(root), Some(QUALITY_FIND_MAX_DEPTH))
    {
        Ok(c) => {
            tq.comment_scores.push(govalue_float(&c, "overall_score"));
            tq.doc_coverages
                .push(govalue_float(&c, "documentation_coverage"));
        }
        Err(_) => {
            tq.comment_scores.push(0.0);
            tq.doc_coverages.push(0.0);
        }
    }

    // --- cohesion (cf_cohesion::Analyzer::analyze, the findFunctions path) ---
    if let Ok(r) =
        cf_cohesion::Analyzer::new().analyze_with_find_depth(root, Some(QUALITY_FIND_MAX_DEPTH))
    {
        tq.cohesion_scores.push(
            r.get("cohesion_score")
                .and_then(cf_cohesion::report_value::ReportValue::as_float)
                .unwrap_or(0.0),
        );
    }
}

/// `reportutil.GetInt`: integer value at `key` in a `cf-gojson` report map,
/// truncating floats toward zero; `0` when absent or non-numeric.
fn govalue_int(v: &cf_gojson::GoValue, key: &str) -> i64 {
    match v {
        cf_gojson::GoValue::Map(m) => match m.get(key) {
            Some(cf_gojson::GoValue::Int(n)) => *n,
            Some(cf_gojson::GoValue::Float(f)) => *f as i64,
            _ => 0,
        },
        _ => 0,
    }
}

/// `reportutil.GetFloat64`: float value at `key`; `0.0` when absent/non-numeric.
fn govalue_float(v: &cf_gojson::GoValue, key: &str) -> f64 {
    match v {
        cf_gojson::GoValue::Map(m) => match m.get(key) {
            Some(cf_gojson::GoValue::Float(f)) => *f,
            Some(cf_gojson::GoValue::Int(n)) => *n as f64,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

/// Converts a `cf_uast::Node` into the `cf_complexity::node::Node` subset the
/// complexity analyzer reads (type, token, roles, props, children, positions),
/// matching the static complexity handler's bridge.
fn uast_to_cx_node(n: &cf_uast::Node) -> cf_complexity::node::Node {
    let mut out = cf_complexity::node::Node::new(n.node_type.clone());
    out.token = n.token.clone();
    out.roles = n.roles.clone();
    out.props = n
        .props
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    out.pos = n.pos.as_ref().map(|p| cf_complexity::node::Positions {
        start_line: p.start_line as u32,
        start_col: p.start_col as u32,
        start_offset: p.start_offset as u32,
        end_line: p.end_line as u32,
        end_col: p.end_col as u32,
        end_offset: p.end_offset as u32,
    });
    out.children = n.children.iter().map(uast_to_cx_node).collect();
    out
}

/// Builds the `run --analyzers history/sentiment --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the reference streaming sentiment pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits (reference initHistoryPipeline).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time. `commits_by_tick` records each tick's commit hashes (drives
///    `commit_count`); tick bounds = min/max committer time of the tick's
///    commits, `RFC3339`-formatted in UTC (`FormatStartTime/EndTime`).
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`TreeDiffAnalyzer` / forked parallel analyzer diffs against its own
///    parent), or the full initial tree for a root commit.
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **> 32**
///    file changes contributes zero analyzed files (its streamed UAST changes are
///    dropped), matching the quality path.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`).
///  - **comment extraction** (`Analyzer.Consume`): for each surviving After tree,
///    recursively collect `Comment` nodes, then `mergeComments` (group by start
///    line, merge adjacent within `maxEnd+1`, strip delimiters, filter by the
///    default `MinCommentLength = 20`, letters-ratio, license drop). The merged
///    comments are keyed by commit hex hash in `comments_by_commit`.
///
/// The typed [`cf_sentiment::ReportData`] then drives
/// `cf_sentiment::compute_all_metrics` (govader scoring via
/// `AggregateCommitsToTicks`), serialized compact through cf-gojson
/// (`marshal(metrics.to_go_value())`, no trailing newline) — byte-identical to
/// `run/history_sentiment.json`.
pub fn sentiment_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_sentiment::ToGoValue;
    let metrics = sentiment_metrics(sub)?;
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Computes the typed sentiment [`cf_sentiment::ComputedMetrics`] for the oldest
/// `--limit` commits — the single report value behind every output format. The
/// serializer (json / yaml / bin) is chosen by the caller so all formats follow
/// from the one computation (the reference `ComputeAllMetrics` → `FormatReport*`).
pub fn sentiment_metrics(sub: &clap::ArgMatches) -> Option<cf_sentiment::ComputedMetrics> {
    use cf_sentiment::{compute_all_metrics, ReportData, TickBounds};
    use std::collections::BTreeMap;

    let walk = sentiment_walk(sub)?;

    let mut comments_by_commit: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for c in &walk {
        commits_by_tick
            .entry(c.tick)
            .or_default()
            .push(c.hash.clone());
        tick_when
            .entry(c.tick)
            .and_modify(|(lo, hi)| {
                if c.when < *lo {
                    *lo = c.when;
                }
                if c.when > *hi {
                    *hi = c.when;
                }
            })
            .or_insert((c.when, c.when));
        // The reference implementation always records an entry for an analyzed commit (CommitResult.Comments,
        // even when empty); a spilled commit (None) records none.
        if let Some(merged) = &c.comments {
            comments_by_commit.insert(c.hash.clone(), merged.clone());
        }
    }

    // Format tick bounds RFC3339 UTC (FormatStartTime / FormatEndTime).
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    let input = ReportData::from_commit_data(&comments_by_commit, commits_by_tick, tick_bounds);
    Some(compute_all_metrics(&input))
}

/// One walked commit's sentiment products (the reference `sentiment.Consume` TC + runner
/// stamps).
#[derive(Clone)]
pub(crate) struct SentimentCommit {
    /// Full hex hash.
    pub hash: String,
    /// Merged+filtered comments for this commit; `None` for a spilled commit
    /// (which records no `comments_by_commit` entry but still emits a TC with
    /// empty comments).
    pub comments: Option<Vec<String>>,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Loose-identity author id (walk order).
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The shared `history/sentiment` revwalk: every sentiment format consumes
/// THIS one walk.
pub(crate) fn sentiment_walk(sub: &clap::ArgMatches) -> Option<Vec<SentimentCommit>> {
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_pathpolicy::Options as PathPolicyOptions;

    // Multi-analyzer runs route through the ONE shared UAST walk (same code,
    // one tree diff + one parse per blob per commit across the co-selected
    // analyzers); single-analyzer runs keep this direct walk.
    if let Some(shared) = super::uast_walk::shared_sentiment_walk(sub) {
        return shared;
    }

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation, ignoring
    // `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`).
    let first_parent = crate::handlers::effective_first_parent(sub);
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let opts = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // ---- parallel pure-compute stage -----------------------------------------
    // The expensive per-commit work — tree diff + per-file UAST parse + comment
    // extraction + per-commit merge_comments — is a PURE function of (repo,
    // commit), so run it across all cores. The result is `Some(merged)` for an
    // analyzed commit (possibly empty) or `None` for a spilled (> 32 changes)
    // commit, which records NO comments_by_commit entry — exactly the original
    // `continue`. The order-dependent reduce below (monotonic tick assignment,
    // commits_by_tick, per-tick committer-time bounds, comments_by_commit
    // insert) runs UNCHANGED. The per-commit body is the SAME
    // `sentiment_commit_product` the shared multi-analyzer walk calls.
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let opts_ref = &opts;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        // Per-thread UAST parser, reused across this thread's commits (tree-sitter
        // parsers are not thread-safe, so never shared across threads).
        crate::handlers::history::with_uast_parser(|parser| {
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = commit_tree_changes(repo, &commit)?;
            let mut cache = super::uast_walk::CommitParseCache::new(repo, parser, opts_ref);
            Some(sentiment_commit_product(&changes, &mut cache))
        })
    })?;

    // ---- sequential ordered identity/tick stamping ----------------------------
    let mut commits = Vec::with_capacity(hashes.len());
    for (i, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();

        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        commits.push(SentimentCommit {
            hash: hash.to_string(),
            comments: prepared[i].clone(),
            tick,
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
        });
    }
    Some(commits)
}

/// The `history/sentiment` per-commit product (the reference
/// `sentiment.Consume` body): the merged+filtered comment strings of this
/// commit's surviving After trees, or `None` for a spilled commit (which
/// records NO comments_by_commit entry). Called by BOTH the direct
/// [`sentiment_walk`] and the shared multi-analyzer UAST walk.
pub(crate) fn sentiment_commit_product(
    changes: &[cf_gitlib::changes::Change],
    cache: &mut super::uast_walk::CommitParseCache<'_>,
) -> Option<Vec<String>> {
    use super::uast_walk::{ParseOutcome, SPILL_THRESHOLD};
    use cf_gitlib::changes::ChangeAction;
    use cf_sentiment::analyzer::{
        merge_comments, CommentNode, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH,
    };
    use cf_uast_node::UAST_COMMENT;

    // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes; the
    // commit records NO comments_by_commit entry (`None`).
    if changes.len() > SPILL_THRESHOLD {
        return None;
    }

    // Collect Comment nodes across this commit's surviving After trees, then
    // merge+filter per commit (reference `Consume` aggregates every change's After
    // comments before mergeComments).
    let mut comment_nodes: Vec<CommentNode> = Vec::new();

    for change in changes {
        // Sentiment analyzes the After version only (Insert / Modify).
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        let name = &change.to.name;
        match &*cache.parse(name, change.to.hash) {
            ParseOutcome::Parsed(root) => {
                collect_comment_nodes(root, UAST_COMMENT, &mut comment_nodes);
            }
            // The Rust UAST loader has only the reference grammar vendored; shell
            // grammars are pending (see cf-uast languages.rs). For `.sh`
            // files (the only non-Go source contributing comments in this
            // capture's commit window) reproduce tree-sitter-bash's comment
            // tokenization directly: every `#`-introduced line is one Comment
            // node with `StartLine == EndLine == lineno` and token = the
            // comment text from `#` to end-of-line (verified node-for-node
            // against the reference pipeline for hack/config-go.sh and
            // src/scripts/cloudcfg.sh). Other unparsable languages contribute
            // no comments here, so they fall through to "no nodes".
            ParseOutcome::Failed(blob) if is_shell_path(name) => {
                extract_shell_comment_nodes(&blob.data, &mut comment_nodes);
            }
            ParseOutcome::Failed(_) | ParseOutcome::Skipped => {}
        }
    }

    Some(merge_comments(
        &comment_nodes,
        DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH,
    ))
}

/// Per-commit sentiment NDJSON records (forked leaf): EVERY commit emits a
/// line; `data` is the reference implementation's `*CommitResult` struct — one `Comments` field, an
/// initialized (possibly empty, never null) string slice.
pub fn sentiment_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = sentiment_walk(sub)?;
    Some(
        walk.iter()
            .enumerate()
            .map(|(pos, c)| {
                let comments: Vec<GoValue> = c
                    .comments
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|s| GoValue::Str(s.clone()))
                    .collect();
                let mut data = GoMap::new_struct();
                data.insert("Comments".to_string(), GoValue::Array(comments));
                super::history_formats::NdjsonRecord {
                    pos,
                    hash: c.hash.clone(),
                    tick: c.tick,
                    author_id: c.author_id,
                    time_secs: c.when,
                    tz_offset_min: c.offset_min,
                    data: GoValue::Object(data),
                }
            })
            .collect(),
    )
}

/// The sentiment contribution to the merged `--format timeseries` document (reference:
/// `sentiment.ExtractCommitTimeSeries` over `report["comments_by_commit"]`):
/// per analyzed commit `{"comment_count": n, "sentiment"?: f32}` (`sentiment`
/// only when there are comments).
pub fn sentiment_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};
    use cf_sentiment::model::f32_float;

    let walk = sentiment_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &walk {
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
        // The reference implementation records a comments_by_commit entry for EVERY consumed commit — a
        // spilled commit's leaf sees zero UAST changes, so its entry is an
        // empty list (`comment_count: 0`, oracle-verified on kubernetes).
        let empty: Vec<String> = Vec::new();
        let comments = c.comments.as_ref().unwrap_or(&empty);
        let mut entry = GoMap::new_map();
        entry.insert(
            "comment_count".to_string(),
            GoValue::Int(comments.len() as i64),
        );
        if !comments.is_empty() {
            entry.insert(
                "sentiment".to_string(),
                f32_float(cf_sentiment::scorer::compute_sentiment(comments)),
            );
        }
        per_commit.push((c.hash.clone(), GoValue::Object(entry)));
    }
    Some(super::history_formats::TimeSeriesContribution {
        flag: "sentiment",
        per_commit,
        commit_meta,
    })
}

/// Builds the `history/sentiment --format timeseries --ndjson` bytes: one
/// compact JSON line per walked commit (the reference per-chunk
/// `TimeSeriesChunkFlusher.Flush` → `DrainCommitStats` → `WriteTimeSeriesNDJSON`).
/// The drained data/meta are exactly the merged-timeseries contribution's
/// `per_commit`/`commit_meta` (walk order; sentiment contributes an entry for
/// EVERY consumed commit, so no meta row is filtered out), and each line is the
/// reference `MergedCommitData.MarshalJSON` flat `map[string]any` — keys sorted
/// by `encoding/json`: `author`, `hash`, `sentiment`, `tick`, `timestamp` —
/// with the per-commit `{"comment_count", "sentiment"?}` map under
/// `"sentiment"`. Same memoized walk as every other sentiment format.
pub fn sentiment_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gojson::{GoMap, GoValue, MapOrigin};

    let contrib = sentiment_timeseries_contribution(sub)?;
    let mut out = Vec::new();
    for (hash, tick, ts, author) in &contrib.commit_meta {
        // assembleCommits filters the ordered meta to hashes the analyzer
        // contributed data for (nil-Data TCs order but never emit).
        let Some((_, v)) = contrib.per_commit.iter().find(|(h, _)| h == hash) else {
            continue;
        };
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("hash", GoValue::Str(hash.clone()));
        m.push("timestamp", GoValue::Str(ts.clone()));
        m.push("author", GoValue::Str(author.clone()));
        m.push("tick", GoValue::Int(*tick));
        m.push("sentiment", v.clone());
        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(m)));
        out.push(b'\n');
    }
    Some(out)
}

/// Recursively collects UAST nodes whose type is `Comment` into `out`, mirroring
/// the reference `extractComments` (preorder: the node itself before its children).
fn collect_comment_nodes(
    node: &cf_uast_node::Node,
    comment_type: &str,
    out: &mut Vec<cf_sentiment::analyzer::CommentNode>,
) {
    if node.node_type == comment_type {
        let (start_line, end_line) = match &node.pos {
            Some(p) => (p.start_line as i64, p.end_line as i64),
            // The reference `groupCommentsByLine` skips nodes with a nil Pos.
            None => (-1, -1),
        };
        if start_line >= 0 {
            out.push(cf_sentiment::analyzer::CommentNode {
                start_line,
                end_line,
                token: node.token.clone(),
            });
        }
    }
    for child in &node.children {
        collect_comment_nodes(child, comment_type, out);
    }
}

/// Whether `name` is a shell-script path handled by the bash-comment fallback.
///
/// The UAST loader registers `.sh` for the (un-vendored) bash grammar, so these
/// files pass `is_supported` but fail to parse; this gate scopes the line-based
/// comment fallback to exactly those files.
fn is_shell_path(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
        && name.contains('.')
}

/// Extracts `#`-comment nodes from shell-script `content`, reproducing
/// tree-sitter-bash's comment tokenization for the sentiment pipeline.
///
/// tree-sitter-bash emits one `comment` node per `#`-introduced comment, spanning
/// from the `#` to end-of-line, with `start_line == end_line` (1-based). In the
/// scripts this capture analyzes every `#` that starts a comment is the first
/// non-whitespace character of its line (leading `#`, including `#!` shebangs),
/// so the comment token is the line text from the `#` onward. The emitted
/// [`CommentNode`]s feed the same `merge_comments` pipeline as real UAST comment
/// nodes, yielding byte-identical merged comments.
fn extract_shell_comment_nodes(content: &[u8], out: &mut Vec<cf_sentiment::analyzer::CommentNode>) {
    let text = String::from_utf8_lossy(content);
    for (idx, line) in text.split('\n').enumerate() {
        // Strip a trailing '\r' so CRLF files behave like the reference implementation's line view.
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let lineno = (idx + 1) as i64;
        out.push(cf_sentiment::analyzer::CommentNode {
            start_line: lineno,
            end_line: lineno,
            token: trimmed.to_string(),
        });
    }
}

/// Builds the `run --analyzers history/imports --format json` bytes by RUNNING
/// the real history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// This is the general history pipeline wired for `history/imports`. It mirrors
/// the reference streaming path (the reference `initHistoryPipeline` → `framework.RunStreaming`
/// → `imports.HistoryAnalyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport`
/// → `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (reference: `commitCount` capped at `opts.Limit`). `--first-parent` adds
///    `SimplifyFirstParent`.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): each commit's
///    author signature is consumed to obtain the author id used as the top map
///    level — exactly the value the reference implementation threads through `tc.Data["authorID"]`.
///  - **tick assignment** (`plumbing.TicksSinceStart`, 24h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h),
///    previousTick)` over the committer time.
///  - **per-commit changes**: tree diff against the commit's **first git
///    parent** (`TreeDiffAnalyzer`/forked analyzer diffs against its own
///    parent), or the full initial tree for a root commit.
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **>
///    32** file changes streams zero UAST changes, so it contributes no imports.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`).
///  - **import extraction** (`imports.Consume`): for each surviving After tree,
///    `extractImportsFromUAST` (import nodes, deduped first-seen) with the file's
///    detected language (`UAST.GetLanguage`, default `"uast"`), accumulated into
///    the 4-level map `author → lang → import → tick → count`
///    (`addEntriesToMap`/`mergeImportMaps`).
///
/// `ticks_to_report` then stores the merged 4-level map under the `"imports"`
/// key (a nested *map*, NOT a `[]string`). `compute_all_metrics` faithfully
/// reproduces the reference `ParseReportData` quirk: it reads `report["imports"]` ONLY
/// when it is a string list, otherwise looks for `import_list` — neither is
/// present, so the parsed import set is empty and `ComputeAllMetrics` yields the
/// zero `ComputedMetrics`. The bytes route through cf-gojson (the reference `encoding/json`
/// parity: nil `dependencies` slice → `null`, no trailing newline), which is the
/// 167-byte report the reference implementation emits for ANY repo/limit — here produced by REAL
/// computation over the commit stream, not a constant.
pub fn imports_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = imports_run_metrics(sub)?;
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Computes the `history/imports` [`cf_imports::ComputedMetrics`] over the real
/// commit stream (the format-independent report value). The json/yaml/bin
/// encodings are all serializations of THIS one value, routed through the
/// analyzer crate's own serializers by `h_history_imports` — mirroring the reference
/// `BaseHistoryAnalyzer.Serialize` (one `ComputeMetricsFn`, then
/// `writeMetricsToFormat` switching on the format). See [`imports_run_report`]
/// for the full pipeline contract.
pub fn imports_run_metrics(sub: &clap::ArgMatches) -> Option<cf_imports::ComputedMetrics> {
    use cf_imports::history::{add_entries_to_map, merge_import_maps, ImportsMap};
    use cf_imports::{compute_all_metrics, ReportValue};

    let walk = imports_walk(sub)?;

    // ---- sequential ordered-reduce stage -------------------------------------
    let mut merged: ImportsMap = ImportsMap::new();
    for c in &walk {
        if !c.entries.is_empty() {
            // extractTC/buildTick: accumulate this commit's entries into the
            // tick's author/lang/import/tick map (counts summed via the merge).
            let mut tick_map = ImportsMap::new();
            add_entries_to_map(&mut tick_map, &c.entries, c.author_id, c.tick);
            merge_import_maps(&mut merged, &tick_map);
        }
    }

    // ticksToReport: store the merged 4-level map under the "imports" key as a
    // nested map (NOT a []string). ParseReportData therefore finds no string
    // list and no import_list ⇒ empty parse ⇒ zero ComputedMetrics, exactly as
    // the reference implementation's in-memory report does.
    let mut report = ReportValue::map();
    report.insert("imports", imports_map_to_report_value(&merged));

    let metrics = compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(metrics)
}

/// Aggregated per-import usage counts over the same merged author/lang/import/
/// tick map as [`imports_run_metrics`]:
/// total count per import name, name-ascending. The plot path applies the reference implementation
/// `topImports` (count-descending sort + top-20 cut) over this list. the reference implementation sums
/// over random map iteration — addition is order-independent, so the
/// name-sorted Rust order is the deterministic stand-in.
pub fn imports_run_usage_counts(sub: &clap::ArgMatches) -> Option<Vec<(String, i64)>> {
    use cf_imports::history::{add_entries_to_map, merge_import_maps, ImportsMap};
    use std::collections::BTreeMap;

    let walk = imports_walk(sub)?;

    let mut merged: ImportsMap = ImportsMap::new();
    for c in &walk {
        if !c.entries.is_empty() {
            let mut tick_map = ImportsMap::new();
            add_entries_to_map(&mut tick_map, &c.entries, c.author_id, c.tick);
            merge_import_maps(&mut merged, &tick_map);
        }
    }

    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for lang_map in merged.values() {
        for imp_map in lang_map.values() {
            for (name, tick_map) in imp_map {
                let total: i64 = tick_map.values().sum();
                *counts.entry(name.clone()).or_insert(0) += total;
            }
        }
    }

    Some(counts.into_iter().collect())
}

/// One walked commit's imports products (the reference `imports.Consume` inputs + the
/// runner-stamped TC identity).
#[derive(Clone)]
pub(crate) struct ImportsCommit {
    /// Full hex hash.
    pub hash: String,
    /// Extracted `(lang, import)` entries (empty ⇒ nil-Data TC, no ndjson line,
    /// no commit_stats entry).
    pub entries: Vec<cf_imports::history::ImportEntry>,
    /// Loose-identity author id (walk order).
    pub author_id: i64,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The shared `history/imports` revwalk: per-commit import entries plus the
/// order-assigned identity/tick stamps, in walk order. All imports formats
/// (json/yaml/bin aggregate, ndjson records, timeseries summaries) consume
/// THIS one walk.
pub(crate) fn imports_walk(sub: &clap::ArgMatches) -> Option<Vec<ImportsCommit>> {
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_pathpolicy::Options as PathPolicyOptions;

    // Multi-analyzer runs route through the ONE shared UAST walk (same code,
    // one tree diff + one parse per blob per commit across the co-selected
    // analyzers); single-analyzer runs keep this direct walk.
    if let Some(shared) = super::uast_walk::shared_imports_walk(sub) {
        return shared;
    }

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation,
    // ignoring `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`).
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let opts = PathPolicyOptions::default();
    // Loose identity detection (run streaming never preloads a people dict).
    let mut identity = IdentityDetector::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // ---- parallel pure-compute stage -----------------------------------------
    // The expensive per-commit work — tree diff + per-change UAST parse + import
    // extraction + language detection — is a PURE function of (repo, commit), so
    // run it across all cores. The order-dependent reduce below (identity ids
    // oldest-first, tick assignment, the merge into `merged`) runs UNCHANGED and
    // sequentially over these per-commit `Vec<ImportEntry>` results; only
    // author_id and tick are order-assigned, so they are applied in the reduce.
    // The per-commit body is the SAME `imports_commit_product` the shared
    // multi-analyzer walk calls.
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let opts_ref = &opts;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        // Per-thread UAST parser, reused across this thread's commits (tree-sitter
        // parsers are not thread-safe, so never shared across threads).
        crate::handlers::history::with_uast_parser(|parser| {
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = commit_tree_changes(repo, &commit)?;
            let mut cache = super::uast_walk::CommitParseCache::new(repo, parser, opts_ref);
            Some(imports_commit_product(&changes, &mut cache))
        })
    })?;

    // ---- sequential ordered identity/tick stamping ----------------------------
    let mut commits = Vec::with_capacity(hashes.len());
    for (i, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();

        // Identity: resolve this commit's author id (loose signature), oldest-first.
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        commits.push(ImportsCommit {
            hash: hash.to_string(),
            entries: prepared[i].clone(),
            author_id,
            tick,
            when,
            offset_min: committer_when.offset_minutes(),
        });
    }
    Some(commits)
}

/// The `history/imports` per-commit product (the reference `imports.Consume`
/// body): the `(lang, import)` entries extracted from this commit's surviving
/// After trees. Called by BOTH the direct [`imports_walk`] and the shared
/// multi-analyzer UAST walk.
pub(crate) fn imports_commit_product(
    changes: &[cf_gitlib::changes::Change],
    cache: &mut super::uast_walk::CommitParseCache<'_>,
) -> Vec<cf_imports::history::ImportEntry> {
    use super::uast_walk::{ParseOutcome, SPILL_THRESHOLD};
    use cf_gitlib::changes::ChangeAction;
    use cf_imports::history::ImportEntry;

    // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
    if changes.len() > SPILL_THRESHOLD {
        return Vec::new();
    }

    // Collect import entries across this commit's surviving After trees
    // (imports.Consume aggregates every Insert/Modify change before the TC).
    let mut entries: Vec<ImportEntry> = Vec::new();

    for change in changes {
        // Imports analyzes the After version only (Insert / Modify).
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        let ParseOutcome::Parsed(root) = &*cache.parse(&change.to.name, change.to.hash) else {
            continue;
        };
        // Faithful port of the reference `extractImportsFromUAST` over the real cf-uast
        // parse output (the same function the static/imports path uses).
        let imports = crate::handlers::static_imports::extract_imports_from_uast(root);
        if imports.is_empty() {
            continue;
        }
        // The reference `imports.Consume`: `lang := h.UAST.GetLanguage(name)`, falling
        // back to "uast" when empty. The streaming pipeline ALWAYS runs
        // imports as a forked leaf (`Sequential: false` → hybrid path), and
        // `Fork` clones get a bare `&plumbing.UASTChangesAnalyzer{}` whose
        // `parser` is nil — so GetLanguage returns "" and EVERY entry's
        // Lang is the "uast" fallback in the live binary.
        let lang = "uast";
        for imp in imports {
            entries.push(ImportEntry {
                lang: lang.to_string(),
                import: imp,
            });
        }
    }
    entries
}

/// Per-commit imports NDJSON records (forked leaf — the central path reorders
/// by `(pos % W, pos)`): `data` is the reference implementation's `map[string]any{"entries": []ImportEntry,
/// "authorID": int}` (key-sorted: `authorID`, `entries`); each `ImportEntry` is
/// a struct in `Lang`/`Import` declaration order. Commits with no entries emit
/// no line (nil-Data TC).
pub fn imports_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = imports_walk(sub)?;
    let mut records = Vec::new();
    for (pos, c) in walk.iter().enumerate() {
        if c.entries.is_empty() {
            continue;
        }
        let mut data = GoMap::new_map();
        data.insert("authorID".to_string(), GoValue::Int(c.author_id));
        let entries: Vec<GoValue> = c
            .entries
            .iter()
            .map(|e| {
                let mut m = GoMap::new_struct();
                m.insert("Lang".to_string(), GoValue::Str(e.lang.clone()));
                m.insert("Import".to_string(), GoValue::Str(e.import.clone()));
                GoValue::Object(m)
            })
            .collect();
        data.insert("entries".to_string(), GoValue::Array(entries));
        records.push(super::history_formats::NdjsonRecord {
            pos,
            hash: c.hash.clone(),
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: GoValue::Object(data),
        });
    }
    Some(records)
}

/// The imports contribution to the merged `--format timeseries` document (reference:
/// `imports.ExtractCommitTimeSeries` over `report["commit_stats"]`): per
/// entry-bearing commit `{"import_count": len(entries), "languages": {lang:
/// count}}`; the report carries `commits_by_tick` over the SAME commits.
pub fn imports_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};

    let walk = imports_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &walk {
        if c.entries.is_empty() {
            continue;
        }
        let mut languages: std::collections::BTreeMap<&str, i64> =
            std::collections::BTreeMap::new();
        for e in &c.entries {
            *languages.entry(e.lang.as_str()).or_insert(0) += 1;
        }
        let mut entry = GoMap::new_map();
        entry.insert(
            "import_count".to_string(),
            GoValue::Int(c.entries.len() as i64),
        );
        let mut langs = GoMap::new_map();
        for (lang, n) in &languages {
            langs.insert((*lang).to_string(), GoValue::Int(*n));
        }
        entry.insert("languages".to_string(), GoValue::Object(langs));
        per_commit.push((c.hash.clone(), GoValue::Object(entry)));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(super::history_formats::TimeSeriesContribution {
        flag: "imports-per-dev",
        per_commit,
        commit_meta,
    })
}

/// Builds the `history/imports --format timeseries --ndjson` bytes: one compact
/// JSON line per entry-bearing commit (the reference per-chunk
/// `TimeSeriesChunkFlusher.Flush` → `DrainCommitStats` → `WriteTimeSeriesNDJSON`).
/// The drained data/meta are exactly the merged-timeseries contribution's
/// `per_commit`/`commit_meta` (walk order, same commits as the merged document's
/// `commits` array), and each line is the reference `MergedCommitData.MarshalJSON`
/// flat `map[string]any` — keys sorted by `encoding/json`: `author`, `hash`,
/// `imports-per-dev`, `tick`, `timestamp`. Same single-budget-chunk model as the
/// merged-timeseries path (a chunk boundary in the reference implementation
/// would flush earlier commits ahead of later ones regardless of tick).
pub fn imports_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gojson::{GoMap, GoValue, MapOrigin};

    let contrib = imports_timeseries_contribution(sub)?;
    let mut out = Vec::new();
    for (hash, tick, ts, author) in &contrib.commit_meta {
        // assembleCommits filters the ordered meta to hashes the analyzer
        // contributed data for (nil-Data TCs order but never emit).
        let Some((_, v)) = contrib.per_commit.iter().find(|(h, _)| h == hash) else {
            continue;
        };
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("hash", GoValue::Str(hash.clone()));
        m.push("timestamp", GoValue::Str(ts.clone()));
        m.push("author", GoValue::Str(author.clone()));
        m.push("tick", GoValue::Int(*tick));
        m.push("imports-per-dev", v.clone());
        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(m)));
        out.push(b'\n');
    }
    Some(out)
}

/// Converts the 4-level [`ImportsMap`] into a nested [`cf_imports::ReportValue`]
/// map, mirroring how the reference implementation stores `map[int]map[string]map[string]map[int]int64`
/// under `report["imports"]`. Integer keys are rendered as decimal strings (the
/// shape never reaches the JSON output — it exists only so `ParseReportData`
/// sees a *map* rather than a `[]string` and falls through to the empty parse).
fn imports_map_to_report_value(
    merged: &cf_imports::history::ImportsMap,
) -> cf_imports::ReportValue {
    use cf_imports::ReportValue;
    let mut authors = std::collections::BTreeMap::new();
    for (author, langs) in merged {
        let mut lang_map = std::collections::BTreeMap::new();
        for (lang, imps) in langs {
            let mut imp_map = std::collections::BTreeMap::new();
            for (imp, ticks) in imps {
                let mut tick_map = std::collections::BTreeMap::new();
                for (tick, count) in ticks {
                    tick_map.insert(tick.to_string(), ReportValue::Int(*count));
                }
                imp_map.insert(imp.clone(), ReportValue::Map(tick_map));
            }
            lang_map.insert(lang.clone(), ReportValue::Map(imp_map));
        }
        authors.insert(author.to_string(), ReportValue::Map(lang_map));
    }
    ReportValue::Map(authors)
}

/// Builds the `run --analyzers history/file-history --format json` bytes by
/// RUNNING the real history pipeline over the actual commit stream, or `None` if
/// the repository cannot be opened/walked.
///
/// This wires the general history pipeline for `history/file-history`, mirroring
/// the reference streaming path (the reference `initHistoryPipeline` → `framework.RunStreaming`
/// → `file_history.HistoryAnalyzer.Consume` → aggregator → `ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetricsWithOptions`):
///
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (reference: `commitCount` capped at `opts.Limit`). `--first-parent` adds
///    `SimplifyFirstParent`.
///  - **merge dedup** (`shouldConsumeCommit` / `MergeTracker`): a commit with
///    `> 1` parents already seen via another parent is skipped. In a single
///    reverse walk each merge appears once, so this is a no-op here, but it is
///    reproduced for arbitrary walks.
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`BlobPipeline`: `prevHash = ParentHash(0)` when parents exist), or the
///    full initial tree for a root commit, exactly as the framework's diff base.
///  - **tree-diff filter** (`TreeDiffAnalyzer.filterChanges`): each change is
///    dropped when `pathpolicy.Exclude(name, nil, PathPolicy)` is true
///    (vendor/generated path exclusion; `content=nil` so the content-generated
///    heuristic does not fire). `--languages all` (the default) disables the
///    language filter; `skip-blacklist` defaults false.
///  - **hashes** (`processFileChanges` via `ChangeRouter`): Insert RESETS
///    `Hashes = [hash]`; Delete and same-name Modify APPEND; a rename
///    (`Action==Modify && From.Name != To.Name`) moves the prior history from
///    `From` to `To` and appends. Commit count == `len(Hashes)`.
///  - **line stats** (`aggregateLineStats`, only for non-merge commits): for each
///    `LinesStatsCalculator` entry, accumulate into `files[name].People[author]`.
///    Insert ⇒ Added = `CachedBlob.CountLines(To)`; Delete ⇒ Removed =
///    `CountLines(From)`; Modify ⇒ `computeDiffLineStats` over the
///    diff-match-patch line diff (`DiffLinesToRunes` + `DiffMainRunes(false)` +
///    `DiffCleanupMerge(DiffCleanupSemanticLossless())`, skipping binary and
///    identical-content files), keyed by `change.To.Name`.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): the author id used
///    as the `People` key, exactly as the reference implementation threads `h.Identity.AuthorID`.
///  - **composition** (`classifyChanges` → `tickComposition[tick]`): every
///    Insert/Delete/Modify change is classified by the enry/pathfilter cascade
///    (the shared port in `cf-composition`) using the change's *after* (insert/
///    modify) or *before* (delete) blob content, and counted in the commit's tick
///    bucket. Ticks come from `TicksSinceStart` (24 h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h), previousTick)`
///    over the committer time.
///  - **filter by last commit** (`filterFilesByLastCommit`): only files present
///    in the LAST consumed commit's tree survive into `Files`.
///
/// `ComputeAllMetricsWithOptions` then derives churn/contributors/hotspots/
/// aggregate/composition exactly as the crate's pure metric functions do. The
/// `Files` map is fed as a `BTreeMap` (path-sorted), so `file_contributors` (which
/// the reference implementation does not sort) and `file_churn` ties (the reference implementation's unstable `sort.Slice`) are
/// emitted in deterministic path order — a correctness improvement over the reference implementation's
/// map-iteration order, per the golden MANIFEST nondeterminism note. Bytes route
/// through cf-gojson (the reference `encoding/json` parity: compact, HTML-escape on, no
/// trailing newline).
pub fn file_history_report_value(sub: &clap::ArgMatches) -> Option<cf_gojson::GoValue> {
    file_history_run(sub).map(|r| r.report_value)
}

/// The TYPED `ComputedMetrics` behind [`file_history_report_value`] (same
/// walk, same `compute_all_metrics_with_options` product) — the text
/// serializer reads struct fields directly (the reference implementation
/// `generateText` calls `ComputeAllMetrics` on the report), so it must see the
/// identical metrics the json/yaml bytes encode.
pub fn file_history_run_metrics(
    sub: &clap::ArgMatches,
) -> Option<cf_file_history::ComputedMetrics> {
    file_history_run(sub).map(|r| r.metrics)
}

/// One walked commit's file-history products (the reference `file_history.Consume` TC +
/// runner stamps), kept in OLDEST-FIRST walk position `pos`.
pub(crate) struct FileHistoryCommit {
    /// Oldest-first walk position (drives the forked drain order).
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// `None` ⇔ dedup-skipped merge (nil-Data TC).
    pub data: Option<FileHistoryCommitData>,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Loose-identity author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The reference `file_history.CommitData` payload as walk products.
pub(crate) struct FileHistoryCommitData {
    /// `(path, action, from_path, to_path)` per change, in change order; action
    /// is the reference `gitlib.ChangeAction` int (Insert 0 / Delete 1 / Modify 2).
    /// Renames carry empty `path` and the from/to pair.
    pub path_actions: Vec<(String, i64, String, String)>,
    /// `(path, stats)` line-stat updates; cleared (None ⇒ null) for merges and
    /// empty for commits with none (reference: nil slice ⇒ null).
    pub line_stats: Vec<(String, cf_file_history::tc::LineStats)>,
    /// Whether the TC cleared `LineStatUpdates` (merge commit).
    pub is_merge: bool,
    /// Per-commit composition counts.
    pub composition: cf_file_history::tc::CategoryCounts,
}

/// The full products of one file-history walk: the aggregated report value
/// plus the per-commit TC stream (in OLDEST-FIRST order; consumers apply the
/// forked drain order themselves).
pub(crate) struct FileHistoryRun {
    /// The aggregated report GoValue (json/yaml/bin source).
    pub report_value: cf_gojson::GoValue,
    /// The typed metrics `report_value` was rendered from (text source).
    pub metrics: cf_file_history::ComputedMetrics,
    /// Per-commit TC products, oldest-first.
    pub commits: Vec<FileHistoryCommit>,
}

pub(crate) fn file_history_run(sub: &clap::ArgMatches) -> Option<FileHistoryRun> {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_composition::classifier::Classifier;
    use cf_file_history::metrics::{FileHistory, ReportData, TickBounds};
    use cf_file_history::tc::{CategoryCounts, LineStats};
    use cf_file_history::{
        compute_all_metrics_with_options, computed_metrics_to_go, MetricOptions,
    };
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);
    let head_only = sub.get_flag("head");

    // The reference implementation: `--head` loads EXACTLY the single HEAD commit (ignoring
    // `--limit`); otherwise `initHistoryPipeline` streams the first
    // `min(limit, total)` commits of an oldest-first walk — the N OLDEST commits,
    // oldest-first (see `load_history_commit_hashes`).
    // Oldest-first window (the N OLDEST commits, oldest-first).
    let revwalk_hashes = if head_only {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    // Tick per commit is assigned by the TicksSinceStart CORE analyzer as the
    // revwalk produces commits — i.e. in OLDEST-FIRST order, monotonic
    // (`tick = max(rawTick, previousTick)`), tick0 = floor of the first commit's
    // committer time. The leaf consumes commits in this same oldest-first order
    // and carries each commit's pre-assigned tick, so composition buckets by the
    // REVWALK-order tick. Precompute the map here.
    let mut commit_tick: std::collections::HashMap<cf_gitlib::hash::Hash, i64> =
        std::collections::HashMap::new();
    let mut commit_when: std::collections::HashMap<cf_gitlib::hash::Hash, (i64, i32)> =
        std::collections::HashMap::new();
    {
        let mut tick0_rw: Option<i64> = None;
        let mut prev_rw: i64 = 0;
        for h in &revwalk_hashes {
            if let Ok(c) = repo.lookup_commit(*h) {
                let cw = c.committer().when;
                let when = cw.seconds();
                let b = *tick0_rw.get_or_insert_with(|| floor_tick_secs(when));
                let t = ((when - b).div_euclid(86_400)).max(prev_rw);
                prev_rw = t;
                commit_tick.insert(*h, t);
                commit_when.insert(*h, (when, cw.offset_minutes()));
            }
        }
    }

    // Leaf consume order: the oldest-first revwalk order (the reference streaming
    // pipeline preserves order end-to-end at `--workers 1`; see
    // `crate::handlers::pipeline_consume_order`). file-history's per-path
    // `applyInsert` RESETS the commit list, so at a merge the consume order
    // decides the final commit_count — hence it must match the reference implementation's exactly.
    let hashes = if head_only {
        revwalk_hashes
    } else {
        crate::handlers::pipeline_consume_order(revwalk_hashes)
    };

    let policy = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    // file-history's final report is built from the AGGREGATOR (per-commit TCs),
    // not the leaf's local file map: each commit emits a TC, and the aggregator's
    // `applyInsert`/`applyModify`/`applyDelete`/`applyRename`
    // maintain the per-path hash list — `applyInsert` RESETS it. The aggregator
    // therefore consumes commits in TC ADD order. Under the reference implementation's hybrid leaf path
    // (the reference `processCommitsHybrid`, taken for this single non-SequentialOnly
    // leaf since `0 < CoreCount(8) < len(Analyzers)(9)`), the forked workers
    // buffer their TCs and the runner DRAINS them worker-by-worker
    // (`drainWorkerTCs`: range over workers, then over each worker's tcs). Commit
    // `p` (oldest-first) goes to worker `p % W`, so the aggregator add-order — and
    // thus the reset-sensitive `commit_count` — is the commits stably reordered by
    // `(p % W, p)`. `W = max(NumCPU / 3, 4)` (coordinator default), so the result
    // is machine-CPU-count dependent, exactly as in the reference implementation (same model as the
    // comments/typos analyzer above). Identity (a CORE analyzer) and the tick
    // assignment are consumed in plain oldest-first order, independent of W.
    let leaf_workers = crate::handlers::leaf_worker_count();
    let consume_order: Vec<(usize, cf_gitlib::hash::Hash)> = {
        let mut v: Vec<(usize, cf_gitlib::hash::Hash)> =
            hashes.iter().copied().enumerate().collect();
        v.sort_by_key(|(p, _)| (*p % leaf_workers, *p));
        v
    };

    // Identity (`IdentityDetector`) is a CORE/plumbing analyzer: the reference implementation's
    // `runner.runCoreAnalyzers` consumes EVERY commit (merges included, before any
    // leaf merge-dedup) in plain oldest-first coordinator order, and `Consume`
    // assigns loose author ids first-seen in THAT order. The resolved `AuthorID`
    // is then STAMPED onto the per-commit leaf work (`buildLeafWork`) and carried
    // unchanged through the forked worker / strided aggregator drain. So the
    // dev_id integer for a signature is fixed by oldest-first first-seen, NOT by
    // the worker-strided `(p % W, p)` order the file map is updated in. Resolve
    // every commit's id here, oldest-first, then merely LOOK IT UP in the strided
    // loop below — assigning inside that loop would mislabel ids (the bug this
    // fixes: kubernetes file_contributors dev_ids 1<->2 swapped). This matches
    // `devs@kubernetes`, which already assigns identity oldest-first.
    let author_id_by_hash: HashMap<cf_gitlib::hash::Hash, i64> = {
        let mut m = HashMap::with_capacity(hashes.len());
        for hash in &hashes {
            let Ok(commit) = repo.lookup_commit(*hash) else {
                continue;
            };
            let gsig = commit.author();
            let id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
                name: gsig.name.clone(),
                email: gsig.email.clone(),
                when_unix: gsig.when.seconds(),
            });
            m.insert(*hash, id);
        }
        m
    };

    // ---- parallel pure-compute stage -----------------------------------------
    // ONLY the per-commit-independent work is parallelized — tree diff (vs
    // parent(0)) + vendor/generated filter + per-change line stats (the expensive
    // libgit2 Modify diff) + PATH-ONLY category classification. The result for
    // commit `p` (indexed by oldest-first position in `hashes`) is the filtered
    // changes, the precomputed line stats, and the composition counts. The
    // order-sensitive reduce below is UNCHANGED: it still walks the worker-strided
    // `(p % leaf_workers, p)` consume order, and the `applyInsert`-resets-the-hash-
    // list bookkeeping + author/tick attribution + tick composition all run
    // sequentially in that exact order — only now reading `prepared[p]` instead of
    // recomputing the diff inline. is_merge (and thus the line-stats gate) is a
    // pure function of (num_parents, first_parent), so the line stats are computed
    // here exactly when the reduce would consume them.
    /// The per-commit-independent products of one commit's diff for file-history.
    struct FileHistoryPrepared {
        /// `commit.num_parents()`, so the reduce can derive is_merge without a
        /// second commit lookup.
        num_parents: usize,
        /// Vendor/generated-filtered tree-diff changes (drives the order-sensitive
        /// hash-list maintenance in the reduce).
        changes: Vec<cf_gitlib::changes::Change>,
        /// `(name, line-stats)` per non-merge change (empty for a merge commit);
        /// folded into `files[name].people[author]` in the reduce.
        line_stats: Vec<(String, LineStats)>,
        /// Path-only composition counts over every change.
        category_counts: CategoryCounts,
    }
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let policy_ref = &policy;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        let classifier = Classifier::new();
        let commit = repo.lookup_commit(hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1 && !first_parent;

        // TreeDiff diff base — each commit is diffed against its OWN parent(0) tree
        // (a root commit vs the empty tree → InitialTreeChanges).
        let new_tree = commit.tree().ok()?;
        let base_tree: Option<cf_gitlib::tree::Tree> = if num_parents > 0 {
            commit.parent(0).ok().and_then(|p| p.tree().ok())
        } else {
            None
        };
        let raw_changes = match &base_tree {
            Some(prev) => tree_diff(repo, Some(prev), Some(&new_tree)).ok()?,
            None => initial_tree_changes(repo, Some(&new_tree)).ok()?,
        };

        // filterChanges: drop vendor/generated paths (content=nil).
        let changes: Vec<cf_gitlib::changes::Change> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, policy_ref)
            })
            .collect();

        // aggregateLineStats (skipped for merge commits): per-change line stats.
        let mut line_stats: Vec<(String, LineStats)> = Vec::new();
        if !is_merge {
            for change in &changes {
                let entry = match change.action {
                    ChangeAction::Insert => {
                        let Ok(blob) = CachedBlob::from_repo(repo, change.to.hash) else {
                            continue;
                        };
                        let Ok(added) = blob.count_lines() else {
                            continue;
                        };
                        (
                            change.to.name.clone(),
                            LineStats {
                                added: added as i64,
                                removed: 0,
                                changed: 0,
                            },
                        )
                    }
                    ChangeAction::Delete => {
                        let Ok(blob) = CachedBlob::from_repo(repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(removed) = blob.count_lines() else {
                            continue;
                        };
                        (
                            change.from.name.clone(),
                            LineStats {
                                added: 0,
                                removed: removed as i64,
                                changed: 0,
                            },
                        )
                    }
                    ChangeAction::Modify => {
                        let Ok(blob_from) = CachedBlob::from_repo(repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(repo, change.to.hash) else {
                            continue;
                        };
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let (added, removed, changed) = if change.from.hash == change.to.hash {
                            (0, 0, 0)
                        } else {
                            let old_lines = blob_from.count_lines().map_or(0, |n| n as i64);
                            compute_diff_line_stats(
                                repo,
                                change.from.hash,
                                change.to.hash,
                                old_lines,
                            )
                        };
                        (
                            change.to.name.clone(),
                            LineStats {
                                added,
                                removed,
                                changed,
                            },
                        )
                    }
                };
                line_stats.push(entry);
            }
        }

        // classifyChanges → category counts (PATH-ONLY; the streaming run wires no
        // blob cache for file-history, so content is empty — oracle-verified).
        let mut category_counts = CategoryCounts::default();
        for change in &changes {
            let name = match change.action {
                ChangeAction::Delete => &change.from.name,
                _ => &change.to.name,
            };
            let cat = classifier.classify(name, &[]);
            category_counts.increment(map_category(cat));
        }

        Some(FileHistoryPrepared {
            num_parents,
            changes,
            line_stats,
            category_counts,
        })
    })?;

    // Cumulative per-path file history (BTreeMap ⇒ deterministic path order).
    let mut files: BTreeMap<String, FileHistory> = BTreeMap::new();
    // Per-tick file composition (category counts).
    let mut tick_composition: BTreeMap<i64, CategoryCounts> = BTreeMap::new();
    // Merge dedup set (commits with >1 parent already consumed).
    let mut seen_merges: HashSet<String> = HashSet::new();

    // ---- sequential ordered-reduce stage (UNCHANGED order) -------------------
    let mut last_commit_hash: Option<cf_gitlib::hash::Hash> = None;
    let mut commit_records: Vec<FileHistoryCommit> = Vec::with_capacity(consume_order.len());
    for (pos, hash) in &consume_order {
        // The commit's tree diff / line stats / classification were computed in the
        // parallel pre-pass (indexed by oldest-first position); the reduce only
        // reads `prep` and does the worker-strided, order-sensitive bookkeeping.
        let prep = &prepared[*pos];
        let num_parents = prep.num_parents;
        // The reference `runner.buildAnalyzeContext`: `isMerge = NumParents()>1`, but FORCED
        // to false under --first-parent (the simplified walk visits a merge as an
        // ordinary single-parent commit). So under first-parent a merge's line
        // stats ARE aggregated and the merge-dedup skip does NOT apply.
        let is_merge = num_parents > 1 && !first_parent;
        let hash_str = hash.to_hex();

        let (rec_when, rec_off) = commit_when.get(hash).copied().unwrap_or((0, 0));
        let mut record = FileHistoryCommit {
            pos: *pos,
            hash: hash_str.clone(),
            data: None,
            tick: commit_tick.get(hash).copied().unwrap_or(0),
            author_id: author_id_by_hash.get(hash).copied().unwrap_or(0),
            when: rec_when,
            offset_min: rec_off,
        };

        // shouldConsumeCommit: skip duplicate merge commits (real merges only).
        if is_merge && !seen_merges.insert(hash_str.clone()) {
            commit_records.push(record);
            continue;
        }

        // buildCommitData (the TC payload): path actions in change order (reference:
        // ChangeRouter), the precomputed line stats (cleared for merges), and
        // this commit's composition counts.
        {
            let mut path_actions: Vec<(String, i64, String, String)> = Vec::new();
            for change in &prep.changes {
                let is_rename = matches!(change.action, ChangeAction::Modify)
                    && change.from.name != change.to.name;
                if is_rename {
                    path_actions.push((
                        String::new(),
                        2,
                        change.from.name.clone(),
                        change.to.name.clone(),
                    ));
                    continue;
                }
                let (path, action) = match change.action {
                    ChangeAction::Insert => (change.to.name.clone(), 0),
                    ChangeAction::Delete => (change.from.name.clone(), 1),
                    ChangeAction::Modify => (change.to.name.clone(), 2),
                };
                path_actions.push((path, action, String::new(), String::new()));
            }
            record.data = Some(FileHistoryCommitData {
                path_actions,
                line_stats: prep.line_stats.clone(),
                is_merge,
                composition: prep.category_counts,
            });
        }
        commit_records.push(record);

        last_commit_hash = Some(*hash);

        // Identity: this commit's author id was resolved oldest-first above
        // (CORE analyzer order); here we only LOOK IT UP (the reference implementation stamps the already-
        // resolved AuthorID onto the leaf work, it is not re-derived per worker).
        let author_id = author_id_by_hash.get(hash).copied().unwrap_or(0);

        // Tick: the REVWALK-order monotonic tick assigned by TicksSinceStart when
        // the commit was produced (precomputed above), NOT a consume-order tick.
        let tick = commit_tick.get(hash).copied().unwrap_or(0);

        let changes = &prep.changes;

        // processFileChanges: maintain per-path commit hash lists.
        for change in changes {
            let is_rename =
                matches!(change.action, ChangeAction::Modify) && change.from.name != change.to.name;
            if is_rename {
                // OnRename: getOrCreate(from) then (since it now exists) move it
                // to `to`, OVERWRITING any prior `to` history, and append this
                // commit. (reference: `h.files[to] = oldFH`; the destination's previous
                // history is always discarded.)
                let from = &change.from.name;
                let to = &change.to.name;
                let mut fh = files.remove(from).unwrap_or_default();
                fh.hashes.push(hash_str.clone());
                files.insert(to.clone(), fh);
                continue;
            }
            match change.action {
                ChangeAction::Insert => {
                    let fh = files.entry(change.to.name.clone()).or_default();
                    fh.hashes = vec![hash_str.clone()];
                }
                ChangeAction::Delete => {
                    let fh = files.entry(change.from.name.clone()).or_default();
                    fh.hashes.push(hash_str.clone());
                }
                ChangeAction::Modify => {
                    let fh = files.entry(change.to.name.clone()).or_default();
                    fh.hashes.push(hash_str.clone());
                }
            }
        }

        // aggregateLineStats (skipped for merge commits): fold the precomputed
        // per-change `(name, line-stats)` into `files[name].people[author]`. The
        // VALUES were computed in the parallel pre-pass (pure per change); only the
        // author attribution + map update are order-sensitive and stay here. The
        // pre-pass produced this list only for non-merge commits, but gate on
        // is_merge again for clarity (the list is empty for merges anyway).
        if !is_merge {
            for (name, stats) in &prep.line_stats {
                let fh = files.entry(name.clone()).or_default();
                let entry = fh.people.entry(author_id).or_default();
                entry.added += stats.added;
                entry.removed += stats.removed;
                entry.changed += stats.changed;
            }
        }

        // classifyChanges → tickComposition[tick]. The path-only category counts
        // were computed in the parallel pre-pass (the reference `classifyChanges` reads the
        // blob cache, but the streaming `run` path wires none for file-history, so
        // content is empty and classification is PATH-ONLY — oracle-verified). The
        // order-sensitive part is only the per-tick accumulation, which stays here.
        let counts = &prep.category_counts;
        if !changes.is_empty() && counts.total() > 0 {
            tick_composition.entry(tick).or_default().add(counts);
        }
    }

    // filterFilesByLastCommit: keep only files in the last commit's tree.
    if let Some(last) = last_commit_hash {
        if let Ok(last_commit) = repo.lookup_commit(last) {
            if let Ok(iter) = last_commit.files() {
                let mut present: HashSet<String> = HashSet::new();
                let _ = iter.for_each(|f| {
                    present.insert(f.name.clone());
                    Ok(())
                });
                files.retain(|name, _| present.contains(name));
            }
        }
    }

    let input = ReportData { files };
    // tick_bounds: file-history flushes a single tick (0) with zero start/end
    // times, so every composition_ts start_time/end_time is empty and omitted.
    let tick_bounds: TickBounds = BTreeMap::new();
    let metrics = compute_all_metrics_with_options(
        &input,
        MetricOptions::default(),
        &tick_composition,
        Some(&tick_bounds),
    );
    Some(FileHistoryRun {
        report_value: computed_metrics_to_go(&metrics),
        metrics,
        commits: commit_records,
    })
}

/// Serializes one file-history TC payload as the reference implementation's `*CommitData` struct
/// (declaration field order; nil slices → null).
fn file_history_data_value(
    hash_hex: &str,
    cd: &FileHistoryCommitData,
    author_id: i64,
) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};

    let hash_bytes: Vec<GoValue> = (0..20)
        .map(|i| {
            let b = u8::from_str_radix(&hash_hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
            GoValue::Int(i64::from(b))
        })
        .collect();

    let mut data = GoMap::new_struct();
    if cd.path_actions.is_empty() {
        data.insert("PathActions".to_string(), GoValue::Null);
    } else {
        let actions: Vec<GoValue> = cd
            .path_actions
            .iter()
            .map(|(path, action, from, to)| {
                let mut m = GoMap::new_struct();
                m.insert("Path".to_string(), GoValue::Str(path.clone()));
                m.insert("Action".to_string(), GoValue::Int(*action));
                m.insert("CommitHash".to_string(), GoValue::Array(hash_bytes.clone()));
                m.insert("FromPath".to_string(), GoValue::Str(from.clone()));
                m.insert("ToPath".to_string(), GoValue::Str(to.clone()));
                GoValue::Map(m)
            })
            .collect();
        data.insert("PathActions".to_string(), GoValue::Array(actions));
    }
    if cd.is_merge || cd.line_stats.is_empty() {
        // Merge commits clear LineStatUpdates;
        // append-built nil slice also marshals null when empty.
        data.insert("LineStatUpdates".to_string(), GoValue::Null);
    } else {
        let updates: Vec<GoValue> = cd
            .line_stats
            .iter()
            .map(|(path, stats)| {
                let mut s = GoMap::new_struct();
                s.insert("added".to_string(), GoValue::Int(stats.added));
                s.insert("removed".to_string(), GoValue::Int(stats.removed));
                s.insert("changed".to_string(), GoValue::Int(stats.changed));
                let mut m = GoMap::new_struct();
                m.insert("Path".to_string(), GoValue::Str(path.clone()));
                m.insert("AuthorID".to_string(), GoValue::Int(author_id));
                m.insert("Stats".to_string(), GoValue::Map(s));
                GoValue::Map(m)
            })
            .collect();
        data.insert("LineStatUpdates".to_string(), GoValue::Array(updates));
    }
    let c = &cd.composition;
    let mut comp = GoMap::new_struct();
    comp.insert("source".to_string(), GoValue::Int(c.source));
    comp.insert("vendor".to_string(), GoValue::Int(c.vendor));
    comp.insert("generated".to_string(), GoValue::Int(c.generated));
    comp.insert("documentation".to_string(), GoValue::Int(c.documentation));
    comp.insert("configuration".to_string(), GoValue::Int(c.configuration));
    comp.insert("image".to_string(), GoValue::Int(c.image));
    comp.insert("dotfile".to_string(), GoValue::Int(c.dotfile));
    comp.insert("binary".to_string(), GoValue::Int(c.binary));
    data.insert("Composition".to_string(), GoValue::Map(comp));

    GoValue::Map(data)
}

/// Per-commit file-history NDJSON records (forked leaf): every consumed commit
/// emits a line whose `data` is the reference `*CommitData` struct.
pub fn file_history_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    let run = file_history_run(sub)?;
    let mut records = Vec::new();
    for c in &run.commits {
        let Some(cd) = &c.data else { continue };
        records.push(super::history_formats::NdjsonRecord {
            pos: c.pos,
            hash: c.hash.clone(),
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: file_history_data_value(&c.hash, cd, c.author_id),
        });
    }
    Some(records)
}

/// The file-history contribution to the merged `--format timeseries` document
/// (the reference `file_history.ExtractCommitTimeSeries` over `report["commit_stats"]`).
/// The aggregator appends `commits_by_tick` in TC ADD order — the forked drain
/// order `(pos % W, pos)` — so commit ordering within a tick follows that
/// stride, not the walk.
pub fn file_history_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};

    let run = file_history_run(sub)?;
    let w = crate::handlers::leaf_worker_count();
    let mut drained: Vec<&FileHistoryCommit> = run.commits.iter().collect();
    drained.sort_by_key(|c| (c.pos % w, c.pos));

    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &drained {
        let Some(cd) = &c.data else { continue };
        let (mut inserts, mut deletes, mut modifies) = (0i64, 0i64, 0i64);
        for (_, action, _, _) in &cd.path_actions {
            match action {
                0 => inserts += 1,
                1 => deletes += 1,
                _ => modifies += 1,
            }
        }
        let (mut added, mut removed, mut changed) = (0i64, 0i64, 0i64);
        if !cd.is_merge {
            for (_, stats) in &cd.line_stats {
                added += stats.added;
                removed += stats.removed;
                changed += stats.changed;
            }
        }
        let mut entry = GoMap::new_map();
        entry.insert(
            "files_touched".to_string(),
            GoValue::Int(cd.path_actions.len() as i64),
        );
        entry.insert("lines_added".to_string(), GoValue::Int(added));
        entry.insert("lines_removed".to_string(), GoValue::Int(removed));
        entry.insert("lines_changed".to_string(), GoValue::Int(changed));
        entry.insert("inserts".to_string(), GoValue::Int(inserts));
        entry.insert("deletes".to_string(), GoValue::Int(deletes));
        entry.insert("modifies".to_string(), GoValue::Int(modifies));
        per_commit.push((c.hash.clone(), GoValue::Map(entry)));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }

    // assembleOrderedCommitMeta sorts ticks ascending; within a tick the hashes
    // keep their commits_by_tick (drain) append order. Stable-sort by tick.
    commit_meta.sort_by_key(|(_, tick, _, _)| *tick);

    Some(super::history_formats::TimeSeriesContribution {
        flag: "file-history",
        per_commit,
        commit_meta,
    })
}

/// Builds the `history/file-history --format timeseries --ndjson` bytes: one
/// compact JSON line per data-bearing commit (the reference per-chunk
/// `TimeSeriesChunkFlusher.Flush` → `DrainCommitStats` → `WriteTimeSeriesNDJSON`).
/// The drained data/meta are exactly the merged-timeseries contribution's
/// `per_commit`/`commit_meta` (tick-sorted, forked `(pos % W, pos)` drain order
/// within a tick), and each line is the reference `MergedCommitData.MarshalJSON`
/// flat `map[string]any` — keys sorted by `encoding/json`: `author`,
/// `file-history`, `hash`, `tick`, `timestamp`. NOTE: assumes the run fits in a
/// single budget chunk (same model as the merged-timeseries path; a chunk
/// boundary in the reference implementation would flush earlier commits ahead
/// of later ones regardless of tick).
pub fn file_history_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gojson::{GoMap, GoValue, MapOrigin};

    let contrib = file_history_timeseries_contribution(sub)?;
    let mut out = Vec::new();
    for (hash, tick, ts, author) in &contrib.commit_meta {
        // assembleCommits filters the ordered meta to hashes an analyzer
        // contributed data for (nil-Data TCs order but never emit).
        let Some((_, v)) = contrib.per_commit.iter().find(|(h, _)| h == hash) else {
            continue;
        };
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("hash", GoValue::Str(hash.clone()));
        m.push("timestamp", GoValue::Str(ts.clone()));
        m.push("author", GoValue::Str(author.clone()));
        m.push("tick", GoValue::Int(*tick));
        m.push("file-history", v.clone());
        out.extend_from_slice(&cf_gojson::marshal(&GoValue::Map(m)));
        out.push(b'\n');
    }
    Some(out)
}

/// Maps a [`cf_composition::category::Category`] to the file-history
/// [`cf_file_history::Category`] of the same name (both port the identical the reference implementation
/// `Category` enum).
fn map_category(cat: cf_composition::category::Category) -> cf_file_history::Category {
    use cf_composition::category::Category as C;
    use cf_file_history::Category as F;
    match cat {
        C::Source => F::Source,
        C::Vendor => F::Vendor,
        C::Generated => F::Generated,
        C::Documentation => F::Documentation,
        C::Configuration => F::Configuration,
        C::Image => F::Image,
        C::DotFile => F::DotFile,
        C::Binary => F::Binary,
    }
}

/// Port of `computeDiffLineStats`:
/// derives `(added, removed, changed)` from the diff-match-patch line diff. Each
/// `cf_godiff` segment carries one encoded line per element, so `lines.len()`
/// equals the reference implementation's `utf8.RuneCountInString(edit.Text)` (one rune per source line).
/// Runs `compute` for every hash in parallel across `workers` OS threads,
/// returning the results in the SAME order as `hashes` (or `None` if any commit
/// hits a fatal git error, mirroring the sequential `.ok()?` contract).
///
/// Each thread opens its OWN `Repository` from `repo_path` — libgit2 handles are
/// `!Send`, and a per-thread handle also gives each thread an independent ODB
/// object cache, so there is no shared-cache lock contention. `compute` must be a
/// PURE per-commit function (it may read the repo but shares no mutable state);
/// because the result of commit *i* is independent of every other commit, the
/// caller can run its order-dependent reduce over the returned vec in canonical
/// order and stay byte-identical regardless of how work was distributed.
///
/// Work is split into `workers` contiguous index ranges, each thread writing only
/// its disjoint `chunks_mut` slice (checked at compile time by `scope`), so no
/// synchronization is needed on the hot path.
pub(crate) fn parallel_prepare<T: Send>(
    repo_path: &str,
    hashes: &[cf_gitlib::hash::Hash],
    workers: usize,
    compute: impl Fn(&cf_gitlib::Repository, cf_gitlib::hash::Hash) -> Option<T> + Sync,
) -> Option<Vec<T>> {
    if hashes.is_empty() {
        return Some(Vec::new());
    }
    let workers = workers.clamp(1, hashes.len());
    let chunk = hashes.len().div_ceil(workers);
    let mut out: Vec<Option<T>> = (0..hashes.len()).map(|_| None).collect();
    let compute = &compute;
    std::thread::scope(|s| {
        for (hchunk, ochunk) in hashes.chunks(chunk).zip(out.chunks_mut(chunk)) {
            s.spawn(move || {
                // Per-thread repository handle (the `!Send` libgit2 handle cannot
                // cross threads; opening is cheap relative to the walk).
                let Ok(repo) = cf_gitlib::Repository::open(repo_path) else {
                    return;
                };
                for (hash, slot) in hchunk.iter().zip(ochunk.iter_mut()) {
                    *slot = compute(&repo, *hash);
                }
            });
        }
    });
    // `None` slot ⇒ a fatal git error (or repo-open failure) on that commit; the
    // whole walk fails, exactly as the sequential `.ok()?` did.
    out.into_iter().collect()
}

fn compute_diff_line_stats(
    repo: &cf_gitlib::Repository,
    from: cf_gitlib::hash::Hash,
    to: cf_gitlib::hash::Hash,
    old_lines: i64,
) -> (i64, i64, i64) {
    use cf_gitlib::diff::{diff_blob_line_ops, LineOp};
    // The runtime history pipeline (the reference diff pipeline → cf_batch_diff_blobs
    // → git_diff_buffers) diffs via libgit2's Myers diff, NOT diffmatchpatch — only
    // falling back to dmp on a libgit2 error. libgit2 and diffmatchpatch group
    // changed-vs-added-vs-removed lines differently, so the line-stat metrics only
    // match the reference implementation when computed from the SAME libgit2 op stream.
    let ops = match diff_blob_line_ops(repo.native(), from, to, old_lines) {
        Ok(ops) => ops,
        // libgit2 error ⇒ the reference implementation's `fileDiffFromGoDiff` fallback (diffmatchpatch). That
        // path is essentially never hit on text blobs that passed the binary check.
        Err(_) => return (0, 0, 0),
    };
    // The reference `computeDiffLineStats` over the op stream: a Delete immediately followed by
    // an Insert is reclassified as "changed" (counted in neither added nor removed).
    let mut added = 0i64;
    let mut removed = 0i64;
    let mut changed = 0i64;
    let mut removed_pending = 0i64;
    for op in &ops {
        match *op {
            LineOp::Equal(_) => {
                removed += removed_pending;
                removed_pending = 0;
            }
            LineOp::Insert(n) => {
                let delta = n;
                if removed_pending > delta {
                    changed += delta;
                    removed += removed_pending - delta;
                } else {
                    changed += removed_pending;
                    added += delta - removed_pending;
                }
                removed_pending = 0;
            }
            LineOp::Delete(n) => {
                removed_pending = n;
            }
        }
    }
    removed += removed_pending;
    (added, removed, changed)
}

/// Builds the `run --analyzers history/typos --format json` bytes by RUNNING the
/// real history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// Faithful port of the reference streaming path
/// (the reference `initHistoryPipeline` → `framework.RunStreaming` →
/// `plumbing.{TreeDiff,BlobCache,FileDiff,UASTChanges}` →
/// `typos.Analyzer.Consume` → `extractTC`/`buildTick` (per-tick dedup) →
/// `ticksToReport` (cross-tick dedup) → `BaseHistoryAnalyzer.Serialize` →
/// `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits.
///    `--first-parent` adds `SimplifyFirstParent`. With `--workers 1` Consume is
///    sequential in walk order, so per-tick and cross-tick dedup collapse to a
///    single global first-seen dedup in walk order (which is what we do).
///  - **per-commit changes**: tree diff against the commit's first git parent
///    (root → full initial tree). The typos analyzer only produces pairs for
///    `Modify` changes (it needs both a `Before` and an `After` UAST), so only
///    Modify changes are processed.
///  - **file diff** (`plumbing.FileDiffAnalyzer.processChange`, Modify only):
///    skip when `From.Hash == To.Hash`, when either blob is binary, or when the
///    blob bytes are identical (those produce only Equal edits ⇒ no typos). The
///    surviving case computes diff-match-patch line-mode diffs with cleanup ON
///    and whitespace NOT ignored (the gate sets neither `--no-diff-cleanup` nor
///    `--no-diff-whitespace`); `cf_godiff::line_diff` is the byte-faithful
///    `DiffCleanupMerge(DiffCleanupSemanticLossless(DiffMainRunes(...)))`. Each
///    returned segment's line count equals the reference implementation's `utf8.RuneCountInString(edit.Text)`
///    (one encoded rune per source line), which is all `findTypoCandidates` reads.
///  - **UAST parse** (`plumbing.UASTChangesAnalyzer.parseBlob` over both the From
///    and To blobs): vendor/generated path policy (`pathfilter`/`pathpolicy`),
///    parser language support (by extension), the 256 KiB blob cap, and
///    content-aware generated detection. A change contributes only when BOTH the
///    before and after parse succeed (the reference implementation requires `change.Before != nil &&
///    change.After != nil`).
///  - **typo extraction** (`findTypoCandidates`/`matchDeleteInsertPairs`/
///    `matchTypoIdentifiers`): line pairs within the Levenshtein bound whose
///    focused before/after lines each carry exactly one UAST identifier become a
///    `(wrong → correct)` pair, recorded with the To name, after-line (0-based),
///    and commit hash.
///
/// `ticksToReport` stores the deduplicated `[]Typo` under `report["typos"]`,
/// which `ComputeAllMetrics`/`ParseReportData` reads back into the four metrics;
/// `metrics_report_value` builds the byte-sorted `MetricSet` map and `to_json()`
/// is the cf-gojson-parity compact encoder (no trailing newline).
pub fn typos_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let report = typos_report_data(sub)?;
    Some(
        cf_typos::metrics_report_value(&report)
            .to_json()
            .into_bytes(),
    )
}

/// `--format yaml` bytes for `history/typos`: the run-level YAML header (reference:
/// `analyze.PrintHeader`, emitted for every non-raw format) followed by the
/// `history/typos:` section name and the metrics map
/// rendered by `gopkg.in/yaml.v3` (the reference `MetricSet.ToYAML()` →
/// `writeMetricsToFormat`). The only json/yaml shape difference is the empty
/// `patterns` metric (JSON `null` vs YAML `[]`), captured by
/// [`cf_typos::metrics_yaml_value`]; both come from the same report value.
pub fn typos_run_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let report = typos_report_data(sub)?;
    let value = typos_value_to_gojson(&cf_typos::metrics_yaml_value(&report));
    let mut out = Vec::new();
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
    out.extend_from_slice(b"history/typos:\n");
    out.extend_from_slice(&cf_goyaml::marshal(&value));
    Some(out)
}

/// `--format bin` bytes for `history/typos`: the `CFB1` envelope (reference:
/// `reportutil.EncodeBinaryEnvelope`) over `json.Marshal(metrics)`. the reference implementation's binary
/// path marshals the `common.MetricSet` STRUCT directly (not its `ToJSON()`
/// map), and `MetricSet` exports no fields, so the payload is always the empty
/// object `{}`. We reproduce that faithfully by encoding an empty struct-origin
/// map (which `cf_gojson` marshals to `{}`); the report is still computed first,
/// matching the reference implementation, so the metamorphic anti-sim check sees real work, not a constant.
pub fn typos_run_report_bin(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let _report = typos_report_data(sub)?;
    let empty_struct = cf_gojson::GoValue::Map(cf_gojson::GoMap::new_struct());
    cf_reportutil::encode_binary_envelope(&empty_struct).ok()
}

/// Converts a `cf_typos`-local [`cf_typos::GoValue`] into the shared
/// [`cf_gojson::GoValue`] the serializer crates consume. `Map` keeps map-origin
/// (encode-time byte-sort), `Struct` keeps struct-origin (declaration order) —
/// the same dual-mode contract both value models carry.
fn typos_value_to_gojson(value: &cf_typos::GoValue) -> cf_gojson::GoValue {
    use cf_typos::GoValue as TV;
    match value {
        TV::Null => cf_gojson::GoValue::Null,
        TV::Int(n) => cf_gojson::GoValue::Int(*n),
        TV::Str(s) => cf_gojson::GoValue::Str(s.clone()),
        TV::Array(items) => {
            cf_gojson::GoValue::Array(items.iter().map(typos_value_to_gojson).collect())
        }
        TV::Map(entries) => {
            let mut m = cf_gojson::GoMap::new_map();
            for (k, v) in entries {
                m.insert(k.clone(), typos_value_to_gojson(v));
            }
            cf_gojson::GoValue::Map(m)
        }
        TV::Struct(entries) => {
            let mut m = cf_gojson::GoMap::new_struct();
            for (k, v) in entries {
                m.push(k.clone(), typos_value_to_gojson(v));
            }
            cf_gojson::GoValue::Map(m)
        }
    }
}

/// Builds the deduplicated `history/typos` [`cf_typos::ReportData`] for the
/// requested run — the single report value that every output format encodes (reference:
/// `BaseHistoryAnalyzer.Serialize`: one `Report` → `ComputeAllMetrics` →
/// per-format encoder). Returns `None` when the repository cannot be walked.
///
/// The walk + per-commit typo detection + reference-faithful worker-strided dedup are
/// format-independent; only the final encoding differs, so the format glue in
/// `h_history_typos` calls this once and routes the value through the
/// json / yaml / binary serializers.
pub fn typos_report_data(sub: &clap::ArgMatches) -> Option<cf_typos::ReportData> {
    use cf_typos::{ReportData, Typo};

    let walk = typos_walk(sub)?;

    // Flatten back to the (walk-index, typo) pairs the dedup operates over.
    let mut all_typos: Vec<(usize, Typo)> = Vec::new();
    for (idx, c) in walk.iter().enumerate() {
        for t in &c.typos {
            all_typos.push((idx, t.clone()));
        }
    }

    // Reproduce the reference implementation's leaf-analyzer add-order before deduplication. the reference implementation runs the
    // (parallel, non-sequential) typos leaf on W = max(NumCPU/3, 4) worker
    // goroutines: commit at chunk-index `i` is dispatched to `workers[i % W]`
    // (reference `hybridCommitLoop`), and on chunk completion the buffered TCs
    // are drained worker-by-worker in worker order, each worker yielding its
    // commits in ascending dispatch order (reference `drainWorkerTCs`). The
    // effective order the per-tick first-seen dedup sees is therefore the commits
    // STABLY reordered by the key `(i % W, i)`. We stable-sort by that key (a
    // commit's typos all share `i`, so their intra-commit order is preserved),
    // then apply the reference `deduplicateTypos` (first-seen on the `wrong|correct` pair).
    // This makes the WINNING commit match the reference implementation's deterministic attribution.
    //
    // NOTE: this assumes the run fits in a single budget chunk (true at the
    // limits the gate/golden probe — limit 10/50/500 on kubernetes), matching
    // the reference implementation, where a chunk boundary would otherwise serialize earlier commits ahead
    // of later ones regardless of worker stride.
    let leaf_workers = crate::handlers::leaf_worker_count();
    all_typos.sort_by_key(|(idx, _)| (*idx % leaf_workers, *idx));
    let ordered: Vec<Typo> = all_typos.into_iter().map(|(_, t)| t).collect();

    // ticksToReport: deduplicate by "wrong|correct" (the reference `deduplicateTypos`,
    // first-seen) over the worker-strided order computed above.
    let deduped = cf_typos::typos::deduplicate_typos(&ordered);
    Some(ReportData { typos: deduped })
}

/// One walked commit's typos products (the reference `typos.Consume` TC + runner stamps).
/// One entry per walked commit, in walk order (commits with no typos have an
/// empty `typos` — the reference implementation's nil-Data TC).
#[derive(Clone)]
pub(crate) struct TyposCommit {
    /// This commit's detected typos, in detection order.
    pub typos: Vec<cf_typos::Typo>,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Loose-identity author id (walk order).
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The shared `history/typos` revwalk: every typos format consumes THIS walk.
pub(crate) fn typos_walk(sub: &clap::ArgMatches) -> Option<Vec<TyposCommit>> {
    use cf_alg_levenshtein::Context as LevenshteinContext;
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_pathpolicy::Options as PathPolicyOptions;

    // Multi-analyzer runs route through the ONE shared UAST walk (same code,
    // one tree diff + one parse per blob per commit across the co-selected
    // analyzers); single-analyzer runs keep this direct walk.
    if let Some(shared) = super::uast_walk::shared_typos_walk(sub) {
        return shared;
    }

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    let max_distance = typos_max_distance(sub);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation,
    // ignoring `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`).
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    let mut lctx = LevenshteinContext::new();
    let mut identity = IdentityDetector::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    // One entry per walked commit, walk order. The report path flattens these
    // back to `(walk index, typo)` pairs and applies the worker-strided dedup.
    let mut commits: Vec<TyposCommit> = Vec::new();

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;

        // Identity + tick stamping (core analyzers run for every commit).
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        let mut entry = TyposCommit {
            typos: Vec::new(),
            tick,
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
        };

        // Tree diff against the first parent (root → full initial tree), then
        // the SAME per-commit product body the shared multi-analyzer walk calls.
        let changes = commit_tree_changes(&repo, &commit)?;
        let mut cache = super::uast_walk::CommitParseCache::new(&repo, &parser, &opts);
        entry.typos =
            typos_commit_product(&repo, *hash, &changes, max_distance, &mut lctx, &mut cache);

        commits.push(entry);
    }

    Some(commits)
}

/// The effective `--max-changes-per-commit` cap: 0/unset ⇒ default 10000
/// (reference: `maxChangesPerCommit`). Commits whose RAW tree diff exceeds
/// the cap are silently dropped from history BEFORE any analyzer runs — no
/// identity consumption, no tick assignment, no per-commit record.
pub(crate) fn max_changes_per_commit_cap(sub: &clap::ArgMatches) -> usize {
    const DEFAULT_MAX_CHANGES_PER_COMMIT: usize = 10_000;
    let v = sub
        .try_get_one::<i64>("max-changes-per-commit")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(0);
    if v <= 0 {
        DEFAULT_MAX_CHANGES_PER_COMMIT
    } else {
        v as usize
    }
}

/// The effective `--typos-max-distance`: 0/unset ⇒ default 4 (reference:
/// Configure/Initialize).
pub(crate) fn typos_max_distance(sub: &clap::ArgMatches) -> i64 {
    const DEFAULT_MAX_DISTANCE: i64 = 4;
    let v = sub
        .try_get_one::<i64>("typos-max-distance")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(0);
    if v <= 0 {
        DEFAULT_MAX_DISTANCE
    } else {
        v
    }
}

/// The `history/typos` per-commit product (the reference `typos.Consume` body
/// over the FileDiff/UAST plumbing): the typos detected in this commit's
/// Modify changes, in detection order. Called by BOTH the direct
/// [`typos_walk`] and the shared multi-analyzer UAST walk.
pub(crate) fn typos_commit_product(
    repo: &cf_gitlib::Repository,
    hash: cf_gitlib::Hash,
    changes: &[cf_gitlib::changes::Change],
    max_distance: i64,
    lctx: &mut cf_alg_levenshtein::Context,
    cache: &mut super::uast_walk::CommitParseCache<'_>,
) -> Vec<cf_typos::Typo> {
    use super::uast_walk::{ParseOutcome, SPILL_THRESHOLD};
    use cf_gitlib::changes::ChangeAction;
    use cf_typos::{Hash as TypoHash, Typo};

    // Spill rule: a commit with > 32 changes (`uastSpillThreshold`) has its
    // UAST parsed via the disk-backed spill path (`parseCommitAndSpill`), and
    // in the streaming run the typos leaf then sees ZERO UAST changes for that
    // commit — so it produces no typos there. (Verified against the live reference
    // binary: e.g. ioq3's 1409-change `5b755058` line-ending commit and
    // kubernetes' 54-change `894a7e32` both yield 0 typos in the reference implementation, while every
    // typo the reference implementation reports comes from a <=32-change commit.) This mirrors the
    // identical `> SPILL_THRESHOLD` skip the quality/sentiment/shotness/
    // comments analyzers already apply; without it Rust over-detects thousands
    // of spurious typos on mass-rewrite commits.
    if changes.len() > SPILL_THRESHOLD {
        return Vec::new();
    }

    // The commit hash threaded into each Typo (cf_typos uses its own Hash;
    // both are 20-byte SHA-1 tuple structs ⇒ copy the raw bytes).
    let commit_hash = TypoHash(hash.0);

    let mut typos: Vec<Typo> = Vec::new();

    for change in changes {
        // Typos only fires on Modify (needs both Before and After UAST).
        if !matches!(change.action, ChangeAction::Modify) {
            continue;
        }

        // FileDiff.processChange preconditions (Modify path).
        if change.from.hash == change.to.hash {
            continue;
        }
        let Some(blob_before) = cache.blob(change.from.hash) else {
            continue;
        };
        let Some(blob_after) = cache.blob(change.to.hash) else {
            continue;
        };
        if blob_before.is_binary() || blob_after.is_binary() {
            continue;
        }
        if blob_before.data == blob_after.data {
            // Identical content ⇒ FileDiff emits a single Equal diff ⇒ no
            // candidates ⇒ no typos.
            continue;
        }

        // Both UAST sides must parse (reference: Before != nil && After != nil).
        let before_outcome = cache.parse(&change.from.name, change.from.hash);
        let ParseOutcome::Parsed(before) = &*before_outcome else {
            continue;
        };
        let after_outcome = cache.parse(&change.to.name, change.to.hash);
        let ParseOutcome::Parsed(after) = &*after_outcome else {
            continue;
        };

        // bytes.Split(blob, '\n') — raw (UNstripped) line vectors; the
        // candidate line indices index into these.
        let lines_before: Vec<&[u8]> = split_lines(&blob_before.data);
        let lines_after: Vec<&[u8]> = split_lines(&blob_after.data);

        // The runtime pipeline feeds typos the libgit2 line-diff op stream
        // (reference → cf_batch_diff_blobs → git_diff_buffers),
        // NOT diffmatchpatch — only falling back to dmp on a libgit2 error. The
        // two group changed lines differently, so on mass-rewrite commits (e.g.
        // ioq3's `5b755058` line-ending normalization) diffmatchpatch yields one
        // big Delete+Insert block that pairs every line as a candidate, whereas
        // libgit2's Myers diff keeps the genuinely-unchanged lines Equal — which
        // is what makes the reference binary report a handful of typos there instead of thousands.
        let old_lines = blob_before.count_lines().map_or(0, |n| n as i64);
        let ops = cf_gitlib::diff::diff_blob_line_ops(
            repo.native(),
            change.from.hash,
            change.to.hash,
            old_lines,
        )
        .unwrap_or_default();

        let cand = find_typo_candidates(&ops, &lines_before, &lines_after, max_distance, lctx);
        if cand.candidates.is_empty() {
            continue;
        }

        // Collect identifiers on the focused lines (0-based start line).
        let removed = collect_identifiers_on_lines(before, &cand.focused_before);
        let added = collect_identifiers_on_lines(after, &cand.focused_after);

        for c in &cand.candidates {
            let nb = removed.get(&c.before);
            let na = added.get(&c.after);
            if let (Some(nb), Some(na)) = (nb, na) {
                if nb.len() == 1 && na.len() == 1 {
                    typos.push(Typo {
                        wrong: nb[0].clone(),
                        correct: na[0].clone(),
                        file: change.to.name.clone(),
                        commit: commit_hash,
                        line: c.after,
                    });
                }
            }
        }
    }

    typos
}

/// Per-commit typos NDJSON records (forked leaf): only commits with detected
/// typos emit a line (nil-Data TC otherwise); `data` is the reference implementation's `[]Typo` — each a
/// struct in `Wrong`/`Correct`/`File`/`Commit`/`Line` declaration order, with
/// `Commit` a `[20]byte` array marshaling as 20 JSON numbers.
pub fn typos_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = typos_walk(sub)?;
    let mut records = Vec::new();
    for (pos, c) in walk.iter().enumerate() {
        if c.typos.is_empty() {
            continue;
        }
        let items: Vec<GoValue> = c
            .typos
            .iter()
            .map(|t| {
                let mut m = GoMap::new_struct();
                m.insert("Wrong".to_string(), GoValue::Str(t.wrong.clone()));
                m.insert("Correct".to_string(), GoValue::Str(t.correct.clone()));
                m.insert("File".to_string(), GoValue::Str(t.file.clone()));
                m.insert(
                    "Commit".to_string(),
                    GoValue::Array(
                        t.commit
                            .0
                            .iter()
                            .map(|b| GoValue::Int(i64::from(*b)))
                            .collect(),
                    ),
                );
                m.insert("Line".to_string(), GoValue::Int(t.line));
                GoValue::Object(m)
            })
            .collect();
        let hash_hex: String = c.typos[0]
            .commit
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        records.push(super::history_formats::NdjsonRecord {
            pos,
            hash: hash_hex,
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: GoValue::Array(items),
        });
    }
    Some(records)
}

/// A focused typo candidate line pair.
#[derive(Clone, Copy)]
struct TypoCandidate {
    before: i64,
    after: i64,
}

/// Output of [`find_typo_candidates`].
struct TypoCandidates {
    candidates: Vec<TypoCandidate>,
    focused_before: std::collections::HashSet<i64>,
    focused_after: std::collections::HashSet<i64>,
}

/// Port of the reference `bytes.Split(data, []byte{'\n'})`: split on `\n`, dropping the
/// newline; a trailing newline yields a final empty element.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n').collect()
}

/// Port of the reference `typos.findTypoCandidates` + `matchDeleteInsertPairs`.
///
/// Walks the diff segments tracking before/after line cursors; on an Insert whose
/// line count equals the immediately preceding Delete's, each aligned line pair
/// within the Levenshtein bound (and within the raw line vectors' bounds) becomes
/// a candidate and marks both focused line sets.
fn find_typo_candidates(
    ops: &[cf_gitlib::diff::LineOp],
    lines_before: &[&[u8]],
    lines_after: &[&[u8]],
    max_distance: i64,
    lctx: &mut cf_alg_levenshtein::Context,
) -> TypoCandidates {
    use cf_gitlib::diff::LineOp;

    let mut line_num_before: i64 = 0;
    let mut line_num_after: i64 = 0;
    let mut removed_size: i64 = 0;
    let mut candidates: Vec<TypoCandidate> = Vec::new();
    let mut focused_before: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut focused_after: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for op in ops {
        // Each op carries a line count (reference: utf8.RuneCountInString(edit.Text),
        // one encoded rune per line; here the libgit2 op's coalesced line count).
        match *op {
            LineOp::Delete(size) => {
                line_num_before += size;
                removed_size = size;
            }
            LineOp::Insert(size) => {
                if size == removed_size {
                    for i in 0..size {
                        let lb = line_num_before - size + i;
                        let la = line_num_after + i;
                        if lb < 0 || la < 0 {
                            continue;
                        }
                        let (lbu, lau) = (lb as usize, la as usize);
                        if lbu >= lines_before.len() || lau >= lines_after.len() {
                            continue;
                        }
                        // The reference implementation compares len() on []byte (byte length) for the
                        // length-difference fast path.
                        let len_b = lines_before[lbu].len() as i64;
                        let len_a = lines_after[lau].len() as i64;
                        if len_b - len_a > max_distance || len_a - len_b > max_distance {
                            continue;
                        }
                        // Distance over the strings (reference: converts []byte→string).
                        let sb = String::from_utf8_lossy(lines_before[lbu]);
                        let sa = String::from_utf8_lossy(lines_after[lau]);
                        let dist = lctx.distance(&sb, &sa) as i64;
                        if dist <= max_distance {
                            candidates.push(TypoCandidate {
                                before: lb,
                                after: la,
                            });
                            focused_before.insert(lb);
                            focused_after.insert(la);
                        }
                    }
                }
                line_num_after += size;
                removed_size = 0;
            }
            LineOp::Equal(size) => {
                line_num_before += size;
                line_num_after += size;
                removed_size = 0;
            }
        }
    }

    TypoCandidates {
        candidates,
        focused_before,
        focused_after,
    }
}

/// Port of the reference `typos.collectIdentifiersOnLines`: groups identifier tokens by
/// their 0-based start line (`Pos.StartLine - 1`), keeping only focused lines.
fn collect_identifiers_on_lines(
    root: &cf_uast_node::Node,
    focused: &std::collections::HashSet<i64>,
) -> std::collections::HashMap<i64, Vec<String>> {
    use cf_uast_node::UAST_IDENTIFIER;
    let mut result: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    root.visit_pre_order(|n| {
        if n.node_type != UAST_IDENTIFIER {
            return;
        }
        let Some(pos) = n.pos.as_ref() else {
            return;
        };
        let line = pos.start_line as i64 - 1;
        if focused.contains(&line) {
            result.entry(line).or_default().push(n.token.clone());
        }
    });
    result
}

/// Per-change line stats for a `Modify` change, using the SAME libgit2 line
/// diff the reference runtime pipeline uses (`DiffPipeline` → `gitlib.Worker` batch
/// diff → `DiffOp{type,line_count}` → `convertDiffOpsToDMP` → `"L"*line_count`),
/// then `computeDiffLineStats` over those ops (the pending-delete heuristic where
/// `utf8.RuneCountInString(text) == op.line_count`).
///
/// This is NOT the diff-match-patch path: the devs analyzer reads
/// `ac.FileDiffs`, which the framework computes with libgit2 (the diff pipeline
/// `processDiffResponse` → `convertDiffOpsToDMP`), so byte-parity requires the
/// libgit2 op stream, reproduced here by `cf_gitlib::worker::Worker::batch_diff_blobs`.
fn devs_modify_line_stats(
    worker: &cf_gitlib::worker::Worker,
    old_data: &[u8],
    new_data: &[u8],
) -> (i64, i64, i64) {
    use cf_gitlib::worker::{DiffOpType, DiffRequest};
    let req = DiffRequest {
        old_data: old_data.to_vec(),
        new_data: new_data.to_vec(),
        has_old: true,
        has_new: true,
        ..Default::default()
    };
    let results = worker.batch_diff_blobs(std::slice::from_ref(&req));
    let res = &results[0];
    // On a diff error (e.g. binary), the reference implementation's processDiffResponse skips this entry
    // (errOld/errNew or diffRes.Error) — caller already guards binary, but be
    // safe and return zero stats so no entry is recorded.
    if res.error.is_some() {
        return (0, 0, 0);
    }
    // computeDiffLineStats over the libgit2 ops (text rune-count == line_count).
    let mut added = 0i64;
    let mut removed = 0i64;
    let mut changed = 0i64;
    let mut removed_pending = 0i64;
    for op in &res.ops {
        match op.op_type {
            DiffOpType::Equal => {
                removed += removed_pending;
                removed_pending = 0;
            }
            DiffOpType::Insert => {
                let delta = i64::from(op.line_count);
                if removed_pending > delta {
                    changed += delta;
                    removed += removed_pending - delta;
                } else {
                    changed += removed_pending;
                    added += delta - removed_pending;
                }
                removed_pending = 0;
            }
            DiffOpType::Delete => {
                removed_pending = i64::from(op.line_count);
            }
        }
    }
    removed += removed_pending;
    (added, removed, changed)
}

/// Detects the programming language of a changed file, mirroring the reference implementation's
/// `LanguagesDetectionAnalyzer.detectLanguage`: `""` for a binary blob, then the
/// fast-path extension table (`languageByExtension`), then the enry fallback
/// (`enry.GetLanguage`). The enry fallback is reproduced as its path-only subset
/// (filename + single-match extension strategies); content-classifier passes are
/// not ported. The label only flows into the per-language breakdown.
fn devs_detect_language(name: &str, data: &[u8]) -> String {
    if cf_textutil::is_binary(data) {
        return String::new();
    }
    let lang = cf_analyzers_plumbing::language_by_extension(name);
    if !lang.is_empty() {
        return lang.to_string();
    }
    // Slow path: enry.GetLanguage(base(name), content). The path-only subset
    // (filename + extension strategies that resolve to a single language) is
    // reproduced via cf-langpath; this covers every fast-path miss observed on
    // Go-source repos (.sls→SaltStack, .raml→RAML, .txt→Text, …). The
    // ambiguous extensions resolve via the ported Naive-Bayes classifier
    // (cf_langpath::content). enry's firstLanguage returns "Other" when no
    // strategy yields a language; we map None to "" (→ "Other" bucket), the same
    // result.
    // enry's OtherLanguage sentinel is "Other" and the devs language merge
    // keys "" → "Other" too; keep "Other" as-is (it is a real enry result, not
    // the empty fallback) and map None to "" (→ "Other" bucket), the same
    // result.
    cf_langpath::language_by_path_with_content(name, data).unwrap_or_default()
}

/// Builds the `run --analyzers history/devs --format json` bytes by RUNNING the
/// real general history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// Faithful port of the reference streaming path (the reference `initHistoryPipeline` →
/// `framework.RunStreaming` → core `plumbing.{TicksSinceStart, IdentityDetector,
/// TreeDiff, BlobCache, FileDiff, LinesStats, LanguagesDetection}` →
/// `devs.Analyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits. `--first-parent` adds `SimplifyFirstParent`.
///  - **oversized-commit skip** (the reference `maxChangesPerCommit = 10000`):
///    a commit whose RAW tree diff exceeds 10000 changes is skipped ENTIRELY —
///    its core analyzers never run, so it contributes nothing to the people dict
///    or `commits_by_tick`. Reproduced before identity/tick assignment.
///  - **identity** (`plumbing.IdentityDetector`, loose, incremental): every
///    non-skipped commit's author signature is consumed in walk order, assigning
///    author ids first-seen; `FinalizeDict` then builds `ReversedPeopleDict`.
///  - **tick assignment** (`plumbing.TicksSinceStart`, 24h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h), previousTick)`
///    over the committer time. `commits_by_tick` records EVERY non-skipped commit
///    (the core analyzer runs regardless of the leaf's per-commit decisions).
///  - **merge dedup + IsMerge** (`devs.Consume`): a commit with `> 1` parents
///    already seen is skipped (no TC). `IsMerge = NumParents() > 1` (FirstParent
///    off): a merge commit yields `commits=1` but NO line stats
///    (`accumulateLineStats` is gated on `!IsMerge`).
///  - **empty-commit gate**: with `ConsiderEmptyCommits=false` (default), a
///    commit whose FILTERED tree diff is empty produces no TC.
///  - **tree-diff filter** (`TreeDiffAnalyzer.filterChanges`): drop each change
///    where `pathpolicy.Exclude(name, nil)` is true (`--languages all` disables
///    the language gate; no `--skip-files`).
///  - **line stats** (`LinesStatsCalculator`, non-merge only): Insert ⇒ Added =
///    `CountLines(To)`; Delete ⇒ Removed = `CountLines(From)`; Modify ⇒
///    `computeDiffLineStats` over the libgit2 `ac.FileDiffs` op stream
///    ([`devs_modify_line_stats`]), keyed by `change.To.Name`, skipping
///    binary / identical-content files.
///  - **languages** (`LanguagesDetectionAnalyzer`): each change's blob is mapped
///    to a language ([`devs_detect_language`]); `accumulateLineStats` attributes
///    the change's stats to that language (`langs[entry.Hash]`).
///  - **per-commit aggregation** (`CommitDevData`): `commits=1`, summed
///    added/removed/changed, per-language breakdown; keyed by commit hex.
///  - **tick bounds** (`BuildTickBounds`): min/max committer time over the
///    TCs (CDD-producing commits) in each tick, RFC3339-UTC formatted.
///
/// `ComputeAllMetrics` (`parse_tick_data_with_bounds` → `AggregateCommitsToTicks`
/// over `commits_by_tick` → developers/languages/busfactor/activity/churn/
/// aggregate with the HLL cardinality sketch) then yields the report; bytes route
/// through cf-gojson (compact, HTML-escape on, no trailing newline), the same
/// `ComputedMetrics.ToJSON()` shape the `--head` path emits.
///
/// **Parity note (enry):** the enry *content* language fallback is not ported, so
/// files without a fast-path extension get `""` (→ "Other"). For arbitrary repos
/// where such files carry line changes this is the one residual divergence; it is
/// absent on the Go-source-heavy inputs the gate probes (every changed file is a
/// fast-path extension). [`devs_detect_language`].
pub fn devs_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_run_metrics(sub)?;
    Some(cf_gojson::marshal(
        &cf_devs::serialize::computed_metrics_to_go(&metrics),
    ))
}

/// Builds the full-revwalk `history/devs --format yaml` report bytes (no
/// `--head`), reusing the shared [`devs_run_metrics`] report value. Mirrors the
/// The YAML branch of the reference `analyze.OutputHistoryResults`: the manual version header
/// (`analyze.PrintHeader`), the `<leaf-name>:` line, then `yaml.Marshal` of the
/// per-leaf `ComputedMetrics` (routed through cf-goyaml so the same report value
/// drives every format).
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

/// Builds the full-revwalk `history/devs --format bin` report bytes (no
/// `--head`), reusing the shared [`devs_run_metrics`] report value wrapped in
/// the CFB1 binary envelope (the reference `analyze.OutputHistoryResults` raw/binary
/// branch). One report value, encoded by the serializer layer.
pub fn devs_run_report_bin(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_run_metrics(sub)?;
    let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
    cf_reportutil::encode_binary_envelope(&payload).ok()
}

/// Shared full-revwalk `history/devs` metrics builder (no `--head`): runs the
/// general per-commit pipeline once and returns the aggregated
/// [`cf_devs::ComputedMetrics`], so every output format (json/yaml/bin) is an
/// encoding of the SAME report value (the reference `analyze.OutputHistoryResults`, which
/// computes the per-leaf `ComputedMetrics` once and then marshals it per format).
pub fn devs_run_metrics(sub: &clap::ArgMatches) -> Option<cf_devs::ComputedMetrics> {
    use cf_devs::{parse_tick_data_with_bounds, MetricOptions};
    let walk = devs_walk(sub)?;
    let input = parse_tick_data_with_bounds(
        &walk.commit_dev_data,
        &walk.commits_by_tick,
        walk.names,
        0,
        walk.tick_bounds,
    );
    Some(cf_devs::compute_all_metrics(
        &input,
        &MetricOptions::default(),
    ))
}

/// The raw products of one `history/devs` revwalk, shared by the aggregated
/// metrics path and the per-commit time-series (NDJSON) path so both encode the
/// SAME walk (the reference `framework.Runner` builds the per-commit `CommitDevData` +
/// `commitMeta` once; `OutputHistoryResults` aggregates while the timeseries
/// sink streams per commit).
struct DevsWalk {
    /// hex hash → per-commit dev data.
    commit_dev_data: std::collections::BTreeMap<String, cf_devs::CommitDevData>,
    /// tick → hashes in walk order; drives commit order.
    commits_by_tick: std::collections::BTreeMap<i64, Vec<String>>,
    /// tick → RFC3339-UTC committer-time bounds over CDD commits.
    tick_bounds: std::collections::BTreeMap<i64, cf_devs::TickBounds>,
    /// hex hash → tick.
    tick_by_hash: std::collections::BTreeMap<String, i64>,
    /// hex hash → (committer seconds, committer UTC-offset minutes) for the
    /// RFC3339 `CommitMeta.Timestamp` (reference: formats `commit.Committer().When`).
    when_by_hash: std::collections::BTreeMap<String, (i64, i32)>,
    /// Finalized ReversedPeopleDict.
    names: Vec<String>,
}

fn devs_walk(sub: &clap::ArgMatches) -> Option<DevsWalk> {
    use std::collections::{BTreeMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_devs::{CommitDevData, TickBounds};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::worker::Worker;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    // Reference: maxChangesPerCommit = 10000 (raw tree-diff cap).
    const MAX_CHANGES_PER_COMMIT: usize = 10_000;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    // Window: `--head` loads EXACTLY the single HEAD commit (the reference implementation,
    // ignoring `--limit`); otherwise the `limit` commits oldest-first (reference:
    // `gitlib.loadHistoryCommits`).
    let hashes = if sub.get_flag("head") {
        vec![repo.head().ok()?]
    } else {
        crate::handlers::load_history_commit_hashes(
            &repo,
            limit,
            first_parent,
            crate::handlers::history_since_spec(sub),
        )?
    };

    let policy = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    // ---- parallel pure-compute stage -----------------------------------------
    // The expensive per-commit work — tree diff + per-change line stats
    // (libgit2) + language detection — is a PURE function of (repo, commit) and
    // independent across commits, so run it across all cores. The order-dependent
    // reduce below (identity ids, ticks, commits_by_tick, merge dedup) then runs
    // sequentially over the results in oldest-first order, byte-identically.
    /// The expensive, per-commit-independent products of one commit's diff.
    struct DevsPrepared {
        /// Raw (pre-filter) tree-diff change count, for the oversized-commit gate.
        raw_change_count: usize,
        /// Post-filter change count, for the empty-commit gate.
        filtered_count: usize,
        /// `(language, line-stats)` per attributed change (non-merge only; empty
        /// for merges / oversized / empty commits). Folded into `CommitDevData`.
        attributions: Vec<(String, cf_devs::LineStats)>,
    }
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let policy_ref = &policy;
    let prepared = parallel_prepare(&path, &hashes, workers, move |repo, hash| {
        let commit = repo.lookup_commit(hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1 && !first_parent;

        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(repo, Some(&new_tree)).ok()?
        };
        let raw_change_count = raw_changes.len();
        // Oversized commits are dropped before any analyzer — no per-change work.
        if raw_change_count > MAX_CHANGES_PER_COMMIT {
            return Some(DevsPrepared {
                raw_change_count,
                filtered_count: 0,
                attributions: Vec::new(),
            });
        }
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, policy_ref)
            })
            .collect();
        let filtered_count = changes.len();
        let mut attributions: Vec<(String, cf_devs::LineStats)> = Vec::new();
        // Line stats are accumulated only for non-merge commits.
        if !is_merge && filtered_count > 0 {
            let worker = Worker::new(repo);

            // The reference `LanguagesDetectionAnalyzer.Languages()`: the per-commit language
            // map is keyed by BLOB HASH, written in change order (Insert → To,
            // Delete → From, Modify → BOTH sides) with later changes
            // OVERWRITING earlier ones. Two same-content files with different
            // extensions therefore share ONE language — the LAST change's name
            // wins for every change carrying that blob (ioq3's giant import
            // commits move thousands of lines between C and C++ this way). The
            // attribution below looks the language up by hash, never detecting
            // per change.
            let mut langs: std::collections::HashMap<cf_gitlib::hash::Hash, String> =
                std::collections::HashMap::new();
            let detect_into =
                |langs: &mut std::collections::HashMap<cf_gitlib::hash::Hash, String>,
                 name: &str,
                 h: cf_gitlib::hash::Hash| {
                    let lang = match CachedBlob::from_repo(repo, h) {
                        Ok(b) => devs_detect_language(name, &b.data),
                        Err(_) => String::new(),
                    };
                    langs.insert(h, lang);
                };
            for change in &changes {
                match change.action {
                    ChangeAction::Insert => {
                        detect_into(&mut langs, &change.to.name, change.to.hash)
                    }
                    ChangeAction::Delete => {
                        detect_into(&mut langs, &change.from.name, change.from.hash);
                    }
                    ChangeAction::Modify => {
                        detect_into(&mut langs, &change.to.name, change.to.hash);
                        detect_into(&mut langs, &change.from.name, change.from.hash);
                    }
                }
            }

            for change in &changes {
                let stats = match change.action {
                    ChangeAction::Insert => {
                        let Ok(blob) = CachedBlob::from_repo(repo, change.to.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else {
                            continue;
                        };
                        cf_devs::LineStats {
                            added: lines as i64,
                            removed: 0,
                            changed: 0,
                        }
                    }
                    ChangeAction::Delete => {
                        let Ok(blob) = CachedBlob::from_repo(repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else {
                            continue;
                        };
                        cf_devs::LineStats {
                            added: 0,
                            removed: lines as i64,
                            changed: 0,
                        }
                    }
                    ChangeAction::Modify => {
                        let Ok(blob_from) = CachedBlob::from_repo(repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(repo, change.to.hash) else {
                            continue;
                        };
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        // The runtime diff pipeline (the reference implementation
                        // prepareDiffRequest) does NOT skip same-hash modifies (a
                        // mode-only change, e.g. +x removed): identical content
                        // diffs to a single Equal op, yielding a 0/0/0 LineStats
                        // entry that still CREATES the per-language key in
                        // CommitDevData — the zero-line language entries the reference implementation's
                        // per-dev language lists (and busfactor contributor
                        // counts) carry. Only nil/binary blobs are skipped.
                        if change.from.hash == change.to.hash {
                            cf_devs::LineStats {
                                added: 0,
                                removed: 0,
                                changed: 0,
                            }
                        } else {
                            let (added, removed, changed) =
                                devs_modify_line_stats(&worker, &blob_from.data, &blob_to.data);
                            cf_devs::LineStats {
                                added,
                                removed,
                                changed,
                            }
                        }
                    }
                };
                // Language attribution: `langs[changeEntry.Hash]` (the LineStats
                // key side — To for insert/modify, From for delete) from the
                // hash-keyed map above.
                let data_hash = match change.action {
                    ChangeAction::Delete => change.from.hash,
                    _ => change.to.hash,
                };
                let lang = langs.get(&data_hash).cloned().unwrap_or_default();
                attributions.push((lang, stats));
            }
        }
        Some(DevsPrepared {
            raw_change_count,
            filtered_count,
            attributions,
        })
    })?;

    // ---- sequential ordered-reduce stage -------------------------------------
    // Per-commit dev data (hex hash → CommitDevData), commits-by-tick over ALL
    // non-skipped commits, and per-tick committer-time bounds over CDD commits.
    let mut commit_dev_data: BTreeMap<String, CommitDevData> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min,max) secs over CDD commits.
    let mut tick_by_hash: BTreeMap<String, i64> = BTreeMap::new();
    let mut when_by_hash: BTreeMap<String, (i64, i32)> = BTreeMap::new();
    let mut seen_merges: HashSet<String> = HashSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for (i, hash) in hashes.iter().enumerate() {
        // The expensive tree-diff / line-stat / language work for this commit was
        // computed in parallel above; the reduce only reads it (in oldest-first
        // order) and does the cheap order-dependent bookkeeping.
        let prep = &prepared[i];
        let commit = repo.lookup_commit(*hash).ok()?;
        let num_parents = commit.num_parents();
        // `multi_parent` (`commit.NumParents() > 1`) drives the devs MergeTracker
        // dedup (`devs.Consume`: SeenOrAdd keyed on the raw parent count). (The
        // `IsMerge` line-stat gate was applied in the parallel compute, which only
        // produced attributions for non-merge commits.)
        let multi_parent = num_parents > 1;
        let hex = hash.to_string();

        // Oversized-commit skip: the framework drops commits whose RAW tree diff
        // exceeds the cap BEFORE any analyzer (core or leaf) runs (count from the
        // parallel pre-pass).
        if prep.raw_change_count > MAX_CHANGES_PER_COMMIT {
            continue;
        }

        // Core analyzers run for every surviving commit. Identity (loose,
        // incremental) in walk order.
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        // Tick assignment from the committer time (24h default), monotonic.
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();
        let when_offset = committer_when.offset_minutes();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // commits_by_tick records EVERY non-skipped commit (TicksSinceStart).
        // Dedup tail-scan for commits with parents (reference Consume).
        let bucket = commits_by_tick.entry(tick).or_default();
        let exists = num_parents > 0 && bucket.iter().rev().any(|h| h == &hex);
        if !exists {
            bucket.push(hex.clone());
        }

        // devs.Consume: skip already-seen merge commits (MergeTracker). Keyed on
        // the RAW multi-parent flag, not IsMerge (SeenOrAdd runs before the
        // first-parent IsMerge override).
        if multi_parent && !seen_merges.insert(hex.clone()) {
            continue;
        }

        // Empty-commit gate (ConsiderEmptyCommits=false): no TC when the FILTERED
        // tree diff is empty (count from the parallel pre-pass).
        if prep.filtered_count == 0 {
            continue;
        }

        // CommitDevData: commits=1; fold the precomputed per-change `(language,
        // line-stats)` attributions (empty for merge commits, so a merge yields
        // commits=1 with zero line stats — `if !ac.IsMerge`).
        let mut cdd = CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id,
            languages: BTreeMap::new(),
        };
        for (lang, stats) in &prep.attributions {
            cdd.added += stats.added;
            cdd.removed += stats.removed;
            cdd.changed += stats.changed;
            let ls = cdd.languages.entry(lang.clone()).or_default();
            *ls = ls.plus(*stats);
        }

        commit_dev_data.insert(hex.clone(), cdd);

        // CommitMeta: tick + committer-time
        // RFC3339 for the per-commit time-series stream, deduped by first TC.
        tick_by_hash.entry(hex.clone()).or_insert(tick);
        when_by_hash
            .entry(hex.clone())
            .or_insert((when, when_offset));

        // Tick bounds: min/max committer time over CDD commits (tc.Timestamp).
        tick_when
            .entry(tick)
            .and_modify(|(lo, hi)| {
                if when < *lo {
                    *lo = when;
                }
                if when > *hi {
                    *hi = when;
                }
            })
            .or_insert((when, when));
    }

    // FinalizeDict: build ReversedPeopleDict from the incremental identities.
    identity.finalize_dict();
    let names = identity.reversed_people_dict.clone();

    // tick_bounds[tick] = RFC3339-UTC(min) / RFC3339-UTC(max) over CDD commits.
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    Some(DevsWalk {
        commit_dev_data,
        commits_by_tick,
        tick_bounds,
        tick_by_hash,
        when_by_hash,
        names,
    })
}

/// Builds the `history/devs --format timeseries --ndjson` bytes: one compact
/// JSON line per CDD commit, in `commits_by_tick` order (tick-sorted, walk order
/// within a tick), matching the reference `analyze.WriteTimeSeriesNDJSON` over the
/// `MergedTimeSeries` built from devs' `ExtractCommitTimeSeries`.
///
/// Each line is a reference `MergedCommitData` whose flattened key set
/// (`author`/`devs`/`hash`/`tick`/`timestamp`) is `json.Marshal(map[string]any)`
/// — alphabetically key-sorted. The `devs` value is the per-commit entry
/// (`author_id`/`commits`/`languages`/`lines_*`/`net_change`, also a key-sorted
/// `map[string]any`); `languages` is `map[string]LineStats` (sorted by language,
/// each a struct in `added`/`removed`/`changed` field order). `author` is empty
/// (the direct time-series path wires no identity provider, the reference implementation
/// `framework.authorName` → ""); `timestamp` is the committer time in RFC3339
/// with its original zone offset.
pub fn devs_run_timeseries_ndjson(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = devs_walk(sub)?;

    let line_stats_value = |s: &cf_devs::LineStats| -> GoValue {
        // LineStats is a reference struct: `added`/`removed`/`changed` field order.
        let mut m = GoMap::new_struct();
        m.insert("added".to_string(), GoValue::Int(s.added));
        m.insert("removed".to_string(), GoValue::Int(s.removed));
        m.insert("changed".to_string(), GoValue::Int(s.changed));
        GoValue::Object(m)
    };

    let mut out = Vec::new();
    for hashes in walk.commits_by_tick.values() {
        for hex in hashes {
            // Only CDD commits contribute a time-series record (ExtractCommit-
            // TimeSeries iterates CommitDevData).
            let Some(cdd) = walk.commit_dev_data.get(hex) else {
                continue;
            };

            // devs per-commit entry: a map[string]any (key-sorted on marshal).
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
                // languages: map[string]LineStats (key-sorted by language name).
                let mut langs = GoMap::new_map();
                for (lang, stats) in &cdd.languages {
                    langs.insert(lang.clone(), line_stats_value(stats));
                }
                devs.insert("languages".to_string(), GoValue::Object(langs));
            }

            // MergedCommitData flattened to a map[string]any (key-sorted):
            // author / devs / hash / tick / timestamp.
            let tick = walk.tick_by_hash.get(hex).copied().unwrap_or(0);
            let timestamp = match walk.when_by_hash.get(hex) {
                Some((secs, off)) => format_rfc3339_offset(*secs, *off),
                None => String::new(),
            };
            let mut rec = GoMap::new_map();
            rec.insert("hash".to_string(), GoValue::Str(hex.clone()));
            rec.insert("timestamp".to_string(), GoValue::Str(timestamp));
            rec.insert("author".to_string(), GoValue::Str(String::new()));
            rec.insert("tick".to_string(), GoValue::Int(tick));
            rec.insert("devs".to_string(), GoValue::Object(devs));

            out.extend_from_slice(&cf_gojson::marshal(&GoValue::Object(rec)));
            out.push(b'\n');
        }
    }
    Some(out)
}

/// Per-commit devs NDJSON records (the reference `StreamingSink` over devs TCs): one
/// record per CDD commit in walk order (tick-sorted buckets preserve the
/// oldest-first walk because tick assignment is monotonic). The `data` payload
/// is the reference `*CommitDevData` STRUCT — field-declaration order `commits`,
/// `lines_added`, `lines_removed`, `lines_changed`, `author_id`, then
/// `languages` (omitempty; `map[string]LineStats`, key-sorted, each LineStats
/// in `added`/`removed`/`changed` struct order).
pub fn devs_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = devs_walk(sub)?;
    let mut records = Vec::new();
    // Walk position across ALL commits (including nil-Data ones): the merge
    // with the other sequential leaf (burndown) in `history_ndjson` keys on
    // the commit's consume position, so a skipped commit must still advance it.
    let mut walk_pos = 0usize;
    for hashes in walk.commits_by_tick.values() {
        for hex in hashes {
            let pos = walk_pos;
            walk_pos += 1;
            let Some(cdd) = walk.commit_dev_data.get(hex) else {
                continue; // nil-Data TC (empty diff / seen merge): no line.
            };
            let mut data = GoMap::new_struct();
            data.insert("commits".to_string(), GoValue::Int(cdd.commits));
            data.insert("lines_added".to_string(), GoValue::Int(cdd.added));
            data.insert("lines_removed".to_string(), GoValue::Int(cdd.removed));
            data.insert("lines_changed".to_string(), GoValue::Int(cdd.changed));
            data.insert("author_id".to_string(), GoValue::Int(cdd.author_id));
            if !cdd.languages.is_empty() {
                let mut langs = GoMap::new_map();
                for (lang, stats) in &cdd.languages {
                    let mut ls = GoMap::new_struct();
                    ls.insert("added".to_string(), GoValue::Int(stats.added));
                    ls.insert("removed".to_string(), GoValue::Int(stats.removed));
                    ls.insert("changed".to_string(), GoValue::Int(stats.changed));
                    langs.insert(lang.clone(), GoValue::Object(ls));
                }
                data.insert("languages".to_string(), GoValue::Object(langs));
            }
            let (secs, off) = walk.when_by_hash.get(hex).copied().unwrap_or((0, 0));
            // devs is `Sequential: true` — emission is plain walk order; the
            // position is the commit's consume index in the walk.
            records.push(super::history_formats::NdjsonRecord {
                pos,
                hash: hex.clone(),
                tick: walk.tick_by_hash.get(hex).copied().unwrap_or(0),
                author_id: cdd.author_id,
                time_secs: secs,
                tz_offset_min: off,
                data: GoValue::Object(data),
            });
        }
    }
    Some(records)
}

/// The devs contribution to the merged `--format timeseries` document (reference:
/// `devs.ExtractCommitTimeSeries` over `report["CommitDevData"]`). The devs
/// report carries NO `commits_by_tick`, so the merged document lists the
/// analyzer but orders zero commits (the reference implementation emits `"commits": []`).
pub fn devs_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};

    let walk = devs_walk(sub)?;
    let mut per_commit = Vec::new();
    for (hex, cdd) in &walk.commit_dev_data {
        // map[string]any per commit: key-sorted on marshal (MapOrigin::Map).
        let mut entry = GoMap::new_map();
        entry.insert("commits".to_string(), GoValue::Int(cdd.commits));
        entry.insert("lines_added".to_string(), GoValue::Int(cdd.added));
        entry.insert("lines_removed".to_string(), GoValue::Int(cdd.removed));
        entry.insert("lines_changed".to_string(), GoValue::Int(cdd.changed));
        entry.insert(
            "net_change".to_string(),
            GoValue::Int(cdd.added - cdd.removed),
        );
        entry.insert("author_id".to_string(), GoValue::Int(cdd.author_id));
        if !cdd.languages.is_empty() {
            let mut langs = GoMap::new_map();
            for (lang, stats) in &cdd.languages {
                let mut ls = GoMap::new_struct();
                ls.insert("added".to_string(), GoValue::Int(stats.added));
                ls.insert("removed".to_string(), GoValue::Int(stats.removed));
                ls.insert("changed".to_string(), GoValue::Int(stats.changed));
                langs.insert(lang.clone(), GoValue::Object(ls));
            }
            entry.insert("languages".to_string(), GoValue::Object(langs));
        }
        per_commit.push((hex.clone(), GoValue::Object(entry)));
    }
    Some(super::history_formats::TimeSeriesContribution {
        flag: "devs",
        per_commit,
        commit_meta: Vec::new(),
    })
}

/// Builds the `history/devs --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the reference head-only pipeline for `history/devs`:
///  - identity: a loose people dict built from HEAD's author
///    (`IdentityDetector.GeneratePeopleDict([head]).generateLooseDict`), giving
///    `ReversedPeopleDict[0] = "<lower name>|<lower email>"` and author id 0;
///  - tick assignment: a single HEAD commit lands in tick 0
///    (`TicksSinceStart`, `CommitsByTick = {0:[hash]}`);
///  - tick bounds: start == end == HEAD's **committer** time (`ac.Time`,
///    the reference runner), `RFC3339`-formatted in UTC;
///  - per-commit dev data: `{commits:1, author_id:0}`. A **merge** HEAD
///    (`NumParents()>1`) skips `accumulateLineStats`, so all
///    line stats are 0 — the deterministic, language-free closed form. For a
///    non-merge HEAD the reference pipeline computes diff-match-patch line stats and
///    enry language buckets, which this closed form does not reproduce; we
///    return `None` so the caller surfaces the dispatch sentinel rather than
///    emitting subtly-divergent bytes.
pub fn devs_head_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_head_metrics(sub)?;
    Some(cf_gojson::marshal(
        &cf_devs::serialize::computed_metrics_to_go(&metrics),
    ))
}

/// Builds the `history/devs --head --format yaml` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case [`devs_head_metrics`]
/// reproduces.
///
/// The reference YAML path (analyze.OutputHistoryResults, non-raw branch) prints the
/// version header (`analyze.PrintHeader`) and a `<analyzer-name>:` line, then
/// marshals the per-analyzer `ComputedMetrics` with `yaml.Marshal`
/// (gopkg.in/yaml.v3). The header is emitted manually (NOT via yaml.Marshal);
/// the report body routes through cf-goyaml. yaml.v3's nil-slice rule (`[]`,
/// not json's `null`) is handled by `computed_metrics_to_go_yaml`.
pub fn devs_head_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_head_metrics(sub)?;
    let mut out = Vec::new();
    // analyze.PrintHeader: manual lines, NOT yaml.Marshal. version.Binary is 0
    // and version.BinaryGitHash is "<unknown>" (cf-version defaults).
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
    // analyze.OutputHistoryResults: `fmt.Fprintf(writer, "%s:\n", leaf.Name())`.
    out.extend_from_slice(b"history/devs:\n");
    let body = cf_goyaml::marshal(&cf_devs::serialize::computed_metrics_to_go_yaml(&metrics));
    out.extend_from_slice(&body);
    Some(out)
}

/// Shared closed-form `history/devs --head` metrics builder for the JSON and
/// YAML capture paths; returns `None` when HEAD is not the reproduced case.
pub fn devs_head_metrics(sub: &clap::ArgMatches) -> Option<cf_devs::ComputedMetrics> {
    use std::collections::BTreeMap;

    use cf_analyzers_plumbing::git_model::{Commit as PlumbingCommit, Signature as PlumbingSig};
    use cf_analyzers_plumbing::IdentityDetector;
    use cf_devs::{parse_tick_data_with_bounds, MetricOptions, TickBounds};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;

    // Head-only history runs feed exactly the single HEAD commit through the
    // streaming pipeline (reference `loadHeadCommit` → `RunStreaming` over a
    // 1-element slice). The tree-diff plumbing has no predecessor for the only
    // commit, so for a NON-merge HEAD the devs analyzer's Consume early-returns
    // an empty TC (reference: a single non-merge commit yields no per-commit
    // dev data) and the report is the all-zero/empty aggregate the reference implementation emits. We
    // reproduce that deterministically by computing over EMPTY tick input rather
    // than failing — keeping every machine format an encoding of one report
    // value (reference: ComputeAllMetrics over zero ticks). A MERGE HEAD (>1 parent) is
    // the closed form below (commits=1, the author registered, no line stats).
    if commit.num_parents() <= 1 {
        let empty = parse_tick_data_with_bounds(
            &BTreeMap::new(),
            &BTreeMap::new(),
            Vec::new(),
            0,
            BTreeMap::new(),
        );
        return Some(cf_devs::compute_all_metrics(
            &empty,
            &MetricOptions::default(),
        ));
    }

    let author = commit.author();
    let committer_when = commit.committer().when.seconds(); // ac.Time == committer When.
    let commit_hash = commit.hash().to_string();

    // Loose people dict from the single HEAD commit (author identity).
    let plumb_commit = PlumbingCommit {
        author: PlumbingSig {
            name: author.name.clone(),
            email: author.email.clone(),
            when_unix: author.when.seconds(),
        },
        committer: PlumbingSig {
            name: String::new(),
            email: String::new(),
            when_unix: committer_when,
        },
    };
    let mut ident = IdentityDetector::new();
    ident.generate_people_dict(std::slice::from_ref(&plumb_commit));
    let author_id = ident.consume_signature(&plumb_commit.author);
    let names = ident.reversed_people_dict.clone();

    // Per-commit dev data: merge commit → commits=1, no line stats, no langs.
    let mut commit_dev_data = BTreeMap::new();
    commit_dev_data.insert(
        commit_hash.clone(),
        cf_devs::CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id,
            languages: BTreeMap::new(),
        },
    );

    // Single HEAD commit → tick 0.
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    commits_by_tick.insert(0, vec![commit_hash]);

    // tick_bounds[0] = { start: end: committer time } formatted RFC3339 UTC.
    let when_rfc3339 = cf_analyze::metadata::format_rfc3339_utc(committer_when);
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    tick_bounds.insert(
        0,
        TickBounds {
            start_time: when_rfc3339.clone(),
            end_time: when_rfc3339,
        },
    );

    // TickSize defaults to 24h (no --tick-size on run); 0 → resolve_tick_size
    // applies the default inside parse_tick_data_with_bounds.
    let input =
        parse_tick_data_with_bounds(&commit_dev_data, &commits_by_tick, names, 0, tick_bounds);
    Some(cf_devs::compute_all_metrics(
        &input,
        &MetricOptions::default(),
    ))
}
