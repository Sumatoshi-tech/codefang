//! The chunked history core: incremental identity + tick assignment
//! ([`Stamper`]) and the chunk driver ([`walk_chunks`]) a history walk folds
//! its per-commit products through.
//!
//! A walk computes its pure per-commit products on a worker pool
//! ([`super::pool`]) at most one chunk ([`ChunkPlan`]) ahead of its fold, then
//! folds each kept commit into its reduce state in window order and drops the
//! product, so a walk never holds more than one chunk of products. The chunk
//! is a sliding bound, not a barrier: workers never wait for the slowest
//! commit of a chunk. Stamping happens one commit at a time in window order,
//! so author ids, ticks and the folded output do not depend on the chunk
//! size. After every chunk of folded commits, and once the walk ends, free
//! heap pages go back to the operating system
//! ([`cf_meminfo::release_free_heap`]): the workers' allocator arenas would
//! otherwise keep every worker's largest transient working set resident.

use cf_analyzers_plumbing::{anonymize_names, HistoryTicks, IdentityDetector};
use cf_gitlib::Hash;

use super::budget::{
    concurrent_walks, default_budget, memory_budget_flag, native_bytes, plan_walk, WalkCost,
    BASE_OVERHEAD,
};
use super::pool::prepare_in_order;
use super::{author_signature, CommitStamp, HistoryRun};
use crate::pipeline::HandlerError;

/// The `--chunk-size` flag name.
const FLAG_CHUNK_SIZE: &str = "chunk-size";

/// Incremental identity + tick assignment, one commit at a time, in window
/// order. Ids are first-seen oldest-first and ticks never go backwards, so
/// both depend only on the commits stamped so far — never on how the window
/// was chunked.
pub(crate) struct Stamper {
    identity: IdentityDetector,
    ticks: HistoryTicks,
    anonymize: bool,
}

impl Stamper {
    /// A stamper seeded with the run's identity configuration
    /// (`--exact-signatures`, `--people-dict`), `--tick-size` and
    /// `--anonymize`.
    pub(crate) fn new(run: &HistoryRun) -> Self {
        Self {
            identity: run.identity_seed.clone(),
            ticks: HistoryTicks::new(run.tick_size_hours),
            anonymize: run.anonymize,
        }
    }

    /// Stamps the commit at window position `pos`: its author id, its tick
    /// and the commit facts every per-commit record emits. `None` when the
    /// commit cannot be read.
    pub(crate) fn stamp(
        &mut self,
        repo: &cf_gitlib::Repository,
        pos: usize,
        hash: Hash,
    ) -> Option<CommitStamp> {
        let commit = repo.lookup_commit(hash).ok()?;
        let author_id = self.identity.consume_signature(&author_signature(&commit));
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();
        Some(CommitStamp {
            pos,
            hash,
            hex: hash.to_hex(),
            tick: self.ticks.tick_for(when),
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
            num_parents: commit.num_parents(),
        })
    }

    /// The tick of the newest stamped commit (0 before any).
    pub(crate) const fn last_tick(&self) -> i64 {
        self.ticks.last_tick()
    }

    /// The finished identity state: the developer names and the people slot
    /// of an unmatched author.
    pub(crate) fn finish(mut self) -> StampSummary {
        self.identity.finalize_dict();
        let people = self.identity.reversed_people_dict;
        let unmatched_slot = people
            .iter()
            .position(|n| n == cf_analyzers_plumbing::AUTHOR_MISSING_NAME)
            .unwrap_or(people.len());
        let names = if self.anonymize {
            anonymize_names(&people)
        } else {
            people
        };
        StampSummary {
            names,
            unmatched_slot,
        }
    }
}

/// The identity state of a finished walk.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct StampSummary {
    /// The reversed people dictionary (index = author id), `--anonymize`
    /// applied.
    pub names: Vec<String>,
    /// The people slot an UNMATCHED author (`--people-dict` miss) is counted
    /// under: the dictionary's `<unmatched>` entry when one was loaded, else
    /// one past the known people (never reached without a loaded dictionary,
    /// where every author is registered).
    pub unmatched_slot: usize,
}

