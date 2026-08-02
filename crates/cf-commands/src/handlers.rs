//! Analyzer report handlers + the default registry wiring.
//!
//! Each submodule is the crate-owned orchestration for one analyzer family
//! (static folder walk + per-file UAST analysis, or one git revwalk + per-commit
//! analysis), moved verbatim out of the `codefang` binary's `main.rs` where it
//! had previously been reached through a 31-arm per-`(analyzer, format)` `if`
//! ladder. The analyzer MATH stays in the cf-* analyzer crates these call
//! (cf-complexity, cf-halstead, cf-comments, cf-imports, cf-couples,
//! cf-shotness, cf-analyzer-burndown, …); this module owns only the
//! pipeline-tier orchestration + serialization (cf-gojson / cf-goyaml /
//! cf-reportutil), exactly as the reference framework + run command do.
//!
//! [`default_registry`] builds the single [`crate::pipeline::Registry`] mapping
//! every analyzer id to ONE [`crate::pipeline::RunHandler`]. Each handler owns
//! its own `match format` (mirroring an analyzer's `FormatReport*` family in
//! the reference implementation); dispatch in [`crate::pipeline::run_pipeline`] is a keyed lookup by id,
//! NOT a per-format branch ladder.

pub mod burndown_ndjson;
pub mod couples_run;
pub mod go_sort;
pub mod history;
pub mod history_formats;
pub mod history_plot;
pub mod history_text;
pub mod plot;
pub mod plot_sections;
pub mod section_render;
pub mod shotness_run;
pub mod static_clones;
pub mod static_cohesion;
pub mod static_comments;
pub mod static_complexity;
pub mod static_complexity_bin;
pub mod static_complexity_yaml;
pub mod static_halstead;
pub mod static_imports;
pub mod static_json;
pub(crate) mod uast_walk;

use crate::pipeline::{AnalyzerEntry, Mode, Registry, RunContext};

/// Concurrency cap for independent per-analyzer tasks in the multi-analyzer
/// dispatch paths (combined render, plot orchestrators, the per-id pipeline).
/// Handlers are self-contained (own `Repository` handles, thread-local
/// parsers); the shared-UAST walk is pre-computed and counts as one task.
pub(crate) const ANALYZER_CONCURRENCY: usize = 4;

/// Runs `f(0..n)` across at most `cap` worker threads and returns the results
/// in index order, so the caller can WRITE its outputs in the existing
/// deterministic order while the independent computations overlap. With one
/// task (or `cap <= 1`) it degenerates to the sequential loop.
pub(crate) fn run_concurrent<T: Send>(
    n: usize,
    cap: usize,
    f: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    if n <= 1 || cap <= 1 {
        return (0..n).map(f).collect();
    }
    let workers = cap.min(n);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<T>>> =
        (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    {
        let f = &f;
        let next = &next;
        let slots = &slots;
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(move || loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let value = f(i);
                    *slots[i].lock().expect("task slot poisoned") = Some(value);
                });
            }
        });
    }
    slots
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .expect("task slot poisoned")
                .expect("every task index was dispatched exactly once")
        })
        .collect()
}

/// The process-wide cooperative cancellation flag, set by the SIGINT/SIGTERM
/// handlers installed in [`crate::install_signal_handlers`].
pub(crate) fn cancel_flag() -> &'static std::sync::Arc<std::sync::atomic::AtomicBool> {
    static FLAG: std::sync::OnceLock<std::sync::Arc<std::sync::atomic::AtomicBool>> =
        std::sync::OnceLock::new();
    FLAG.get_or_init(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
}

/// True once SIGINT/SIGTERM was received: long-running walks poll this and
/// bail out so the process can exit promptly instead of grinding through the
/// rest of the tree.
pub(crate) fn run_cancelled() -> bool {
    cancel_flag().load(std::sync::atomic::Ordering::Relaxed)
}

/// Upper bound for a single source file on the static path (64 MiB). No
/// legitimate source file approaches this; without a bound, a pathological
/// file is fully read, tree-sitter-parsed (tree several times the source
/// size), lowered to a UAST, and — on the complexity path — deep-copied
/// again, so peak RSS is a large multiple of the largest file encountered.
/// Oversized files are skipped exactly like unreadable ones.
pub(crate) const MAX_STATIC_FILE_SIZE: u64 = 64 * 1024 * 1024;

