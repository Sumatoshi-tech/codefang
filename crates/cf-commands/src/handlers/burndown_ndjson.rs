//! `history/burndown` over the shared history walk.
//!
//! Burndown is the one analyzer whose walk ALWAYS follows the first parent
//! ([`HistoryRun::open_first_parent`]): its line-survival treaps track the
//! main line of development, so a merge is consumed as the ordinary
//! single-parent change set it introduces on that line — the lines a merge
//! deletes on the main line are debited at the merge's own tick, like any
//! other commit's deletions.
//!
//! The walk:
//! - **window base** — in a `--limit` / `--since` / `--head` window the
//!   tracked state is seeded from the base tree (the first in-window commit's
//!   parent) at tick 0 before the first commit is applied, so a commit's
//!   `LinesAdded` / `LinesRemoved` are the lines it really changed; the base
//!   lines form band 0 of the global curve and have no author;
//! - **cap** — commits whose raw tree diff exceeds `--max-changes-per-commit`
//!   are dropped before the stamps are assigned (the same rule devs applies),
//!   so burndown and devs see the same commit set; a dropped commit's edits
//!   are never applied, and the next commit touching an affected file
//!   re-inserts it at that commit's tick;
//! - **ticks and authors** — the shared [`HistoryRun::stamps`] assignment;
//!   each line's segment value packs `(person, tick)` through a
//!   [`ValuePacking`] (ticks only without `--burndown-people`);
//! - **deltas** — each tracked path is a [`File`] treap whose updater reports
//!   `(current, previous, delta)` into one per-commit sink; the reduce turns
//!   the sink into the commit's global / per-file / per-developer sparse
//!   deltas and the interaction matrix cells. Every re-insertion of a path
//!   first debits whatever was tracked under it, so no line is counted twice
//!   or leaked.
//!
//! All output bytes route through `cf-gojson`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cf_analyzer_burndown::{
    compute_metrics, ComputedMetrics, DenseHistory, Grid, MetricsInput, SparseHistory,
};
use cf_burndown_core::{File, Updater, ValuePacking};
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::{Change, ChangeAction};
use cf_godiff::Op;
use cf_gojson::value::{GoMap, GoValue, MapOrigin};

use crate::handlers::history::{
    commit_tree_changes, fold_planned, parallel_prepare, ChunkPlan, CommitStamp, HistoryRun,
    Stamper, WalkCost,
};
use crate::handlers::{format_rfc3339_offset, HistoryFilter};

/// Burndown band granularity in ticks (the `--granularity` default).
const DEFAULT_GRANULARITY: i64 = 30;
/// Burndown sampling period in ticks (the `--sampling` default).
const DEFAULT_SAMPLING: i64 = 30;
/// Nanoseconds per hour.
const NANOS_PER_HOUR: i64 = 3_600 * 1_000_000_000;
/// Nanoseconds per second.
const NANOS_PER_SECOND: i64 = 1_000_000_000;
/// The tick the lines of a window's base tree are credited to.
const BASE_TICK: i64 = 0;

/// The burndown flags: the sampling grid and the two tracking switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurndownFlags {
    /// `--granularity`: ticks per age band.
    pub granularity: i64,
    /// `--sampling`: ticks per sample.
    pub sampling: i64,
    /// `--burndown-files`: per-file survival.
    pub track_files: bool,
    /// `--burndown-people`: per-developer survival and interactions.
    pub track_people: bool,
}

impl BurndownFlags {
    /// Reads the flags. The preflight (`validate_history_run`) rejects a
    /// non-positive `--granularity` / `--sampling`; an absent flag means the
    /// default (30 ticks).
    fn from_matches(sub: &clap::ArgMatches) -> Self {
        let int = |name: &str, default: i64| {
            sub.try_get_one::<i64>(name)
                .ok()
                .flatten()
                .copied()
                .filter(|v| *v > 0)
                .unwrap_or(default)
        };
        let flag = |name: &str| {
            sub.try_get_one::<bool>(name)
                .ok()
                .flatten()
                .copied()
                .unwrap_or(false)
        };
        Self {
            granularity: int("granularity", DEFAULT_GRANULARITY),
            sampling: int("sampling", DEFAULT_SAMPLING),
            track_files: flag("burndown-files"),
            track_people: flag("burndown-people"),
        }
    }
}

/// One `(current, previous, delta)` report of a tracked file's treap, tagged
/// with the file it came from. `current` / `previous` are packed values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Report {
    file: usize,
    current: i64,
    previous: i64,
    delta: i64,
}

/// The sink every treap updater pushes into; drained once per commit.
type ReportSink = Rc<RefCell<Vec<Report>>>;

/// A tracked path: its stable file id and its line-survival treap.
struct TrackedFile {
    id: usize,
    file: File,
}

/// The persistent line-survival state of one walk.
struct Tracker {
    packing: ValuePacking,
    sink: ReportSink,
    files: HashMap<String, TrackedFile>,
    /// The current path of every file id ever tracked (renames update it).
    paths: Vec<String>,
}

impl Tracker {
    fn new(packing: ValuePacking) -> Self {
        Self {
            packing,
            sink: Rc::new(RefCell::new(Vec::new())),
            files: HashMap::new(),
            paths: Vec::new(),
        }
    }

    /// The updater of file `id`: every report lands in the shared sink.
    fn updater(&self, id: usize) -> Updater {
        let sink = Rc::clone(&self.sink);
        Box::new(move |current, previous, delta| {
            sink.borrow_mut().push(Report {
                file: id,
                current,
                previous,
                delta,
            });
        })
    }

    /// Takes the reports accumulated since the last drain.
    fn drain(&self) -> Vec<Report> {
        std::mem::take(&mut *self.sink.borrow_mut())
    }

    /// The tracked line count of `path`, `None` when untracked.
    fn len_of(&self, path: &str) -> Option<i64> {
        self.files.get(path).map(|t| t.file.len())
    }

    /// Debits every tracked line of `path` at `value` and stops tracking it.
    fn remove(&mut self, path: &str, value: i64) {
        if let Some(mut tracked) = self.files.remove(path) {
            let len = tracked.file.len();
            if len > 0 {
                tracked.file.update(value, 0, 0, len);
            }
        }
    }

