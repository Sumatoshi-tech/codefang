//! `history/file-history` over the shared history walk.
//!
//! A commit whose raw tree diff exceeds `--max-changes-per-commit` is dropped
//! before the walk, as for every other history analyzer. Per commit
//! (chronological order): the filtered tree diff against the first
//! parent drives the per-path commit-hash lists (Insert RESETS the list,
//! Delete / Modify append, a rename moves the history to the new path), the
//! per-change line stats (non-merge commits only) are attributed to the
//! author, and every changed file is classified by path AND blob content
//! into the composition categories — counted once per file in the window
//! and once per file in each tick it changed in, following renames. Only
//! files present in the NEWEST commit of the window survive into the
//! report; the tick bounds come from the window's committer times.

use std::collections::BTreeMap;

use cf_composition::classifier::Classifier;
use cf_file_history::metrics::{CompositionInput, FileHistory, ReportData, TickBounds};
use cf_file_history::tc::{CategoryCounts, LineStats};
use cf_file_history::{compute_all_metrics_with_options, computed_metrics_to_go, MetricOptions};
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::{Change, ChangeAction};

use cf_units::{KIB, MIB};

use super::{
    commit_tree_changes, compute_diff_line_stats, is_merge, walk_chunks, ChunkPlan, CommitStamp,
    HistoryRun, WalkCost,
};

/// Bytes of blob content the composition classifier reads: enough for the
/// binary sniff, a shebang and a generated-code marker.
const CLASSIFY_HEADER_BYTES: usize = 8192;

/// The file-history report value (json/yaml/bin source).
#[must_use]
pub fn file_history_report_value(sub: &clap::ArgMatches) -> Option<cf_gojson::GoValue> {
    file_history_run(sub).map(|r| r.report_value)
}

/// The TYPED `ComputedMetrics` behind [`file_history_report_value`] (same
/// walk, same metrics product) for the text serializer.
#[must_use]
pub fn file_history_run_metrics(
    sub: &clap::ArgMatches,
) -> Option<cf_file_history::ComputedMetrics> {
    file_history_run(sub).map(|r| r.metrics)
}

/// One walked commit's file-history products plus the shared stamps.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileHistoryCommit {
    /// Walk position.
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// The per-commit payload.
    pub data: FileHistoryCommitData,
    /// Tick.
    pub tick: i64,
    /// Author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The per-commit file-history payload.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileHistoryCommitData {
    /// `(path, action, from_path, to_path)` per change, in change order; action
    /// is Insert 0 / Delete 1 / Modify 2. Renames carry an empty `path` and
    /// the from/to pair.
    pub path_actions: Vec<(String, i64, String, String)>,
    /// `(path, stats)` line-stat updates; empty for merges.
    pub line_stats: Vec<(String, LineStats)>,
    /// Whether the commit is a merge (line stats cleared).
    pub is_merge: bool,
    /// Per-commit composition counts (one per changed file).
    pub composition: CategoryCounts,
}

/// The full products of one file-history walk: the aggregated report value
/// plus the per-commit stream (chronological).
#[derive(Debug, PartialEq)]
pub(crate) struct FileHistoryRun {
    /// The aggregated report GoValue (json/yaml/bin source).
    pub report_value: cf_gojson::GoValue,
    /// The typed metrics `report_value` was rendered from (text source).
    pub metrics: cf_file_history::ComputedMetrics,
    /// Per-commit products, oldest-first.
    pub commits: Vec<FileHistoryCommit>,
}

/// The per-commit, order-independent products of one commit's diff.
struct FileHistoryPrepared {
    /// Size of the raw tree diff (before the change filter); a commit above
    /// `--max-changes-per-commit` is dropped and carries no other product.
    raw_change_count: usize,
    /// Filtered tree-diff changes (drives the order-sensitive hash-list
    /// maintenance in the reduce).
    changes: Vec<Change>,
    /// `(name, line-stats)` per non-merge change (empty for a merge commit).
    line_stats: Vec<(String, LineStats)>,
    /// The composition category of every change, parallel to `changes`.
    categories: Vec<cf_file_history::Category>,
}