/// Count of files the static walks skipped (unreadable, oversized, or
/// unparseable). Skips used to be completely silent, so a permissions
/// failure across half a tree produced a plausible-looking but wrong report
/// with no signal; the run summary now surfaces the count on stderr.
static SKIPPED_FILES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records one skipped file for the end-of-run summary.
pub(crate) fn note_skipped_file() {
    SKIPPED_FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Number of files skipped so far in this run.
pub(crate) fn skipped_file_count() -> u64 {
    SKIPPED_FILES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reads a source file for static analysis, refusing oversized files
/// (see [`MAX_STATIC_FILE_SIZE`]). `None` mirrors the walkers' existing
/// skip-on-unreadable behavior.
pub(crate) fn read_source_capped(path: &std::path::Path) -> Option<Vec<u8>> {
    let read = || -> Option<Vec<u8>> {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > MAX_STATIC_FILE_SIZE {
            return None;
        }
        std::fs::read(path).ok()
    };
    let content = read();
    if content.is_none() {
        note_skipped_file();
    }
    content
}

/// Directory-level skip predicate for the static folder walks. The reference
/// pipeline only does `filepath.SkipDir` on `.git`; this EXTENDS that to also
/// skip any directory carrying a `CACHEDIR.TAG` — the BSD cache-directory
/// convention Cargo writes into `target/` (and that ripgrep/fd/tar honor). A
/// bare `codefang run .` in a built Rust repo would otherwise descend into a
/// multi-gigabyte `target/` tree and parse its generated sources, which looks
/// like a hang. The tag check has effectively zero false positives (a real
/// source directory never carries the signature), so analysis is unchanged on
/// repos without such dirs — including the parity gate's kubernetes checkout.
pub(crate) fn should_skip_walk_dir(path: &std::path::Path, name: &std::ffi::OsStr) -> bool {
    name == ".git" || is_cache_tagged_dir(path)
}

/// True when `dir` holds a `CACHEDIR.TAG` whose leading bytes are the standard
/// cache signature (per <https://bford.info/cachedir/>).
fn is_cache_tagged_dir(dir: &std::path::Path) -> bool {
    use std::io::Read as _;
    const SIG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";
    let Ok(mut f) = std::fs::File::open(dir.join("CACHEDIR.TAG")) else {
        return false;
    };
    let mut buf = [0u8; SIG.len()];
    f.read_exact(&mut buf).is_ok() && buf[..] == *SIG
}

// ---------------------------------------------------------------------------
// Shared pipeline helpers (path resolution, tick floor, RFC3339 formatting).
// These mirror the reference implementation / plumbing helpers and are shared by the static
// and history handlers.
// ---------------------------------------------------------------------------

/// Resolves the repository path from `run`'s positional arg or `-p/--path`
/// (the reference implementation: the positional wins when present, else `--path`, default `.`).
#[must_use]
pub fn run_repo_path(sub: &clap::ArgMatches) -> String {
    if let Some(p) = sub.get_one::<String>("path-positional") {
        if !p.is_empty() {
            return p.clone();
        }
    }
    sub.get_one::<String>("path")
        .cloned()
        .unwrap_or_else(|| ".".to_string())
}

/// The effective first-parent mode for the shared history revwalk, mirroring
/// the reference `initHistoryPipeline`: when the resolved leaf set contains
/// `burndown` and `--first-parent` is off, first-parent is forced on.
///
/// The reference implementation forces first-parent for the WHOLE history run (the single shared revwalk
/// that feeds every selected history analyzer) whenever `history/burndown` is in
/// the resolved leaf set, regardless of the `--first-parent` flag. Because every
/// history analyzer in one `run` shares that revwalk, the window selection — and
/// therefore each analyzer's tick assignment and commit set — must observe the
/// same forced flag. A handler that read only `--first-parent` would diverge from
/// the reference implementation whenever burndown is co-selected (e.g. `--analyzers history/devs,history/burndown`,
/// `history/*`, or `*`), even though it is not the burndown handler.
///
/// The burndown membership is computed over the RESOLVED leaf set, so literal ids
/// and globs (`history/*`, `*`) that select burndown all force the flag, exactly
/// as the reference implementation's `slices.Contains(analyzerKeys, "burndown")` does after glob expansion.
#[must_use]
pub fn effective_first_parent(sub: &clap::ArgMatches) -> bool {
    if sub.get_flag("first-parent") {
        return true;
    }
    let patterns: Vec<String> = sub
        .get_many::<String>("analyzers")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let pats: Vec<&str> = patterns.iter().map(String::as_str).collect();
    let (_static_ids, history_ids) = expand_combined_ids(&pats);
    history_ids.iter().any(|id| id == "history/burndown")
}

/// The parsed `--since` state of a history run (reference `--since` parity).
///
/// The reference pipeline resolves `--since` BEFORE planning the streaming
/// chunks, and the observable stdout contract has three measured classes
/// (oracle-verified against the live reference binary on hercules):
///
/// - **cutoff at/before the oldest commit** (filter excludes nothing) — output
///   is byte-identical to a run without `--since`;
/// - **cutoff after every commit** (filter excludes everything, e.g.
///   `--since 2030-01-01` or a duration like `24h`) — the planner plans ZERO
///   chunks and the run succeeds with each analyzer's empty-walk report;
/// - **partial filter** (some commits pass, some don't) — the planner counts
///   the passing commits, but the oldest-first loading iterator stops at the
///   FIRST commit older than the cutoff (the reference `Since` stop filter
///   composed with the reversed walk), yields zero commits, and the run aborts
///   (`expected N commits, got 0: EOF`) with EMPTY stdout and exit 1.
///
/// An unparseable value aborts before walking (reference: `Error: invalid time
/// format for --since`), also with empty stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinceSpec {
    /// `--since` absent or empty: the walk is unfiltered.
    Inactive,
    /// Cutoff in Unix seconds; commits with author time >= cutoff pass
    /// (the reference iterator's stop comparison uses the AUTHOR clock).
    Active(i64),
    /// Unparseable `--since` value: the run aborts with empty stdout.
    Invalid,
}

/// Resolves the run's `--since` flag into a [`SinceSpec`].
#[must_use]
pub fn history_since_spec(sub: &clap::ArgMatches) -> SinceSpec {
    let raw = sub.get_one::<String>("since").map_or("", String::as_str);
    if raw.is_empty() {
        return SinceSpec::Inactive;
    }
    match parse_since_time(raw) {
        Some(secs) => SinceSpec::Active(secs),
        None => SinceSpec::Invalid,
    }
}

/// Parses the reference `--since` value forms: RFC3339
/// (`2006-01-02T15:04:05Z07:00`), a plain UTC date (`2006-01-02`), or a Go
/// duration (`24h`, `1h30m`, …) subtracted from the current wall clock.
/// Returns the cutoff in Unix seconds, or `None` when unparseable.
fn parse_since_time(raw: &str) -> Option<i64> {
    if let Some(secs) = parse_rfc3339_secs(raw) {
        return Some(secs);
    }
    if let Some(secs) = parse_utc_date_secs(raw) {
        return Some(secs);
    }
    if let Some(dur_secs) = parse_go_duration_secs(raw) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        return Some(i64::try_from(now.as_secs()).unwrap_or(i64::MAX) - dur_secs);
    }
    None
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm;
/// the inverse of [`civil_from_days`]).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses `YYYY-MM-DD` as UTC midnight (Go `time.Parse("2006-01-02", …)`).
fn parse_utc_date_secs(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// Parses an RFC3339 timestamp (`YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`) to
/// Unix seconds (fractional seconds truncated, exactly as a seconds-granularity
/// git comparison observes them).
fn parse_rfc3339_secs(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let date_secs = parse_utc_date_secs(s.get(0..10)?)?;
    let hh: i64 = s.get(11..13)?.parse().ok()?;
    let mm: i64 = s.get(14..16)?.parse().ok()?;
    let ss: i64 = s.get(17..19)?.parse().ok()?;
    if hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Skip fractional seconds, then parse the zone designator.
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    let zone = s.get(i..)?;
    let offset_secs: i64 = if zone == "Z" || zone == "z" {
        0
    } else {
        let zb = zone.as_bytes();
        if zb.len() != 6 || (zb[0] != b'+' && zb[0] != b'-') || zb[3] != b':' {
            return None;
        }
        let oh: i64 = zone.get(1..3)?.parse().ok()?;
        let om: i64 = zone.get(4..6)?.parse().ok()?;
        let sign = if zb[0] == b'-' { -1 } else { 1 };
        sign * (oh * 3600 + om * 60)
    };
    Some(date_secs + hh * 3600 + mm * 60 + ss - offset_secs)
}

/// Parses a Go `time.ParseDuration` string (`300s`, `1h30m`, `24h`, …) to
/// whole seconds (sub-second remainder truncated).
fn parse_go_duration_secs(s: &str) -> Option<i64> {
    let mut rest = s;
    let mut total_ns: f64 = 0.0;
    let mut any = false;
    if rest.starts_with('-') || rest.starts_with('+') {
        return None; // negative/signed durations never select commits meaningfully
    }
    while !rest.is_empty() {
        let num_len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if num_len == 0 {
            return None;
        }
        let value: f64 = rest.get(0..num_len)?.parse().ok()?;
        rest = rest.get(num_len..)?;
        let (unit_ns, unit_len) = if rest.starts_with("ns") {
            (1.0, 2)
        } else if rest.starts_with("us") || rest.starts_with("µs") {
            (1e3, if rest.starts_with("µs") { 3 } else { 2 })
        } else if rest.starts_with("ms") {
            (1e6, 2)
        } else if rest.starts_with('s') {
            (1e9, 1)
        } else if rest.starts_with('m') {
            (6e10, 1)
        } else if rest.starts_with('h') {
            (3.6e12, 1)
        } else {
            return None;
        };
        total_ns += value * unit_ns;
        rest = rest.get(unit_len..)?;
        any = true;
    }
    if !any {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // contractual truncation to seconds
    Some((total_ns / 1e9) as i64)
}

/// Replicates the reference implementation `initHistoryPipeline` (the iterator path the real
/// `run` command uses — NOT `gitlib.LoadCommits`): walks history oldest-first
/// (`SortTime|SortTopological|SortReverse`) and feeds the analyzer the FIRST
/// `commitCount = min(limit, total)` commits. That selects the N OLDEST
/// reachable commits, oldest-first (oracle-verified against the live reference binary —
/// `--limit 20` on hercules yields the repo's first 20 commits, with ascending
/// composition ticks). `limit <= 0` returns the full oldest-first history.
///
/// `since` applies the reference `--since` contract (see [`SinceSpec`]): the
/// planner counts the commits at/after the cutoff (newest-first walk with the
/// stop filter), a zero count yields an EMPTY walk (each analyzer's empty
/// report), and a partial filter aborts (`None` ⇒ empty stdout) because the
/// reference oldest-first loading iterator stops at the first too-old commit
/// and under-fills the planned chunk.
#[must_use]
pub fn load_history_commit_hashes(
    repo: &cf_gitlib::Repository,
    limit: i64,
    first_parent: bool,
    since: SinceSpec,
) -> Option<Vec<cf_gitlib::Hash>> {
    use cf_gitlib::repository::LogOptions;

    match since {
        SinceSpec::Invalid => return None,
        SinceSpec::Active(cutoff) => {
            let since_time = Some(cf_gitlib::repository::time_from_unix_secs(cutoff));
            // Planner count: newest-first walk with the reference stop filter.
            let plan_opts = LogOptions {
                since: since_time,
                first_parent,
                reverse: false,
            };
            let mut plan_iter = repo.log(&plan_opts).ok()?;
            let mut n_since: i64 = 0;
            while plan_iter.next_commit().is_some() {
                n_since += 1;
            }
            if n_since == 0 {
                // Zero chunks planned: the run succeeds over an empty walk.
                return Some(Vec::new());
            }
            let expected = if limit > 0 {
                limit.min(n_since)
            } else {
                n_since
            };
            // Loading: the reference oldest-first iterator applies the SAME stop
            // filter, so it ends at the first commit older than the cutoff.
            let load_opts = LogOptions {
                since: since_time,
                first_parent,
                reverse: true,
            };
            let mut iter = repo.log(&load_opts).ok()?;
            let mut hashes = Vec::new();
            while (hashes.len() as i64) < expected {
                match iter.next_commit() {
                    Some(c) => hashes.push(c.hash()),
                    None => break,
                }
            }
            if (hashes.len() as i64) < expected {
                // Under-filled chunk: the reference pipeline aborts with empty stdout
                // ("expected N commits, got M: EOF").
                return None;
            }
            return Some(hashes);
        }
        SinceSpec::Inactive => {}
    }
    // ORACLE-VERIFIED window selection. The real `run` command uses
    // the reference `initStreamingIterator`, which sets `logOpts.Reverse = true`
    // (oldest-first walk) and then streams the FIRST `commitCount =
    // min(limit, total)` commits — i.e. the `limit` OLDEST reachable commits,
    // oldest-first. (NOT `gitlib.loadHistoryCommits`'s newest-N+reverse: the live
    // reference binary at `--limit 2` on hercules emits the repo's first two commits —
    // analyser.go/LICENSE — proving the OLDEST set is selected, even though the
    // repo has 1006 commits.) Do NOT switch to `reverse: false` + post-reverse.
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
    Some(hashes)
}

/// Returns the reference implementation's streaming-pipeline *consume* order for an oldest-first commit
/// window: the IDENTITY (oldest-first revwalk order).
///
/// At `--workers 1` (the only config the differential gate exercises) every
/// stage of the reference coordinator pipeline preserves input order, so the leaf
/// analyzers consume commits in exactly the oldest-first order they are fed:
///
/// * the reference `Stream` emits contiguous oldest-first
///   batches (`commits[i:end]`) from a single goroutine.
/// * the reference blob and diff pipelines use
///   `pipeline.RunPC`: a single producer emits jobs
///   in input order onto one FIFO channel, and a single consumer reads and
///   emits them in that same order — the parallel blob/diff prefetch only
///   shares one batched worker request, it never resequences commits.
/// * the reference `Process` is explicitly order-preserving
///   ("Output order matches input order via a slot-based approach"): the
///   `emit` goroutine waits on each slot's `done` in dispatch order.
/// * the reference coordinator `Process` and drain stage
///   `SignalOnDrain` each forward items one-for-one from a single worker.
/// * the reference `processCommitsSerial`/`hybridCommitLoop` range over
///   the coordinator's `dataChan` in arrival order, and the per-commit `Index`
///   carried through is the plain oldest-first revwalk index
///   (`batch.StartIndex + job.index`).
///
/// So the order in which the COORDINATOR pipeline yields commits is the identity
/// of the oldest-first window. Earlier code reordered into round-robin
/// `PIPELINE_CHUNK` blocks here; that was incorrect — no pipeline stage performs
/// that reordering.
///
/// This is the order the CORE (plumbing) analyzers consume — notably the
/// `IdentityDetector`, which assigns loose author ids strictly oldest-first. It
/// is NOT necessarily the order in which a LEAF analyzer's order-sensitive state
/// is updated: at the default `LeafWorkers = max(NumCPU / 3, 4)` (which
/// `--workers` does NOT override — that flag only sets the blob/diff `Workers`
/// pool), the reference implementation's hybrid leaf path (the reference `processCommitsHybrid`) forks the
/// leaf across workers, dispatching consume position `p` to worker `p % W`. The
/// effect is leaf-specific and handled at the leaf consumer, not here:
///   - couples: each fork has an INDEPENDENT seen-files Bloom; commits stay in
///     oldest-first order WITHIN a worker (see `couples_run`).
///   - file-history: forked TCs are drained worker-by-worker into one aggregator
///     whose insert handling resets a path's hash list, so its add-order is the
///     commits stably reordered by `(p % W, p)` (see `file_history_run`).
///
/// Both use [`leaf_worker_count`] for `W`, reproducing the reference binary on
/// this machine.
#[must_use]
pub fn pipeline_consume_order(hashes: Vec<cf_gitlib::Hash>) -> Vec<cf_gitlib::Hash> {
    hashes
}

/// Reference leaf-worker divisor (`framework` `leafWorkerDivisor`): `LeafWorkers =
/// NumCPU / divisor`.
const LEAF_WORKER_DIVISOR: usize = 3;
/// Reference minimum leaf-worker count (`framework` `minLeafWorkers`).
const MIN_LEAF_WORKERS: usize = 4;

/// Number of forked leaf-analyzer workers the reference implementation dispatches commits across, mirroring
/// `framework.DefaultCoordinatorConfig`: `max(NumCPU / 3, 4)`, where `NumCPU` is
/// the machine's logical CPU count.
///
/// The reference implementation's hybrid leaf path (the reference `processCommitsHybrid`, taken for a single
/// non-`SequentialOnly` leaf when `0 < CoreCount < len(Analyzers)`) forks the
/// leaf across this many workers and dispatches consume position `p` to worker
/// `p % count`, each worker holding INDEPENDENT analyzer state (e.g. couples'
/// seen-files Bloom, file-history's per-path map). That makes the order-sensitive
/// parts of those analyzers depend on this count, so a byte-exact port must use
/// the same value as the live binary on this machine (which the oracle also runs
/// on). The `--workers` flag only overrides `Workers` (the blob/diff pool), never
/// `LeafWorkers`, so this is unaffected by `--workers 1`.
#[must_use]
pub fn leaf_worker_count() -> usize {
    let num_cpu = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (num_cpu / LEAF_WORKER_DIVISOR).max(MIN_LEAF_WORKERS)
}

/// Rounds Unix `secs` down to the start of its 24-hour tick (reference:
/// `plumbing.FloorTime(when, 24h)`). `time.Round` rounds half away from zero;
/// the post-round correction yields the floor.
#[must_use]
pub fn floor_tick_secs(secs: i64) -> i64 {
    const PERIOD: i64 = 86_400;
    let rounded = ((secs + PERIOD / 2).div_euclid(PERIOD)) * PERIOD;
    if rounded > secs {
        rounded - PERIOD
    } else {
        rounded
    }
}

/// Formats Unix seconds as the reference `time.RFC3339` in the zone given by
/// `offset_minutes`; a zero offset prints the literal `Z`.
#[must_use]
pub fn format_rfc3339_offset(unix_secs: i64, offset_minutes: i32) -> String {
    let local = unix_secs + i64::from(offset_minutes) * 60;
    let days = local.div_euclid(86400);
    let secs_of_day = local.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    let date = format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}");
    if offset_minutes == 0 {
        format!("{date}Z")
    } else {
        let sign = if offset_minutes < 0 { '-' } else { '+' };
        let abs = offset_minutes.unsigned_abs();
        format!("{date}{sign}{:02}:{:02}", abs / 60, abs % 60)
    }
}

/// Serializes a history analyzer's report value across the json/yaml/bin
/// machine formats, mirroring the reference `OutputHistoryResults` +
/// `BaseHistoryAnalyzer.Serialize` (`writeMetricsToFormat`): the *same* report
/// value is encoded each way, so a handler computes the value once and routes
/// it here rather than re-deriving per format.
///
/// - `json` (a "raw" format): `json.Marshal(metrics.ToJSON())` — `json_value`,
///   no header, no trailing newline (cf-gojson `marshal`).
/// - `binary` (a "raw" format): `reportutil.EncodeBinaryEnvelope(metrics)` — a
///   CFB1 envelope wrapping the same `json_value` bytes (no header).
/// - `yaml` (non-raw): `PrintHeader` (`codefang (v2):` / version / hash) then
///   `<analyzer_name>:\n` then `yaml.Marshal(metrics.ToYAML())` — `yaml_value`.
///
/// `analyzer_name` is the history analyzer's `Name()` (the YAML section header,
/// e.g. `ImportsPerDeveloper`). Returns `None` for any non-machine format, so
/// the caller surfaces the same dispatch error the reference implementation does.
#[must_use]
pub fn serialize_history_metrics(
    format: &str,
    analyzer_name: &str,
    json_value: &cf_gojson::GoValue,
    yaml_value: &cf_gojson::GoValue,
) -> Option<Vec<u8>> {
    match format {
        // Raw formats: no version header, no per-analyzer section name.
        "json" => Some(cf_gojson::marshal(json_value)),
        "binary" | "bin" => cf_reportutil::encode_binary_envelope(json_value).ok(),
        // Non-raw: PrintHeader + "<Name>:\n" + yaml body.
        "yaml" => {
            let mut out = Vec::new();
            out.extend_from_slice(b"codefang (v2):\n");
            out.extend_from_slice(
                format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
            );
            out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
            out.extend_from_slice(format!("{analyzer_name}:\n").as_bytes());
            out.extend_from_slice(&cf_goyaml::marshal(yaml_value));
            Some(out)
        }
        _ => None,
    }
}

/// Civil date from a day count since the Unix epoch (Howard Hinnant's algorithm).
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// Static analyzer glob/bin helpers (registry-ordered multi-analyzer bin output).
// ---------------------------------------------------------------------------

/// The static analyzers in registry order (reference `defaultUASTAnalyzers ++
/// defaultRawFileAnalyzers`). `bin_ported` is true for the analyzers whose
/// `--format bin` payload is reproduced byte-for-byte; cohesion is not yet
/// ported. clones IS ported: its bin payload is the CFB1 envelope of
/// `computeMetricsFromReport` over the cross-file aggregate report.
pub const STATIC_BIN_ANALYZERS: &[(&str, bool)] = &[
    ("static/clones", true),
    ("static/complexity", true),
    ("static/comments", true),
    ("static/halstead", true),
    ("static/cohesion", true),
    ("static/imports", true),
    ("static/composition", true),
];

/// True when `pat` is a literal static analyzer ID or a glob that could match
/// static IDs (and no history ID).
#[must_use]
pub fn is_static_id_or_glob(pat: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        let any_static = STATIC_BIN_ANALYZERS
            .iter()
            .any(|(id, _)| go_path_match(pat, id));
        any_static && !history_glob_matches(pat)
    } else {
        STATIC_BIN_ANALYZERS.iter().any(|(id, _)| *id == pat)
    }
}

/// True when the glob matches any known history analyzer ID.
#[must_use]
pub fn history_glob_matches(pat: &str) -> bool {
    const HISTORY_IDS: &[&str] = &[
        "history/burndown",
        "history/couples",
        "history/devs",
        "history/file-history",
        "history/imports",
        "history/shotness",
        "history/typos",
        "history/sentiment",
        "history/quality",
        "history/anomaly",
    ];
    HISTORY_IDS.iter().any(|id| go_path_match(pat, id))
}

/// Expands the requested patterns over the registry-ordered static analyzers and
/// concatenates each selected analyzer's CFB1 bin envelope (reference:
/// `FormatPerAnalyzer(FormatBinary)`). Returns `None` if any selected analyzer
/// is not ported (clones/cohesion) or any folder walk fails.
#[must_use]
pub fn static_multi_bin(patterns: &[&str], path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let mut selected: Vec<(&str, bool)> = Vec::new();
    for &(id, ported) in STATIC_BIN_ANALYZERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if matched {
            selected.push((id, ported));
        }
    }
    if selected.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (id, ported) in selected {
        if !ported {
            return None;
        }
        let env = static_single_bin(id, path, filter)?;
        out.extend_from_slice(&env);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Static analyzer multi-analyzer JSON merge (the reference `renderer.SectionsToJSON` over
// several analyzers). For `codefang run --format json` with more than one static
// analyzer (or a glob that selects several), the reference implementation renders ONE `renderer.JSONReport`
// whose `sections` are the per-analyzer sections in REGISTRY order and whose
// `overall_score` is the executive-summary average of the scored sections
// (info-only sections, score < 0, are excluded; all-info ⇒ overall is -1 / Info).
// Each analyzer's section value comes from its own crate-owned report builder
// (the same GoValue the single-analyzer JSON path serializes), so the merge owns
// no analyzer math and every format follows the same report value.
// ---------------------------------------------------------------------------

/// Registry-ordered map of
/// static analyzer id → the crate-owned builder of that analyzer's single-section
/// `renderer.JSONReport` GoValue. Used by [`static_multi_json`] to merge several
/// analyzers' sections; the merge never branches per format — the same GoValue
/// feeds the serializer. The `bool` is the run's `--per-file` flag: every
/// builder owns its section's `files` enrichment (the reference
/// `EnrichWithPerFileData` gives EVERY section at least an empty `files` array
/// under the flag, so no post-processing happens in the merge). The
/// [`StaticFilter`] is the run's shared walk filter (`--languages` + path
/// policy), applied by every builder's folder walk.
type ReportValueFn = fn(&str, &StaticFilter, bool) -> Option<GoValue>;

use cf_gojson::{GoMap, GoValue, MapOrigin};

const STATIC_JSON_VALUE_BUILDERS: &[(&str, ReportValueFn)] = &[
    ("static/clones", static_clones::clones_report_value_flags),
    (
        "static/complexity",
        static_complexity::complexity_report_value_flags,
    ),
    (
        "static/comments",
        static_comments::comments_report_value_flags,
    ),
    (
        "static/halstead",
        static_halstead::halstead_report_value_flags,
    ),
    (
        "static/cohesion",
        static_cohesion::cohesion_report_value_flags,
    ),
    ("static/imports", static_imports::imports_report_value_flags),
    (
        "static/composition",
        static_json::composition_report_value_flags,
    ),
];

/// Pulls the `sections` array and `overall_score` out of a single-analyzer
/// `renderer.JSONReport` GoValue (`{overall_score_label, sections, overall_score}`).
/// Returns the section GoValues and the contained `overall_score` (each section's
/// own `score` field is what the merge re-averages, but the single-analyzer
/// `overall_score` equals that section's score for one section, so we read the
/// per-section `score` directly for robustness against future multi-section
/// analyzers).
fn extract_sections(report: &GoValue) -> Vec<GoValue> {
    report
        .as_map()
        .and_then(|m| m.get("sections"))
        .and_then(|s| match s {
            GoValue::Array(items) => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Reads a section's numeric `score` field, defaulting
/// to the info-only sentinel `-1.0` when absent.
fn section_score(section: &GoValue) -> f64 {
    match section.as_map().and_then(|m| m.get("score")) {
        Some(GoValue::Float(f)) => *f,
        Some(GoValue::Int(i)) => *i as f64,
        _ => -1.0,
    }
}

/// The reference `terminal.FormatScore`: `round(score*10)/10` → `"N/10"`; a negative score
/// (info-only) renders `"Info"`.
fn overall_score_label(score: f64) -> String {
    if score < 0.0 {
        return "Info".to_string();
    }
    let n = (score * 10.0).round() as i64;
    format!("{n}/10")
}

/// Expands `patterns` over the registry-ordered static analyzers and renders ONE
/// merged `renderer.JSONReport`: sections in
/// registry order, `overall_score` the average of the scored (`score >= 0`)
/// sections (or `-1` when none are scored). `None` if no static analyzer is
/// selected or any selected analyzer cannot produce a report (the caller then
/// falls through to the same error path the reference implementation takes).
#[must_use]
pub fn static_multi_json(
    patterns: &[&str],
    path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let root = static_multi_report_value(patterns, path, filter, per_file)?;
    let bytes = cf_gojson::Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&root);
    Some(bytes)
}

/// The merged multi-static-analyzer `--format text` render (the reference
/// `FormatText` over ALL selected sections at once): a ≥2-section selection
/// carries the executive-summary header (`CODE ANALYSIS REPORT` + per-analyzer
/// score table) followed by each analyzer's section rendered EXACTLY as its
/// solo `--format text` body — the sections come from the same per-analyzer
/// TEXT-shaped section values the solo text handlers feed the renderer (NOT
/// the JSON report values, whose extra `distribution`/`issues` data the
/// reference text sections for e.g. complexity do not carry).
#[must_use]
pub fn static_multi_text(patterns: &[&str], path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    type TextValueFn = fn(&str, &StaticFilter) -> Option<GoValue>;
    // Registry order (the reference registry / summary-table order).
    const TEXT_VALUE_BUILDERS: &[(&str, TextValueFn)] = &[
        ("static/clones", |p, f| {
            static_clones::clones_report_value_flags(p, f, false)
        }),
        (
            "static/complexity",
            static_complexity::complexity_report_value_summary,
        ),
        (
            "static/comments",
            static_comments::comments_report_value_summary,
        ),
        (
            "static/halstead",
            static_halstead::halstead_report_value_summary,
        ),
        (
            "static/cohesion",
            static_cohesion::cohesion_report_value_summary,
        ),
        ("static/imports", |p, f| {
            static_imports::imports_report_value_flags(p, f, false)
        }),
        (
            "static/composition",
            static_json::composition_report_value_opts as TextValueFn,
        ),
    ];

    let mut sections: Vec<GoValue> = Vec::new();
    for &(id, build) in TEXT_VALUE_BUILDERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if !matched {
            continue;
        }
        let report = build(path, filter)?;
        sections.extend(extract_sections(&report));
    }
    if sections.is_empty() {
        return None;
    }

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("sections", GoValue::Array(sections));
    Some(section_render::render_text_report(&GoValue::Map(root)))
}

/// Builds the ONE merged `renderer.JSONReport` GoValue for a multi-static
/// selection: sections in registry order, `overall_score` the executive-summary
/// average of the scored sections. Every output format renders from this same
/// value (rule: one report value, many encodings).
#[must_use]
fn static_multi_report_value(
    patterns: &[&str],
    path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let mut sections: Vec<GoValue> = Vec::new();
    let mut score_total = 0.0_f64;
    let mut score_count = 0_usize;

    for &(id, build) in STATIC_JSON_VALUE_BUILDERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if !matched {
            continue;
        }
        let report = build(path, filter, per_file)?;
        for section in extract_sections(&report) {
            let s = section_score(&section);
            if s >= 0.0 {
                score_total += s;
                score_count += 1;
            }
            sections.push(section);
        }
    }

    if sections.is_empty() {
        return None;
    }

    let overall = if score_count == 0 {
        -1.0
    } else {
        score_total / score_count as f64
    };

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "overall_score_label",
        GoValue::Str(overall_score_label(overall)),
    );
    root.push("sections", GoValue::Array(sections));
    root.push("overall_score", GoValue::Float(overall));

    Some(GoValue::Map(root))
}

/// The reference `EnrichWithPerFileData` INITIALIZATION step: under
/// `--per-file` every section gets at least an EMPTY `files` array (inserted
/// between `issues` and `score`, the renderer.JSONSection field order), even
/// when its analyzer retains no per-file snapshots (clones is not
/// `PerFileModeEnabled`). Sections that already carry `files` are unchanged.
pub(crate) fn ensure_sections_files_key(report: GoValue) -> GoValue {
    let GoValue::Map(root) = report else {
        return report;
    };
    let mut new_root = GoMap::new(MapOrigin::Struct);
    for (key, value) in root.iter() {
        if key != "sections" {
            new_root.push(key, value.clone());
            continue;
        }
        let GoValue::Array(sections) = value else {
            new_root.push(key, value.clone());
            continue;
        };
        let enriched: Vec<GoValue> = sections
            .iter()
            .map(|section| {
                let GoValue::Map(m) = section else {
                    return section.clone();
                };
                if m.get("files").is_some() {
                    return section.clone();
                }
                let mut out = GoMap::new(MapOrigin::Struct);
                for (k, v) in m.iter() {
                    if k == "score" {
                        out.push("files", GoValue::Array(Vec::new()));
                    }
                    out.push(k, v.clone());
                }
                GoValue::Map(out)
            })
            .collect();
        new_root.push(key, GoValue::Array(enriched));
    }
    GoValue::Map(new_root)
}

/// True when `patterns` select MORE THAN ONE static analyzer (a literal multi-id
/// list or a glob matching several), so the JSON path must merge sections rather
/// than emit a single-analyzer document.
#[must_use]
pub fn static_json_selects_multiple(patterns: &[&str]) -> bool {
    let mut matched = 0usize;
    for &(id, _) in STATIC_JSON_VALUE_BUILDERS {
        let hit = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if hit {
            matched += 1;
            if matched > 1 {
                return true;
            }
        }
    }
    false
}

/// Produces a single static analyzer's CFB1 bin envelope.
#[must_use]
pub fn static_single_bin(id: &str, path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    match id {
        "static/clones" => static_clones::clones_report_bin(path, filter),
        "static/complexity" => static_complexity_bin::complexity_report_bin(path, filter),
        "static/comments" => static_comments::comments_report_bin(path, filter),
        "static/halstead" => static_halstead::halstead_bin_report(path, filter),
        "static/cohesion" => static_cohesion::cohesion_report_bin(path, filter),
        "static/imports" => static_imports::imports_report_bin(path, filter),
        "static/composition" => static_json::composition_bin_opts(path, filter),
        _ => None,
    }
}

/// The history analyzers in the reference implementation phase/registry order (the reference `defaultHistoryLeaves`
/// as emitted by the combined unified-model path). Used to expand `*`/globs and
/// to order the history phase of the combined static+history render.
pub const HISTORY_COMBINED_ORDER: &[&str] = &[
    "history/typos",
    "history/file-history",
    "history/imports",
    "history/shotness",
    "history/anomaly",
    "history/burndown",
    "history/couples",
    "history/devs",
    "history/quality",
    "history/sentiment",
];

/// The history analyzers in the reference implementation's *separate-phase* per-analyzer emit order — the
/// order `runHistoryPhase` writes each leaf's standalone report when the run is
/// NOT a mixed static+history combined render (i.e. a history-only selection,
/// literal list or glob, in a machine format). This is the pipeline leaf order
/// (`pl.Leaves` → `selectLeaves`), which differs from both the registry id sort
/// and [`HISTORY_COMBINED_ORDER`]. Verified against the live reference binary
/// (`--analyzers history/* --format json`): the concatenated per-analyzer reports
/// appear in exactly this sequence.
pub const HISTORY_PHASE_EMIT_ORDER: &[&str] = &[
    "history/quality",
    "history/sentiment",
    "history/shotness",
    "history/couples",
    "history/imports",
    "history/typos",
    "history/anomaly",
    "history/burndown",
    "history/devs",
    "history/file-history",
];

/// Expands a requested pattern list into the concrete history leaf ids it
/// selects, in the reference implementation's separate-phase emit order ([`HISTORY_PHASE_EMIT_ORDER`]).
/// Literal ids match exactly; globs use the reference `path.Match` semantics. Used by the
/// history-only-glob per-analyzer concatenation path (the reference `runHistoryPhase` over a
/// glob-expanded selection), so a `history/*` or multi-id history selection emits
/// each leaf's standalone report in the same order the reference implementation does.
/// Whether any requested pattern selects the analyzer `id`, mirroring the reference
/// `Registry.resolvePattern`: a bare `*` matches EVERY id (reference:
/// special-cases `pattern == "*"` to `allIDs()` BEFORE `path.Match`, because
/// `path.Match("*", "history/typos")` is false — `*` does not cross `/`); other
/// globs use the reference `path.Match` semantics ([`go_path_match`]); a literal id matches
/// exactly. Without the `*` special case, `--analyzers '*'` would select nothing,
/// while `--analyzers 'history/*'` would still work (the literal `history/`
/// prefix anchors the match).
#[must_use]
fn pattern_selects_id(patterns: &[&str], id: &str) -> bool {
    let is_glob = |p: &str| p.contains(['*', '?', '[']);
    patterns.iter().any(|p| {
        if *p == "*" {
            true
        } else if is_glob(p) {
            go_path_match(p, id)
        } else {
            *p == id
        }
    })
}

/// Expands a requested pattern list into the concrete history leaf ids it
/// selects, in the reference implementation's separate-phase emit order ([`HISTORY_PHASE_EMIT_ORDER`]).
#[must_use]
pub fn expand_history_phase_ids(patterns: &[&str]) -> Vec<String> {
    let selected = |id: &str| pattern_selects_id(patterns, id);
    HISTORY_PHASE_EMIT_ORDER
        .iter()
        .filter(|id| selected(id))
        .map(|id| (*id).to_string())
        .collect()
}

/// Expands the requested analyzer patterns into concrete (static, history) id
/// lists in the reference implementation SELECTION order — the `registry.ExpandPatterns` semantics the
/// run command applies before `registry.Split`: patterns
/// resolve IN ORDER (a literal id to itself, a glob to the matching registry
/// ids in registration order), duplicates dropped first-wins, then ids are
/// divided by mode preserving that order. The plot path needs this order:
/// The reference implementation renders pages (and the index cards) in the resolved id order.
#[must_use]
pub fn expand_selection_ids(patterns: &[&str]) -> (Vec<String>, Vec<String>) {
    let is_glob = |p: &str| p.contains(['*', '?', '[']);
    let mut statics: Vec<String> = Vec::new();
    let mut history: Vec<String> = Vec::new();
    let push_unique = |list: &mut Vec<String>, id: &str| {
        if !list.iter().any(|have| have == id) {
            list.push(id.to_string());
        }
    };
    for pat in patterns {
        if is_glob(pat) {
            // Glob: the full registry in registration order (statics, then
            // history leaves), filtered by the reference `path.Match` semantics.
            for (id, _) in STATIC_BIN_ANALYZERS {
                if *pat == "*" || go_path_match(pat, id) {
                    push_unique(&mut statics, id);
                }
            }
            for id in HISTORY_COMBINED_ORDER {
                if *pat == "*" || go_path_match(pat, id) {
                    push_unique(&mut history, id);
                }
            }
            continue;
        }
        // Literal id: itself, in its pattern position (unknown ids are kept
        // out; the caller surfaces the dispatch diagnostic).
        if STATIC_BIN_ANALYZERS.iter().any(|(id, _)| id == pat) {
            push_unique(&mut statics, pat);
        } else if HISTORY_COMBINED_ORDER.iter().any(|id| id == pat) {
            push_unique(&mut history, pat);
        }
    }
    (statics, history)
}

/// Expands the requested analyzer patterns into concrete (static, history) id
/// lists in the reference implementation combined-model order: static analyzers in [`STATIC_BIN_ANALYZERS`]
/// registry order, then history analyzers in [`HISTORY_COMBINED_ORDER`]. Literal
/// (non-glob) ids are matched exactly; globs use the reference `path.Match` semantics. This
/// mirrors the reference `registry.Split` + `combinedIDsAndModes` ordering used by the
/// combined render.
#[must_use]
pub fn expand_combined_ids(patterns: &[&str]) -> (Vec<String>, Vec<String>) {
    let matches = |id: &str| pattern_selects_id(patterns, id);
    let statics: Vec<String> = STATIC_BIN_ANALYZERS
        .iter()
        .filter(|(id, _)| matches(id))
        .map(|(id, _)| (*id).to_string())
        .collect();
    let history: Vec<String> = HISTORY_COMBINED_ORDER
        .iter()
        .filter(|id| matches(id))
        .map(|id| (*id).to_string())
        .collect();
    (statics, history)
}

/// Renders the combined static+history run as the single `codefang.run.v1`
/// unified-model envelope, the Rust analogue of the reference `renderCombinedDirect`
///. Each selected analyzer is dispatched through its registry
/// handler with `--format bin`, producing a CFB1 envelope whose payload is the
/// analyzer's raw report JSON. The concatenated envelopes are decoded into a
/// [`cf_analyze::conversion::UnifiedModel`],
/// stamped with run metadata, and re-serialized in the
/// requested `output_format` via [`cf_analyze::conversion::write_converted_output`]
/// so every machine format (json/yaml/bin/ndjson/timeseries) follows from the
/// one model value.
///
/// Returns `None` if any selected analyzer cannot produce its bin payload (e.g.
/// an unported history analyzer), so the caller can fall back to the per-analyzer
/// pipeline rather than emit a partial envelope.
#[must_use]
pub fn render_combined(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
    output_format: &str,
) -> Option<Vec<u8>> {
    let registry = default_registry();
    let mut raw: Vec<u8> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut modes: Vec<cf_analyze::AnalyzerMode> = Vec::new();

    // Pre-compute the ONE shared UAST walk for the co-selected heavy history
    // analyzers (imports/quality/sentiment/shotness/typos) before fanning out,
    // so it runs as a single task instead of N tasks blocking on its store.
    uast_walk::prewarm(ctx.matches);

    // Static phase, then history phase — the reference `renderCombinedDirect` order. Each
    // handler is dispatched with the literal "bin" format; its CFB1 envelope is
    // appended to the combined buffer (reference: staticExec/historyExec into &raw).
    // Each leaf's raw report is gathered via its CFB1 bin envelope. Handlers
    // match on the NORMALIZED format name (the reference `ValidateFormat` maps the `bin`
    // alias to `binary`), so pass the normalized name here — passing the bare
    // `bin` alias would miss any handler that only accepts `binary` (e.g.
    // static/halstead), aborting the whole combined render.
    //
    // The per-analyzer envelopes are independent (each handler owns its own
    // repository handle and parsers), so they are COMPUTED concurrently and
    // then appended in the same deterministic static-then-history order.
    let all_ids: Vec<&String> = static_ids.iter().chain(history_ids.iter()).collect();
    let envelopes: Vec<Option<Vec<u8>>> =
        run_concurrent(all_ids.len(), ANALYZER_CONCURRENCY, |i| {
            let entry = registry.lookup(all_ids[i])?;
            (entry.run)(ctx, "binary")
        });
    for (i, (id, env)) in all_ids.iter().zip(envelopes).enumerate() {
        raw.extend_from_slice(&env?);
        ids.push((*id).clone());
        modes.push(if i < static_ids.len() {
            cf_analyze::AnalyzerMode::static_mode()
        } else {
            cf_analyze::AnalyzerMode::history()
        });
    }

    let mut model =
        cf_analyze::conversion::decode_combined_binary_reports(&raw, &ids, &modes).ok()?;
    model.metadata = Some(cf_analyze::metadata::new_analysis_metadata(&ctx.path));

    // Normalize the requested format to the canonical name the conversion
    // serializer matches on (reference `ValidateUniversalFormat`: "bin" -> "binary",
    // case-folded), then apply the --ndjson modifier on timeseries exactly as
    // the reference `renderCombinedDirect` does.
    let normalized = crate::formats::normalize_format(output_format);
    let render_format = if ctx.ndjson() && normalized == "timeseries" {
        "timeseries+ndjson".to_string()
    } else {
        normalized
    };

    let mut out: Vec<u8> = Vec::new();
    cf_analyze::conversion::write_converted_output(&model, &render_format, &mut out, None).ok()?;
    Some(out)
}

/// The reference `path.Match` semantics over an analyzer ID (`*`, `?`, `[...]`).
#[must_use]
pub fn go_path_match(pattern: &str, name: &str) -> bool {
    go_path_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn go_path_match_inner(mut pat: &[u8], mut name: &[u8]) -> bool {
    while !pat.is_empty() {
        match pat[0] {
            b'*' => {
                while !pat.is_empty() && pat[0] == b'*' {
                    pat = &pat[1..];
                }
                if pat.is_empty() {
                    return !name.contains(&b'/');
                }
                let mut i = 0;
                loop {
                    if go_path_match_inner(pat, &name[i..]) {
                        return true;
                    }
                    if i >= name.len() || name[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            b'?' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
            b'[' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                let (matched, rest) = match_class(&pat[1..], name[0]);
                if !matched {
                    return false;
                }
                pat = rest;
                name = &name[1..];
            }
            c => {
                if name.is_empty() || name[0] != c {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
        }
    }
    name.is_empty()
}

fn match_class(pat: &[u8], ch: u8) -> (bool, &[u8]) {
    let mut i = 0;
    let mut negate = false;
    if i < pat.len() && (pat[i] == b'^' || pat[i] == b'!') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    while i < pat.len() && pat[i] != b']' {
        let lo = pat[i];
        i += 1;
        if i + 1 < pat.len() && pat[i] == b'-' && pat[i + 1] != b']' {
            let hi = pat[i + 1];
            i += 2;
            if lo <= ch && ch <= hi {
                matched = true;
            }
        } else if lo == ch {
            matched = true;
        }
    }
    let rest = if i < pat.len() {
        &pat[i + 1..]
    } else {
        &pat[i..]
    };
    (matched ^ negate, rest)
}

// ---------------------------------------------------------------------------
// Per-analyzer registry handlers. Each owns its own `match format`; this is the
// ONE place that knows how to format a given analyzer (reference: each analyzer's
// FormatReport* family). One handler per analyzer id — NOT one per (id,format).
// ---------------------------------------------------------------------------

fn h_static_clones(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    let filter = static_filter(ctx).ok()?;
    // `format` is the resolved/normalized value: `--format bin` arrives here as
    // `"binary"` (the reference `ValidateFormat` maps the `bin` alias to `binary`); accept
    // both spellings for robustness.
    match format {
        "json" => {
            static_clones::clones_report_json_flags(path, &filter, ctx.matches.get_flag("per-file"))
        }
        "yaml" => static_clones::clones_report_yaml(path, &filter),
        "binary" | "bin" => static_clones::clones_report_bin(path, &filter),
        "compact" => static_clones::clones_report_compact(path, &filter),
        "text" => Some(section_render::render_text_report(
            &static_clones::clones_report_value_flags(path, &filter, false)?,
        )),
        _ => None,
    }
}

/// The shared static-phase path-policy options from the run flags
/// (the reference `pathPolicyFromFlags`: `--include-vendored` /
/// `--include-generated`; `--extra-excluded-prefixes` is not exposed on `run`).
pub(crate) fn static_path_policy(ctx: &RunContext) -> cf_pathpolicy::Options {
    cf_pathpolicy::Options {
        include_vendored: ctx.matches.get_flag("include-vendored"),
        include_generated: ctx.matches.get_flag("include-generated"),
        ..cf_pathpolicy::Options::default()
    }
}

/// The static-phase file filter shared by EVERY output format of every static
/// analyzer, mirroring the reference `StaticService` walk callback:
///
/// 1. `matchesLanguageGlobs(path, svc.LanguageGlobs)` — the `--languages`
///    restriction, fnmatch globs over the file BASENAME (built from the flag
///    via `langpath.Globs`; `None` ⇔ empty/`all` → no restriction);
/// 2. `pathpolicy.Exclude(path, nil, svc.PathPolicy)` — the
///    `--include-vendored` / `--include-generated` policy.
#[derive(Debug, Clone, Default)]
pub struct StaticFilter {
    /// Vendor / generated exclusion policy (`pathPolicyFromFlags`).
    pub policy: cf_pathpolicy::Options,
    /// Basename globs from `--languages`; `None` disables the filter
    /// (the reference `LanguageGlobs == nil`).
    pub language_globs: Option<Vec<String>>,
}

impl StaticFilter {
    /// Filter with the given path policy and no language restriction (the
    /// pre-`--languages` behavior; used by legacy no-flag wrappers).
    #[must_use]
    pub fn from_policy(policy: cf_pathpolicy::Options) -> Self {
        StaticFilter {
            policy,
            language_globs: None,
        }
    }

    /// Reports whether the walk must skip `path` — the reference walk callback's
    /// `!matchesLanguageGlobs(...) || pathpolicy.Exclude(...)`.
    #[must_use]
    pub fn skips(&self, path: &str) -> bool {
        if let Some(globs) = &self.language_globs {
            // filepath.Base: the final path element.
            let base = path.rsplit('/').next().unwrap_or(path);
            if !globs.iter().any(|g| go_path_match(g, base)) {
                return true;
            }
        }
        cf_pathpolicy::exclude(path, None, &self.policy)
    }
}

/// Builds the [`StaticFilter`] from the run flags: the path policy
/// (`static_path_policy`) plus the `--languages` globs (the reference
/// `applyStaticLanguageFilter` → `langpath.Globs`).
///
/// # Errors
///
/// Returns the reference error message (`static --languages: unknown language:
/// "<token>"`) when a `--languages` token resolves to no Linguist language.
pub(crate) fn static_filter(ctx: &RunContext) -> Result<StaticFilter, String> {
    let policy = static_path_policy(ctx);
    let mut tokens: Vec<String> = ctx
        .matches
        .get_many::<String>("languages")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    // `--languages ""` parity: cobra's GetStringSlice runs the raw value
    // through encoding/csv, and csv.Read on an empty string yields an EMPTY
    // slice (no filter) — while clap's value_delimiter yields one empty
    // token, which would error as an unknown language. A value like ",Go"
    // still carries a non-empty token alongside the empty one and errors on
    // the empty token in BOTH implementations, so only the all-empty case
    // collapses.
    if tokens.iter().all(String::is_empty) {
        tokens.clear();
    }
    let globs = cf_langpath::globs(&tokens).map_err(|e| format!("static --languages: {e}"))?;
    Ok(StaticFilter {
        policy,
        language_globs: if globs.wants_all {
            None
        } else {
            Some(globs.globs)
        },
    })
}

fn h_static_complexity(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_complexity::complexity_report_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_complexity_yaml::complexity_report_yaml(path, &filter),
        "binary" | "bin" => static_complexity_bin::complexity_report_bin(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_complexity::complexity_report_value_summary(path, &filter)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_complexity::complexity_report_value_summary(path, &filter)?,
        )),
        _ => None,
    }
}

fn h_static_cohesion(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name (accept the raw alias too).
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_cohesion::cohesion_report_json_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_cohesion::cohesion_report_yaml(path, &filter),
        "binary" | "bin" => static_cohesion::cohesion_report_bin(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_cohesion::cohesion_report_value_summary(path, &filter)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_cohesion::cohesion_report_value_summary(path, &filter)?,
        )),
        _ => None,
    }
}

fn h_static_composition(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // The reference static walk filters files through the language globs +
    // pathpolicy.Exclude with the RUN flags (`--languages`,
    // `--include-vendored` / `--include-generated`), so every encoding of the
    // aggregated report must honor them.
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_json::composition_report_opts_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_json::composition_yaml_opts(path, &filter),
        // The pipeline resolves the `bin` alias to the canonical `binary`
        // (formats::normalize_format) before dispatch, so match that; accept the
        // raw alias too for direct callers.
        "binary" | "bin" => static_json::composition_bin_opts(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_json::composition_report_value_opts(path, &filter)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_json::composition_report_value_opts(path, &filter)?,
        )),
        _ => None,
    }
}

fn h_static_halstead(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_halstead::halstead_json_report_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_halstead::halstead_yaml_report(path, &filter),
        "binary" => static_halstead::halstead_bin_report(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_halstead::halstead_report_value_summary(path, &filter)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_halstead::halstead_report_value_summary(path, &filter)?,
        )),
        _ => None,
    }
}