    /// Tracks `path` as `lines` lines written at `value`, debiting whatever
    /// was tracked under the path first.
    fn insert(&mut self, path: &str, lines: i64, value: i64) {
        self.remove(path, value);
        let id = self.paths.len();
        self.paths.push(path.to_string());
        let file = File::new(value, lines, vec![self.updater(id)]);
        self.files
            .insert(path.to_string(), TrackedFile { id, file });
    }

    /// Moves the tracked file `from` to `to` (a rename), debiting whatever
    /// was tracked under `to`.
    fn rename(&mut self, from: &str, to: &str, value: i64) {
        if from == to {
            return;
        }
        if let Some(tracked) = self.files.remove(from) {
            self.remove(to, value);
            self.paths[tracked.id] = to.to_string();
            self.files.insert(to.to_string(), tracked);
        }
    }

    /// Drives the treap of `path` through a line diff (see [`apply_diffs`]).
    fn apply_diffs(&mut self, path: &str, diffs: &[(Op, i64)], value: i64) {
        if let Some(tracked) = self.files.get_mut(path) {
            apply_diffs(&mut tracked.file, diffs, value);
        }
    }

    /// The live lines of every tracked file per known author
    /// (`path -> person -> lines`); lines with no author are left out.
    fn ownership(&self) -> BTreeMap<String, BTreeMap<i64, i64>> {
        let unknown = self.packing.unknown_person();
        self.files
            .iter()
            .map(|(path, tracked)| {
                let mut by_person: BTreeMap<i64, i64> = BTreeMap::new();
                for seg in tracked.file.segments() {
                    let (person, _) = self.packing.unpack(i64::from(seg.value));
                    if person != unknown {
                        *by_person.entry(person).or_default() += seg.length;
                    }
                }
                (path.clone(), by_person)
            })
            .collect()
    }
}

/// One commit's (or the window base's) line deltas.
#[derive(Debug, Default, PartialEq)]
struct CommitDeltas {
    /// `curTick -> prevTick -> delta`.
    global: SparseHistory,
    /// Per file id (`--burndown-files`).
    files: BTreeMap<usize, SparseHistory>,
    /// Per author of the affected lines (`--burndown-people`).
    people: BTreeMap<i64, SparseHistory>,
    /// `author -> modifier -> lines of the author the modifier deleted or
    /// replaced` (`--burndown-people`).
    matrix: BTreeMap<i64, BTreeMap<i64, i64>>,
    /// Lines written by the commit.
    added: i64,
    /// Lines deleted or replaced by the commit.
    removed: i64,
}

/// Adds one cell to a sparse history.
fn add_cell(history: &mut SparseHistory, current: i64, previous: i64, delta: i64) {
    *history
        .entry(current)
        .or_default()
        .entry(previous)
        .or_default() += delta;
}

/// Reduces one commit's reports into its deltas. `added` sums every positive
/// report, `removed` every negative one, so a line added and another removed
/// in the same tick never cancel.
fn reduce_reports(reports: &[Report], packing: ValuePacking, flags: BurndownFlags) -> CommitDeltas {
    let unknown = packing.unknown_person();
    let mut d = CommitDeltas::default();
    for r in reports {
        let (cur_person, cur_tick) = packing.unpack(r.current);
        let (prev_person, prev_tick) = packing.unpack(r.previous);
        add_cell(&mut d.global, cur_tick, prev_tick, r.delta);
        if r.delta > 0 {
            d.added += r.delta;
        } else {
            d.removed -= r.delta;
        }
        if flags.track_files {
            add_cell(
                d.files.entry(r.file).or_default(),
                cur_tick,
                prev_tick,
                r.delta,
            );
        }
        if flags.track_people && prev_person != unknown {
            add_cell(
                d.people.entry(prev_person).or_default(),
                cur_tick,
                prev_tick,
                r.delta,
            );
            if r.delta < 0 {
                *d.matrix
                    .entry(prev_person)
                    .or_default()
                    .entry(cur_person)
                    .or_default() -= r.delta;
            }
        }
    }
    d
}

/// One walked commit's burndown products plus the shared stamps.
struct BurndownCommit {
    /// The shared identity/tick stamp.
    stamp: CommitStamp,
    /// This commit's deltas.
    deltas: CommitDeltas,
}

/// The full products of one burndown walk.
struct BurndownWalk {
    /// Per-commit products, oldest-first.
    commits: Vec<BurndownCommit>,
    /// The window base tree's lines (tick 0, no author); empty for a walk
    /// starting at a root commit.
    base: CommitDeltas,
    /// The tick of the newest kept commit.
    last_tick: i64,
    /// The run's tick size in hours.
    tick_size_hours: i64,
    /// The repository path as given.
    path: String,
    /// The burndown flags.
    flags: BurndownFlags,
    /// The final path of every file id.
    paths: Vec<String>,
    /// Final line ownership per path (both trackings on).
    ownership: BTreeMap<String, BTreeMap<i64, i64>>,
    /// Display name per person id (`--burndown-people`).
    names: Vec<String>,
}

/// The person slot of a stamp: the people-dict `<unmatched>` slot for an
/// unmatched author, else the author id.
fn person_slot(author_id: i64, unmatched: i64) -> i64 {
    if author_id == i64::from(cf_analyzers_plumbing::AUTHOR_MISSING) {
        unmatched
    } else {
        author_id
    }
}

/// The display name of every person id (the same resolver devs uses).
fn display_names(names: &[String]) -> Vec<String> {
    (0..names.len() as i64)
        .map(|id| cf_devs::dev_name_and_email(id, names).0)
        .collect()
}

