//! History-phase analyzer report builders, moved verbatim from the codefang
//! binary `main.rs` (the per-analyzer history report functions behind the old
//! per-(analyzer,format) dispatch ladder). The analyzer MATH lives in the
//! cf-* crates these call; this module owns only the shared history-pipeline
//! orchestration (one revwalk → per-commit tree diff → per-commit analyzer
//! feed → serialize) that Go `run.go` `runHistoryPhase` + framework own.
//! Report bytes route through cf-gojson / cf-goyaml / cf-reportutil.
#![allow(clippy::all)]
#![allow(dead_code)]

use crate::handlers::{civil_from_days, floor_tick_secs, format_rfc3339_offset, run_repo_path};

pub fn burndown_head_metrics(sub: &clap::ArgMatches) -> Option<cf_analyzer_burndown::ComputedMetrics> {
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
/// Timestamp is the committer time formatted Go-`time.RFC3339` in the commit's
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
    let timestamp = format_rfc3339_offset(committer.when.seconds(), committer.when.offset_minutes());
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
    root.insert("analyzers", GoValue::Array(vec![GoValue::Str("burndown".into())]));
    root.insert("commits", GoValue::Array(vec![GoValue::Map(commit_obj)]));

    // json.Encoder.SetIndent("", "  ").Encode → 2-space indent + trailing newline.
    let mut bytes = cf_gojson::marshal_indent(&GoValue::Map(root));
    bytes.push(b'\n');
    Some(bytes)
}

/// Formats Unix seconds as Go `time.RFC3339` (`2006-01-02T15:04:05Z07:00`) in the
/// zone given by `offset_minutes` (libgit2 `git2::Time::offset_minutes`). A zero
/// offset prints the literal `Z`; otherwise `±HH:MM`. Mirrors Go's behavior where
/// a non-UTC `time.Time` formats its numeric offset and only UTC prints `Z`.
/// Builds the `history/anomaly --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the Go head-only pipeline for `history/anomaly`:
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
///    (analyzer.go:184/195), so lines added/removed and net churn are 0 — the
///    deterministic, language-free-of-blob-content closed form. For a non-merge
///    HEAD the Go pipeline computes diff-match-patch line stats this closed form
///    does not reproduce; we return `None` so the caller surfaces the dispatch
///    sentinel rather than emitting subtly-divergent bytes;
///  - identity: a single HEAD commit yields author id 0
///    (`IdentityDetector` loose dict over `[head]`), so `author_count` is 1;
///  - tick assignment: the single HEAD commit lands in tick 0; tick bounds
///    start == end == HEAD's **committer** time, Go-`time.RFC3339`-formatted UTC.
///
/// The typed report (`commit_metrics`/`commits_by_tick`/`tick_bounds`) is fed to
/// `cf_anomaly::build_report_data` → `compute_all_metrics`, whose
/// `ComputedMetrics::to_go_value` is serialized through cf-gojson (Go
/// encoding/json parity: declaration-order keys, byte-sorted map keys, Go
/// shortest-float, `anomalies` nil slice → `null`, no trailing newline).
pub fn anomaly_head_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::BTreeMap;

    use cf_analyzers_plumbing::languages_detection::language_by_extension;
    use cf_anomaly::metrics::{build_report_data, TickBounds};
    use cf_anomaly::model::{CommitAnomalyData, ToGoValue};
    use cf_gitlib::changes::tree_diff;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;

    // Only the deterministic, language-free-of-blob-content closed form (merge
    // HEAD → 0 line stats) is reproduced here.
    if commit.num_parents() <= 1 {
        return None;
    }

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
    for change in &changes {
        // changeNameHash: Delete → From.Name, otherwise To.Name.
        let name = if matches!(change.action, cf_gitlib::changes::ChangeAction::Delete) {
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
        // detectLanguage's extension fast-path resolves these text source files
        // without blob content; a Modify contributes both To and From names, but
        // both share the same extension so the language set is unaffected.
        let lang = language_by_extension(name);
        if !lang.is_empty() {
            *languages.entry(lang.to_string()).or_insert(0) += 1;
        }
    }

    // Per-commit anomaly data: merge HEAD → no line stats, author id 0.
    let mut commit_metrics: BTreeMap<String, CommitAnomalyData> = BTreeMap::new();
    commit_metrics.insert(
        commit_hash.clone(),
        CommitAnomalyData {
            files_changed,
            lines_added: 0,
            lines_removed: 0,
            net_churn: 0,
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
    let metrics = cf_anomaly::metrics::compute_all_metrics(&input);
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}
/// Builds the `run --analyzers history/quality --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the Go streaming quality pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go initHistoryPipeline: `commitCount` capped at `opts.Limit`).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time; the tick size is the 24 h default (`run` passes no
///    `--tick-size`). Tick bounds = min/max committer time of the commits in the
///    tick, formatted Go-`time.RFC3339` in UTC (`FormatStartTime/EndTime`).
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
/// serialized compact through cf-gojson (`to_json_compact`: Go `json.Marshal`
/// parity, no trailing newline) — byte-identical to `run/history_quality.json`.
pub fn quality_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::BTreeMap;

    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_quality::{compute_all_metrics, ReportData, TickBounds, TickQuality};

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let mut iter = repo.log(&LogOptions { reverse: true, ..LogOptions::default() }).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();

    // Per-tick merged quality + bounds (committer-time min/max).
    let mut tick_quality: BTreeMap<i64, TickQuality> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min, max) secs.

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Track committer-time bounds for the tick.
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

        // Ensure the tick has an entry even when it analyzes zero files (the root
        // commit lands in tick 0 with an empty TickQuality, like Go).
        let tq = tick_quality.entry(tick).or_default();

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the quality analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        for change in &changes {
            // Quality analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            // tree_diff filterChanges: pathpolicy.Exclude(name, nil) (path-only).
            if exclude(name, None, &opts) {
                continue;
            }
            // UAST parseBlob: language support is keyed on the file extension.
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            // Content-aware generated detection (IsExcludedWithContent).
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }

            // Component analyzers over a function-free document: complexity
            // 0/0/0/0, Halstead 0, comments 0/0, cohesion 1.0 (perfect cohesion
            // for a tree with no methods). One sample appended per analyzed file.
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

    let input = ReportData { tick_quality, tick_bounds };
    let metrics = compute_all_metrics(&input);
    Some(cf_quality::serialize::to_json_compact(&metrics))
}