fn h_static_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    // `format` is the resolved/normalized format from `resolve_formats`, where the
    // `bin` CLI alias has already been normalized to `binary` (formats.rs
    // `normalize_format`). Match the normalized name so `--format bin` dispatches
    // to the CFB1 envelope builder rather than falling through to `None`.
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_imports::imports_report_json_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_imports::imports_report_yaml(path, &filter),
        "binary" => static_imports::imports_report_bin(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_imports::imports_report_value_flags(path, &filter, false)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_imports::imports_report_value_flags(path, &filter, false)?,
        )),
        _ => None,
    }
}

fn h_static_comments(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    let filter = static_filter(ctx).ok()?;
    match format {
        "json" => static_comments::comments_report_json_flags(
            path,
            &filter,
            ctx.matches.get_flag("per-file"),
        ),
        "yaml" => static_comments::comments_report_yaml(path, &filter),
        // The pipeline resolves the `bin` alias to the canonical `binary`
        // (formats::normalize_format) before dispatch, so match that; accept the
        // raw alias too for direct callers.
        "binary" | "bin" => static_comments::comments_report_bin(path, &filter),
        "compact" => Some(section_render::render_compact_report(
            &static_comments::comments_report_value_summary(path, &filter)?,
        )),
        "text" => Some(section_render::render_text_report(
            &static_comments::comments_report_value_summary(path, &filter)?,
        )),
        _ => None,
    }
}