/// Seeds the tracker with the window's base tree — the first in-window
/// commit's parent — at tick 0 with no author. A window starting at a root
/// commit has no base.
fn seed_base_tree(run: &HistoryRun, tracker: &mut Tracker, value: i64) -> Option<()> {
    let Some(first) = run.hashes.first() else {
        return Some(());
    };
    let commit = run.repo.lookup_commit(*first).ok()?;
    if commit.num_parents() == 0 {
        return Some(());
    }
    let tree = commit.parent(0).ok()?.tree().ok()?;
    let changes = cf_gitlib::changes::initial_tree_changes(&run.repo, Some(&tree)).ok()?;
    apply_changes(&run.repo, &run.filter, tracker, &changes, value);
    Some(())
}

/// Commits with at least this many raw changes keep their tree diff from the
/// raw-count pre-pass for the main pass: rename detection on such commits
/// (vendoring, mass moves) dominates the walk, and computing it twice put
/// each of them on the critical path twice.
const REUSED_DIFF_CHANGES: usize = 256;

/// The burndown walk's measured cost. Kubernetes, 31,923 commits: one
/// worker peaks at 377 MB, 24 workers at 2058 MB (71 MiB per added worker:
/// each holds a commit's blobs and line diffs); the whole window holds 23
/// bytes more per commit than 50-commit chunks.
const BURNDOWN_COST: WalkCost = WalkCost {
    worker_bytes: 80 * cf_units::MIB,
    product_bytes: cf_units::KIB,
};

/// Runs the burndown walk: the forced-first-parent window, the cap, the
/// shared stamps, the base seed, and the per-path treaps driven in
/// chronological order.
fn burndown_walk(sub: &clap::ArgMatches) -> Option<BurndownWalk> {
    let run = HistoryRun::open_first_parent(sub)?;
    let flags = BurndownFlags::from_matches(sub);
    let max_changes = run.max_changes;
    let raw = parallel_prepare(
        &run.path,
        &run.hashes,
        HistoryRun::workers(),
        |repo, hash| {
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = commit_tree_changes(repo, &commit)?;
            let count = changes.len();
            let keep = (REUSED_DIFF_CHANGES..=max_changes).contains(&count);
            Some((count, keep.then_some(changes)))
        },
    )?;
    // Stamp every kept commit first: the value packing needs the last tick
    // before the first line is attributed.
    let mut stamper = Stamper::new(&run);
    let mut stamps = Vec::with_capacity(run.hashes.len());
    let mut reused: HashMap<cf_gitlib::Hash, Vec<Change>> = HashMap::new();
    for (pos, (hash, (count, changes))) in run.hashes.iter().zip(raw).enumerate() {
        if count <= max_changes {
            stamps.push(stamper.stamp(&run.repo, pos, *hash)?);
            if let Some(changes) = changes {
                reused.insert(*hash, changes);
            }
        }
    }
    let last_tick = stamper.last_tick();
    let summary = stamper.finish();
    let unmatched = summary.unmatched_slot as i64;
    let packing = if flags.track_people {
        ValuePacking::for_people(last_tick)
    } else {
        ValuePacking::ticks_only()
    };
    let mut tracker = Tracker::new(packing);

    seed_base_tree(
        &run,
        &mut tracker,
        packing.pack(packing.unknown_person(), BASE_TICK),
    )?;
    let base = reduce_reports(&tracker.drain(), packing, flags);

    // The per-change blob, filter and line-diff work runs on the worker
    // pool; the treap updates fold in commit order.
    let kept: Vec<cf_gitlib::Hash> = stamps.iter().map(|s| s.hash).collect();
    let plan = ChunkPlan::for_walk(sub, "history/burndown", kept.len(), BURNDOWN_COST).ok()?;
    let filter = &run.filter;
    let mut stamps = stamps.into_iter();
    let mut commits = Vec::with_capacity(kept.len());
    fold_planned(
        &run.path,
        &kept,
        plan,
        |repo, hash| {
            if let Some(changes) = reused.get(&hash) {
                return Some(prepare_change_facts(repo, filter, changes));
            }
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = commit_tree_changes(repo, &commit)?;
            Some(prepare_change_facts(repo, filter, &changes))
        },
        |_, facts| {
            let stamp = stamps.next()?;
            let value = packing.pack(person_slot(stamp.author_id, unmatched), stamp.tick);
            fold_change_facts(&mut tracker, facts, value);
            commits.push(BurndownCommit {
                stamp,
                deltas: reduce_reports(&tracker.drain(), packing, flags),
            });
            Some(())
        },
    )?;

    let ownership = if flags.track_files && flags.track_people {
        tracker.ownership()
    } else {
        BTreeMap::new()
    };
    let names = if flags.track_people {
        display_names(&summary.names)
    } else {
        Vec::new()
    };
    Some(BurndownWalk {
        commits,
        base,
        last_tick,
        tick_size_hours: run.tick_size_hours(),
        path: run.path,
        flags,
        paths: tracker.paths,
        ownership,
        names,
    })
}

/// The per-commit `{lines_added, lines_removed}` map of the timeseries
/// formats.
fn line_stats_map(c: &BurndownCommit) -> GoValue {
    let mut burndown = GoMap::new_map();
    burndown.insert("lines_added", GoValue::Int(c.deltas.added));
    burndown.insert("lines_removed", GoValue::Int(c.deltas.removed));
    GoValue::Map(burndown)
}

/// The burndown contribution to the merged `--format timeseries` document:
/// per commit `{"lines_added", "lines_removed"}` in walk order, each commit
/// under its own tick.
#[must_use]
pub fn burndown_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<crate::handlers::history_formats::TimeSeriesContribution> {
    let walk = burndown_walk(sub)?;
    let per_commit = walk
        .commits
        .iter()
        .map(|c| (c.stamp.hex.clone(), line_stats_map(c)))
        .collect();
    let commit_meta = walk
        .commits
        .iter()
        .map(|c| {
            (
                c.stamp.hex.clone(),
                c.stamp.tick,
                format_rfc3339_offset(c.stamp.when, c.stamp.offset_min),
                String::new(),
            )
        })
        .collect();
    Some(crate::handlers::history_formats::TimeSeriesContribution {
        flag: "burndown",
        per_commit,
        commit_meta,
    })
}

