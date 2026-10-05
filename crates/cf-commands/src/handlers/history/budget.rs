//! The history memory budget: `--memory-budget` (or the automatic budget)
//! turned into each walk's chunk size and worker count.
//!
//! The model counts what a walk controls (FRD-bounded-memory-history): a
//! fixed base (binary, libgit2, embedded parsers), one working set per worker
//! (its largest commit's tree diff, blobs and parses; glibc keeps each
//! worker's high-water mark) and the products in flight between the workers
//! and the fold. Half of what the base leaves is reserved for worker working
//! sets, the other half for products in flight and fold state. libgit2's
//! object cache and mapped pack windows take a fixed part of the budget
//! first ([`native_limits`]). Fold state grows with the window whatever the
//! chunk size; it is not planned here.

use cf_streaming::Planner;
use cf_units::{GIB, MIB};

use super::chunks::ChunkPlan;
use crate::pipeline::HandlerError;

/// The `--memory-budget` flag name.
pub(crate) const FLAG_MEMORY_BUDGET: &str = "memory-budget";

/// Resident memory a history run holds before any walk work: the binary,
/// libgit2 and the embedded parsers. Measured on kubernetes: one-commit
/// `history/devs`, `history/sentiment` and `history/burndown` runs peak at
/// 76, 79 and 102 MB.
pub(crate) const BASE_OVERHEAD: i64 = 128 * MIB;

/// The smallest budget accepted: the base overhead plus room for one worker
/// and a minimal chunk.
pub(crate) const MIN_MEMORY_BUDGET: i64 = 256 * MIB;

/// Ceiling of the automatic budget (the Go default).
const AUTO_BUDGET_CAP: i64 = 2 * GIB;

/// Percent of physical RAM the automatic budget may take.
const AUTO_BUDGET_RAM_PERCENT: u64 = 50;

/// Percent of the memory above [`BASE_OVERHEAD`] reserved for worker
/// working sets; the rest holds products in flight and fold state.
const WORKER_SHARE_PERCENT: i64 = 50;

/// Divisor of the percentages above.
const PERCENT: i64 = 100;

/// Products in flight per worker, at least, so no worker waits for the fold
/// to make room while another commit is still being prepared.
pub(crate) const MIN_PRODUCTS_PER_WORKER: usize = 2;

/// What one walk costs, measured per analyzer on kubernetes prefixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalkCost {
    /// Resident bytes one worker holds for its largest commit.
    pub worker_bytes: i64,
    /// Bytes one commit's product holds while it waits for the fold.
    pub product_bytes: i64,
}

/// The `--memory-budget` value in bytes, `None` when the flag is absent.
///
/// # Errors
///
/// [`HandlerError::InvalidFlag`] for a value that is not a size or is below
/// [`MIN_MEMORY_BUDGET`].
pub(crate) fn memory_budget_flag(sub: &clap::ArgMatches) -> Result<Option<i64>, HandlerError> {
    let Some(raw) = sub.try_get_one::<String>(FLAG_MEMORY_BUDGET).ok().flatten() else {
        return Ok(None);
    };
    let invalid = |why: &str| {
        HandlerError::InvalidFlag(format!("invalid --{FLAG_MEMORY_BUDGET}: {raw:?} {why}"))
    };
    let bytes = cf_framework::config::parse_bytes(raw.trim())
        .ok()
        .and_then(|b| i64::try_from(b).ok())
        .ok_or_else(|| invalid("is not a size (examples: 512MB, 2GB, 1.5GiB)"))?;
    if bytes < MIN_MEMORY_BUDGET {
        return Err(invalid(&format!(
            "is below the minimum {} MiB",
            MIN_MEMORY_BUDGET / MIB
        )));
    }
    Ok(Some(bytes))
}

/// The budget used without `--memory-budget`: half the physical RAM, at most
/// [`AUTO_BUDGET_CAP`]; the cap when the RAM size is unknown (`0`).
pub(crate) fn auto_budget(total_ram: u64) -> i64 {
    if total_ram == 0 {
        return AUTO_BUDGET_CAP;
    }
    let share = u128::from(total_ram) * u128::from(AUTO_BUDGET_RAM_PERCENT) / (PERCENT as u128);
    i64::try_from(share).map_or(AUTO_BUDGET_CAP, |share| share.min(AUTO_BUDGET_CAP))
}