/// Line stats of one change (`None` for binary / unreadable blobs).
fn change_line_stats(repo: &cf_gitlib::Repository, change: &Change) -> Option<(String, LineStats)> {
    match change.action {
        ChangeAction::Insert => {
            let added = CachedBlob::from_repo(repo, change.to.hash)
                .ok()?
                .count_lines()
                .ok()?;
            Some((
                change.to.name.clone(),
                LineStats {
                    added: added as i64,
                    removed: 0,
                    changed: 0,
                },
            ))
        }
        ChangeAction::Delete => {
            let removed = CachedBlob::from_repo(repo, change.from.hash)
                .ok()?
                .count_lines()
                .ok()?;
            Some((
                change.from.name.clone(),
                LineStats {
                    added: 0,
                    removed: removed as i64,
                    changed: 0,
                },
            ))
        }
        ChangeAction::Modify => {
            let blob_from = CachedBlob::from_repo(repo, change.from.hash).ok()?;
            let blob_to = CachedBlob::from_repo(repo, change.to.hash).ok()?;
            if blob_from.is_binary() || blob_to.is_binary() {
                return None;
            }
            let (added, removed, changed) = if change.from.hash == change.to.hash {
                (0, 0, 0)
            } else {
                let old_lines = blob_from.count_lines().map_or(0, |n| n as i64);
                compute_diff_line_stats(repo, change.from.hash, change.to.hash, old_lines)
            };
            Some((
                change.to.name.clone(),
                LineStats {
                    added,
                    removed,
                    changed,
                },
            ))
        }
    }
}

/// The path a change is classified and counted under: the new side, or the
/// old side of a deletion.
fn composition_path(change: &Change) -> &str {
    match change.action {
        ChangeAction::Delete => &change.from.name,
        _ => &change.to.name,
    }
}

/// Whether a Modify change carries a rename.
fn is_rename(change: &Change) -> bool {
    change.action == ChangeAction::Modify && change.from.name != change.to.name
}

/// The composition category of one change: the shared classifier over the
/// path and the leading [`CLASSIFY_HEADER_BYTES`] of the blob (the new side,
/// or the old side of a deletion; an unreadable blob classifies by path).
fn change_category(
    repo: &cf_gitlib::Repository,
    classifier: &Classifier,
    change: &Change,
) -> cf_file_history::Category {
    let hash = match change.action {
        ChangeAction::Delete => change.from.hash,
        _ => change.to.hash,
    };
    let path = composition_path(change);
    let category = match repo.lookup_blob(hash) {
        Ok(blob) => {
            let content = blob.content();
            classifier.classify(path, &content[..content.len().min(CLASSIFY_HEADER_BYTES)])
        }
        Err(_) => classifier.classify(path, &[]),
    };
    map_category(category)
}

/// The per-commit pure product: filtered changes, line stats (non-merge) and
/// the content-aware composition category of every change — nothing but the
/// raw diff size for a commit above `max_changes`.
fn file_history_commit_product(
    repo: &cf_gitlib::Repository,
    commit: &cf_gitlib::commit::Commit<'_>,
    filter: &crate::handlers::HistoryFilter,
    first_parent: bool,
    max_changes: usize,
) -> Option<FileHistoryPrepared> {
    let mut changes = commit_tree_changes(repo, commit)?;
    let raw_change_count = changes.len();
    if raw_change_count > max_changes {
        return Some(FileHistoryPrepared {
            raw_change_count,
            changes: Vec::new(),
            line_stats: Vec::new(),
            categories: Vec::new(),
        });
    }
    let classifier = Classifier::new();
    filter.retain_changes(repo, &mut changes);

    let line_stats: Vec<(String, LineStats)> =
        if super::is_merge(commit.num_parents(), first_parent) {
            Vec::new()
        } else {
            changes
                .iter()
                .filter_map(|change| change_line_stats(repo, change))
                .collect()
        };

    let categories = changes
        .iter()
        .map(|change| change_category(repo, &classifier, change))
        .collect();

    Some(FileHistoryPrepared {
        raw_change_count,
        changes,
        line_stats,
        categories,
    })
}