/// The per-commit `data` payload: `GlobalDeltas` (`curTick -> prevTick ->
/// delta`), `FileDeltas` (per final path; `null` without `--burndown-files`),
/// `PeopleDeltas` (per author of the affected lines) and `MatrixDeltas`
/// (`author -> modifier -> lines`; both `null` without `--burndown-people`),
/// `LinesAdded`, `LinesRemoved`.
fn commit_data(c: &BurndownCommit, walk: &BurndownWalk) -> GoValue {
    let d = &c.deltas;
    let mut data = GoMap::new(MapOrigin::Struct);
    data.insert("GlobalDeltas", sparse_to_value(&d.global));
    data.insert(
        "FileDeltas",
        if walk.flags.track_files {
            let mut files = GoMap::new_map();
            for (id, history) in &d.files {
                files.insert(walk.paths[*id].clone(), sparse_to_value(history));
            }
            GoValue::Map(files)
        } else {
            GoValue::Null
        },
    );
    data.insert(
        "PeopleDeltas",
        if walk.flags.track_people {
            let mut people = GoMap::new_int_map();
            for (person, history) in &d.people {
                people.insert(person.to_string(), sparse_to_value(history));
            }
            GoValue::Map(people)
        } else {
            GoValue::Null
        },
    );
    data.insert(
        "MatrixDeltas",
        if walk.flags.track_people {
            int_matrix_to_value(&d.matrix)
        } else {
            GoValue::Null
        },
    );
    data.insert("LinesAdded", GoValue::Int(d.added));
    data.insert("LinesRemoved", GoValue::Int(d.removed));
    GoValue::Map(data)
}

/// The per-commit burndown ndjson records, in the shared
/// [`crate::handlers::history_formats::NdjsonRecord`] shape so the centralized
/// multi-analyzer `--format ndjson` path can interleave burndown with the
/// other leaves per commit. The window base is not a commit and has no
/// record.
#[must_use]
pub fn burndown_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<crate::handlers::history_formats::NdjsonRecord>> {
    let walk = burndown_walk(sub)?;
    let records = walk
        .commits
        .iter()
        .map(|c| crate::handlers::history_formats::NdjsonRecord {
            pos: c.stamp.pos,
            hash: c.stamp.hex.clone(),
            tick: c.stamp.tick,
            author_id: c.stamp.author_id,
            time_secs: c.stamp.when,
            tz_offset_min: c.stamp.offset_min,
            data: commit_data(c, &walk),
        })
        .collect();
    Some(records)
}

/// `history/burndown --format json`: the line-survival report over the window.
#[must_use]
pub fn burndown_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = burndown_run_metrics(sub)?;
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// The shared `history/burndown` report value: the aggregated
/// [`ComputedMetrics`] every output format (json/yaml/bin/text) encodes.
#[must_use]
pub fn burndown_run_metrics(sub: &clap::ArgMatches) -> Option<ComputedMetrics> {
    Some(burndown_run_aggregate(sub)?.metrics)
}

/// The aggregated products of one burndown run: the computed metrics plus the
/// chart-rendering state the plot and text paths need.
pub struct BurndownRunAggregate {
    /// The computed report metrics (json/yaml/bin source).
    pub metrics: ComputedMetrics,
    /// Dense global history (negatives NOT yet clamped — the chart-data
    /// builder clamps).
    pub global_dense: DenseHistory,
    /// The sampling grid the report was laid out on.
    pub grid: Grid,
    /// Tick size, nanoseconds.
    pub tick_size_ns: i64,
    /// Aggregator end time (max committer time), Unix nanoseconds.
    pub end_time_ns: i64,
    /// Repository base name (empty for `""`/`"."`).
    pub project_name: String,
}

/// Adds every cell of `src` into `dst`.
fn merge_sparse(dst: &mut SparseHistory, src: &SparseHistory) {
    for (cur, row) in src {
        for (prev, delta) in row {
            add_cell(dst, *cur, *prev, *delta);
        }
    }
}

/// The walk-level accumulation of every commit's deltas (plus the base).
#[derive(Default)]
struct WalkTotals {
    global: SparseHistory,
    files: BTreeMap<String, SparseHistory>,
    people: BTreeMap<i64, SparseHistory>,
    matrix: BTreeMap<i64, BTreeMap<i64, i64>>,
}

impl WalkTotals {
    fn merge(&mut self, d: &CommitDeltas, paths: &[String]) {
        merge_sparse(&mut self.global, &d.global);
        for (id, history) in &d.files {
            merge_sparse(self.files.entry(paths[*id].clone()).or_default(), history);
        }
        for (person, history) in &d.people {
            merge_sparse(self.people.entry(*person).or_default(), history);
        }
        for (author, row) in &d.matrix {
            let dst = self.matrix.entry(*author).or_default();
            for (modifier, lines) in row {
                *dst.entry(*modifier).or_default() += *lines;
            }
        }
    }
}

/// Runs the burndown aggregation once, returning both the report metrics and
/// the chart-data inputs (shared by [`burndown_run_metrics`] and the plot
/// path).
#[must_use]
pub fn burndown_run_aggregate(sub: &clap::ArgMatches) -> Option<BurndownRunAggregate> {
    let walk = burndown_walk(sub)?;
    let mut totals = WalkTotals::default();
    totals.merge(&walk.base, &walk.paths);
    let mut end_time_secs: i64 = 0;
    for c in &walk.commits {
        totals.merge(&c.deltas, &walk.paths);
        end_time_secs = end_time_secs.max(c.stamp.when);
    }

    let grid = Grid {
        sampling: walk.flags.sampling,
        granularity: walk.flags.granularity,
        last_tick: walk.last_tick,
        tick_size_hours: walk.tick_size_hours,
    };
    let metrics = compute_metrics(&MetricsInput {
        global: &totals.global,
        files: walk.flags.track_files.then_some(&totals.files),
        people: walk.flags.track_people.then_some(&totals.people),
        ownership: &walk.ownership,
        matrix: &totals.matrix,
        names: &walk.names,
        grid,
    });

    Some(BurndownRunAggregate {
        metrics,
        global_dense: grid.densify(&totals.global),
        grid,
        tick_size_ns: walk.tick_size_hours * NANOS_PER_HOUR,
        end_time_ns: end_time_secs.saturating_mul(NANOS_PER_SECOND),
        project_name: repo_base_name(&walk.path),
    })
}