/// The automatic budget of this host.
pub(crate) fn default_budget() -> i64 {
    auto_budget(cf_framework::coordinator::detect_total_memory_bytes())
}

/// The budget the run plans against: `--memory-budget`, or the automatic
/// budget; `None` when the flag is invalid (the preflight reports it).
pub(crate) fn run_budget(sub: &clap::ArgMatches) -> Option<i64> {
    memory_budget_flag(sub)
        .ok()
        .map(|flag| flag.unwrap_or_else(default_budget))
}

/// Percent of the budget libgit2's object cache may hold. Measured on the
/// 31,923-commit kubernetes prefix (the five non-UAST history walks run
/// together under the automatic budget): libgit2's 256 MiB default ran 183 s,
/// 512 MiB 109 s, 1 GiB 85 s; the parallel walks' repository handles share
/// the cache and evict each other's trees below that.
const OBJECT_CACHE_PERCENT: i64 = 25;

/// libgit2's part of a run within `budget`: a quarter for its object cache
/// and the Go split for mapped pack windows (a quarter of the budget for
/// libgit2, 30 % of that for pack windows: 7.5 %).
pub(crate) fn native_limits(budget: i64) -> cf_gitlib::NativeLimits {
    let bytes = |b: i64| usize::try_from(b).unwrap_or(0);
    cf_gitlib::NativeLimits {
        mapped_pack: bytes(cf_budget::native_limits_for_budget(budget).mwindow_mapped_limit),
        object_cache: bytes(
            i64::try_from(
                i128::from(budget) * i128::from(OBJECT_CACHE_PERCENT) / i128::from(PERCENT),
            )
            .unwrap_or(0),
        ),
    }
}

/// The bytes of `budget` libgit2 may hold ([`native_limits`]); the walks
/// share the rest.
pub(crate) fn native_bytes(budget: i64) -> i64 {
    let limits = native_limits(budget);
    i64::try_from(limits.mapped_pack + limits.object_cache).unwrap_or(i64::MAX)
}

/// Registers the run's libgit2 memory limits; must run before the process
/// opens its first repository (the history preflight is the first to open
/// one).
pub(crate) fn limit_native_memory(sub: &clap::ArgMatches) {
    if let Some(budget) = run_budget(sub) {
        cf_gitlib::limit_native_memory(native_limits(budget));
    }
}

/// The history analyzers that share ONE pass ([`crate::handlers::uast_walk`]).
const UAST_PASS_IDS: [&str; 5] = [
    "history/quality",
    "history/sentiment",
    "history/shotness",
    "history/imports",
    "history/typos",
];

/// The history analyzer ids this run selects, in phase order.
fn selected_history_ids(sub: &clap::ArgMatches) -> Vec<String> {
    let ctx = crate::pipeline::RunContext::from_matches(sub);
    let ids = ctx.analyzer_ids();
    let patterns: Vec<&str> = ids.iter().map(String::as_str).collect();
    crate::handlers::expand_history_phase_ids(&patterns)
}

/// How many history walks run at once and so share the budget: one per
/// selected history analyzer, the UAST analyzers counting once (they share
/// one pass), at most [`crate::handlers::ANALYZER_CONCURRENCY`].
pub(crate) fn concurrent_walks(sub: &clap::ArgMatches) -> usize {
    let selected = selected_history_ids(sub);
    let uast = selected
        .iter()
        .filter(|id| UAST_PASS_IDS.contains(&id.as_str()))
        .count();
    (selected.len() - uast + usize::from(uast > 0)).clamp(1, crate::handlers::ANALYZER_CONCURRENCY)
}