/// The per-commit path actions in change order (renames as from/to pairs).
fn path_actions(changes: &[Change]) -> Vec<(String, i64, String, String)> {
    let mut out = Vec::with_capacity(changes.len());
    for change in changes {
        if is_rename(change) {
            out.push((
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
        out.push((path, action, String::new(), String::new()));
    }
    out
}

/// The file-history pre-pass over `run`'s change filter and flags: the pure
/// per-commit product a walk computes in parallel. Captures only the
/// shareable pieces of the run (its repository handle is not `Sync`).
fn file_history_prepare(
    run: &HistoryRun,
) -> impl Fn(&cf_gitlib::Repository, cf_gitlib::Hash) -> Option<FileHistoryPrepared> + Sync + '_ {
    let filter = &run.filter;
    let first_parent = run.first_parent;
    let max_changes = run.max_changes;
    move |repo, hash| {
        let commit = repo.lookup_commit(hash).ok()?;
        file_history_commit_product(repo, &commit, filter, first_parent, max_changes)
    }
}

/// Maintains the per-path commit hash lists for one commit: Insert RESETS the
/// list, Delete and same-name Modify APPEND, a rename moves the prior history
/// from `from` to `to` (overwriting any prior `to` history) and appends.
fn apply_path_changes(
    files: &mut BTreeMap<String, FileHistory>,
    changes: &[Change],
    hash_hex: &str,
) {
    for change in changes {
        if is_rename(change) {
            let mut fh = files.remove(&change.from.name).unwrap_or_default();
            fh.hashes.push(hash_hex.to_string());
            files.insert(change.to.name.clone(), fh);
            continue;
        }
        match change.action {
            ChangeAction::Insert => {
                let fh = files.entry(change.to.name.clone()).or_default();
                fh.hashes = vec![hash_hex.to_string()];
            }
            ChangeAction::Delete => {
                let fh = files.entry(change.from.name.clone()).or_default();
                fh.hashes.push(hash_hex.to_string());
            }
            ChangeAction::Modify => {
                let fh = files.entry(change.to.name.clone()).or_default();
                fh.hashes.push(hash_hex.to_string());
            }
        }
    }
}

/// The category of every distinct file changed in a scope (the window or one
/// tick), following renames.
type FileCategories = BTreeMap<String, cf_file_history::Category>;

/// Records one commit's changed files in `scope`: a rename retires the old
/// path, and the path is counted once under its latest category.
fn record_changed_files(scope: &mut FileCategories, prep: &FileHistoryPrepared) {
    for (change, &category) in prep.changes.iter().zip(&prep.categories) {
        if is_rename(change) {
            scope.remove(&change.from.name);
        }
        scope.insert(composition_path(change).to_string(), category);
    }
}

/// The per-category counts of a scope's distinct files.
fn category_counts(scope: &FileCategories) -> CategoryCounts {
    let mut counts = CategoryCounts::default();
    for &category in scope.values() {
        counts.increment(category);
    }
    counts
}

/// The per-commit composition counts (one per changed file, for the ndjson
/// record).
fn commit_composition(prep: &FileHistoryPrepared) -> CategoryCounts {
    let mut counts = CategoryCounts::default();
    for &category in &prep.categories {
        counts.increment(category);
    }
    counts
}

/// The RFC3339 tick bounds from the per-tick `(earliest, latest)` committer
/// times.
fn tick_bounds(tick_when: &BTreeMap<i64, (i64, i64)>) -> TickBounds {
    tick_when
        .iter()
        .map(|(&tick, &(lo, hi))| {
            (
                tick,
                (
                    cf_analyze::metadata::format_rfc3339_utc(lo),
                    cf_analyze::metadata::format_rfc3339_utc(hi),
                ),
            )
        })
        .collect()
}

/// The file-history reduce state: the per-path histories, the composition
/// scopes, the per-tick committer-time bounds and the per-commit records.
/// Each kept commit's changes and categories are folded in window order and
/// dropped; only its record (what the per-commit formats emit) is kept.
struct FileHistoryFold {
    /// The walk's first-parent mode (a merge is an ordinary commit when set).
    first_parent: bool,
    /// path → commit hashes and per-author line stats.
    files: BTreeMap<String, FileHistory>,
    /// The category of every distinct file changed in the window.
    window_files: FileCategories,
    /// tick → the category of every distinct file changed in that tick.
    tick_files: BTreeMap<i64, FileCategories>,
    /// tick → `(earliest, latest)` committer seconds.
    tick_when: BTreeMap<i64, (i64, i64)>,
    /// The per-commit records, oldest-first.
    commits: Vec<FileHistoryCommit>,
}

impl FileHistoryFold {
    /// An empty fold for a walk in `first_parent` mode.
    const fn new(first_parent: bool) -> Self {
        Self {
            first_parent,
            files: BTreeMap::new(),
            window_files: BTreeMap::new(),
            tick_files: BTreeMap::new(),
            tick_when: BTreeMap::new(),
            commits: Vec::new(),
        }
    }

    /// Folds one kept commit, in window order: its changes update the
    /// per-path histories and the composition scopes, its line stats the
    /// author's per-file totals, then its record is kept.
    fn consume(&mut self, stamp: &CommitStamp, prep: FileHistoryPrepared) {
        let is_merge = is_merge(stamp.num_parents, self.first_parent);
        apply_path_changes(&mut self.files, &prep.changes, &stamp.hex);
        if !is_merge {
            for (name, stats) in &prep.line_stats {
                let fh = self.files.entry(name.clone()).or_default();
                let entry = fh.people.entry(stamp.author_id).or_default();
                *entry = *entry + *stats;
            }
        }
        self.tick_when
            .entry(stamp.tick)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(stamp.when);
                *hi = (*hi).max(stamp.when);
            })
            .or_insert((stamp.when, stamp.when));
        if !prep.changes.is_empty() {
            record_changed_files(&mut self.window_files, &prep);
            record_changed_files(self.tick_files.entry(stamp.tick).or_default(), &prep);
        }
        self.commits.push(FileHistoryCommit {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            data: FileHistoryCommitData {
                path_actions: path_actions(&prep.changes),
                composition: commit_composition(&prep),
                line_stats: prep.line_stats,
                is_merge,
            },
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        });
    }

    /// The finished walk: the histories of the files present in the NEWEST
    /// commit of `run`'s window, the metrics over them and the composition
    /// scopes, plus the per-commit records.
    fn finish(mut self, run: &HistoryRun) -> FileHistoryRun {
        retain_newest_files(&mut self.files, run);
        let input = ReportData { files: self.files };
        let composition = CompositionInput {
            files: category_counts(&self.window_files),
            ticks: self
                .tick_files
                .iter()
                .map(|(&tick, scope)| (tick, category_counts(scope)))
                .collect(),
            tick_bounds: tick_bounds(&self.tick_when),
        };
        let metrics =
            compute_all_metrics_with_options(&input, MetricOptions::default(), &composition);
        FileHistoryRun {
            report_value: computed_metrics_to_go(&metrics),
            metrics,
            commits: self.commits,
        }
    }
}

