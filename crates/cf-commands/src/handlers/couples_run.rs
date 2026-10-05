//! `history/couples` over the shared history walk.
//!
//! A commit whose raw tree diff exceeds `--max-changes-per-commit` is dropped
//! before the walk, as for every other history analyzer. Per commit
//! (chronological order): the filtered tree diff against the first parent
//! feeds `process_change` — the coupling context (files co-changed in
//! the commit), the per-file author touch, rename pairs — and the aggregator
//! sums the file co-occurrence and people matrices, applying each rename as
//! it happens. Every change is counted ONCE over the walk: a merge commit
//! contributes only the files no earlier commit of the window changed
//! (`seen_files`, ONE exact set over the whole walk) and records no
//! deletions. A commit above the changeset cap contributes no coupling
//! context but still owns every file it touched. The report is reduced to
//! the files present in the NEWEST commit of the window, with their line
//! counts.
//!
//! Output bytes route through `cf-gojson`; never `serde_json`.

use std::collections::{BTreeMap, HashSet};

use cf_analyzers_plumbing::author_missing_id;
use cf_couples::aggregator::Aggregator;
use cf_couples::tc::{CommitData, RenamePair};
use cf_couples::{compute_all_metrics, report};
use cf_gitlib::changes::ChangeAction;

use cf_units::{KIB, MIB};

use crate::handlers::history::{
    commit_tree_changes, is_merge, walk_chunks, ChunkPlan, CommitStamp, HistoryRun, StampSummary,
    Stamper, WalkCost,
};

/// The changeset cap: a commit touching more files feeds no coupling context.
const MAX_MEANINGFUL_CONTEXT: usize = cf_couples::COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE;

/// Builds the `history/couples` report value (the single value behind
/// json/yaml/bin) over the commit window. Returns `None` if the repository
/// cannot be walked.
#[must_use]
pub fn couples_run_value(sub: &clap::ArgMatches) -> Option<cf_gojson::GoValue> {
    couples_run(sub).map(|r| r.report_value)
}

/// The TYPED `ComputedMetrics` behind [`couples_run_value`] (same walk, same
/// `compute_all_metrics` product) for the text serializer.
#[must_use]
pub fn couples_run_metrics(sub: &clap::ArgMatches) -> Option<cf_couples::ComputedMetrics> {
    couples_run(sub).map(|r| r.metrics)
}

/// One walked commit's couples payload plus the shared stamps.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CouplesCommit {
    /// Position in the commit window (the ndjson merge key; a dropped
    /// oversized commit keeps its slot).
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// The per-commit payload.
    pub data: CommitData,
    /// Tick.
    pub tick: i64,
    /// Author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The full products of one couples walk: the aggregated report value plus the
/// per-commit stream (one entry per walked commit, walk order).
pub(crate) struct CouplesRun {
    /// The aggregated `ComputedMetrics` GoValue (json/yaml/bin source).
    pub report_value: cf_gojson::GoValue,
    /// The typed metrics `report_value` was rendered from (text source).
    pub metrics: cf_couples::ComputedMetrics,
    /// Per-commit products, walk order.
    pub commits: Vec<CouplesCommit>,
}

/// The data the `history/couples` plot sections consume.
pub struct CouplesPlotData {
    /// `file_coupling` records (co-change-descending, the report's list).
    pub file_coupling: Vec<cf_couples::FileCouplingData>,
    /// `ownership` records.
    pub ownership: Vec<cf_couples::FileOwnershipData>,
}

/// Builds the `history/couples` plot-section data over the shared couples walk.
#[must_use]
pub fn couples_plot_data(sub: &clap::ArgMatches) -> Option<CouplesPlotData> {
    let run = couples_run(sub)?;
    Some(CouplesPlotData {
        file_coupling: run.metrics.file_coupling,
        ownership: run.metrics.file_ownership,
    })
}

/// The per-commit, order-independent product: the raw tree-diff size and the
/// filtered changes (none for a commit above `max_changes`, which the walk
/// drops).
struct CouplesPrepared {
    /// Size of the raw tree diff (before the change filter), for the
    /// `--max-changes-per-commit` gate.
    raw_change_count: usize,
    /// The filtered tree-diff changes, in diff order.
    changes: Vec<cf_gitlib::changes::Change>,
}

