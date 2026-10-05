//! The SHARED history-walk core every `history/*` analyzer runs through, plus
//! one submodule per analyzer owning that analyzer's per-commit product and
//! report shaping.
//!
//! The shared pieces live here so they exist exactly once:
//!
//! * [`HistoryOptions`] — the validated run flags (`--head`, `--limit`,
//!   `--first-parent`, `--since`, `--tick-size`, `--max-changes-per-commit`,
//!   `--exact-signatures`, `--people-dict`, `--anonymize`, the `--languages`
//!   / vendor / generated change filter);
//! * [`HistoryRun`] — the opened repository, the commit window (oldest-first),
//!   the change filter, and the identity/tick configuration;
//! * [`Stamper`] — the ONE identity + tick assignment, one commit at a time
//!   in window order (author ids first-seen oldest-first, ticks monotonic);
//! * [`walk_chunks`] — the chunked walk driver: the pure per-commit pre-pass
//!   (tree diff, line stats, parses) runs on a worker pool
//!   ([`pool::prepare_in_order`]) at most one [`ChunkPlan`] chunk ahead of
//!   the fold, and each kept commit is stamped and folded into the walk's
//!   reduce state in window order, then its product is dropped. The output is
//!   a pure function of the repository and the flags — never of the host CPU
//!   count or the chunk size;
//! * [`Gated`] / [`gated_product`] — the `--max-changes-per-commit` drop gate
//!   every walk applies to its RAW tree diff before stamping.
//!
//! The analyzer MATH lives in the cf-* crates the submodules call; report
//! bytes route through cf-gojson / cf-goyaml / cf-reportutil.

pub mod anomaly;
mod budget;
mod chunks;
pub mod devs;
pub mod file_history;
#[cfg(test)]
pub(crate) mod fold_testutil;
pub mod imports;
mod pool;
pub mod quality;
pub mod sentiment;
pub mod typos;

pub use anomaly::{anomaly_ndjson_records, anomaly_run_metrics, anomaly_timeseries_contribution};
pub use devs::{
    devs_ndjson_records, devs_run_metrics, devs_run_report, devs_run_report_bin,
    devs_run_report_yaml, devs_timeseries_contribution,
};
pub use file_history::{
    file_history_ndjson_records, file_history_report_value, file_history_run_metrics,
    file_history_timeseries_contribution,
};
pub use imports::{
    imports_ndjson_records, imports_run_metrics, imports_run_usage_counts,
    imports_timeseries_contribution,
};
pub use quality::{quality_metrics, quality_ndjson_records, quality_timeseries_contribution};
pub use sentiment::{
    sentiment_metrics, sentiment_ndjson_records, sentiment_timeseries_contribution,
};
pub use typos::{
    typos_ndjson_records, typos_report_data, typos_run_report, typos_run_report_bin,
    typos_run_report_yaml,
};

pub(crate) use budget::{limit_native_memory, other_walks_follow_the_uast_pass, WalkCost};
pub(crate) use chunks::{fold_planned, walk_chunks, ChunkPlan, StampSummary, Stamper};

use cf_analyzers_plumbing::{IdentityDetector, Signature};
use cf_gitlib::Hash;

use crate::handlers::{
    history_filter, history_since_spec, load_history_commit_hashes, run_repo_path, HistoryFilter,
    HistoryWindow, SinceSpec,
};
use crate::pipeline::HandlerError;

thread_local! {
    /// One [`cf_uast::Parser`] per worker thread, reused across every commit that
    /// thread processes in a pooled ([`pool::prepare_in_order`]) UAST walk. `Parser::new()`
    /// registers a lazy parser for every embedded language mapping (hundreds of
    /// bloom inserts), so constructing it per commit dominated the parallel
    /// runtime. A thread-local amortizes that construction to once per thread
    /// while keeping each thread its OWN parser (tree-sitter parsers are not
    /// thread-safe, so the parser is never shared ACROSS threads).
    static UAST_PARSER: cf_uast::Parser = cf_uast::Parser::new();
}

/// Runs `f` with this worker thread's reusable [`cf_uast::Parser`] (see
/// [`UAST_PARSER`]).
pub(crate) fn with_uast_parser<R>(f: impl FnOnce(&cf_uast::Parser) -> R) -> R {
    UAST_PARSER.with(|p| f(p))
}

/// Default `--max-changes-per-commit` cap (`0` on the flag).
const DEFAULT_MAX_CHANGES_PER_COMMIT: usize = 10_000;