fn h_history_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // `timeseries+ndjson` (the `--ndjson` modifier) is NOT an encoding of the
    // report value: the reference implementation streams per-commit lines
    // through the per-chunk TimeSeriesChunkFlusher (DrainCommitStats), so it
    // has its own emitter over the same memoized walk.
    if format == "timeseries+ndjson" {
        return history::imports_timeseries_ndjson(ctx.matches);
    }
    // One report value, encoded per format by the shared history serializer
    //. The YAML section header is the reference implementation
    // analyzer Name() (`imports.HistoryAnalyzer.Name` == "ImportsPerDeveloper").
    let metrics = history::imports_run_metrics(ctx.matches)?;
    serialize_history_metrics(
        format,
        "ImportsPerDeveloper",
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_typos(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => history::typos_run_report(ctx.matches),
        "yaml" => history::typos_run_report_yaml(ctx.matches),
        "binary" => history::typos_run_report_bin(ctx.matches),
        _ => None,
    }
}

fn h_history_couples(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One report value (the reference `ComputedMetrics`, behind ToJSON/ToYAML); every
    // machine format is a serializer over it. ToJSON == ToYAML for couples, so
    // json_value and yaml_value share the same tree. The YAML section name is
    // the analyzer's the reference `Name()` ("Couples").
    let value = couples_run::couples_run_value(ctx.matches)?;
    serialize_history_metrics(format, "Couples", &value, &value)
}