/// The per-commit pure product of `hash`: its raw tree-diff size and its
/// filtered changes (none above `max_changes`). `None` on a fatal git error.
fn couples_commit_product(
    repo: &cf_gitlib::Repository,
    hash: cf_gitlib::Hash,
    filter: &crate::handlers::HistoryFilter,
    max_changes: usize,
) -> Option<CouplesPrepared> {
    let commit = repo.lookup_commit(hash).ok()?;
    let mut changes = commit_tree_changes(repo, &commit)?;
    let raw_change_count = changes.len();
    if raw_change_count > max_changes {
        changes.clear();
    } else {
        filter.retain_changes(repo, &mut changes);
    }
    Some(CouplesPrepared {
        raw_change_count,
        changes,
    })
}

/// The couples pre-pass over `run`'s change filter and flags: the pure
/// per-commit product a walk computes in parallel. Captures only the
/// shareable pieces of the run (its repository handle is not `Sync`).
fn couples_prepare(
    run: &HistoryRun,
) -> impl Fn(&cf_gitlib::Repository, cf_gitlib::Hash) -> Option<CouplesPrepared> + Sync + '_ {
    let filter = &run.filter;
    let max_changes = run.max_changes;
    move |repo, hash| couples_commit_product(repo, hash, filter, max_changes)
}

/// The couples reduce state: the co-change and ownership aggregates, the
/// files changed so far (ONE exact set over the whole walk, so the output is
/// a pure function of the repository and the flags) and the per-commit
/// records. Each kept commit's changes are folded in window order and
/// dropped; only its record (what the per-commit formats emit) is kept.
struct CouplesFold {
    /// The walk's first-parent mode (a merge is an ordinary commit when set).
    first_parent: bool,
    /// The people slot an unmatched author is counted under.
    unmatched_slot: usize,
    /// Every file changed by a commit folded so far.
    seen_files: HashSet<String>,
    /// The co-change and per-developer touch sums.
    agg: Aggregator,
    /// The per-commit records, walk order.
    commits: Vec<CouplesCommit>,
}

impl CouplesFold {
    /// An empty fold for `run`. The unmatched slot is fixed before the
    /// walk: only a run with a loaded `--people-dict` stamps an unmatched
    /// author, and that dictionary never changes while commits are stamped,
    /// so an unstamped identity state already names the slot the finished
    /// one does.
    fn new(run: &HistoryRun) -> Self {
        Self {
            first_parent: run.first_parent,
            unmatched_slot: Stamper::new(run).finish().unmatched_slot,
            seen_files: HashSet::new(),
            // The aggregator grows its people slices on demand, so the
            // initial PeopleNumber is 0 (identities are discovered
            // incrementally).
            agg: Aggregator::new(0),
            commits: Vec::new(),
        }
    }

    /// Folds one kept commit, in window order: its changes become the
    /// commit's coupling context, author touches and renames, which the
    /// aggregator sums before the record is kept.
    fn consume(&mut self, stamp: &CommitStamp, prep: CouplesPrepared) {
        let merge_mode = is_merge(stamp.num_parents, self.first_parent);
        let author = if stamp.author_id == author_missing_id() {
            self.unmatched_slot
        } else {
            stamp.author_id as usize
        };

        let mut data = CommitData {
            commit_counted: true,
            ..CommitData::default()
        };
        let couple = prep.changes.len() <= MAX_MEANINGFUL_CONTEXT;
        for change in &prep.changes {
            process_change(change, merge_mode, couple, &mut data, &mut self.seen_files);
        }

        self.agg.add(author, &data);
        self.commits.push(CouplesCommit {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            data,
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        });
    }

    /// The finished walk: the report reduced to the files present in the
    /// NEWEST commit of `run`'s window, with their line counts and the
    /// developer names of `summary`, plus the per-commit records.
    fn finish(self, summary: StampSummary, run: &HistoryRun) -> CouplesRun {
        let (current_files, files_lines) =
            collect_current_and_lines(&run.repo, run.hashes.last().copied());
        let mut report_data = self.agg.build_report(current_files.as_ref(), &files_lines);
        report_data.reversed_people_dict = summary.names;
        let metrics = compute_all_metrics(&report_data);
        CouplesRun {
            report_value: report::computed_metrics_to_value(&metrics),
            metrics,
            commits: self.commits,
        }
    }
}