/// The validated history run flags.
#[derive(Debug, Clone)]
pub(crate) struct HistoryOptions {
    /// `--head`: the window is exactly `[HEAD]`.
    pub head: bool,
    /// `--limit`: the N newest commits (`0` = unlimited).
    pub limit: usize,
    /// `--first-parent` as the user set it (burndown forces its own walk).
    pub first_parent: bool,
    /// `--since`.
    pub since: SinceSpec,
    /// `--tick-size` in hours (validated `> 0`).
    pub tick_size_hours: i64,
    /// `--max-changes-per-commit` (`0` ⇒ the default cap).
    pub max_changes: usize,
    /// `--exact-signatures`.
    pub exact_signatures: bool,
    /// `--people-dict` path when set.
    pub people_dict: Option<String>,
    /// `--anonymize`.
    pub anonymize: bool,
    /// The shared change filter (`--languages`, vendor / generated policy).
    pub filter: HistoryFilter,
}

/// Reads an `i64` flag that may be absent from a reduced command tree.
pub(crate) fn flag_i64(sub: &clap::ArgMatches, name: &str, default: i64) -> i64 {
    sub.try_get_one::<i64>(name)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(default)
}

/// Reads an `f64` flag that may be absent from a reduced command tree.
pub(crate) fn flag_f64(sub: &clap::ArgMatches, name: &str, default: f64) -> f64 {
    sub.try_get_one::<f64>(name)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(default)
}

/// Reads a boolean dynamic flag (registered as `--flag[=<bool>]`).
fn flag_bool(sub: &clap::ArgMatches, name: &str) -> bool {
    sub.try_get_one::<bool>(name)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
}

/// Reads a string dynamic flag; empty ⇒ `None`.
fn flag_string(sub: &clap::ArgMatches, name: &str) -> Option<String> {
    sub.try_get_one::<String>(name)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .cloned()
}

impl HistoryOptions {
    /// Parses and validates the history flags.
    ///
    /// # Errors
    ///
    /// [`HandlerError::InvalidFlag`] for a negative `--limit`, a non-positive
    /// `--tick-size`, a negative `--max-changes-per-commit`, or an unknown
    /// `--languages` token (the anomaly flags are validated by
    /// [`validate_history_run`]).
    pub(crate) fn from_matches(sub: &clap::ArgMatches) -> Result<Self, HandlerError> {
        let limit = flag_i64(sub, "limit", 0);
        if limit < 0 {
            return Err(HandlerError::InvalidFlag(format!(
                "invalid --limit: must be >= 0, got {limit}"
            )));
        }
        let tick_size_hours = flag_i64(
            sub,
            "tick-size",
            cf_analyzers_plumbing::DEFAULT_TICK_SIZE_HOURS,
        );
        if tick_size_hours <= 0 {
            return Err(HandlerError::InvalidFlag(format!(
                "invalid --tick-size: must be > 0, got {tick_size_hours}"
            )));
        }
        let max_changes = flag_i64(sub, "max-changes-per-commit", 0);
        if max_changes < 0 {
            return Err(HandlerError::InvalidFlag(format!(
                "invalid --max-changes-per-commit: must be >= 0, got {max_changes}"
            )));
        }
        let filter = history_filter(sub).map_err(HandlerError::InvalidFlag)?;
        Ok(Self {
            head: sub.get_flag("head"),
            limit: limit as usize,
            first_parent: sub.get_flag("first-parent"),
            since: history_since_spec(sub),
            tick_size_hours,
            max_changes: if max_changes == 0 {
                DEFAULT_MAX_CHANGES_PER_COMMIT
            } else {
                max_changes as usize
            },
            exact_signatures: flag_bool(sub, "exact-signatures"),
            people_dict: flag_string(sub, "people-dict"),
            anonymize: flag_bool(sub, "anonymize"),
            filter,
        })
    }

    /// The identity detector configured by `--exact-signatures` and
    /// `--people-dict`.
    fn identity(&self) -> std::io::Result<IdentityDetector> {
        let mut identity = IdentityDetector::new();
        identity.exact_signatures = self.exact_signatures;
        if let Some(path) = &self.people_dict {
            identity.load_people_dict(path)?;
        }
        Ok(identity)
    }
}

/// Validates everything a history run needs BEFORE any walk: the flags, the
/// repository, the `--since` spec against it, the `--people-dict` file, and
/// the shotness DSL expressions.
///
/// # Errors
///
/// The [`HandlerError`] naming the first failing input.
pub(crate) fn validate_history_run(sub: &clap::ArgMatches) -> Result<(), HandlerError> {
    let opts = HistoryOptions::from_matches(sub)?;
    ChunkPlan::from_matches(sub)?;
    budget::memory_budget_flag(sub)?;
    validate_positive_flags(sub)?;
    anomaly::validate_anomaly_flags(sub)?;
    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path)
        .map_err(|_| HandlerError::CannotOpenRepository(path.clone()))?;
    if let SinceSpec::Active(spec) = &opts.since {
        repo.resolve_time(spec).map_err(|e| {
            HandlerError::InvalidFlag(format!("cannot resolve --since {spec:?}: {e}"))
        })?;
    }
    if let Some(dict) = &opts.people_dict {
        opts.identity().map_err(|e| {
            HandlerError::InvalidFlag(format!("cannot read --people-dict {dict}: {e}"))
        })?;
    }
    sentiment::validate_sentiment_flags(sub)?;
    crate::handlers::shotness_run::validate_shotness_dsl(sub)?;
    Ok(())
}