/// The repository path's base name, or empty when the path is `""` or `"."`.
#[must_use]
pub fn repo_base_name(path: &str) -> String {
    if path.is_empty() || path == "." {
        return String::new();
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(pos) => trimmed[pos + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// `history/burndown --format yaml`: the `codefang (v2)` header, the
/// `history/burndown:` section name and the metrics.
#[must_use]
pub fn burndown_run_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = burndown_run_metrics(sub)?;
    let mut out = Vec::new();
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
    out.extend_from_slice(b"history/burndown:\n");
    out.extend_from_slice(&cf_goyaml::marshal(&metrics.to_go_value()));
    Some(out)
}

/// `history/burndown --format bin`: the CFB1 envelope over the json value.
#[must_use]
pub fn burndown_run_report_bin(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = burndown_run_metrics(sub)?;
    cf_reportutil::encode_binary_envelope(&metrics.to_go_value()).ok()
}

/// Serializes a sparse `curTick -> prevTick -> delta` map as an int-keyed
/// object of int-keyed objects.
fn sparse_to_value(history: &SparseHistory) -> GoValue {
    let mut outer = GoMap::new_int_map();
    for (cur, row) in history {
        let mut inner = GoMap::new_int_map();
        for (prev, delta) in row {
            inner.insert(prev.to_string(), GoValue::Int(*delta));
        }
        outer.insert(cur.to_string(), GoValue::Map(inner));
    }
    GoValue::Map(outer)
}

/// Serializes an `author -> modifier -> lines` matrix as an int-keyed object
/// of int-keyed objects.
fn int_matrix_to_value(matrix: &BTreeMap<i64, BTreeMap<i64, i64>>) -> GoValue {
    let mut outer = GoMap::new_int_map();
    for (author, row) in matrix {
        let mut inner = GoMap::new_int_map();
        for (modifier, lines) in row {
            inner.insert(modifier.to_string(), GoValue::Int(*lines));
        }
        outer.insert(author.to_string(), GoValue::Map(inner));
    }
    GoValue::Map(outer)
}

/// Applies one change set's filtered changes to the tracker at `value` (the
/// window base seed): [`prepare_change_facts`] then [`fold_change_facts`].
fn apply_changes(
    repo: &cf_gitlib::Repository,
    filter: &HistoryFilter,
    tracker: &mut Tracker,
    changes: &[Change],
    value: i64,
) {
    fold_change_facts(tracker, prepare_change_facts(repo, filter, changes), value);
}

/// One change's effect on the line tracker, computed without the tracker so
/// it can run on the worker pool; [`fold_change_facts`] applies it in commit
/// order with the sequential walk's rules.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ChangeFacts {
    /// `path` (re)written whole: its line count, `None` for a binary or
    /// unreadable blob.
    Insert { path: String, lines: Option<i64> },
    /// `path` deleted.
    Delete { path: String },
    /// A modification, possibly with a rename.
    Modify {
        from: String,
        to: String,
        /// The To blob's line count, `None` for a binary or unreadable blob:
        /// the whole-file fallback.
        to_lines: Option<i64>,
        /// `None` when either blob is unreadable: the change is skipped.
        diff: Option<ModifyDiff>,
    },
}

/// The line diff of a readable modification; it applies only while the From
/// path's tracked length equals `from_lines` and the To blob is text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModifyDiff {
    /// The From blob's line count, `None` when it cannot be counted.
    from_lines: Option<i64>,
    /// The To blob is binary.
    to_binary: bool,
    /// The line ops from From to To (empty when the diff cannot apply).
    ops: Vec<(Op, i64)>,
}

/// The tracker-independent facts of one commit's changes that pass the
/// change filter (path policy, then `--languages`, by the change's name and
/// hash: To for insert and modify, From for delete).
fn prepare_change_facts(
    repo: &cf_gitlib::Repository,
    filter: &HistoryFilter,
    changes: &[Change],
) -> Vec<ChangeFacts> {
    changes
        .iter()
        .filter(|change| {
            let (name, hash) = match change.action {
                ChangeAction::Delete => (&change.from.name, change.from.hash),
                _ => (&change.to.name, change.to.hash),
            };
            filter.includes_change(repo, name, hash)
        })
        .map(|change| match change.action {
            ChangeAction::Insert => ChangeFacts::Insert {
                path: change.to.name.clone(),
                lines: text_lines(repo, change.to.hash),
            },
            ChangeAction::Delete => ChangeFacts::Delete {
                path: change.from.name.clone(),
            },
            ChangeAction::Modify => ChangeFacts::Modify {
                from: change.from.name.clone(),
                to: change.to.name.clone(),
                to_lines: text_lines(repo, change.to.hash),
                diff: modify_diff(repo, change),
            },
        })
        .collect()
}

/// The line diff of `change`, `None` when either blob cannot be read.
fn modify_diff(repo: &cf_gitlib::Repository, change: &Change) -> Option<ModifyDiff> {
    let (Ok(blob_from), Ok(blob_to)) = (
        CachedBlob::from_repo(repo, change.from.hash),
        CachedBlob::from_repo(repo, change.to.hash),
    ) else {
        return None;
    };
    let from_lines = blob_from.count_lines().ok().map(|n| n as i64);
    let to_binary = blob_to.is_binary();
    let ops = match from_lines {
        Some(old_lines) if !to_binary => {
            compute_line_diff(repo, change, &blob_from, &blob_to, old_lines)
        }
        _ => Vec::new(),
    };
    Some(ModifyDiff {
        from_lines,
        to_binary,
        ops,
    })
}