/// How one walk runs: the pre-pass products of at most `size` commits are
/// alive at once, prepared by `workers` threads. Neither reaches the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChunkPlan {
    /// Commits per chunk (at least 1).
    pub size: usize,
    /// Worker threads preparing products (at least 1).
    pub workers: usize,
}

impl ChunkPlan {
    /// `size` commits per chunk on every core.
    pub(crate) fn sized(size: usize) -> Self {
        Self {
            size,
            workers: HistoryRun::workers(),
        }
    }

    /// One chunk spanning any window: the walk holds every product at once.
    pub(crate) fn whole_window() -> Self {
        Self::sized(usize::MAX)
    }

    /// The plan the run flags ask for: `--chunk-size <n>` fixes the commits
    /// per chunk; without it the whole window is one chunk.
    ///
    /// # Errors
    ///
    /// [`HandlerError::InvalidFlag`] for a `--chunk-size` below 1.
    pub(crate) fn from_matches(sub: &clap::ArgMatches) -> Result<Self, HandlerError> {
        let Some(size) = sub
            .try_get_one::<i64>(FLAG_CHUNK_SIZE)
            .ok()
            .flatten()
            .copied()
        else {
            return Ok(Self::whole_window());
        };
        usize::try_from(size)
            .ok()
            .filter(|&n| n >= 1)
            .map(Self::sized)
            .ok_or_else(|| {
                HandlerError::InvalidFlag(format!(
                    "invalid --{FLAG_CHUNK_SIZE}: must be >= 1, got {size}"
                ))
            })
    }
}

/// Where a walk's [`ChunkPlan`] came from, for the `-v` plan line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanSource {
    /// `--chunk-size`.
    ChunkSize,
    /// The walk's share of the memory budget, in bytes.
    Budget(i64),
}

impl ChunkPlan {
    /// The plan of the walk named `walk` over `commits` commits: `--chunk-size` when
    /// given, otherwise the walk's share of `--memory-budget` (or of the
    /// automatic budget) planned with its measured `cost`. With `-v` the plan
    /// is printed on stderr.
    ///
    /// # Errors
    ///
    /// [`HandlerError::InvalidFlag`] for an invalid `--chunk-size` or
    /// `--memory-budget` (the preflight reports these before any walk).
    pub(crate) fn for_walk(
        sub: &clap::ArgMatches,
        walk: &str,
        commits: usize,
        cost: WalkCost,
    ) -> Result<Self, HandlerError> {
        Self::planned(sub, walk, commits, cost, concurrent_walks(sub))
    }

    /// [`Self::for_walk`] for a walk that runs before the others start (the
    /// pre-computed UAST pass): it plans against the whole budget.
    ///
    /// # Errors
    ///
    /// As [`Self::for_walk`].
    pub(crate) fn for_lone_walk(
        sub: &clap::ArgMatches,
        walk: &str,
        commits: usize,
        cost: WalkCost,
    ) -> Result<Self, HandlerError> {
        Self::planned(sub, walk, commits, cost, 1)
    }

    /// The plan of `walk` when `concurrent` walks share the budget.
    fn planned(
        sub: &clap::ArgMatches,
        walk: &str,
        commits: usize,
        cost: WalkCost,
        concurrent: usize,
    ) -> Result<Self, HandlerError> {
        let chunk_size_given = sub
            .try_get_one::<i64>(FLAG_CHUNK_SIZE)
            .ok()
            .flatten()
            .is_some();
        let (plan, source) = if chunk_size_given {
            (Self::from_matches(sub)?, PlanSource::ChunkSize)
        } else {
            let budget = memory_budget_flag(sub)?.unwrap_or_else(default_budget);
            // libgit2's part and the base overhead are paid once per
            // process, not once per walk.
            let walks = budget
                .saturating_sub(BASE_OVERHEAD)
                .saturating_sub(native_bytes(budget))
                .max(0);
            let share = BASE_OVERHEAD + walks / concurrent.max(1) as i64;
            (
                plan_walk(commits, HistoryRun::workers(), share, cost),
                PlanSource::Budget(share),
            )
        };
        if sub.try_get_one::<bool>("verbose").ok().flatten() == Some(&true) {
            eprintln!("{}", plan.describe(walk, commits, source));
        }
        Ok(plan)
    }