/// Analyzer flags whose non-positive values have no meaning: the burndown
/// band and sample sizes and the typo distance.
const POSITIVE_FLAGS: &[&str] = &["granularity", "sampling", "typos-max-distance"];

/// Rejects a non-positive value of every [`POSITIVE_FLAGS`] flag (it used to
/// be replaced by the default without a word).
///
/// # Errors
///
/// [`HandlerError::InvalidFlag`] naming the first offending flag.
fn validate_positive_flags(sub: &clap::ArgMatches) -> Result<(), HandlerError> {
    for flag in POSITIVE_FLAGS {
        if let Some(value) = sub.try_get_one::<i64>(flag).ok().flatten().copied() {
            if value <= 0 {
                return Err(HandlerError::InvalidFlag(format!(
                    "invalid --{flag}: must be > 0, got {value}"
                )));
            }
        }
    }
    Ok(())
}

/// One history run: the repository, the oldest-first commit window and the
/// per-run configuration every walk reads.
pub(crate) struct HistoryRun {
    /// The repository path as given on the command line.
    pub path: String,
    /// The opened repository (main-thread handle).
    pub repo: cf_gitlib::Repository,
    /// The commit window, oldest-first.
    pub hashes: Vec<Hash>,
    /// The shared change filter.
    pub filter: HistoryFilter,
    /// The effective first-parent mode of THIS walk: a merge is treated as an
    /// ordinary single-parent commit when set.
    pub first_parent: bool,
    /// `--max-changes-per-commit` cap: commits whose raw tree diff exceeds it
    /// are dropped before any analyzer sees them.
    pub max_changes: usize,
    tick_size_hours: i64,
    anonymize: bool,
    identity_seed: IdentityDetector,
}

impl HistoryRun {
    /// Opens the run with the user's `--first-parent` setting.
    pub(crate) fn open(sub: &clap::ArgMatches) -> Option<Self> {
        let opts = HistoryOptions::from_matches(sub).ok()?;
        let first_parent = opts.first_parent;
        Self::open_with(sub, opts, first_parent)
    }

    /// Opens the run with first-parent simplification FORCED on — the burndown
    /// walk, whose line-survival tracking follows the main line only. This
    /// never leaks into any other analyzer's walk.
    pub(crate) fn open_first_parent(sub: &clap::ArgMatches) -> Option<Self> {
        let opts = HistoryOptions::from_matches(sub).ok()?;
        Self::open_with(sub, opts, true)
    }

    fn open_with(sub: &clap::ArgMatches, opts: HistoryOptions, first_parent: bool) -> Option<Self> {
        let path = run_repo_path(sub);
        let repo = cf_gitlib::Repository::open(&path).ok()?;
        let hashes = load_history_commit_hashes(
            &repo,
            &HistoryWindow {
                head: opts.head,
                limit: opts.limit,
                first_parent,
                since: opts.since.clone(),
            },
        )?;
        let identity_seed = opts.identity().ok()?;
        Some(Self {
            path,
            repo,
            hashes,
            filter: opts.filter,
            first_parent,
            max_changes: opts.max_changes,
            tick_size_hours: opts.tick_size_hours,
            anonymize: opts.anonymize,
            identity_seed,
        })
    }

    /// The configured tick size in hours.
    pub(crate) const fn tick_size_hours(&self) -> i64 {
        self.tick_size_hours
    }

    /// Worker count for the pure per-commit pre-pass. Only the WORK is
    /// distributed; every reduce runs in chronological order, so this never
    /// reaches the output.
    pub(crate) fn workers() -> usize {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    }
}

/// The run-level stamps of one walked commit: the identity and tick the
/// shared core assigned, plus the commit facts every per-commit record emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitStamp {
    /// Position in the ORIGINAL window (oversized-dropped commits count).
    pub pos: usize,
    /// The commit hash.
    pub hash: Hash,
    /// The commit hash, 40 hex digits.
    pub hex: String,
    /// The tick (`--tick-size` buckets, monotonic).
    pub tick: i64,
    /// The author id (loose or exact identity, `--people-dict` aware).
    pub author_id: i64,
    /// Committer time, unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
    /// Number of parents.
    pub num_parents: usize,
}