/// Applies one commit's prepared change facts to the tracker at `value`, in
/// change order. A modification follows its line diff only when the From
/// path's tracked length matches the From blob and the To blob is text;
/// otherwise the tracked lines are debited and the To blob is (re)inserted
/// whole.
fn fold_change_facts(tracker: &mut Tracker, facts: Vec<ChangeFacts>, value: i64) {
    for fact in facts {
        match fact {
            ChangeFacts::Insert { path, lines } => insert_lines(tracker, &path, lines, value),
            ChangeFacts::Delete { path } => tracker.remove(&path, value),
            ChangeFacts::Modify {
                from,
                to,
                to_lines,
                diff,
            } => {
                let Some(tracked_len) = tracker.len_of(&from) else {
                    insert_lines(tracker, &to, to_lines, value);
                    continue;
                };
                let Some(diff) = diff else {
                    continue;
                };
                if diff.from_lines != Some(tracked_len) || diff.to_binary {
                    tracker.remove(&from, value);
                    insert_lines(tracker, &to, to_lines, value);
                    continue;
                }
                tracker.apply_diffs(&from, &diff.ops, value);
                tracker.rename(&from, &to, value);
            }
        }
    }
}

/// Tracks `path` as `lines` text lines written at `value`; a binary or
/// unreadable blob (`None`) is not tracked. Whatever was tracked under the
/// path is debited first.
fn insert_lines(tracker: &mut Tracker, path: &str, lines: Option<i64>, value: i64) {
    match lines {
        Some(lines) => tracker.insert(path, lines, value),
        None => tracker.remove(path, value),
    }
}

/// The line count of a text blob; `None` for a binary or unreadable blob.
fn text_lines(repo: &cf_gitlib::Repository, hash: cf_gitlib::Hash) -> Option<i64> {
    let blob = CachedBlob::from_repo(repo, hash).ok()?;
    blob.count_lines().ok().map(|n| n as i64)
}

/// The line-diff op stream: libgit2 `git_diff_blobs` line ops
/// (`diff_blob_line_ops`), falling back to the diff-match-patch engine only
/// when libgit2 errors.
fn compute_line_diff(
    repo: &cf_gitlib::Repository,
    change: &Change,
    blob_from: &CachedBlob,
    blob_to: &CachedBlob,
    old_lines: i64,
) -> Vec<(Op, i64)> {
    use cf_gitlib::diff::{diff_blob_line_ops, LineOp};
    match diff_blob_line_ops(repo.native(), change.from.hash, change.to.hash, old_lines) {
        Ok(ops) => ops
            .iter()
            .map(|op| match *op {
                LineOp::Equal(n) => (Op::Equal, n),
                LineOp::Insert(n) => (Op::Insert, n),
                LineOp::Delete(n) => (Op::Delete, n),
            })
            .collect(),
        Err(_) => cf_godiff::line_diff(&blob_from.data, &blob_to.data, true)
            .iter()
            .map(|s| (s.op, s.lines.len() as i64))
            .collect(),
    }
}