    /// The `-v` plan line of `walk` over `commits` commits.
    pub(crate) fn describe(self, walk: &str, commits: usize, source: PlanSource) -> String {
        let chunks = commits.div_ceil(self.size.max(1));
        let origin = match source {
            PlanSource::ChunkSize => format!("--{FLAG_CHUNK_SIZE}"),
            PlanSource::Budget(bytes) => format!("budget {} MiB", bytes / cf_units::MIB),
        };
        format!(
            "progress: {walk} plan: {} commits per chunk, {chunks} chunks, {} workers ({origin})",
            self.size, self.workers
        )
    }
}

/// Drives one walk over the window: `prepare` runs on the worker pool (pure,
/// at most `plan.size` commits ahead of the fold), `oversized` applies the
/// `--max-changes-per-commit` drop gate to a product, and `consume` folds
/// each kept commit IN WINDOW ORDER with its stamp. Each product is moved
/// into `consume` (or dropped by the gate), so at most `plan.size` products
/// are alive at once. Free heap pages are released after every `plan.size`
/// folded commits and at the end. Returns the finished identity state, or
/// `None` when any commit hits a fatal git error.
pub(crate) fn walk_chunks<P: Send>(
    run: &HistoryRun,
    plan: ChunkPlan,
    prepare: impl Fn(&cf_gitlib::Repository, Hash) -> Option<P> + Sync,
    oversized: impl Fn(&P) -> bool,
    mut consume: impl FnMut(&CommitStamp, P),
) -> Option<StampSummary> {
    let mut stamper = Stamper::new(run);
    fold_planned(&run.path, &run.hashes, plan, prepare, |pos, product| {
        if oversized(&product) {
            return Some(());
        }
        let stamp = stamper.stamp(&run.repo, pos, run.hashes[pos])?;
        consume(&stamp, product);
        Some(())
    })?;
    Some(stamper.finish())
}