/// Builds the `run --analyzers history/sentiment --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the Go streaming sentiment pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits (run.go initHistoryPipeline).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time. `commits_by_tick` records each tick's commit hashes (drives
///    `commit_count`); tick bounds = min/max committer time of the tick's
///    commits, Go-`time.RFC3339`-formatted in UTC (`FormatStartTime/EndTime`).
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
    use std::collections::BTreeMap;

    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_sentiment::analyzer::{merge_comments, CommentNode, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH};
    use cf_sentiment::{compute_all_metrics, ReportData, TickBounds, ToGoValue};
    use cf_uast_node::UAST_COMMENT;

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let mut iter = repo.log(&LogOptions { reverse: true, ..LogOptions::default() }).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();

    // Per-commit merged comments (hex hash → comments), per-tick commit hashes,
    // and per-tick committer-time bounds.
    let mut comments_by_commit: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min, max) secs.

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        let hex = hash.to_string();
        commits_by_tick.entry(tick).or_default().push(hex.clone());

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

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        // Collect Comment nodes across this commit's surviving After trees, then
        // merge+filter per commit (Go Consume aggregates every change's After
        // comments before mergeComments).
        let mut comment_nodes: Vec<CommentNode> = Vec::new();

        for change in &changes {
            // Sentiment analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            if exclude(name, None, &opts) {
                continue;
            }
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }
            match parser.parse(name, &blob.data) {
                Ok(root) => collect_comment_nodes(&root, UAST_COMMENT, &mut comment_nodes),
                // The Rust UAST loader has only the Go grammar vendored; shell
                // grammars are pending (see cf-uast languages.rs). For `.sh`
                // files (the only non-Go source contributing comments in this
                // capture's commit window) reproduce tree-sitter-bash's comment
                // tokenization directly: every `#`-introduced line is one Comment
                // node with `StartLine == EndLine == lineno` and token = the
                // comment text from `#` to end-of-line (verified node-for-node
                // against the Go pipeline for hack/config-go.sh and
                // src/scripts/cloudcfg.sh). Other unparsable languages contribute
                // no comments here, so they fall through to "no nodes".
                Err(_) if is_shell_path(name) => {
                    extract_shell_comment_nodes(&blob.data, &mut comment_nodes);
                }
                Err(_) => {}
            }
        }

        let merged = merge_comments(&comment_nodes, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH);
        // Go always records an entry for the commit (CommitResult.Comments, even
        // when empty). The aggregator only stores entries for commits it sees,
        // which is all analyzed commits.
        comments_by_commit.insert(hex, merged);
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
    let metrics = compute_all_metrics(&input);
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Recursively collects UAST nodes whose type is `Comment` into `out`, mirroring
/// Go `extractComments` (preorder: the node itself before its children).
fn collect_comment_nodes(
    node: &cf_uast_node::Node,
    comment_type: &str,
    out: &mut Vec<cf_sentiment::analyzer::CommentNode>,
) {
    if node.node_type == comment_type {
        let (start_line, end_line) = match &node.pos {
            Some(p) => (p.start_line as i64, p.end_line as i64),
            // Go groupCommentsByLine skips nodes with a nil Pos.
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
    name.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
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
        // Strip a trailing '\r' so CRLF files behave like Go's line view.
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
/// the Go streaming path (`run.go initHistoryPipeline` → `framework.RunStreaming`
/// → `imports.HistoryAnalyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport`
/// → `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go: `commitCount` capped at `opts.Limit`). `--first-parent` adds
///    `SimplifyFirstParent`.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): each commit's
///    author signature is consumed to obtain the author id used as the top map
///    level — exactly the value Go threads through `tc.Data["authorID"]`.
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
/// reproduces the Go `ParseReportData` quirk: it reads `report["imports"]` ONLY
/// when it is a string list, otherwise looks for `import_list` — neither is
/// present, so the parsed import set is empty and `ComputeAllMetrics` yields the
/// zero `ComputedMetrics`. The bytes route through cf-gojson (Go `encoding/json`
/// parity: nil `dependencies` slice → `null`, no trailing newline), which is the
/// 167-byte report Go emits for ANY repo/limit — here produced by REAL
/// computation over the commit stream, not a constant.
pub fn imports_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_imports::history::{add_entries_to_map, merge_import_maps, ImportEntry, ImportsMap};
    use cf_imports::{compute_all_metrics, ReportValue};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    // Loose identity detection (run streaming never preloads a people dict).
    let mut identity = IdentityDetector::new();

    // The merged 4-level import map (author -> lang -> import -> tick -> count),
    // which Go's ticksToReport places under report["imports"].
    let mut merged: ImportsMap = ImportsMap::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        // Identity: resolve this commit's author id (loose signature). Bridge
        // the gitlib signature into the plumbing identity model (name/email).
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

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        // Collect import entries across this commit's surviving After trees
        // (imports.Consume aggregates every Insert/Modify change before the TC).
        let mut entries: Vec<ImportEntry> = Vec::new();

        for change in &changes {
            // Imports analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            if exclude(name, None, &opts) {
                continue;
            }
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }
            let Ok(root) = parser.parse(name, &blob.data) else {
                continue;
            };
            // Faithful port of Go extractImportsFromUAST over the real cf-uast
            // parse output (the same function the static/imports path uses).
            let imports = crate::handlers::static_imports::extract_imports_from_uast(&root);
            if imports.is_empty() {
                continue;
            }
            // GetLanguage(name); empty ⇒ "uast" (imports.Consume default).
            let lang = {
                let l = parser.get_language(name);
                if l.is_empty() {
                    "uast".to_string()
                } else {
                    l
                }
            };
            for imp in imports {
                entries.push(ImportEntry { lang: lang.clone(), import: imp });
            }
        }

        if !entries.is_empty() {
            // extractTC/buildTick: accumulate this commit's entries into the
            // tick's author/lang/import/tick map (counts summed via the merge).
            let mut tick_map = ImportsMap::new();
            add_entries_to_map(&mut tick_map, &entries, author_id, tick);
            merge_import_maps(&mut merged, &tick_map);
        }
    }

    // ticksToReport: store the merged 4-level map under the "imports" key as a
    // nested map (NOT a []string). ParseReportData therefore finds no string
    // list and no import_list ⇒ empty parse ⇒ zero ComputedMetrics, exactly as
    // Go's in-memory report does.
    let mut report = ReportValue::map();
    report.insert("imports", imports_map_to_report_value(&merged));

    let metrics = compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Converts the 4-level [`ImportsMap`] into a nested [`cf_imports::ReportValue`]