/// Walks a line diff and issues `File::update(value, position, insLen,
/// delLen)` calls: a pending delete merges with a following insert into a
/// single replace; equal runs flush any pending edit and advance the
/// position.
fn apply_diffs(file: &mut File, diffs: &[(Op, i64)], value: i64) {
    let mut position: i64 = 0;
    let mut pending: Option<(Op, i64)> = None;

    // Apply a deferred edit (an insert advances the position; a delete does
    // not) and clear it.
    fn flush(file: &mut File, pending: &mut Option<(Op, i64)>, position: &mut i64, value: i64) {
        if let Some((op, len)) = pending.take() {
            if op == Op::Insert {
                file.update(value, *position, len, 0);
                *position += len;
            } else {
                file.update(value, *position, 0, len);
            }
        }
    }

    for &(op, len) in diffs {
        match op {
            Op::Equal => {
                flush(file, &mut pending, &mut position, value);
                position += len;
            }
            Op::Insert => {
                if let Some((_, del_len)) = pending {
                    file.update(value, position, len, del_len);
                    position += len;
                    pending = None;
                } else {
                    pending = Some((Op::Insert, len));
                }
            }
            Op::Delete => {
                // A delete is always followed by its insert or an equality,
                // so a pending insert never precedes a delete.
                pending = Some((Op::Delete, len));
            }
        }
    }
    flush(file, &mut pending, &mut position, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAILY_PEOPLE: BurndownFlags = BurndownFlags {
        granularity: 30,
        sampling: 30,
        track_files: true,
        track_people: true,
    };
    const GLOBAL_ONLY: BurndownFlags = BurndownFlags {
        granularity: 30,
        sampling: 30,
        track_files: false,
        track_people: false,
    };

    /// The sequential walk's change application, kept as the reference the
    /// prepared facts are checked against.
    fn reference_apply_changes(
        repo: &cf_gitlib::Repository,
        filter: &HistoryFilter,
        tracker: &mut Tracker,
        changes: &[Change],
        value: i64,
    ) {
        for change in changes {
            let (filter_name, filter_hash) = match change.action {
                ChangeAction::Delete => (&change.from.name, change.from.hash),
                _ => (&change.to.name, change.to.hash),
            };
            if !filter.includes_change(repo, filter_name, filter_hash) {
                continue;
            }
            match change.action {
                ChangeAction::Insert => {
                    insert_blob(repo, tracker, &change.to.name, change.to.hash, value);
                }
                ChangeAction::Delete => tracker.remove(&change.from.name, value),
                ChangeAction::Modify => apply_modification(repo, tracker, change, value),
            }
        }
    }

    /// Tracks `path` as the text blob `hash` written at `value`; a binary blob
    /// is not tracked. Whatever was tracked under the path is debited first.
    fn insert_blob(
        repo: &cf_gitlib::Repository,
        tracker: &mut Tracker,
        path: &str,
        hash: cf_gitlib::Hash,
        value: i64,
    ) {
        match text_lines(repo, hash) {
            Some(lines) => tracker.insert(path, lines, value),
            None => tracker.remove(path, value),
        }
    }

    /// A modification (including a rename: `from.name != to.name`). When the
    /// tracked state matches the From blob and the To blob is text, the treap
    /// follows the line diff and moves to the To path; otherwise the tracked
    /// lines are debited and the To blob is (re)inserted whole.
    fn apply_modification(
        repo: &cf_gitlib::Repository,
        tracker: &mut Tracker,
        change: &Change,
        value: i64,
    ) {
        let (from, to) = (change.from.name.as_str(), change.to.name.as_str());
        let Some(tracked_len) = tracker.len_of(from) else {
            insert_blob(repo, tracker, to, change.to.hash, value);
            return;
        };
        let (Ok(blob_from), Ok(blob_to)) = (
            CachedBlob::from_repo(repo, change.from.hash),
            CachedBlob::from_repo(repo, change.to.hash),
        ) else {
            return;
        };
        let in_sync =
            blob_from.count_lines().ok() == Some(tracked_len as usize) && !blob_to.is_binary();
        if !in_sync {
            tracker.remove(from, value);
            insert_blob(repo, tracker, to, change.to.hash, value);
            return;
        }
        let diffs = compute_line_diff(repo, change, &blob_from, &blob_to, tracked_len);
        tracker.apply_diffs(from, &diffs, value);
        tracker.rename(from, to, value);
    }

    /// `n` numbered text lines tagged `tag`.
    fn numbered(n: usize, tag: &str) -> Vec<u8> {
        (0..n)
            .map(|i| format!("{tag} {i}\n"))
            .collect::<String>()
            .into_bytes()
    }

    /// Per change, the facts prepared off the tracker (on the worker pool)
    /// fold into exactly the reports and paths the sequential walk produces:
    /// through inserts, a modification, a file whose tracked length diverged
    /// from its blob (its previous edit was dropped, like a commit over the
    /// cap), a rename, an edit after the rename, a text-to-binary switch and
    /// a delete.
    #[test]
    fn prepared_changes_fold_like_the_sequential_walk() {
        use cf_gitlib::testutil::{init_repo, CommitBuilder};
        const T0: i64 = 1_609_459_200;
        const DROPPED: usize = 2;
        let test = init_repo().expect("init");
        let at = |i: i64| T0 + i;
        let hashes = [
            CommitBuilder::new(&test, "add", at(0))
                .write("a.txt", &numbered(5, "a"))
                .write("b.txt", &numbered(3, "b"))
                .commit(),
            CommitBuilder::new(&test, "modify", at(1))
                .write("a.txt", &[numbered(5, "a"), numbered(1, "more")].concat())
                .commit(),
            CommitBuilder::new(&test, "dropped", at(2))
                .write("b.txt", &numbered(9, "b"))
                .commit(),
            CommitBuilder::new(&test, "diverged", at(3))
                .write("b.txt", &[numbered(9, "b"), numbered(1, "x")].concat())
                .commit(),
            CommitBuilder::new(&test, "rename", at(4))
                .rename("a.txt", "c.txt")
                .commit(),
            CommitBuilder::new(&test, "edit renamed", at(5))
                .write("c.txt", &numbered(4, "a"))
                .commit(),
            CommitBuilder::new(&test, "binary", at(6))
                .write("c.txt", b"\x00\x01\x02binary")
                .commit(),
            CommitBuilder::new(&test, "delete", at(7))
                .delete("b.txt")
                .commit(),
        ]
        .map(|h| h.expect("commit"));
        let repo = cf_gitlib::Repository::open(test.path().to_str().expect("utf-8")).expect("open");
        let filter = HistoryFilter::from_policy(cf_pathpolicy::Options::default());
        let packing = ValuePacking::ticks_only();
        let (mut sequential, mut prepared) = (Tracker::new(packing), Tracker::new(packing));
        for (i, hash) in hashes.iter().enumerate() {
            if i == DROPPED {
                continue;
            }
            let commit = repo.lookup_commit(*hash).expect("lookup");
            let changes = commit_tree_changes(&repo, &commit).expect("changes");
            let value = i as i64 + 1;
            reference_apply_changes(&repo, &filter, &mut sequential, &changes, value);
            fold_change_facts(
                &mut prepared,
                prepare_change_facts(&repo, &filter, &changes),
                value,
            );
            assert_eq!(prepared.drain(), sequential.drain(), "commit {i}");
            assert_eq!(prepared.paths, sequential.paths, "commit {i}");
        }
    }

    /// Commits big enough to reuse their pre-pass tree diff (an add and an
    /// edit of more than [`REUSED_DIFF_CHANGES`] files) fold into the same
    /// per-commit deltas as the sequential reference.
    #[test]
    fn heavy_commits_reuse_their_diff_and_fold_like_the_reference() {
        use cf_gitlib::testutil::{init_repo, CommitBuilder};
        const T0: i64 = 1_609_459_200;
        const DAY: i64 = 86_400;
        let files = REUSED_DIFF_CHANGES + 1;
        let test = init_repo().expect("init");
        let mut add = CommitBuilder::new(&test, "add", T0);
        let mut edit = CommitBuilder::new(&test, "edit", T0 + DAY);
        for i in 0..files {
            let name = format!("f{i}.txt");
            add = add.write(&name, &numbered(3, "a"));
            edit = edit.write(&name, &[numbered(2, "a"), numbered(2, "b")].concat());
        }
        let first = add.commit().expect("add");
        let second = edit.commit().expect("edit");
        let path = test.path().to_str().expect("utf-8");
        let sub = crate::build_codefang_command()
            .try_get_matches_from(["codefang", "run", "-a", "history/burndown", path])
            .expect("args")
            .subcommand_matches("run")
            .expect("run")
            .clone();
        let walk = burndown_walk(&sub).expect("walk");

        let repo = cf_gitlib::Repository::open(path).expect("open");
        let filter = HistoryFilter::from_policy(cf_pathpolicy::Options::default());
        let packing = ValuePacking::ticks_only();
        let mut tracker = Tracker::new(packing);
        assert_eq!(walk.commits.len(), 2);
        for (commit, hash) in walk.commits.iter().zip([first, second]) {
            let changes = commit_tree_changes(&repo, &repo.lookup_commit(hash).expect("lookup"))
                .expect("changes");
            assert!(changes.len() > REUSED_DIFF_CHANGES);
            let value = packing.pack(packing.unknown_person(), commit.stamp.tick);
            reference_apply_changes(&repo, &filter, &mut tracker, &changes, value);
            let expected = reduce_reports(&tracker.drain(), packing, walk.flags);
            assert_eq!(
                commit.deltas, expected,
                "commit at tick {}",
                commit.stamp.tick
            );
        }
        assert!(
            walk.commits[1].deltas.removed > 0,
            "the edit replaced lines"
        );
    }

    /// `insert` / `remove` / `rename` keep the per-path state and report
    /// every line exactly once.
    #[test]
    fn tracker_debits_before_reinsert_and_follows_renames() {
        let packing = ValuePacking::ticks_only();
        let mut t = Tracker::new(packing);
        t.insert("a.go", 10, 0);
        // Re-inserting a tracked path debits its 10 lines first.
        t.insert("a.go", 4, 5);
        // A rename keeps the treap and updates the id's path.
        t.rename("a.go", "b.go", 6);
        assert_eq!(t.len_of("b.go"), Some(4));
        assert_eq!(t.len_of("a.go"), None);
        assert_eq!(
            t.paths,
            vec!["a.go", "b.go"],
            "a dead id keeps its last path"
        );
        // Renaming onto a tracked path debits that path's lines.
        t.insert("c.go", 2, 7);
        t.rename("b.go", "c.go", 8);
        t.remove("c.go", 9);
        assert!(t.files.is_empty());
        let d = reduce_reports(&t.drain(), packing, GLOBAL_ONLY);
        assert_eq!(d.added, 10 + 4 + 2);
        assert_eq!(d.removed, 10 + 2 + 4);
        assert_eq!(d.global[&5][&0], -10);
        assert_eq!(d.global[&8][&7], -2);
        assert_eq!(d.global[&9][&5], -4);
    }

    /// Added and removed never cancel within a commit, even in one tick.
    #[test]
    fn reduce_counts_gross_added_and_removed() {
        let packing = ValuePacking::ticks_only();
        let reports = [
            Report {
                file: 0,
                current: 3,
                previous: 3,
                delta: 6,
            },
            Report {
                file: 1,
                current: 3,
                previous: 3,
                delta: -2,
            },
        ];
        let d = reduce_reports(&reports, packing, GLOBAL_ONLY);
        assert_eq!((d.added, d.removed), (6, 2));
        assert_eq!(d.global[&3][&3], 4);
        assert!(d.files.is_empty() && d.people.is_empty() && d.matrix.is_empty());
    }

    /// With people tracking the reduce attributes deltas to the author of
    /// the affected lines, fills the matrix on deletions, and ignores lines
    /// with no author.
    #[test]
    fn reduce_attributes_people_and_matrix() {
        let packing = ValuePacking::for_people(100);
        let unknown = packing.unknown_person();
        let alice_t0 = packing.pack(0, 0);
        let bob_t5 = packing.pack(1, 5);
        let base = packing.pack(unknown, 0);
        let reports = [
            Report {
                file: 0,
                current: bob_t5,
                previous: bob_t5,
                delta: 3,
            },
            Report {
                file: 0,
                current: bob_t5,
                previous: alice_t0,
                delta: -2,
            },
            Report {
                file: 1,
                current: bob_t5,
                previous: base,
                delta: -4,
            },
        ];
        let d = reduce_reports(&reports, packing, DAILY_PEOPLE);
        assert_eq!(d.people[&1][&5][&5], 3);
        assert_eq!(d.people[&0][&5][&0], -2);
        assert!(!d.people.contains_key(&unknown));
        assert_eq!(d.matrix[&0][&1], 2);
        assert!(!d.matrix.contains_key(&unknown));
        assert_eq!(d.global[&5][&0], -6, "the global curve keeps base lines");
        assert_eq!(d.files[&1][&5][&0], -4);
        assert_eq!((d.added, d.removed), (3, 6));
    }

    /// Ownership reads the live segments per author and skips base lines.
    #[test]
    fn ownership_counts_live_lines_per_author() {
        let packing = ValuePacking::for_people(100);
        let mut t = Tracker::new(packing);
        t.insert("f.py", 4, packing.pack(packing.unknown_person(), 0));
        let bob = packing.pack(1, 5);
        t.apply_diffs(
            "f.py",
            &[
                (Op::Equal, 1),
                (Op::Delete, 1),
                (Op::Insert, 1),
                (Op::Equal, 2),
                (Op::Insert, 2),
            ],
            bob,
        );
        let alice = packing.pack(0, 9);
        t.apply_diffs("f.py", &[(Op::Equal, 6), (Op::Insert, 1)], alice);
        assert_eq!(t.len_of("f.py"), Some(7));
        let own = t.ownership();
        assert_eq!(own["f.py"], BTreeMap::from([(0, 1), (1, 3)]));
    }

    /// The diff applier merges a pending delete with its insert into one
    /// replace and flushes a trailing insert.
    #[test]
    fn apply_diffs_pending_delete_merges_with_insert() {
        let reports = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&reports);
        let updater: Updater = Box::new(move |c, p, d| sink.borrow_mut().push((c, p, d)));
        let mut file = File::new(0, 4, vec![updater]);
        reports.borrow_mut().clear();
        apply_diffs(
            &mut file,
            &[
                (Op::Equal, 1),
                (Op::Delete, 1),
                (Op::Insert, 1),
                (Op::Equal, 2),
                (Op::Insert, 2),
            ],
            7,
        );
        assert_eq!(file.len(), 6);
        assert_eq!(*reports.borrow(), vec![(7, 7, 1), (7, 0, -1), (7, 7, 2)]);
    }

    #[test]
    fn repo_base_name_cases() {
        assert_eq!(repo_base_name(""), "");
        assert_eq!(repo_base_name("."), "");
        assert_eq!(repo_base_name("/"), "/");
        assert_eq!(repo_base_name("/tmp/repo/"), "repo");
        assert_eq!(repo_base_name("repo"), "repo");
    }

    #[test]
    fn person_slot_maps_unmatched_authors_to_the_dict_slot() {
        let missing = i64::from(cf_analyzers_plumbing::AUTHOR_MISSING);
        assert_eq!(person_slot(missing, 4), 4);
        assert_eq!(person_slot(2, 4), 2);
    }
}