/// Prepares `hashes` on the worker pool as `plan` says and folds each
/// product on the calling thread in window order (`fold` gets the window
/// position), releasing free heap pages after every `plan.size` folded
/// commits and at the end: the driver every chunked walk runs on. `None`
/// when a commit's preparation or the fold fails.
pub(crate) fn fold_planned<P: Send>(
    repo_path: &str,
    hashes: &[Hash],
    plan: ChunkPlan,
    prepare: impl Fn(&cf_gitlib::Repository, Hash) -> Option<P> + Sync,
    mut fold: impl FnMut(usize, P) -> Option<()>,
) -> Option<()> {
    let window = plan.size.max(1);
    prepare_in_order(
        repo_path,
        hashes,
        plan.workers,
        window,
        prepare,
        |pos, product| {
            if (pos + 1) % window == 0 {
                cf_meminfo::release_free_heap();
            }
            fold(pos, product)
        },
    )?;
    // The workers are gone; what their arenas freed is garbage now.
    cf_meminfo::release_free_heap();
    Some(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use cf_gitlib::testutil::{init_repo, CommitBuilder, TestRepo};

    use super::*;

    /// 2021-01-01T00:00:00Z.
    const T0: i64 = 1_609_459_200;
    /// One default tick.
    const DAY: i64 = 86_400;
    /// The chunk sizes every equivalence test runs (the window length is
    /// added per fixture).
    const CHUNK_SIZES: [usize; 3] = [1, 2, 3];

    /// One fixture commit: author name, author email, author time and an
    /// optional skewed committer time.
    type Author = (&'static str, &'static str, i64, Option<i64>);

    /// A linear history, one file write per commit, in `authors` order.
    fn history(authors: &[Author]) -> (TestRepo, Vec<Hash>) {
        let test = init_repo().expect("init");
        let mut hashes = Vec::new();
        for (i, (name, email, when, committer)) in authors.iter().enumerate() {
            let body = format!("v{i}\n");
            let mut builder = CommitBuilder::new(&test, &format!("c{i}"), *when)
                .author(name, email)
                .write("f.txt", body.as_bytes());
            if let Some(at) = committer {
                builder = builder.committer_time(*at);
            }
            hashes.push(builder.commit().expect("commit"));
        }
        (test, hashes)
    }

    /// Aliases (`alice` / `Alice Smith` by email, then by name), a second and
    /// a third developer, and a committer clock that runs backwards (`c3`).
    fn alias_history() -> (TestRepo, Vec<Hash>) {
        history(&[
            ("alice", "alice@x", T0, None),
            ("Alice Smith", "alice@x", T0 + DAY, None),
            ("bob", "bob@x", T0 + 2 * DAY, None),
            ("Alice Smith", "smith@y", T0 + 3 * DAY, Some(T0 + DAY / 2)),
            ("carol", "carol@x", T0 + 5 * DAY, None),
        ])
    }

    /// The parsed `run` matches for `-a history/devs <extra> <path>`.
    fn run_matches(test: &TestRepo, extra: &[&str]) -> clap::ArgMatches {
        let path = test.path().to_str().expect("utf-8 path");
        let argv: Vec<&str> = ["codefang", "run", "-a", "history/devs"]
            .into_iter()
            .chain(extra.iter().copied())
            .chain([path])
            .collect();
        crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse")
            .subcommand_matches("run")
            .expect("run")
            .clone()
    }

    fn open(test: &TestRepo, extra: &[&str]) -> HistoryRun {
        HistoryRun::open(&run_matches(test, extra)).expect("open run")
    }

    /// The stamps and the summary of one chunked walk over `run` whose
    /// product is the commit hash, dropping the hashes in `dropped`.
    fn stamp_in_chunks(
        run: &HistoryRun,
        size: usize,
        dropped: &[Hash],
    ) -> (Vec<CommitStamp>, StampSummary) {
        let mut stamps = Vec::new();
        let summary = walk_chunks(
            run,
            ChunkPlan::sized(size),
            |_, hash| Some(hash),
            |hash| dropped.contains(hash),
            |stamp, hash| {
                assert_eq!(stamp.hash, hash, "the product belongs to its stamp");
                stamps.push(stamp.clone());
            },
        )
        .expect("walk");
        (stamps, summary)
    }

    /// `(pos, tick, author id)` of each stamp.
    fn view(stamps: &[CommitStamp]) -> Vec<(usize, i64, i64)> {
        stamps
            .iter()
            .map(|s| (s.pos, s.tick, s.author_id))
            .collect()
    }

    /// Stamping in chunks of every size equals stamping the whole window
    /// once, field for field.
    fn assert_chunk_invariant(
        run: &HistoryRun,
        dropped: &[Hash],
    ) -> (Vec<CommitStamp>, StampSummary) {
        let whole = stamp_in_chunks(run, ChunkPlan::whole_window().size, dropped);
        for size in CHUNK_SIZES.into_iter().chain([run.hashes.len()]) {
            assert_eq!(
                stamp_in_chunks(run, size, dropped),
                whole,
                "chunk size {size}"
            );
        }
        whole
    }

    #[test]
    fn chunked_stamps_follow_aliases_and_a_backward_clock() {
        let (test, hashes) = alias_history();
        let run = open(&test, &[]);
        assert_eq!(run.hashes, hashes);
        let (stamps, summary) = assert_chunk_invariant(&run, &[]);
        // c3's committer clock runs backwards: its tick is clamped to c2's.
        assert_eq!(
            view(&stamps),
            [(0, 0, 0), (1, 1, 0), (2, 2, 1), (3, 2, 0), (4, 5, 2)]
        );
        assert_eq!(
            summary.names,
            [
                "alice|alice smith|alice@x|smith@y",
                "bob|bob@x",
                "carol|carol@x"
            ]
        );
        assert_eq!(summary.unmatched_slot, summary.names.len());
        assert_eq!(stamps[3].when, T0 + DAY / 2);
        assert_eq!(stamps[2].hex, hashes[2].to_hex());
    }

    #[test]
    fn chunked_stamps_apply_anonymize() {
        let (test, _) = alias_history();
        let (_, summary) = assert_chunk_invariant(&open(&test, &["--anonymize"]), &[]);
        assert_eq!(summary.names, ["Developer-A", "Developer-B", "Developer-C"]);
    }

    #[test]
    fn chunked_stamps_resolve_a_people_dict_and_its_unmatched_slot() {
        let (test, _) = alias_history();
        let dict = tempfile::NamedTempFile::new().expect("dict file");
        std::fs::write(dict.path(), "Alice|alice@x|smith@y\nBob|bob@x\n").expect("write dict");
        let dict_path = dict.path().to_str().expect("utf-8 path");
        let missing = cf_analyzers_plumbing::author_missing_id();

        let (stamps, summary) =
            assert_chunk_invariant(&open(&test, &["--people-dict", dict_path]), &[]);
        assert_eq!(
            view(&stamps),
            [(0, 0, 0), (1, 1, 0), (2, 2, 1), (3, 2, 0), (4, 5, missing)]
        );
        assert_eq!(summary.names, ["Alice", "Bob", "<unmatched>"]);
        assert_eq!(summary.unmatched_slot, 2);

        let (_, anonymized) = assert_chunk_invariant(
            &open(&test, &["--people-dict", dict_path, "--anonymize"]),
            &[],
        );
        assert_eq!(
            anonymized.names,
            ["Developer-A", "Developer-B", "<unmatched>"]
        );
        assert_eq!(anonymized.unmatched_slot, 2);
    }

    /// A dropped commit consumes no author id and advances no tick; the kept
    /// stamps keep their ORIGINAL window positions.
    #[test]
    fn dropped_commits_are_never_stamped() {
        let (test, hashes) = alias_history();
        let run = open(&test, &[]);
        let (stamps, summary) = assert_chunk_invariant(&run, &[hashes[2]]);
        assert_eq!(view(&stamps), [(0, 0, 0), (1, 1, 0), (3, 1, 0), (4, 5, 1)]);
        assert_eq!(
            summary.names,
            ["alice|alice smith|alice@x|smith@y", "carol|carol@x"]
        );
    }

    /// An empty window stamps nothing and still finishes the identity state.
    #[test]
    fn an_empty_window_yields_an_empty_summary() {
        let (test, _) = alias_history();
        let run = open(&test, &["--since", "2100-01-01"]);
        assert!(run.hashes.is_empty());
        let (stamps, summary) = assert_chunk_invariant(&run, &[]);
        assert!(stamps.is_empty());
        assert!(summary.names.is_empty());
    }

    /// `last_tick` is the tick of the newest stamped commit (0 before any).
    #[test]
    fn last_tick_follows_the_stamped_commits() {
        let (test, hashes) = alias_history();
        let run = open(&test, &[]);
        let mut stamper = Stamper::new(&run);
        assert_eq!(stamper.last_tick(), 0);
        let mut ticks = Vec::new();
        for (pos, hash) in hashes.iter().enumerate() {
            let stamp = stamper.stamp(&run.repo, pos, *hash).expect("stamp");
            assert_eq!(stamper.last_tick(), stamp.tick);
            ticks.push(stamp.tick);
        }
        assert_eq!(ticks, [0, 1, 2, 2, 5]);
        assert_eq!(stamper.finish().names.len(), 3);
    }

    /// A product that counts its live instances, so a test can observe how
    /// many products a walk holds at once.
    struct Tracked {
        live: Arc<AtomicUsize>,
    }

    impl Tracked {
        fn new(live: &Arc<AtomicUsize>) -> Self {
            live.fetch_add(1, Ordering::SeqCst);
            Self {
                live: Arc::clone(live),
            }
        }
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.live.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Products are dropped once folded: no more than one chunk of them is
    /// ever alive, and the fold sees every commit in window order.
    #[test]
    fn walk_chunks_holds_at_most_one_chunk_of_products() {
        let (test, hashes) = alias_history();
        let run = open(&test, &[]);
        for size in CHUNK_SIZES {
            let live = Arc::new(AtomicUsize::new(0));
            let mut seen = Vec::new();
            walk_chunks(
                &run,
                ChunkPlan::sized(size),
                |_, _| Some(Tracked::new(&live)),
                |_| false,
                |stamp, product| {
                    let now = live.load(Ordering::SeqCst);
                    assert!(now <= size, "{now} live products with chunk size {size}");
                    seen.push(stamp.pos);
                    drop(product);
                },
            )
            .expect("walk");
            assert_eq!(live.load(Ordering::SeqCst), 0, "every product dropped");
            assert_eq!(seen, (0..hashes.len()).collect::<Vec<_>>());
        }
    }

    /// A failed pre-pass on any commit fails the walk (the `.ok()?`
    /// contract of every history walk).
    #[test]
    fn a_failed_prepare_fails_the_walk() {
        let (test, hashes) = alias_history();
        let run = open(&test, &[]);
        let result = walk_chunks(
            &run,
            ChunkPlan::sized(2),
            |_, hash| (hash != hashes[3]).then_some(()),
            |()| false,
            |_, ()| {},
        );
        assert!(result.is_none());
    }

    /// `ChunkPlan::from_matches` reads `--chunk-size`; an absent flag plans
    /// the whole window as one chunk.
    #[test]
    fn chunk_plan_reads_the_flag() {
        let (test, _) = alias_history();
        let plan = |extra: &[&str]| ChunkPlan::from_matches(&run_matches(&test, extra));
        assert_eq!(
            plan(&[]).expect("plan").size,
            ChunkPlan::whole_window().size
        );
        assert_eq!(plan(&["--chunk-size", "7"]).expect("plan").size, 7);
        for bad in ["0", "-3"] {
            let flag = format!("--chunk-size={bad}");
            match plan(&[flag.as_str()]) {
                Err(HandlerError::InvalidFlag(msg)) => assert_eq!(
                    msg,
                    format!("invalid --chunk-size: must be >= 1, got {bad}")
                ),
                Err(other) => panic!("expected InvalidFlag, got {other:?}"),
                Ok(plan) => panic!("--chunk-size {bad} accepted as {}", plan.size),
            }
        }
    }

    /// The walk the plan tests name.
    const PLAN_WALK: &str = "history/devs";
    /// A walk cost for the plan tests: 64 MiB per worker, 4 KiB per product.
    const PLAN_COST: WalkCost = WalkCost {
        worker_bytes: 64 * cf_units::MIB,
        product_bytes: 4 * 1024,
    };
    /// A window long enough for the budget, not its length, to bound a chunk.
    const PLAN_WINDOW: usize = 100_000;

    /// `--chunk-size` fixes the chunk on every core, whatever the budget.
    #[test]
    fn the_chunk_size_flag_wins_over_the_budget() {
        let (test, _) = alias_history();
        let sub = run_matches(&test, &["--chunk-size", "3", "--memory-budget", "256MB"]);
        let plan = ChunkPlan::for_walk(&sub, PLAN_WALK, PLAN_WINDOW, PLAN_COST).expect("plan");
        assert_eq!(plan, ChunkPlan::sized(3));
    }

    /// `--memory-budget` plans the walk against the budget less libgit2's
    /// part (one history walk selected, so the rest is its share).
    #[test]
    fn a_memory_budget_plans_the_walk() {
        let (test, _) = alias_history();
        let sub = run_matches(&test, &["--memory-budget", "1GB"]);
        let plan = ChunkPlan::for_walk(&sub, PLAN_WALK, PLAN_WINDOW, PLAN_COST).expect("plan");
        assert_eq!(
            plan,
            plan_walk(
                PLAN_WINDOW,
                HistoryRun::workers(),
                1_000_000_000 - native_bytes(1_000_000_000),
                PLAN_COST
            )
        );
    }

    /// A walk that runs before the others start plans against the whole
    /// budget less libgit2's part; a walk running next to another plans
    /// against the base overhead (paid once per process) plus half of the
    /// rest.
    #[test]
    fn a_lone_walk_plans_against_the_whole_budget() {
        let (test, _) = alias_history();
        let sub = run_matches(
            &test,
            &[
                "-a",
                "history/devs,history/couples",
                "--memory-budget",
                "1GB",
            ],
        );
        let shared = ChunkPlan::for_walk(&sub, PLAN_WALK, PLAN_WINDOW, PLAN_COST).expect("plan");
        let lone = ChunkPlan::for_lone_walk(&sub, PLAN_WALK, PLAN_WINDOW, PLAN_COST).expect("plan");
        let cores = HistoryRun::workers();
        let walks = 1_000_000_000 - BASE_OVERHEAD - native_bytes(1_000_000_000);
        let half = BASE_OVERHEAD + walks / 2;
        assert_eq!(shared, plan_walk(PLAN_WINDOW, cores, half, PLAN_COST));
        assert_eq!(
            lone,
            plan_walk(PLAN_WINDOW, cores, BASE_OVERHEAD + walks, PLAN_COST)
        );
    }

    /// Without either flag the automatic budget plans the walk.
    #[test]
    fn without_flags_the_automatic_budget_plans_the_walk() {
        let (test, _) = alias_history();
        let plan = ChunkPlan::for_walk(&run_matches(&test, &[]), PLAN_WALK, PLAN_WINDOW, PLAN_COST)
            .expect("plan");
        let budget = super::super::budget::default_budget();
        assert_eq!(
            plan,
            plan_walk(
                PLAN_WINDOW,
                HistoryRun::workers(),
                budget - native_bytes(budget),
                PLAN_COST
            )
        );
    }

    /// The `-v` plan line names the walk, the chunk, the chunk count, the
    /// workers and where the plan came from.
    #[test]
    fn the_plan_line_describes_the_plan() {
        let plan = ChunkPlan {
            size: 1000,
            workers: 8,
        };
        assert_eq!(
            plan.describe(PLAN_WALK, 2500, PlanSource::ChunkSize),
            "progress: history/devs plan: 1000 commits per chunk, 3 chunks, 8 workers (--chunk-size)"
        );
        assert_eq!(
            plan.describe(PLAN_WALK, 0, PlanSource::Budget(512 * cf_units::MIB)),
            "progress: history/devs plan: 1000 commits per chunk, 0 chunks, 8 workers (budget 512 MiB)"
        );
    }

    /// The preflight rejects a bad `--chunk-size` before any walk.
    #[test]
    fn preflight_rejects_a_zero_chunk_size() {
        let (test, _) = alias_history();
        match super::super::validate_history_run(&run_matches(&test, &["--chunk-size", "0"])) {
            Err(HandlerError::InvalidFlag(msg)) => {
                assert_eq!(msg, "invalid --chunk-size: must be >= 1, got 0");
            }
            other => panic!("expected InvalidFlag, got {other:?}"),
        }
    }
}