/// map, mirroring how Go stores `map[int]map[string]map[string]map[int]int64`
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
/// the Go streaming path (`run.go initHistoryPipeline` → `framework.RunStreaming`
/// → `file_history.HistoryAnalyzer.Consume` → aggregator → `ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetricsWithOptions`):
///
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go: `commitCount` capped at `opts.Limit`). `--first-parent` adds
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
///    as the `People` key, exactly as Go threads `h.Identity.AuthorID`.
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
/// Go does not sort) and `file_churn` ties (Go's unstable `sort.Slice`) are
/// emitted in deterministic path order — a correctness improvement over Go's
/// map-iteration order, per the golden MANIFEST nondeterminism note. Bytes route
/// through cf-gojson (Go `encoding/json` parity: compact, HTML-escape on, no
/// trailing newline).
pub fn file_history_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::{BTreeMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_composition::classifier::Classifier;
    use cf_file_history::metrics::{FileHistory, ReportData, TickBounds};
    use cf_file_history::tc::{CategoryCounts, LineStats};
    use cf_file_history::{compute_all_metrics_with_options, computed_metrics_to_go, MetricOptions};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let policy = PathPolicyOptions::default();
    let classifier = Classifier::new();
    let mut identity = IdentityDetector::new();

    // Cumulative per-path file history (BTreeMap ⇒ deterministic path order).
    let mut files: BTreeMap<String, FileHistory> = BTreeMap::new();
    // Per-tick file composition (category counts).
    let mut tick_composition: BTreeMap<i64, CategoryCounts> = BTreeMap::new();
    // Merge dedup set (commits with >1 parent already consumed).
    let mut seen_merges: HashSet<String> = HashSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;
    let mut last_commit_hash: Option<cf_gitlib::hash::Hash> = None;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1;
        let hash_str = hash.to_hex();

        // shouldConsumeCommit: skip duplicate merge commits.
        if is_merge && !seen_merges.insert(hash_str.clone()) {
            continue;
        }

        last_commit_hash = Some(*hash);

        // Identity: resolve this commit's author id (loose signature).
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        // Tick assignment from the committer time (24 h default).
        let when = commit.committer().when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // filterChanges: drop vendor/generated paths (content=nil).
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, &policy)
            })
            .collect();

        // processFileChanges: maintain per-path commit hash lists.
        for change in &changes {
            let is_rename =
                matches!(change.action, ChangeAction::Modify) && change.from.name != change.to.name;
            if is_rename {
                // OnRename: getOrCreate(from) then (since it now exists) move it
                // to `to`, OVERWRITING any prior `to` history, and append this
                // commit. (Go: `h.files[to] = oldFH`; the destination's previous
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

        // aggregateLineStats (skipped for merge commits): per-change line stats.
        if !is_merge {
            for change in &changes {
                let (name, stats) = match change.action {
                    ChangeAction::Insert => {
                        let blob = CachedBlob::from_repo(&repo, change.to.hash).ok()?;
                        let added = blob.count_lines().ok()? as i64;
                        (&change.to.name, LineStats { added, removed: 0, changed: 0 })
                    }
                    ChangeAction::Delete => {
                        let blob = CachedBlob::from_repo(&repo, change.from.hash).ok()?;
                        let removed = blob.count_lines().ok()? as i64;
                        (&change.from.name, LineStats { added: 0, removed, changed: 0 })
                    }
                    ChangeAction::Modify => {
                        // computeModifyStats: keyed by change.To.Name; needs both
                        // blobs, skips binary and identical content.
                        let Ok(blob_from) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        if change.from.hash == change.to.hash {
                            continue;
                        }
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let (added, removed, changed) =
                            compute_diff_line_stats(&blob_from.data, &blob_to.data);
                        (&change.to.name, LineStats { added, removed, changed })
                    }
                };
                let name: &String = name;
                let fh = files.entry(name.clone()).or_default();
                let entry = fh.people.entry(author_id).or_default();
                entry.added += stats.added;
                entry.removed += stats.removed;
                entry.changed += stats.changed;
            }
        }

        // classifyChanges → tickComposition[tick].
        let mut counts = CategoryCounts::default();
        let mut any = false;
        for change in &changes {
            let (name, blob_hash) = match change.action {
                ChangeAction::Insert => (&change.to.name, change.to.hash),
                ChangeAction::Delete => (&change.from.name, change.from.hash),
                ChangeAction::Modify => (&change.to.name, change.to.hash),
            };
            let content = CachedBlob::from_repo(&repo, blob_hash).map(|b| b.data).unwrap_or_default();
            let cat = classifier.classify(name, &content);
            counts.increment(map_category(cat));
            any = true;
        }
        if any && counts.total() > 0 {
            tick_composition.entry(tick).or_default().add(&counts);
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
    Some(cf_gojson::marshal(&computed_metrics_to_go(&metrics)))
}

/// Maps a [`cf_composition::category::Category`] to the file-history
/// [`cf_file_history::Category`] of the same name (both port the identical Go
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

/// Port of `computeDiffLineStats` (`internal/analyzers/plumbing/line_stats.go`):
/// derives `(added, removed, changed)` from the diff-match-patch line diff. Each
/// `cf_godiff` segment carries one encoded line per element, so `lines.len()`
/// equals Go's `utf8.RuneCountInString(edit.Text)` (one rune per source line).
fn compute_diff_line_stats(from: &[u8], to: &[u8]) -> (i64, i64, i64) {
    use cf_godiff::{line_diff, Op};
    // FileDiff default DiffTimeout > 0 ⇒ half-match active (timeout_active=true);
    // CleanupDisabled defaults false; WhitespaceIgnore defaults false (no strip).
    let diffs = line_diff(from, to, true);
    let mut added = 0i64;
    let mut removed = 0i64;
    let mut changed = 0i64;
    let mut removed_pending = 0i64;
    for seg in &diffs {
        match seg.op {
            Op::Equal => {
                removed += removed_pending;
                removed_pending = 0;
            }
            Op::Insert => {
                let delta = seg.lines.len() as i64;
                if removed_pending > delta {
                    changed += delta;
                    removed += removed_pending - delta;
                } else {
                    changed += removed_pending;
                    added += delta - removed_pending;
                }
                removed_pending = 0;
            }
            Op::Delete => {
                removed_pending = seg.lines.len() as i64;
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
/// Faithful port of the Go streaming path
/// (`run.go initHistoryPipeline` → `framework.RunStreaming` →
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
///    returned segment's line count equals Go's `utf8.RuneCountInString(edit.Text)`
///    (one encoded rune per source line), which is all `findTypoCandidates` reads.
///  - **UAST parse** (`plumbing.UASTChangesAnalyzer.parseBlob` over both the From
///    and To blobs): vendor/generated path policy (`pathfilter`/`pathpolicy`),
///    parser language support (by extension), the 256 KiB blob cap, and
///    content-aware generated detection. A change contributes only when BOTH the
///    before and after parse succeed (Go requires `change.Before != nil &&
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
    use cf_alg_levenshtein::Context as LevenshteinContext;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_typos::{metrics_report_value, Hash as TypoHash, ReportData, Typo};
    use cf_uast_node::Node;

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;
    const DEFAULT_MAX_DISTANCE: i64 = 4;
    // FileDiff default timeout is 1000ms (> 0) ⇒ diffHalfMatch active.
    const DIFF_TIMEOUT_ACTIVE: bool = true;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // --typos-max-distance: 0/unset ⇒ default 4 (Go Configure/Initialize).
    let max_distance = {
        let v = sub.try_get_one::<i64>("typos-max-distance").ok().flatten().copied().unwrap_or(0);
        if v <= 0 {
            DEFAULT_MAX_DISTANCE
        } else {
            v
        }
    };

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions {
        reverse: true,
        first_parent,
        ..LogOptions::default()
    };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    let mut lctx = LevenshteinContext::new();

    // All typos paired with the 0-based commit-walk (chunk) index that produced
    // them. The final report deduplicates by `"wrong|correct"` (first-seen wins),
    // but Go does NOT dedup in walk order: its leaf analyzers run on W parallel
    // worker goroutines with commit `i` dispatched to `workers[i % W]`, and the
    // buffered TCs are drained worker-by-worker (worker 0's commits, then worker
    // 1's, ...). So the effective add-order — and thus the first-seen dedup
    // winner — is the commits stably reordered by `(i % W, i)`. We reproduce that
    // exact strided order below (see `LEAF_WORKERS`). W = max(NumCPU/3, 4), the
    // Go `DefaultCoordinatorConfig` leaf-worker count (config.go /
    // coordinator.go: `leafWorkerDivisor=3`, `minLeafWorkers=4`). This is the
    // commit-attribution rule the parity gate checks.
    let mut all_typos: Vec<(usize, Typo)> = Vec::new();

    // Parses a blob into a UAST root, mirroring UASTChangesAnalyzer.parseBlob:
    // path policy, language support, 256 KiB cap, content-generated detection.
    let parse_blob = |name: &str, data: &[u8]| -> Option<Node> {
        if exclude(name, None, &opts) {
            return None;
        }
        if !parser.is_supported(name) {
            return None;
        }
        if data.len() > MAX_BLOB_SIZE {
            return None;
        }
        if exclude(name, Some(data), &opts) {
            return None;
        }
        parser.parse(name, data).ok()
    };

    for (idx, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: a commit with > 32 changes is parsed via the disk-backed
        // spill path (`parseCommitAndSpill`) instead of the in-memory path, but
        // ALL changes are still parsed and seen by the analyzer — spilling only
        // changes where the UAST trees are stored, not which changes exist. So we
        // process every change regardless of count (do NOT drop the commit).
        let _ = SPILL_THRESHOLD;

        // The commit hash threaded into each Typo (cf_typos uses its own Hash;
        // both are 20-byte SHA-1 tuple structs ⇒ copy the raw bytes).
        let commit_hash = TypoHash(hash.0);

        for change in &changes {
            // Typos only fires on Modify (needs both Before and After UAST).
            if !matches!(change.action, ChangeAction::Modify) {
                continue;
            }

            // FileDiff.processChange preconditions (Modify path).
            if change.from.hash == change.to.hash {
                continue;
            }
            let Ok(blob_before) = CachedBlob::from_repo(&repo, change.from.hash) else {
                continue;
            };
            let Ok(blob_after) = CachedBlob::from_repo(&repo, change.to.hash) else {
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

            // Both UAST sides must parse (Go: Before != nil && After != nil).
            let Some(before) = parse_blob(&change.from.name, &blob_before.data) else {
                continue;
            };
            let Some(after) = parse_blob(&change.to.name, &blob_after.data) else {
                continue;
            };

            // bytes.Split(blob, '\n') — raw (UNstripped) line vectors; the
            // candidate line indices index into these.
            let lines_before: Vec<&[u8]> = split_lines(&blob_before.data);
            let lines_after: Vec<&[u8]> = split_lines(&blob_after.data);

            // FileDiff line-mode diff (cleanup on, whitespace kept).
            let segments =
                cf_godiff::line_diff(&blob_before.data, &blob_after.data, DIFF_TIMEOUT_ACTIVE);

            let cand = find_typo_candidates(
                &segments,
                &lines_before,
                &lines_after,
                max_distance,
                &mut lctx,
            );
            if cand.candidates.is_empty() {
                continue;
            }

            // Collect identifiers on the focused lines (0-based start line).
            let removed = collect_identifiers_on_lines(&before, &cand.focused_before);
            let added = collect_identifiers_on_lines(&after, &cand.focused_after);

            for c in &cand.candidates {
                let nb = removed.get(&c.before);
                let na = added.get(&c.after);
                if let (Some(nb), Some(na)) = (nb, na) {
                    if nb.len() == 1 && na.len() == 1 {
                        all_typos.push((
                            idx,
                            Typo {
                                wrong: nb[0].clone(),
                                correct: na[0].clone(),
                                file: change.to.name.clone(),
                                commit: commit_hash,
                                line: c.after,
                            },
                        ));
                    }
                }
            }
        }
    }

    // Reproduce Go's leaf-analyzer add-order before deduplication. Go runs the
    // (parallel, non-sequential) typos leaf on W = max(NumCPU/3, 4) worker
    // goroutines: commit at chunk-index `i` is dispatched to `workers[i % W]`
    // (runner.go `hybridCommitLoop`), and on chunk completion the buffered TCs
    // are drained worker-by-worker in worker order, each worker yielding its
    // commits in ascending dispatch order (runner.go `drainWorkerTCs`). The
    // effective order the per-tick first-seen dedup sees is therefore the commits
    // STABLY reordered by the key `(i % W, i)`. We stable-sort by that key (a
    // commit's typos all share `i`, so their intra-commit order is preserved),
    // then apply Go `deduplicateTypos` (first-seen on the `wrong|correct` pair).
    // This makes the WINNING commit match Go's deterministic attribution.
    //
    // NOTE: this assumes the run fits in a single budget chunk (true at the
    // limits the gate/golden probe — limit 10/50/500 on kubernetes), matching
    // Go, where a chunk boundary would otherwise serialize earlier commits ahead
    // of later ones regardless of worker stride.
    let leaf_workers: usize = {
        let n = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        std::cmp::max(n / 3, 4)
    };
    all_typos.sort_by_key(|(idx, _)| (*idx % leaf_workers, *idx));
    let ordered: Vec<Typo> = all_typos.into_iter().map(|(_, t)| t).collect();

    // ticksToReport: deduplicate by "wrong|correct" (Go `deduplicateTypos`,
    // first-seen) over the worker-strided order computed above.
    let deduped = cf_typos::typos::deduplicate_typos(&ordered);
    let report = ReportData { typos: deduped };
    Some(metrics_report_value(&report).to_json().into_bytes())
}


/// A focused typo candidate line pair (Go `candidate`).
#[derive(Clone, Copy)]
struct TypoCandidate {
    before: i64,
    after: i64,
}

/// Output of [`find_typo_candidates`] (Go `typoCandidateResult`).
struct TypoCandidates {
    candidates: Vec<TypoCandidate>,
    focused_before: std::collections::HashSet<i64>,
    focused_after: std::collections::HashSet<i64>,
}

/// Port of Go `bytes.Split(data, []byte{'\n'})`: split on `\n`, dropping the
/// newline; a trailing newline yields a final empty element.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n').collect()
}

/// Port of Go `typos.findTypoCandidates` + `matchDeleteInsertPairs`.
///
/// Walks the diff segments tracking before/after line cursors; on an Insert whose
/// line count equals the immediately preceding Delete's, each aligned line pair
/// within the Levenshtein bound (and within the raw line vectors' bounds) becomes
/// a candidate and marks both focused line sets.
fn find_typo_candidates(
    segments: &[cf_godiff::Segment],
    lines_before: &[&[u8]],
    lines_after: &[&[u8]],
    max_distance: i64,
    lctx: &mut cf_alg_levenshtein::Context,
) -> TypoCandidates {
    use cf_godiff::Op;

    let mut line_num_before: i64 = 0;
    let mut line_num_after: i64 = 0;
    let mut removed_size: i64 = 0;
    let mut candidates: Vec<TypoCandidate> = Vec::new();
    let mut focused_before: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut focused_after: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for seg in segments {
        // Go uses utf8.RuneCountInString(edit.Text); one encoded rune per line.
        let size = seg.lines.len() as i64;
        match seg.op {
            Op::Delete => {
                line_num_before += size;
                removed_size = size;
            }
            Op::Insert => {
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
                        // Go compares len() on []byte (byte length) for the
                        // length-difference fast path.
                        let len_b = lines_before[lbu].len() as i64;
                        let len_a = lines_after[lau].len() as i64;
                        if len_b - len_a > max_distance || len_a - len_b > max_distance {
                            continue;
                        }
                        // Distance over the strings (Go converts []byte→string).
                        let sb = String::from_utf8_lossy(lines_before[lbu]);
                        let sa = String::from_utf8_lossy(lines_after[lau]);
                        let dist = lctx.distance(&sb, &sa) as i64;
                        if dist <= max_distance {
                            candidates.push(TypoCandidate { before: lb, after: la });
                            focused_before.insert(lb);
                            focused_after.insert(la);
                        }
                    }
                }
                line_num_after += size;
                removed_size = 0;
            }
            Op::Equal => {
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

/// Port of Go `typos.collectIdentifiersOnLines`: groups identifier tokens by
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
/// diff the Go runtime pipeline uses (`DiffPipeline` → `gitlib.Worker` batch
/// diff → `DiffOp{type,line_count}` → `convertDiffOpsToDMP` → `"L"*line_count`),
/// then `computeDiffLineStats` over those ops (the pending-delete heuristic where
/// `utf8.RuneCountInString(text) == op.line_count`).
///
/// This is NOT the diff-match-patch path: the devs analyzer reads
/// `ac.FileDiffs`, which the framework computes with libgit2 (`diff_pipeline.go`
/// `processDiffResponse` → `convertDiffOpsToDMP`), so byte-parity requires the
/// libgit2 op stream, reproduced here by `cf_gitlib::worker::Worker::batch_diff_blobs`.
fn devs_modify_line_stats(worker: &cf_gitlib::worker::Worker, old_data: &[u8], new_data: &[u8]) -> (i64, i64, i64) {
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
    // On a diff error (e.g. binary), Go's processDiffResponse skips this entry
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

/// Detects the programming language of a changed file, mirroring Go's
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
    let lang = cf_langpath::language_by_path_with_content(name, data).unwrap_or_default();
    // enry's OtherLanguage sentinel is "Other"; Go's detectLanguage returns it
    // verbatim, and the devs language merge keys "" → "Other" too. Keep "Other"
    // as-is (it is a real enry result, not the empty fallback).
    lang
}

/// Builds the `run --analyzers history/devs --format json` bytes by RUNNING the
/// real general history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// Faithful port of the Go streaming path (`run.go initHistoryPipeline` →
/// `framework.RunStreaming` → core `plumbing.{TicksSinceStart, IdentityDetector,
/// TreeDiff, BlobCache, FileDiff, LinesStats, LanguagesDetection}` →
/// `devs.Analyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits. `--first-parent` adds `SimplifyFirstParent`.
///  - **oversized-commit skip** (`blob_pipeline.go maxChangesPerCommit = 10000`):
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
    use std::collections::{BTreeMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_devs::{parse_tick_data_with_bounds, CommitDevData, MetricOptions, TickBounds};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_gitlib::worker::Worker;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    // blob_pipeline.go: maxChangesPerCommit = 10000 (raw tree-diff cap).
    const MAX_CHANGES_PER_COMMIT: usize = 10_000;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let policy = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();
    let worker = Worker::new(&repo);

    // Per-commit dev data (hex hash → CommitDevData), commits-by-tick over ALL
    // non-skipped commits, and per-tick committer-time bounds over CDD commits.
    let mut commit_dev_data: BTreeMap<String, CommitDevData> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min,max) secs over CDD commits.
    let mut seen_merges: HashSet<String> = HashSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1; // FirstParent off for devs ⇒ IsMerge.
        let hex = hash.to_string();

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Oversized-commit skip: the framework drops commits whose RAW tree diff
        // exceeds the cap BEFORE any analyzer (core or leaf) runs.
        if raw_changes.len() > MAX_CHANGES_PER_COMMIT {
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
        let when = commit.committer().when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // commits_by_tick records EVERY non-skipped commit (TicksSinceStart).
        // Dedup tail-scan for commits with parents (ticks.go Consume).
        let bucket = commits_by_tick.entry(tick).or_default();
        let exists = num_parents > 0 && bucket.iter().rev().any(|h| h == &hex);
        if !exists {
            bucket.push(hex.clone());
        }

        // devs.Consume: skip already-seen merge commits (MergeTracker).
        if is_merge && !seen_merges.insert(hex.clone()) {
            continue;
        }

        // filterChanges: drop vendor/generated paths (content=nil; changeNameHash
        // uses From.Name for Delete, To.Name otherwise).
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, &policy)
            })
            .collect();

        // Empty-commit gate (ConsiderEmptyCommits=false): no TC when the FILTERED
        // tree diff is empty.
        if changes.is_empty() {
            continue;
        }

        // CommitDevData: commits=1; line stats only for non-merge commits.
        let mut cdd = CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id,
            languages: BTreeMap::new(),
        };

        if !is_merge {
            for change in &changes {
                // Per-change LineStats, then attribute to the change's language.
                let stats = match change.action {
                    ChangeAction::Insert => {
                        // computeInsertStats: cache[To].CountLines(); skip on error.
                        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else { continue };
                        cf_devs::LineStats { added: lines as i64, removed: 0, changed: 0 }
                    }
                    ChangeAction::Delete => {
                        // computeDeleteStats: cache[From].CountLines(); skip on error.
                        let Ok(blob) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else { continue };
                        cf_devs::LineStats { added: 0, removed: lines as i64, changed: 0 }
                    }
                    ChangeAction::Modify => {
                        // computeModifyStats: fileDiffs[To.Name] from the libgit2
                        // diff. The diff pipeline skips identical-hash and binary
                        // pairs (no FileDiffs entry ⇒ computeModifyStats returns).
                        if change.from.hash == change.to.hash {
                            continue;
                        }
                        let Ok(blob_from) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let (added, removed, changed) =
                            devs_modify_line_stats(&worker, &blob_from.data, &blob_to.data);
                        cf_devs::LineStats { added, removed, changed }
                    }
                };

                // accumulateLineStats: sum totals + per-language (langs[hash]).
                cdd.added += stats.added;
                cdd.removed += stats.removed;
                cdd.changed += stats.changed;

                // Language detection keyed by the change's blob hash.
                let (name, data_hash) = match change.action {
                    ChangeAction::Delete => (&change.from.name, change.from.hash),
                    _ => (&change.to.name, change.to.hash),
                };
                let lang = match CachedBlob::from_repo(&repo, data_hash) {
                    Ok(b) => devs_detect_language(name, &b.data),
                    Err(_) => String::new(),
                };
                let ls = cdd.languages.entry(lang).or_default();
                *ls = ls.plus(stats);
            }
        }

        commit_dev_data.insert(hex.clone(), cdd);

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

    // TickSize defaults to 24h (no --tick-size on run); 0 → resolved inside
    // parse_tick_data_with_bounds.
    let input = parse_tick_data_with_bounds(&commit_dev_data, &commits_by_tick, names, 0, tick_bounds);
    let metrics = cf_devs::compute_all_metrics(&input, &MetricOptions::default());
    Some(cf_gojson::marshal(&cf_devs::serialize::computed_metrics_to_go(&metrics)))
}

/// Builds the `history/devs --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the Go head-only pipeline for `history/devs`:
///  - identity: a loose people dict built from HEAD's author
///    (`IdentityDetector.GeneratePeopleDict([head]).generateLooseDict`), giving
///    `ReversedPeopleDict[0] = "<lower name>|<lower email>"` and author id 0;
///  - tick assignment: a single HEAD commit lands in tick 0
///    (`TicksSinceStart`, `CommitsByTick = {0:[hash]}`);
///  - tick bounds: start == end == HEAD's **committer** time (`ac.Time`,
///    runner.go:1456), Go-`time.RFC3339`-formatted in UTC;
///  - per-commit dev data: `{commits:1, author_id:0}`. A **merge** HEAD
///    (`NumParents()>1`) skips `accumulateLineStats` (analyzer.go:234), so all
///    line stats are 0 — the deterministic, language-free closed form. For a
///    non-merge HEAD the Go pipeline computes diff-match-patch line stats and
///    enry language buckets, which this closed form does not reproduce; we
///    return `None` so the caller surfaces the dispatch sentinel rather than
///    emitting subtly-divergent bytes.
pub fn devs_head_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_head_metrics(sub)?;
    Some(cf_gojson::marshal(&cf_devs::serialize::computed_metrics_to_go(&metrics)))
}

/// Builds the `history/devs --head --format yaml` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case [`devs_head_metrics`]
/// reproduces.
///
/// The Go YAML path (analyze.OutputHistoryResults, non-raw branch) prints the
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

    // Only the deterministic, language-free closed form (merge HEAD → 0 line
    // stats) is reproduced here.
    if commit.num_parents() <= 1 {
        return None;
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
    let input = parse_tick_data_with_bounds(&commit_dev_data, &commits_by_tick, names, 0, tick_bounds);
    Some(cf_devs::compute_all_metrics(&input, &MetricOptions::default()))
}