fn h_history_shotness(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One report value (the reference `ComputedMetrics`, the value behind ToJSON/ToYAML);
    // every machine format is just a serializer over it. ToJSON == ToYAML for
    // shotness, so json_value and yaml_value share `to_go_value()`. The YAML
    // section name is the analyzer's the reference `Name()` ("Shotness").
    let metrics = shotness_run::shotness_run_metrics(ctx.matches)?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, "Shotness", &value, &value)
}

fn h_history_devs(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    match (format, head) {
        ("json", true) => history::devs_head_report(sub),
        ("json", false) => history::devs_run_report(sub),
        ("yaml", true) => history::devs_head_report_yaml(sub),
        ("yaml", false) => history::devs_run_report_yaml(sub),
        ("timeseries+ndjson", false) => history::devs_run_timeseries_ndjson(sub),
        // The pipeline resolves the `bin` alias to canonical `binary`
        // (formats::normalize_format) before dispatch; accept the raw alias too
        // for direct callers.
        ("binary" | "bin", false) => history::devs_run_report_bin(sub),
        ("binary" | "bin", true) => {
            let metrics = history::devs_head_metrics(sub)?;
            let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
            cf_reportutil::encode_binary_envelope(&payload).ok()
        }
        _ => None,
    }
}

fn h_history_anomaly(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    use cf_anomaly::model::ToGoValue;
    if ctx.head() {
        // Closed-form merge-HEAD path (analyzer's deterministic head case): ONE
        // report value, every machine format an encoding of it via the shared
        // serializer — so the combined `*` model can request `binary` here too.
        let metrics = history::anomaly_head_report(ctx.matches)?;
        let value = metrics.to_go_value();
        return serialize_history_metrics(format, "TemporalAnomaly", &value, &value);
    }
    // Full revwalk (no --head): one report value (reference `ComputeAllMetrics` →
    // ComputedMetrics), every machine format an encoding of it via the shared
    // history serializer. ToJSON == ToYAML for anomaly, so json/yaml share the
    // same GoValue. The YAML section name is the analyzer's the reference `Name()`
    // ("TemporalAnomaly").
    let metrics = history::anomaly_run_metrics(ctx.matches)?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, "TemporalAnomaly", &value, &value)
}

