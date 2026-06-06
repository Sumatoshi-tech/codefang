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
use cf_gitlib::repository::LogOptions;
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

/// Builds the `run --analyzers history/couples --format json` bytes for either
/// the HEAD commit (`--head`) or the oldest `--limit` commits (streaming
/// Reverse walk). Returns `None` if the repository cannot be opened/walked.
pub fn couples_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let path = crate::handlers::run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let head_only = sub.get_flag("head");
    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Commit window: HEAD-only loads the single HEAD commit; streaming walks
    // oldest-first (Reverse), truncated to --limit.
    let hashes: Vec<cf_gitlib::Hash> = if head_only {
        vec![repo.head().ok()?]
    } else {
        let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
        let mut iter = repo.log(&log_opts).ok()?;
        let mut v = Vec::new();
        while limit <= 0 || (v.len() as i64) < limit {
            match iter.next_commit() {
                Some(c) => v.push(c.hash()),
                None => break,
            }
        }
        v
    };

    let opts = PathPolicyOptions::default();
    // Loose identity detection (run streaming never preloads a people dict).
    let mut identity = IdentityDetector::new();

    // Seen-files Bloom filter (merge-mode coupling dedup; Go: c.seenFiles).
    let mut seen_files =
        cf_alg_bloom::Filter::new_with_estimates(SEEN_FILES_BLOOM_EXPECTED, SEEN_FILES_BLOOM_FP)
            .ok()?;
    // Merge dedup tracker (Go: c.merges.SeenOrAdd over NumParents() > 1).
    let mut seen_merges: HashSet<cf_gitlib::Hash> = HashSet::new();

    // The aggregator grows its people slices on demand (ensure_capacity), so the
    // initial PeopleNumber is 0 (loose detection discovers authors incrementally).
    let mut agg = Aggregator::new(0);

    let mut last_commit_hash: Option<cf_gitlib::Hash> = None;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;

        // Merge dedup: a merge commit (NumParents > 1) seen twice contributes an
        // empty CommitData (Go: SeenOrAdd → return empty). With unique hashes in
        // a single window this never triggers, but mirror Go faithfully.
        let is_multi_parent = commit.num_parents() > 1;
        if is_multi_parent && !seen_merges.insert(*hash) {
            // Already seen: empty CommitData, not counted, no author attribution.
            continue;
        }

        // IsMerge (Go runner.buildAnalyzeContext): NumParents > 1, unless
        // --first-parent forces single-parent semantics.
        let merge_mode = is_multi_parent && !first_parent;

        // lastCommit is set on every (non-dedup-skipped) consume.
        last_commit_hash = Some(*hash);

        // Identity: resolve this commit's author id (loose signature).
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });
        // Go: author = Identity.AuthorID; if AuthorMissing → PeopleNumber.
        // With loose detection author_id is always a real id, never AuthorMissing.
        let author = if author_id == AUTHOR_MISSING { 0 } else { author_id as usize };

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // TreeDiffAnalyzer.shouldIncludeChange: path-policy exclusion (no blob
        // content, no language filter for the default all-languages case).
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|c| {
                let name = if c.action == ChangeAction::Delete { &c.from.name } else { &c.to.name };
                !exclude(name, None, &opts)
            })
            .collect();

        // Build this commit's CommitData (Go: Consume).
        let mut data = CommitData { commit_counted: true, ..CommitData::default() };

        // Oversized changeset: skip coupling/ownership extraction, but the commit
        // is still counted (CommitCounted = true) — Go returns &data early.
        if changes.len() > MAX_MEANINGFUL_CONTEXT {
            agg.add(author, &data);
            continue;
        }

        for change in &changes {
            process_change(change, merge_mode, author, &mut data, &mut seen_files);
        }

        agg.add(author, &data);
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

    let metrics = compute_all_metrics(&report_data);
    Some(report::to_go_json(&report::computed_metrics_to_value(&metrics)))
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

    // Merge mode. Go's source gates this on `!seenFiles.Test(name)`, but the Go
    // run BINARY's streaming pipeline does not actually suppress a merge's
    // parent(0)-diff files against the first-parent line: observed behavior is
    // that a file changed on a merged-in branch is counted both on that branch's
    // commit and again at the merge. We reproduce the observed binary behavior
    // (no merge-context suppression), which is byte/canonically identical to the
    // Go binary across the probed window. The `seen_files` Bloom is still
    // maintained (populated by non-merge commits) for the test below, which is a
    // no-op in this branch; kept for structural fidelity with the Go analyzer.
    let _ = seen_files;
    data.coupling_files.push(name.clone());
    if author != AUTHOR_MISSING_IDX {
        data.author_files.insert(name, 1);
    }
}

/// `identity.AuthorMissing` as a `usize` index (Go uses it both as the sentinel
/// id and, when present, as the `PeopleNumber` slot). Under loose detection the
/// resolved author is never this sentinel.
const AUTHOR_MISSING_IDX: usize = (1 << 18) - 1;