/// The couples walk's measured cost. Kubernetes, 31,923 commits: one worker
/// peaks at 1345 MB (the co-change state), 24 workers at 1591 MB (10 MiB per
/// added worker); the whole window and 50-commit chunks peak alike.
const COUPLES_COST: WalkCost = WalkCost {
    worker_bytes: 16 * MIB,
    product_bytes: KIB,
};

/// Runs the `history/couples` walk over the run's chunk plan: the
/// per-commit changes are computed in parallel one chunk at a time,
/// oversized commits dropped, and folded in window order ([`walk_chunks`]).
/// `None` when the repository cannot be walked.
pub(crate) fn couples_run(sub: &clap::ArgMatches) -> Option<CouplesRun> {
    let run = HistoryRun::open(sub)?;
    let plan = ChunkPlan::for_walk(sub, "history/couples", run.hashes.len(), COUPLES_COST).ok()?;
    let max_changes = run.max_changes;
    let mut fold = CouplesFold::new(&run);
    let summary = walk_chunks(
        &run,
        plan,
        couples_prepare(&run),
        |prep| prep.raw_change_count > max_changes,
        |stamp, prep| fold.consume(stamp, prep),
    )?;
    Some(fold.finish(summary, &run))
}

/// Per-commit couples NDJSON records: every walked commit emits a line;
/// `data` is `{CouplingFiles, AuthorFiles, Renames, CommitCounted}`.
#[must_use]
pub fn couples_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<crate::handlers::history_formats::NdjsonRecord>> {
    use cf_gojson::value::{GoMap, GoValue};

    let run = couples_run(sub)?;
    let mut records = Vec::new();
    for c in &run.commits {
        let cd = &c.data;
        let mut data = GoMap::new_struct();
        data.insert(
            "CouplingFiles".to_string(),
            GoValue::Array(
                cd.coupling_files
                    .iter()
                    .map(|f| GoValue::Str(f.clone()))
                    .collect(),
            ),
        );
        let mut authors = GoMap::new_map();
        for (f, n) in &cd.author_files {
            authors.insert(f.clone(), GoValue::Int(*n));
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
        data.insert(
            "CommitCounted".to_string(),
            GoValue::Bool(cd.commit_counted),
        );
        records.push(crate::handlers::history_formats::NdjsonRecord {
            pos: c.pos,
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

/// The couples contribution to the merged `--format timeseries` document: per
/// commit `{"files_touched": <files the author touched>, "author_id": id}`
/// under the commit's tick.
#[must_use]
pub fn couples_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<crate::handlers::history_formats::TimeSeriesContribution> {
    use cf_gojson::value::{GoMap, GoValue};

    let run = couples_run(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &run.commits {
        let mut entry = GoMap::new_map();
        entry.insert(
            "files_touched".to_string(),
            GoValue::Int(c.data.author_files.len() as i64),
        );
        entry.insert("author_id".to_string(), GoValue::Int(c.author_id));
        per_commit.push((c.hash.clone(), GoValue::Map(entry)));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
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

/// Collects the current-file set and per-file newline counts from the commit
/// at `newest`. Returns `(None, empty)` when there is no commit or its tree
/// cannot be read (the report then keeps every accumulated file with zero
/// lines).
fn collect_current_and_lines(
    repo: &cf_gitlib::Repository,
    newest: Option<cf_gitlib::Hash>,
) -> (Option<HashSet<String>>, BTreeMap<String, i32>) {
    let Some(lc) = newest else {
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
        let n = match f.contents() {
            Ok(data) => data.iter().filter(|&&b| b == b'\n').count() as i32,
            Err(_) => 0,
        };
        lines.insert(f.name.clone(), n);
        set.insert(f.name);
    }
    (Some(set), lines)
}

/// Per-commit change processing: renames, the author touch and (when
/// `couple`, i.e. the commit is within the changeset cap) the coupling
/// context. A merge records no deletions and skips every file already
/// changed earlier in the walk — that change was counted at its own commit.
fn process_change(
    change: &cf_gitlib::changes::Change,
    merge_mode: bool,
    couple: bool,
    data: &mut CommitData,
    seen_files: &mut HashSet<String>,
) {
    if change.action == ChangeAction::Delete {
        if merge_mode {
            return;
        }
        let name = change.from.name.clone();
        seen_files.insert(name.clone());
        data.author_files.insert(name, 1);
        return;
    }

    let name = change.to.name.clone();
    if change.action == ChangeAction::Modify && change.from.name != name {
        data.renames.push(RenamePair {
            from_name: change.from.name.clone(),
            to_name: name.clone(),
        });
    }
    if merge_mode && seen_files.contains(&name) {
        return;
    }
    seen_files.insert(name.clone());
    if couple {
        data.coupling_files.push(name.clone());
    }
    data.author_files.insert(name, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::history::fold_testutil::{
        assert_chunk_invariant, chunk_fixture, open_run, run_matches, OVERSIZED_POS,
    };
    use cf_gitlib::changes::{Change, ChangeEntry};
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

    fn run(changes: &[Change], merge_mode: bool, couple: bool, seen: &[&str]) -> CommitData {
        let mut seen_files: HashSet<String> = seen.iter().map(|s| (*s).to_string()).collect();
        let mut data = CommitData::default();
        for c in changes {
            process_change(c, merge_mode, couple, &mut data, &mut seen_files);
        }
        data
    }

    #[test]
    fn a_regular_commit_couples_and_owns_every_non_deleted_file() {
        let data = run(
            &[
                change(ChangeAction::Insert, "", "a"),
                change(ChangeAction::Modify, "b", "b"),
                change(ChangeAction::Delete, "c", ""),
            ],
            false,
            true,
            &[],
        );
        assert_eq!(data.coupling_files, vec!["a", "b"]);
        assert_eq!(
            data.author_files.keys().collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        assert!(data.renames.is_empty());
    }

    #[test]
    fn a_rename_is_recorded_and_counted_on_the_new_path() {
        let data = run(&[change(ChangeAction::Modify, "b", "d")], false, true, &[]);
        assert_eq!(
            data.renames,
            vec![RenamePair {
                from_name: "b".into(),
                to_name: "d".into()
            }]
        );
        assert_eq!(data.coupling_files, vec!["d"]);
        assert_eq!(data.author_files.keys().collect::<Vec<_>>(), vec!["d"]);
    }

    #[test]
    fn a_merge_skips_files_already_changed_in_the_walk_and_deletions() {
        let data = run(
            &[
                change(ChangeAction::Insert, "", "b"),
                change(ChangeAction::Modify, "c", "c"),
                change(ChangeAction::Insert, "", "fresh"),
                change(ChangeAction::Delete, "gone", ""),
            ],
            true,
            true,
            &["b", "c"],
        );
        assert_eq!(data.coupling_files, vec!["fresh"]);
        assert_eq!(data.author_files.keys().collect::<Vec<_>>(), vec!["fresh"]);
    }

    /// `--max-changes-per-commit 2` over `c1 (alice: a, b) ; c2 (bob: a, b,
    /// c) ; c3 (alice: a)`: Bob's three-change commit is dropped before the
    /// walk, as devs and burndown drop it — no co-change, no ownership, no
    /// developer.
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
            let argv: Vec<&str> = ["codefang", "run", "-a", "history/couples"]
                .into_iter()
                .chain(extra.iter().copied())
                .chain([path])
                .collect();
            let matches = crate::build_codefang_command()
                .try_get_matches_from(argv)
                .expect("run args parse");
            couples_run_metrics(matches.subcommand_matches("run").expect("run"))
                .expect("couples report")
        };
        let co_changes = |m: &cf_couples::ComputedMetrics| -> Vec<(String, String, i64)> {
            m.file_coupling
                .iter()
                .map(|c| (c.file1.clone(), c.file2.clone(), c.co_changes))
                .collect()
        };

        let all = metrics(&[]);
        assert!(
            co_changes(&all).contains(&("a".into(), "b".into(), 2)),
            "{all:?}"
        );
        assert_eq!(all.aggregate.total_developers, 2, "{all:?}");

        let capped = metrics(&["--max-changes-per-commit", "2"]);
        assert_eq!(co_changes(&capped), [("a".to_string(), "b".to_string(), 1)]);
        assert_eq!(capped.aggregate.total_developers, 1, "{capped:?}");
        let owned: Vec<&str> = capped
            .file_ownership
            .iter()
            .map(|o| o.file.as_str())
            .collect();
        assert!(
            !owned.contains(&"c"),
            "c was only touched by the dropped commit: {owned:?}"
        );
    }

    #[test]
    fn an_oversized_commit_owns_without_coupling() {
        let data = run(
            &[
                change(ChangeAction::Insert, "", "a"),
                change(ChangeAction::Modify, "b", "d"),
            ],
            false,
            false,
            &[],
        );
        assert!(data.coupling_files.is_empty());
        assert_eq!(data.author_files.keys().collect::<Vec<_>>(), vec!["a", "d"]);
        assert_eq!(data.renames.len(), 1);
    }

    /// The analyzer id the fold tests run.
    const ID: &str = "history/couples";

    /// The comparable value of a finished couples walk: the report value and
    /// the per-commit records.
    fn comparable(run: CouplesRun) -> (cf_gojson::GoValue, Vec<CouplesCommit>) {
        (run.report_value, run.commits)
    }

    /// Folds the chunk fixture opened with `extra` in chunks of every size,
    /// asserts it finishes to the one-pass walk (the oversized commit
    /// dropped) and that the walk returns it for any `--chunk-size`.
    fn assert_couples_chunk_invariant(extra: &[&str]) {
        let test = chunk_fixture();
        let run = open_run(&test, ID, extra);
        let max_changes = run.max_changes;
        let whole = assert_chunk_invariant(
            &run,
            couples_prepare(&run),
            |prep| prep.raw_change_count > max_changes,
            || CouplesFold::new(&run),
            CouplesFold::consume,
            |fold, summary| comparable(fold.finish(summary, &run)),
        );
        let positions: Vec<usize> = whole.1.iter().map(|c| c.pos).collect();
        assert_eq!(positions.len(), run.hashes.len() - 1, "{positions:?}");
        assert!(!positions.contains(&OVERSIZED_POS), "{positions:?}");
        for size in ["1", "4"] {
            let args: Vec<&str> = extra
                .iter()
                .copied()
                .chain(["--chunk-size", size])
                .collect();
            let chunked = couples_run(&run_matches(&test, ID, &args)).map(comparable);
            assert_eq!(chunked.as_ref(), Some(&whole), "--chunk-size {size}");
        }
    }

    /// The couples walk folds per chunk: the rename, the delete and the
    /// merge's already-seen files carry across every chunk boundary.
    #[test]
    fn the_walk_folds_in_chunks_to_the_one_pass_value() {
        assert_couples_chunk_invariant(&[]);
    }

    /// With a `--people-dict` that misses Bob, his touches are counted under
    /// the dictionary's `<unmatched>` slot in every chunking, so Alice
    /// couples with `<unmatched>` — the slot is fixed before the first
    /// commit is folded.
    #[test]
    fn an_unmatched_author_folds_into_the_unmatched_slot_in_every_chunking() {
        let dict = tempfile::NamedTempFile::new().expect("dict file");
        std::fs::write(dict.path(), "Alice|alice@example.com|alice@elsewhere.org\n")
            .expect("write dict");
        let dict_path = dict.path().to_str().expect("utf-8 path");
        assert_couples_chunk_invariant(&["--people-dict", dict_path]);

        let test = chunk_fixture();
        let sub = run_matches(
            &test,
            ID,
            &["--people-dict", dict_path, "--chunk-size", "1"],
        );
        let metrics = couples_run_metrics(&sub).expect("couples report");
        let pairs: Vec<(&str, &str)> = metrics
            .developer_coupling
            .iter()
            .map(|d| (d.developer1.as_str(), d.developer2.as_str()))
            .collect();
        assert_eq!(pairs, [("Alice", "<unmatched>")], "{metrics:?}");
    }
}