/// Keeps only the files present in the NEWEST commit of `run`'s window (all
/// of them when that commit or its file list cannot be read).
fn retain_newest_files(files: &mut BTreeMap<String, FileHistory>, run: &HistoryRun) {
    let Some(newest) = run.hashes.last() else {
        return;
    };
    let Ok(newest_commit) = run.repo.lookup_commit(*newest) else {
        return;
    };
    let Ok(iter) = newest_commit.files() else {
        return;
    };
    let mut present: std::collections::HashSet<String> = std::collections::HashSet::new();
    let _ = iter.for_each(|f| {
        present.insert(f.name.clone());
        Ok(())
    });
    files.retain(|name, _| present.contains(name));
}

/// The file-history walk's measured cost. Kubernetes, 31,923 commits: one
/// worker peaks at 875 MB, 24 workers at 1420 MB (23 MiB per added worker);
/// the whole window holds 122 bytes more per commit than 50-commit chunks.
const FILE_HISTORY_COST: WalkCost = WalkCost {
    worker_bytes: 32 * MIB,
    product_bytes: KIB,
};

/// Runs the `history/file-history` walk over the run's chunk plan: the
/// per-commit products are computed in parallel one chunk at a time,
/// oversized commits dropped, and folded in window order ([`walk_chunks`]).
/// `None` when the repository cannot be walked.
pub(crate) fn file_history_run(sub: &clap::ArgMatches) -> Option<FileHistoryRun> {
    let run = HistoryRun::open(sub)?;
    let plan = ChunkPlan::for_walk(
        sub,
        "history/file-history",
        run.hashes.len(),
        FILE_HISTORY_COST,
    )
    .ok()?;
    let max_changes = run.max_changes;
    let mut fold = FileHistoryFold::new(run.first_parent);
    walk_chunks(
        &run,
        plan,
        file_history_prepare(&run),
        |prep| prep.raw_change_count > max_changes,
        |stamp, prep| fold.consume(stamp, prep),
    )?;
    Some(fold.finish(&run))
}

/// Serializes one file-history payload as the per-commit record struct
/// (declaration field order; empty slices → null).
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