fn h_history_quality(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // `timeseries+ndjson` (the `--ndjson` modifier) is NOT an encoding of the
    // report value: the reference implementation streams per-commit lines
    // through the per-chunk TimeSeriesChunkFlusher (DrainCommitStats), so it
    // has its own emitter over the same memoized walk.
    if format == "timeseries+ndjson" {
        return history::quality_timeseries_ndjson(ctx.matches);
    }
    // `--head` is handled inside `quality_metrics` (single HEAD-commit window),
    // so every format is the same encoding of one computed value — including the
    // `binary` payload the combined `*` model gathers.
    // One computed report value, three encodings routed
    // through the shared serializer (reference: FormatReportJSON/YAML/Binary): json/bin
    // marshal the encoding/json value tree; yaml wraps the same struct-origin
    // value tree in the `codefang (v2)` envelope under `history/quality:`.
    let metrics = history::quality_metrics(ctx.matches)?;
    let value = cf_quality::serialize::computed_metrics_value(&metrics);
    serialize_history_metrics(format, "history/quality", &value, &value)
}

fn h_history_sentiment(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    use cf_sentiment::ToGoValue;
    // `timeseries+ndjson` (the `--ndjson` modifier) is NOT an encoding of the
    // report value: the reference implementation streams per-commit lines
    // through the per-chunk TimeSeriesChunkFlusher (DrainCommitStats), so it
    // has its own emitter over the same memoized walk.
    if format == "timeseries+ndjson" {
        return history::sentiment_timeseries_ndjson(ctx.matches);
    }
    // `--head` is handled inside `sentiment_metrics` (single HEAD-commit window),
    // so every format (including the combined `*` model's `binary`) is one
    // encoding of the same computed value.
    // One computed report value, three encodings (reference `ComputeAllMetrics` →
    // FormatReportJSON/YAML/Binary): json/bin marshal the encoding/json value
    // tree (nil slice → null); yaml wraps the yaml.v3 value tree (nil → []) in
    // the `codefang (v2)` envelope. Routed through the shared serializer so every
    // format follows the one computation (same path as the other history leaves).
    let metrics = history::sentiment_metrics(ctx.matches)?;
    serialize_history_metrics(
        format,
        "history/sentiment",
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_file_history(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    // One computed report value, three machine
    // encodings (json/bin/yaml). The crate's `computed_metrics_to_go` is the
    // single `ToJSON`/`ToYAML` value tree (file_history's ToJSON == ToYAML);
    // route it through the shared history-metrics serializer so all formats are
    // encodings of THE SAME value. The YAML
    // section header is the analyzer's Name(): `FileHistoryAnalysis`.
    // `timeseries+ndjson` (the `--ndjson` modifier) is NOT an encoding of the
    // report value: the reference implementation streams per-commit lines
    // through the per-chunk TimeSeriesChunkFlusher (DrainCommitStats), so it
    // has its own emitter over the same walk (`--head` loads the single HEAD
    // commit inside `file_history_run`, matching the reference head window).
    if format == "timeseries+ndjson" {
        return history::file_history_timeseries_ndjson(ctx.matches);
    }
    let value = history::file_history_report_value(ctx.matches)?;
    serialize_history_metrics(format, "FileHistoryAnalysis", &value, &value)
}

fn h_history_burndown(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    let ndjson = ctx.ndjson();
    match (format, head, ndjson) {
        ("timeseries", true, false) => history::burndown_head_timeseries(sub),
        ("timeseries+ndjson", false, _) => burndown_ndjson::burndown_timeseries_ndjson(sub),
        ("ndjson", false, _) => burndown_ndjson::burndown_record_ndjson(sub),
        ("json", false, _) => burndown_ndjson::burndown_run_report(sub),
        ("yaml", false, _) => burndown_ndjson::burndown_run_report_yaml(sub),
        ("binary" | "bin", false, _) => burndown_ndjson::burndown_run_report_bin(sub),
        ("json" | "yaml" | "binary" | "bin", true, _) => {
            let metrics = history::burndown_head_metrics(sub)?;
            let bytes = match format {
                "json" => cf_gojson::marshal(&metrics.to_go_value()),
                "binary" | "bin" => {
                    cf_reportutil::encode_binary_envelope(&metrics.to_go_value()).ok()?
                }
                _ => {
                    let mut out = Vec::new();
                    out.extend_from_slice(b"codefang (v2):\n");
                    out.extend_from_slice(
                        format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
                    );
                    out.extend_from_slice(
                        format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes(),
                    );
                    out.extend_from_slice(b"history/burndown:\n");
                    out.extend_from_slice(&cf_goyaml::marshal(&metrics.to_go_value_yaml()));
                    out
                }
            };
            Some(bytes)
        }
        _ => None,
    }
}

/// Builds the single default analyzer [`Registry`] — the Rust analogue of the reference implementation
/// `defaultRegistry()` (`analyze.NewRegistry(defaultUASTAnalyzers,
/// defaultRawFileAnalyzers, defaultHistoryLeaves)`). One registry insertion per
/// analyzer; dispatch is a keyed lookup by id, not a per-format match ladder.
#[must_use]
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    let s = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::Static,
        run,
    };
    let h = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::History,
        run,
    };

    r.register(s("static/clones", h_static_clones));
    r.register(s("static/complexity", h_static_complexity));
    r.register(s("static/cohesion", h_static_cohesion));
    r.register(s("static/composition", h_static_composition));
    r.register(s("static/halstead", h_static_halstead));
    r.register(s("static/imports", h_static_imports));
    r.register(s("static/comments", h_static_comments));

    r.register(h("history/imports", h_history_imports));
    r.register(h("history/typos", h_history_typos));
    r.register(h("history/couples", h_history_couples));
    r.register(h("history/shotness", h_history_shotness));
    r.register(h("history/devs", h_history_devs));
    r.register(h("history/anomaly", h_history_anomaly));
    r.register(h("history/quality", h_history_quality));
    r.register(h("history/sentiment", h_history_sentiment));
    r.register(h("history/file-history", h_history_file_history));
    r.register(h("history/burndown", h_history_burndown));

    r
}