/// Whether history walks other than the UAST pass run in this invocation
/// (a non-UAST history analyzer is selected): the pass's finished results
/// then wait for rendering while those walks run.
pub(crate) fn other_walks_follow_the_uast_pass(sub: &clap::ArgMatches) -> bool {
    selected_history_ids(sub)
        .iter()
        .any(|id| !UAST_PASS_IDS.contains(&id.as_str()))
}

/// The plan of one walk over `commits` commits on `cores` cores within
/// `share` bytes: as many workers as the worker reserve holds (at least one,
/// at most `cores`), and the chunk [`Planner`] fits in the rest, never
/// smaller than [`MIN_PRODUCTS_PER_WORKER`] per worker nor larger than the
/// window.
pub(crate) fn plan_walk(commits: usize, cores: usize, share: i64, cost: WalkCost) -> ChunkPlan {
    let available = share.saturating_sub(BASE_OVERHEAD).max(0);
    let worker_reserve = available / PERCENT * WORKER_SHARE_PERCENT;
    let fit = usize::try_from(worker_reserve / cost.worker_bytes.max(1)).unwrap_or(usize::MAX);
    let workers = fit.clamp(1, cores.max(1));
    let planner = Planner {
        total_commits: i64::try_from(commits).unwrap_or(i64::MAX),
        memory_budget: share,
        aggregate_growth_per_commit: cost.product_bytes.max(1),
        pipeline_overhead: BASE_OVERHEAD + worker_reserve,
    };
    let planned = planner.plan().first().map_or(1, |c| c.end - c.start);
    let size = planned
        .max(MIN_PRODUCTS_PER_WORKER * workers)
        .min(commits)
        .max(1);
    ChunkPlan { size, workers }
}

#[cfg(test)]
mod tests {
    use cf_units::{GIB, MIB};

    use super::*;

    /// A walk whose worker holds 64 MiB and whose product is 4 KiB.
    const COST: WalkCost = WalkCost {
        worker_bytes: 64 * MIB,
        product_bytes: 4 * 1024,
    };
    /// Commits in a large window.
    const WINDOW: usize = 100_000;
    /// Cores of a large host.
    const CORES: usize = 24;

    fn matches(argv: &[&str]) -> clap::ArgMatches {
        crate::build_codefang_command()
            .try_get_matches_from(["codefang", "run"].iter().chain(argv).copied())
            .expect("run args parse")
            .subcommand_matches("run")
            .expect("run")
            .clone()
    }

    fn budget(value: &str) -> Result<Option<i64>, HandlerError> {
        memory_budget_flag(&matches(&["--memory-budget", value, "."]))
    }

    fn message(result: Result<Option<i64>, HandlerError>) -> String {
        result.expect_err("an invalid budget").to_string()
    }

    #[test]
    fn a_budget_reads_si_and_iec_sizes() {
        assert_eq!(budget("512MB").expect("ok"), Some(512_000_000));
        assert_eq!(budget("2GiB").expect("ok"), Some(2 * GIB));
        assert_eq!(budget("1.5GB").expect("ok"), Some(1_500_000_000));
        assert_eq!(memory_budget_flag(&matches(&["."])).expect("ok"), None);
    }

    #[test]
    fn an_unparsable_budget_is_a_named_error() {
        let msg = message(budget("banana"));
        assert!(
            msg.starts_with("invalid --memory-budget: \"banana\" is not a size"),
            "{msg}"
        );
    }

    #[test]
    fn a_budget_below_the_minimum_is_a_named_error() {
        let msg = message(budget("1MB"));
        assert!(
            msg.starts_with("invalid --memory-budget: \"1MB\" is below the minimum"),
            "{msg}"
        );
        assert!(budget(&format!("{MIN_MEMORY_BUDGET}")).is_ok());
    }

    #[test]
    fn the_automatic_budget_is_half_the_ram_up_to_two_gib() {
        assert_eq!(auto_budget(8 * GIB as u64), 2 * GIB);
        assert_eq!(auto_budget(2 * GIB as u64), GIB);
        assert_eq!(auto_budget(0), 2 * GIB, "unknown RAM");
    }