/// Per-commit file-history NDJSON records: every walked commit emits a line.
#[must_use]
pub fn file_history_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    let run = file_history_run(sub)?;
    Some(
        run.commits
            .iter()
            .map(|c| super::super::history_formats::NdjsonRecord {
                pos: c.pos,
                hash: c.hash.clone(),
                tick: c.tick,
                author_id: c.author_id,
                time_secs: c.when,
                tz_offset_min: c.offset_min,
                data: file_history_data_value(&c.hash, &c.data, c.author_id),
            })
            .collect(),
    )
}

/// The file-history contribution to the merged `--format timeseries` document:
/// per commit the files-touched / line / action counts, in chronological
/// order (stable by tick).
#[must_use]
pub fn file_history_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};

    let run = file_history_run(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &run.commits {
        let cd = &c.data;
        let (mut inserts, mut deletes, mut modifies) = (0i64, 0i64, 0i64);
        for (_, action, _, _) in &cd.path_actions {
            match action {
                0 => inserts += 1,
                1 => deletes += 1,
                _ => modifies += 1,
            }
        }
        let mut lines = LineStats::default();
        if !cd.is_merge {
            for (_, stats) in &cd.line_stats {
                lines = lines + *stats;
            }
        }
        let mut entry = GoMap::new_map();
        entry.insert(
            "files_touched".to_string(),
            GoValue::Int(cd.path_actions.len() as i64),
        );
        entry.insert("lines_added".to_string(), GoValue::Int(lines.added));
        entry.insert("lines_removed".to_string(), GoValue::Int(lines.removed));
        entry.insert("lines_changed".to_string(), GoValue::Int(lines.changed));
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
    commit_meta.sort_by_key(|(_, tick, _, _)| *tick);

    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "file-history",
        per_commit,
        commit_meta,
    })
}

/// Maps a [`cf_composition::category::Category`] to the file-history
/// [`cf_file_history::Category`] of the same name.
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

#[cfg(test)]
mod tests {
    use super::super::fold_testutil::{
        assert_chunk_invariant, chunk_fixture, open_run, run_matches, OVERSIZED_POS,
    };
    use super::*;
    use cf_file_history::Category;
    use cf_gitlib::changes::ChangeEntry;
    use cf_gitlib::Hash;

    fn change(action: ChangeAction, from: &str, to: &str) -> Change {
        Change {
            action,
            from: ChangeEntry {
                name: from.to_string(),
                hash: Hash::ZERO,
                size: 0,
                mode: 0,
            },
            to: ChangeEntry {
                name: to.to_string(),
                hash: Hash::ZERO,
                size: 0,
                mode: 0,
            },
        }
    }

    fn prepared(changes: Vec<Change>, categories: Vec<Category>) -> FileHistoryPrepared {
        FileHistoryPrepared {
            raw_change_count: changes.len(),
            changes,
            line_stats: Vec::new(),
            categories,
        }
    }

    /// A file edited in three commits is ONE file of the scope; a rename
    /// retires the old path; a deletion still counts the file as changed.
    #[test]
    fn changed_files_are_counted_once_and_follow_renames() {
        let mut scope: FileCategories = BTreeMap::new();
        for _ in 0..3 {
            record_changed_files(
                &mut scope,
                &prepared(
                    vec![change(ChangeAction::Modify, "a.go", "a.go")],
                    vec![Category::Source],
                ),
            );
        }
        record_changed_files(
            &mut scope,
            &prepared(
                vec![
                    change(ChangeAction::Modify, "a.go", "b.go"),
                    change(ChangeAction::Delete, "img.png", ""),
                ],
                vec![Category::Source, Category::Image],
            ),
        );
        assert_eq!(
            scope,
            BTreeMap::from([
                ("b.go".to_string(), Category::Source),
                ("img.png".to_string(), Category::Image),
            ])
        );
        let counts = category_counts(&scope);
        assert_eq!((counts.source, counts.image, counts.total()), (1, 1, 2));
    }

    /// The latest classification of a path wins (text that became binary).
    #[test]
    fn latest_category_wins() {
        let mut scope: FileCategories = BTreeMap::new();
        record_changed_files(
            &mut scope,
            &prepared(
                vec![change(ChangeAction::Insert, "", "blob.dat")],
                vec![Category::Source],
            ),
        );
        record_changed_files(
            &mut scope,
            &prepared(
                vec![change(ChangeAction::Modify, "blob.dat", "blob.dat")],
                vec![Category::Binary],
            ),
        );
        assert_eq!(category_counts(&scope).binary, 1);
        assert_eq!(category_counts(&scope).source, 0);
    }