/// Whether a commit with `num_parents` is a merge for a walk in `first_parent`
/// mode: under first-parent simplification a merge is visited as an ordinary
/// single-parent commit (its diff against `parent(0)` is a real change set),
/// so it is NOT treated as a merge. Free function so the `!Sync` run handle
/// never has to be captured by the parallel pre-pass.
pub(crate) const fn is_merge(num_parents: usize, first_parent: bool) -> bool {
    num_parents > 1 && !first_parent
}

/// The identity-detector view of a commit's author.
pub(crate) fn author_signature(commit: &cf_gitlib::commit::Commit<'_>) -> Signature {
    let gsig = commit.author();
    Signature {
        name: gsig.name,
        email: gsig.email,
        when_unix: gsig.when.seconds(),
    }
}

/// Computes a commit's tree changes against its first parent (root commit →
/// the full initial tree) — the ONE diff every history walk starts from.
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

/// One commit's pre-pass result behind the `--max-changes-per-commit` gate:
/// the RAW (pre-filter) tree-diff change count the gate reads, and the
/// analyzer's product over the FILTERED changes. An oversized commit carries
/// the default product (it does no per-file work) and is dropped before
/// stamping, exactly as every other walk drops it.
pub(crate) struct Gated<P> {
    /// RAW (pre-filter) tree-diff change count.
    pub raw_change_count: usize,
    /// The analyzer's per-commit product.
    pub product: P,
}

impl<P> Gated<P> {
    /// Whether the commit's raw diff exceeds `max_changes`, so no analyzer
    /// sees it.
    pub(crate) const fn oversized(&self, max_changes: usize) -> bool {
        self.raw_change_count > max_changes
    }
}

/// The gated pre-pass of a walk whose product is a pure function of the
/// commit and its FILTERED changes: the raw tree diff against the first
/// parent, the drop gate on its size, the shared change filter, then
/// `compute`. `None` on a fatal git error.
pub(crate) fn gated_product<P: Default>(
    repo: &cf_gitlib::Repository,
    hash: Hash,
    filter: &HistoryFilter,
    max_changes: usize,
    compute: impl FnOnce(&cf_gitlib::commit::Commit<'_>, &[cf_gitlib::changes::Change]) -> P,
) -> Option<Gated<P>> {
    let commit = repo.lookup_commit(hash).ok()?;
    let mut changes = commit_tree_changes(repo, &commit)?;
    let raw_change_count = changes.len();
    if raw_change_count > max_changes {
        return Some(Gated {
            raw_change_count,
            product: P::default(),
        });
    }
    filter.retain_changes(repo, &mut changes);
    Some(Gated {
        raw_change_count,
        product: compute(&commit, &changes),
    })
}

/// Runs `compute` for every hash in parallel across `workers` OS threads,
/// returning the results in the SAME order as `hashes` (or `None` if any commit
/// hits a fatal git error, mirroring the sequential `.ok()?` contract).
///
/// The whole-window form of [`pool::prepare_in_order`]: every product is
/// kept, so use it only for products that are small per commit (burndown's
/// raw change counts); walks fold through [`walk_chunks`] instead.
pub(crate) fn parallel_prepare<T: Send>(
    repo_path: &str,
    hashes: &[Hash],
    workers: usize,
    compute: impl Fn(&cf_gitlib::Repository, Hash) -> Option<T> + Sync,
) -> Option<Vec<T>> {
    let mut out = Vec::with_capacity(hashes.len());
    pool::prepare_in_order(
        repo_path,
        hashes,
        workers,
        hashes.len(),
        compute,
        |_, product| {
            out.push(product);
            Some(())
        },
    )?;
    Some(out)
}

/// Per-change `(added, removed, changed)` line stats for a Modify change over
/// the libgit2 line-diff op stream (`diff_blob_line_ops`): a Delete run
/// immediately followed by an Insert run is reclassified as "changed" lines
/// (counted in neither added nor removed), which is the churn model every
/// line-stat consumer (devs, file-history, anomaly) shares.
pub(crate) fn compute_diff_line_stats(
    repo: &cf_gitlib::Repository,
    from: Hash,
    to: Hash,
    old_lines: i64,
) -> (i64, i64, i64) {
    use cf_gitlib::diff::{diff_blob_line_ops, LineOp};
    let Ok(ops) = diff_blob_line_ops(repo.native(), from, to, old_lines) else {
        return (0, 0, 0);
    };
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
            LineOp::Insert(delta) => {
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

/// Detects the language of a changed file for the per-language breakdowns:
/// `""` for a binary blob, else the full enry cascade over the path AND the
/// blob content (`cf-langpath`), so `.h` resolves to C/C++/Objective-C by
/// content and an extensionless script by its shebang. `None` (no strategy
/// matched) maps to `""`, the "Other" bucket.
pub(crate) fn detect_language(name: &str, data: &[u8]) -> String {
    if cf_textutil::is_binary(data) {
        return String::new();
    }
    cf_langpath::language_by_path_with_content(name, data).unwrap_or_default()
}