#[cfg(test)]
mod static_filter_tests {
    use super::*;

    /// Fully permissive path policy, isolating the language-glob leg of the
    /// filter from the vendored/generated policy leg.
    fn permissive_policy() -> cf_pathpolicy::Options {
        cf_pathpolicy::Options {
            include_vendored: true,
            include_generated: true,
            ..cf_pathpolicy::Options::default()
        }
    }

    /// Parses `run` args through the REAL command tree and builds the shared
    /// walk filter exactly as the dispatchers do (`static_filter(ctx)`).
    fn filter_for(args: &[&str]) -> Result<StaticFilter, String> {
        let argv: Vec<&str> = ["codefang", "run"]
            .iter()
            .chain(args.iter())
            .copied()
            .chain(std::iter::once("."))
            .collect();
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches
            .subcommand_matches("run")
            .expect("run subcommand matches");
        let ctx = RunContext::from_matches(sub);
        static_filter(&ctx)
    }

    #[test]
    fn skips_matches_language_globs_on_basename() {
        // The reference matchesLanguageGlobs runs fnmatch over filepath.Base,
        // so a `*.go` glob must match a NESTED path's basename.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(vec!["*.go".to_string()]),
        };
        assert!(!filter.skips("src/pkg/deep/file.go"));
        assert!(filter.skips("src/pkg/deep/file.py"));
    }

    #[test]
    fn skips_language_filter_applies_before_path_policy() {
        // A language-mismatched path is skipped even under the fully
        // permissive policy: the glob leg decides first, independent of the
        // vendored/generated leg.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(vec!["*.py".to_string()]),
        };
        assert!(filter.skips("vendor/dep.go"));
        // A language-matched path still falls through to the path policy:
        // vendor is excluded under the DEFAULT policy despite the glob match.
        let filter = StaticFilter {
            policy: cf_pathpolicy::Options::default(),
            language_globs: Some(vec!["*.go".to_string()]),
        };
        assert!(filter.skips("vendor/dep.go"));
    }

    #[test]
    fn skips_none_globs_means_no_language_restriction() {
        // `LanguageGlobs == nil` (flag absent / `all`): only the path policy
        // decides.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: None,
        };
        assert!(!filter.skips("a.py"));
        assert!(!filter.skips("vendor/dep.go"));
    }

    #[test]
    fn skips_empty_globs_matches_nothing() {
        // An empty glob VECTOR (as opposed to `None`) restricts to the empty
        // language set: every path is skipped.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(Vec::new()),
        };
        assert!(filter.skips("a.go"));
        assert!(filter.skips("a.py"));
    }

    #[test]
    fn static_filter_parses_languages_into_basename_globs() {
        let filter = filter_for(&["--languages", "Go"]).expect("Go resolves");
        let globs = filter
            .language_globs
            .expect("--languages Go must restrict the walk");
        assert!(
            globs.iter().any(|g| g == "*.go"),
            "Go must contribute the *.go basename glob, got {globs:?}"
        );
    }

    #[test]
    fn static_filter_all_token_disables_the_restriction() {
        // The `all` sentinel (langpath wants_all) maps to `None` — the same
        // no-restriction state as an absent flag.
        let filter = filter_for(&["--languages", "all"]).expect("all resolves");
        assert!(filter.language_globs.is_none());
        let filter = filter_for(&[]).expect("absent flag resolves");
        assert!(filter.language_globs.is_none());
    }

    #[test]
    fn static_filter_unknown_language_token_errors() {
        let err = filter_for(&["--languages", "NotALang"])
            .expect_err("unresolvable token must abort the run");
        assert!(
            err.contains("unknown language"),
            "error must carry the CLI contract wording, got {err:?}"
        );
    }
}