    /// `--max-changes-per-commit 2` over `c1 (alice: a, b) ; c2 (bob: a, b,
    /// c) ; c3 (alice: a)`: Bob's three-change commit is dropped before the
    /// walk, as devs and burndown drop it.
    #[test]
    fn max_changes_per_commit_drops_the_oversized_commit() {
        use cf_gitlib::testutil::{commit_files_as, init_repo};
        const T0: i64 = 1_609_459_200;
        const DAY: i64 = 86_400;
        let test = init_repo().expect("init");
        commit_files_as(
            &test,
            "alice",
            "a@x",
            "c1",
            T0,
            &[("a", b"1\n"), ("b", b"1\n")],
        )
        .expect("c1");
        commit_files_as(
            &test,
            "bob",
            "b@x",
            "c2",
            T0 + DAY,
            &[("a", b"2\n"), ("b", b"2\n"), ("c", b"2\n")],
        )
        .expect("c2");
        commit_files_as(&test, "alice", "a@x", "c3", T0 + 2 * DAY, &[("a", b"3\n")]).expect("c3");
        let path = test.path().to_str().expect("utf-8");
        let metrics = |extra: &[&str]| {
            let argv: Vec<&str> = ["codefang", "run", "-a", "history/file-history"]
                .into_iter()
                .chain(extra.iter().copied())
                .chain([path])
                .collect();
            let matches = crate::build_codefang_command()
                .try_get_matches_from(argv)
                .expect("run args parse");
            file_history_run_metrics(matches.subcommand_matches("run").expect("run"))
                .expect("file-history report")
        };
        let commits = |m: &cf_file_history::ComputedMetrics| -> Vec<(String, i64)> {
            let mut v: Vec<(String, i64)> = m
                .file_churn
                .iter()
                .map(|f| (f.path.clone(), f.commit_count))
                .collect();
            v.sort();
            v
        };

        let all = metrics(&[]);
        assert_eq!(
            commits(&all),
            [
                ("a".to_string(), 3),
                ("b".to_string(), 2),
                ("c".to_string(), 1)
            ]
        );

        let capped = metrics(&["--max-changes-per-commit", "2"]);
        assert_eq!(
            commits(&capped),
            [("a".to_string(), 2), ("b".to_string(), 1)]
        );
        assert_eq!(capped.aggregate.total_contributors, 1, "{capped:?}");
    }

    #[test]
    fn tick_bounds_are_rfc3339_utc() {
        let bounds = tick_bounds(&BTreeMap::from([(3, (1_609_459_200, 1_609_462_800))]));
        assert_eq!(
            bounds[&3],
            (
                "2021-01-01T00:00:00Z".to_string(),
                "2021-01-01T01:00:00Z".to_string()
            )
        );
    }

    /// The analyzer id the fold tests run.
    const ID: &str = "history/file-history";

    /// The file-history walk folds per chunk: folding the chunk fixture
    /// (a rename and a delete after the files' first commits, a merge, a
    /// file first written by the dropped oversized commit) in chunks of every
    /// size finishes to the one-pass walk, and that is what the walk returns
    /// for any `--chunk-size`.
    #[test]
    fn the_walk_folds_in_chunks_to_the_one_pass_value() {
        let test = chunk_fixture();
        let run = open_run(&test, ID, &[]);
        let max_changes = run.max_changes;
        let whole = assert_chunk_invariant(
            &run,
            file_history_prepare(&run),
            |prep| prep.raw_change_count > max_changes,
            || FileHistoryFold::new(run.first_parent),
            FileHistoryFold::consume,
            |fold, _| fold.finish(&run),
        );
        let positions: Vec<usize> = whole.commits.iter().map(|c| c.pos).collect();
        assert_eq!(positions.len(), run.hashes.len() - 1, "{positions:?}");
        assert!(!positions.contains(&OVERSIZED_POS), "{positions:?}");
        for size in ["1", "4"] {
            let chunked = file_history_run(&run_matches(&test, ID, &["--chunk-size", size]));
            assert_eq!(chunked.as_ref(), Some(&whole), "--chunk-size {size}");
        }
    }
}