    #[test]
    fn more_budget_never_shrinks_the_plan() {
        let mut previous = plan_walk(WINDOW, CORES, MIN_MEMORY_BUDGET, COST);
        for share in [512 * MIB, GIB, 2 * GIB, 4 * GIB, 16 * GIB] {
            let plan = plan_walk(WINDOW, CORES, share, COST);
            assert!(plan.size >= previous.size, "size at {share}");
            assert!(plan.workers >= previous.workers, "workers at {share}");
            previous = plan;
        }
    }

    #[test]
    fn a_tight_budget_trades_workers_for_memory() {
        let tight = plan_walk(WINDOW, CORES, MIN_MEMORY_BUDGET, COST);
        let roomy = plan_walk(WINDOW, CORES, 16 * GIB, COST);
        assert!(tight.workers < CORES, "{tight:?}");
        assert_eq!(roomy.workers, CORES);
        let used = BASE_OVERHEAD + tight.workers as i64 * COST.worker_bytes;
        assert!(used <= MIN_MEMORY_BUDGET, "{used} > {MIN_MEMORY_BUDGET}");
    }

    #[test]
    fn workers_stay_between_one_and_the_cores() {
        for cores in [1, 2, 64] {
            for share in [MIN_MEMORY_BUDGET, 64 * GIB] {
                let plan = plan_walk(WINDOW, cores, share, COST);
                assert!((1..=cores).contains(&plan.workers), "{plan:?}");
            }
        }
    }

    #[test]
    fn the_chunk_fits_the_window_and_keeps_every_worker_busy() {
        for commits in [0, 1, 7, WINDOW] {
            for share in [MIN_MEMORY_BUDGET, 4 * GIB] {
                let plan = plan_walk(commits, CORES, share, COST);
                assert!(plan.size >= 1, "{plan:?}");
                assert!(plan.size <= commits.max(1), "{commits}: {plan:?}");
                assert!(
                    plan.size >= commits.min(MIN_PRODUCTS_PER_WORKER * plan.workers),
                    "{commits}: {plan:?}"
                );
            }
        }
    }

    /// libgit2 gets a fixed part of the budget: a quarter for its object
    /// cache and 7.5 % for mapped pack windows, both growing with the
    /// budget.
    #[test]
    fn the_native_limits_follow_the_budget() {
        let mut previous = cf_gitlib::NativeLimits::default();
        for budget in [MIN_MEMORY_BUDGET, GIB, 2 * GIB, 4 * GIB] {
            let limits = native_limits(budget);
            assert_eq!(limits.object_cache as i64, budget / 4, "{budget}");
            assert!(limits.mapped_pack > previous.mapped_pack, "{budget}");
            assert!((limits.mapped_pack as i64) < budget / 10, "{budget}");
            assert_eq!(
                native_bytes(budget),
                (limits.object_cache + limits.mapped_pack) as i64
            );
            previous = limits;
        }
    }

    /// The UAST results wait for rendering only when another history walk
    /// runs after the pass.
    #[test]
    fn other_walks_follow_the_pass_only_with_a_non_uast_history_analyzer() {
        let follow = |ids: &str| other_walks_follow_the_uast_pass(&matches(&["-a", ids, "."]));
        assert!(!follow("history/quality,history/sentiment"));
        assert!(!follow("static/complexity,history/typos"));
        assert!(follow("history/quality,history/devs"));
        assert!(follow("history/*"));
    }

    #[test]
    fn concurrent_walks_share_the_budget() {
        let walks = |ids: &str| concurrent_walks(&matches(&["-a", ids, "."]));
        assert_eq!(walks("history/devs"), 1);
        assert_eq!(walks("history/devs,history/couples"), 2);
        assert_eq!(walks("static/complexity,history/devs"), 1);
        assert_eq!(
            walks("history/quality,history/sentiment,history/typos"),
            1,
            "the UAST analyzers share one pass"
        );
        assert_eq!(walks("history/quality,history/sentiment,history/devs"), 2);
        assert_eq!(
            walks("history/devs,history/couples,history/burndown,history/anomaly,history/file-history"),
            crate::handlers::ANALYZER_CONCURRENCY
        );
    }
}