#[cfg(test)]
mod per_file_multi_tests {
    use super::*;

    /// Fixture: one Go file with functions/comments/imports plus a plain-text
    /// file, exercising every static analyzer through the merged JSON path.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nimport \"fmt\"\n\n// main prints.\nfunc main() {\n\tfmt.Println(\"hi\")\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "just text\n").unwrap();
        dir
    }

    #[test]
    fn per_file_flag_enriches_every_merged_section() {
        let dir = fixture();
        let bytes = static_multi_json(
            &["static/*"],
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            true,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        // Every merged section gets a files key — the retaining analyzers with
        // real entries, clones with the initialized empty array.
        let sections = json.matches("\"title\"").count();
        let files_keys = json.matches("\"files\"").count();
        assert_eq!(
            sections, files_keys,
            "every section must carry a files key (sections={sections} files={files_keys}):\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "per-file entries missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files_in_merge() {
        let dir = fixture();
        let bytes = static_multi_json(
            &["static/*"],
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            false,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
    }
}

#[cfg(test)]
mod dispatch_flag_tests {
    //! Walk-filter flags exercised through the REAL registry dispatch
    //! (`default_registry().lookup(id).run`), NOT by calling the report
    //! builders with a hand-made filter. This is the layer the original
    //! regression lived in: `h_static_*` built the filter for the json arm but
    //! passed a default filter on the text/compact arms — a bug the
    //! builder-level tests cannot see.

    use super::*;

    /// Fixture where the walk-filter flags change every total: one plain Go
    /// function, one generated-path Go function, one vendored Go function, one
    /// Python function.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nfunc plain(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("gen.pb.go"),
            "package main\n\nfunc gen(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/dep")).unwrap();
        std::fs::write(
            dir.path().join("vendor/dep/d.go"),
            "package dep\n\nfunc vendored(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.py"),
            "def py_fn(a):\n    if a > 0:\n        return a\n    return -a\n",
        )
        .unwrap();
        dir
    }

    /// Runs `codefang run --analyzers <id> --format <format> <extra..> <root>`
    /// through the real argv parse + registry handler, returning the rendered
    /// bytes — the exact dispatch path the CLI takes.
    fn dispatch(id: &str, format: &str, extra: &[&str], root: &std::path::Path) -> Vec<u8> {
        let root = root.to_str().unwrap();
        let mut argv = vec!["codefang", "run", "--analyzers", id, "--format", format];
        argv.extend_from_slice(extra);
        argv.push(root);
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run subcommand");
        let ctx = RunContext::from_matches(sub);
        let registry = default_registry();
        let entry = registry.lookup(id).expect("analyzer registered");
        (entry.run)(&ctx, format).expect("handler produced a report")
    }

    /// Extracts the numeric "Total Functions" figure from a rendered text
    /// report.
    fn text_total_functions(bytes: &[u8]) -> i64 {
        let text = String::from_utf8_lossy(bytes);
        let line = text
            .lines()
            .find(|l| l.contains("Total Functions"))
            .unwrap_or_else(|| panic!("no Total Functions line in:\n{text}"));
        let rest = line.split("Total Functions").nth(1).unwrap();
        rest.split_whitespace()
            .next()
            .and_then(|w| w.parse().ok())
            .unwrap_or_else(|| panic!("unparseable Total Functions line: {line}"))
    }

    #[test]
    fn text_dispatch_applies_include_flags() {
        let dir = fixture();
        for id in ["static/complexity", "static/halstead"] {
            let default_total = text_total_functions(&dispatch(id, "text", &[], dir.path()));
            let permissive_total = text_total_functions(&dispatch(
                id,
                "text",
                &["--include-generated", "--include-vendored"],
                dir.path(),
            ));
            // Default policy: a.go + b.py = 2; permissive adds gen.pb.go +
            // vendor/dep/d.go = 4. `>` (not the exact figures) is the
            // regression guard: the flags must REACH the text-format walk.
            assert!(
                permissive_total > default_total,
                "{id}: text dispatch dropped the include flags \
                 (default={default_total}, permissive={permissive_total})"
            );
        }
    }

    #[test]
    fn text_dispatch_applies_language_filter() {
        let dir = fixture();
        let py_total = text_total_functions(&dispatch(
            "static/complexity",
            "text",
            &[
                "--include-generated",
                "--include-vendored",
                "--languages",
                "Python",
            ],
            dir.path(),
        ));
        assert_eq!(
            py_total, 1,
            "text dispatch must restrict the walk to the one Python function"
        );
    }

    #[test]
    fn compact_dispatch_applies_language_filter() {
        let dir = fixture();
        // Ruby matches no fixture file, so the compact render must collapse to
        // the empty-report section — byte-different from the unfiltered one.
        // Under a dispatch that drops the filter both renders are identical.
        let unfiltered = dispatch("static/complexity", "compact", &[], dir.path());
        let ruby_only = dispatch(
            "static/complexity",
            "compact",
            &["--languages", "Ruby"],
            dir.path(),
        );
        assert_ne!(
            unfiltered, ruby_only,
            "compact dispatch dropped --languages (both renders identical)"
        );
    }

    #[test]
    fn empty_languages_value_means_no_filter() {
        // `--languages ""` parity with the reference CLI: cobra's csv reader
        // yields an empty slice for an explicit empty value, so the run
        // proceeds unfiltered instead of failing on an unknown "" language.
        let dir = fixture();
        let empty = text_total_functions(&dispatch(
            "static/complexity",
            "text",
            &["--languages", ""],
            dir.path(),
        ));
        let absent = text_total_functions(&dispatch("static/complexity", "text", &[], dir.path()));
        assert_eq!(empty, absent, "--languages \"\" must behave as no filter");
    }
}
