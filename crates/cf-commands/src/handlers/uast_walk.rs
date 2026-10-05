//! The UAST history pass behind the five tree-sitter-heavy history analyzers
//! (`history/{imports,quality,sentiment,shotness,typos}`).
//!
//! Each of those analyzers consumes the SAME per-commit inputs: the filtered
//! tree diff against the first parent, the 256 KiB blob cap, and the UAST
//! parse of the changed blobs — the dominant cost of a history run. ONE
//! chunked pass of the history core ([`walk_chunks`]) feeds every analyzer
//! the run selected: per commit, the tree diff is computed once, then the
//! changes are visited CHANGE-MAJOR — each change's two sides are read and
//! parsed at most once ([`ChangeParses`]), every selected analyzer adds that
//! change's contribution to its per-commit product, and the parses are
//! dropped before the next change. A worker's parse live set is therefore
//! the two sides of one change, never a whole commit's (the per-thread
//! memory peak follows the largest live set, not the window or the commit
//! size). Each commit's [`UastCommitProduct`] is folded in window order into
//! the selected analyzers' finished per-commit records ([`UastFold`]) and
//! dropped, so no whole-window product vector exists.
//!
//! The finished results ([`UastResults`]) are memoized process-wide (one CLI
//! invocation = one walk parameter set), so every consumer — the combined
//! render, the plot orchestrators, the per-id pipeline loop, the merged
//! ndjson/timeseries/text documents — reads the one pass through a shared
//! [`Arc`] instead of re-walking or copying. A single-analyzer run is the
//! same pass with one analyzer selected, so a report never depends on which
//! other UAST analyzers are co-selected.

use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::{Change, ChangeEntry};
use cf_gitlib::Hash;

use crate::handlers::history::imports::{ImportsCommit, ImportsWalk};
use crate::handlers::history::quality::QualityCommit;
use crate::handlers::history::sentiment::SentimentCommit;
use crate::handlers::history::typos::{TyposCommit, TyposContext};
use cf_units::{KIB, MIB};

use crate::handlers::history::{
    self, gated_product, walk_chunks, ChunkPlan, CommitStamp, Gated, HistoryRun, StampSummary,
    WalkCost,
};
use crate::handlers::shotness_run::{
    self, CompiledDsl, ShotnessChangeProduct, ShotnessCommit, ShotnessDsl, ShotnessFold,
};
use crate::handlers::spill::{SpillError, Spilled};
use crate::handlers::{expand_combined_ids, run_repo_path, HistoryFilter};

/// UAST blob size cap: larger blobs are not parsed.
pub(crate) const MAX_BLOB_SIZE: usize = 256 * 1024;

/// Seconds in one `--tick-size` hour.
const SECS_PER_HOUR: i64 = 3_600;

/// Outcome of one gated UAST parse of a change side, memoized by
/// [`ChangeParses`]. The three cases preserve the distinctions the analyzers
/// branch on: a gate-skipped file contributes nothing anywhere, while a
/// gates-passed-but-unparsable file feeds sentiment's shell-comment fallback
/// (which reads the side's blob bytes).
///
/// An outcome is built in place in one of the two parse cells of a
/// [`ChangeParses`] and never moved, so the size gap between `Skipped` and
/// the inline root never reaches a hot copy (boxing the root would only add
/// an allocation per parse).
#[allow(clippy::large_enum_variant)]
pub(crate) enum ParseOutcome {
    /// A pre-parse gate rejected the file: zero hash, unsupported extension,
    /// blob read failure, or the 256 KiB cap.
    Skipped,
    /// Every gate passed but the tree-sitter parse failed.
    Failed,
    /// The parsed UAST root.
    Parsed(cf_uast::Node),
}

/// One side of a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// The old side (`change.from`; empty for an insert).
    Before,
    /// The new side (`change.to`; empty for a delete).
    After,
}

/// The blob reads and gated UAST parses of ONE change's two sides, shared by
/// every analyzer the pass computes and dropped before the next change
/// ([`for_each_change`]), so a worker holds the parses of at most one change
/// at a time. Each side reads its blob and parses at most once, however many
/// analyzers ask; a Before side identical to the After side (same blob, and
/// for the parse the same name) shares the After side's read and parse. The
/// vendor / generated / language policy was already applied to the change
/// list by the shared [`HistoryFilter`], so the gates here are purely about
/// parseability: zero hash, parser language support, blob read, the 256 KiB
/// cap, then the parse.
pub(crate) struct ChangeParses<'a> {
    repo: &'a cf_gitlib::Repository,
    parser: &'a cf_uast::Parser,
    change: &'a Change,
    before_blob: OnceCell<Option<CachedBlob>>,
    after_blob: OnceCell<Option<CachedBlob>>,
    before_parse: OnceCell<ParseOutcome>,
    after_parse: OnceCell<ParseOutcome>,
}

impl<'a> ChangeParses<'a> {
    /// An empty scope over `change`, this worker thread's repository handle
    /// and UAST parser.
    fn new(
        repo: &'a cf_gitlib::Repository,
        parser: &'a cf_uast::Parser,
        change: &'a Change,
    ) -> Self {
        Self {
            repo,
            parser,
            change,
            before_blob: OnceCell::new(),
            after_blob: OnceCell::new(),
            before_parse: OnceCell::new(),
            after_parse: OnceCell::new(),
        }
    }

    /// The change this scope parses.
    pub(crate) const fn change(&self) -> &'a Change {
        self.change
    }

    /// This worker thread's repository handle.
    pub(crate) const fn repo(&self) -> &'a cf_gitlib::Repository {
        self.repo
    }

    /// This worker thread's UAST parser.
    pub(crate) const fn parser(&self) -> &'a cf_uast::Parser {
        self.parser
    }

    /// The change entry of `side`.
    pub(crate) const fn entry(&self, side: Side) -> &'a ChangeEntry {
        match side {
            Side::Before => &self.change.from,
            Side::After => &self.change.to,
        }
    }

    /// Reads (once) the blob of `side`; `None` mirrors the
    /// `CachedBlob::from_repo` error every analyzer treats as "skip this
    /// change".
    pub(crate) fn blob(&self, side: Side) -> Option<&CachedBlob> {
        let same_blob = self.change.from.hash == self.change.to.hash;
        let cell = match side {
            Side::Before if !same_blob => &self.before_blob,
            _ => &self.after_blob,
        };
        cell.get_or_init(|| CachedBlob::from_repo(self.repo, self.entry(side).hash).ok())
            .as_ref()
    }

    /// Runs (once) the gated UAST parse of `side`.
    pub(crate) fn parse(&self, side: Side) -> &ParseOutcome {
        let (from, to) = (&self.change.from, &self.change.to);
        let same_file = from.hash == to.hash && from.name == to.name;
        let cell = match side {
            Side::Before if !same_file => &self.before_parse,
            _ => &self.after_parse,
        };
        cell.get_or_init(|| {
            #[cfg(test)]
            live_parses::born();
            self.gated_parse(side)
        })
    }

    /// The one gated parse of `side`.
    fn gated_parse(&self, side: Side) -> ParseOutcome {
        let entry = self.entry(side);
        if entry.hash.is_zero() || !self.parser.is_supported(&entry.name) {
            return ParseOutcome::Skipped;
        }
        let Some(blob) = self.blob(side) else {
            return ParseOutcome::Skipped;
        };
        if blob.data.len() > MAX_BLOB_SIZE {
            return ParseOutcome::Skipped;
        }
        match self.parser.parse(&entry.name, &blob.data) {
            Ok(root) => ParseOutcome::Parsed(root),
            Err(_) => ParseOutcome::Failed,
        }
    }
}

/// Visits `changes` in change order, each through a FRESH [`ChangeParses`]
/// that is dropped before the next change: the parse live set of a worker is
/// the two sides of one change, never a whole commit's.
fn for_each_change(
    repo: &cf_gitlib::Repository,
    parser: &cf_uast::Parser,
    changes: &[Change],
    mut visit: impl FnMut(&ChangeParses<'_>),
) {
    for change in changes {
        visit(&ChangeParses::new(repo, parser, change));
    }
}

/// Test-only accounting of the [`ParseOutcome`]s alive on the current
/// thread: the parse live set of a worker is what its memory peak follows.
#[cfg(test)]
mod live_parses {
    use std::cell::Cell;

    thread_local! {
        /// Outcomes alive now.
        static LIVE: Cell<usize> = const { Cell::new(0) };
        /// The most outcomes alive at once since the last [`reset`].
        static PEAK: Cell<usize> = const { Cell::new(0) };
        /// Outcomes created since the last [`reset`].
        static BORN: Cell<usize> = const { Cell::new(0) };
    }

    /// Counts a new outcome.
    pub(super) fn born() {
        let live = LIVE.get() + 1;
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
        BORN.set(BORN.get() + 1);
    }

    /// Counts a dropped outcome.
    pub(super) fn died() {
        LIVE.set(LIVE.get() - 1);
    }

    /// Restarts the peak and birth counts from the outcomes alive now.
    pub(super) fn reset() {
        PEAK.set(LIVE.get());
        BORN.set(0);
    }

    /// `(alive now, peak, created)` since the last [`reset`].
    pub(super) fn snapshot() -> (usize, usize, usize) {
        (LIVE.get(), PEAK.get(), BORN.get())
    }
}

#[cfg(test)]
impl Drop for ParseOutcome {
    fn drop(&mut self) {
        live_parses::died();
    }
}

/// One of the five UAST-heavy history analyzers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UastAnalyzer {
    /// `history/quality`.
    Quality,
    /// `history/sentiment`.
    Sentiment,
    /// `history/imports`.
    Imports,
    /// `history/typos`.
    Typos,
    /// `history/shotness`.
    Shotness,
}

impl UastAnalyzer {
    /// The analyzer id.
    const fn id(self) -> &'static str {
        match self {
            Self::Quality => "history/quality",
            Self::Sentiment => "history/sentiment",
            Self::Imports => "history/imports",
            Self::Typos => "history/typos",
            Self::Shotness => "history/shotness",
        }
    }
}

/// Which of the five UAST-heavy history analyzers one pass feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct UastSelection {
    quality: bool,
    sentiment: bool,
    imports: bool,
    typos: bool,
    shotness: bool,
}

impl UastSelection {
    /// Resolves the `--analyzers` patterns to the five-analyzer membership the
    /// same way every dispatch path does (`expand_combined_ids`).
    fn from_matches(sub: &clap::ArgMatches) -> Self {
        let patterns: Vec<String> = sub
            .get_many::<String>("analyzers")
            .map(|vals| vals.cloned().collect())
            .unwrap_or_default();
        let pats: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let (_statics, history_ids) = expand_combined_ids(&pats);
        let has = |id: &str| history_ids.iter().any(|have| have == id);
        Self {
            quality: has("history/quality"),
            sentiment: has("history/sentiment"),
            imports: has("history/imports"),
            typos: has("history/typos"),
            shotness: has("history/shotness"),
        }
    }

    /// This selection plus `analyzer`.
    const fn with(mut self, analyzer: UastAnalyzer) -> Self {
        match analyzer {
            UastAnalyzer::Quality => self.quality = true,
            UastAnalyzer::Sentiment => self.sentiment = true,
            UastAnalyzer::Imports => self.imports = true,
            UastAnalyzer::Typos => self.typos = true,
            UastAnalyzer::Shotness => self.shotness = true,
        }
        self
    }

    /// The selected analyzers' ids in report order
    /// ([`crate::handlers::HISTORY_PHASE_EMIT_ORDER`]), comma-joined: the
    /// pass's name in the `-v` plan line.
    fn walk_name(self) -> String {
        let selected = [
            ("history/quality", self.quality),
            ("history/sentiment", self.sentiment),
            ("history/shotness", self.shotness),
            ("history/imports", self.imports),
            ("history/typos", self.typos),
        ];
        selected
            .iter()
            .filter(|(_, on)| *on)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// How many of the five are selected.
    fn count(self) -> usize {
        usize::from(self.quality)
            + usize::from(self.sentiment)
            + usize::from(self.imports)
            + usize::from(self.typos)
            + usize::from(self.shotness)
    }
}

/// Everything that determines the pass results; a key mismatch (impossible
/// within one CLI invocation, possible across library tests) recomputes.
#[derive(Clone, PartialEq, Eq)]
struct WalkKey {
    path: String,
    head: bool,
    limit: usize,
    first_parent: bool,
    since: crate::handlers::SinceSpec,
    tick_size_hours: i64,
    max_distance: i64,
    min_comment_len: i64,
    max_changes: usize,
    exact_signatures: bool,
    people_dict: Option<String>,
    anonymize: bool,
    selection: UastSelection,
    /// The change filter (policy + `--languages`): part of the key because it
    /// changes the walk outputs.
    filter: HistoryFilter,
    /// The shotness entity selection (`--shotness-dsl-struct`,
    /// `--shotness-dsl-name`); the default when shotness is not selected.
    shotness_dsl: ShotnessDsl,
}

impl WalkKey {
    fn from_matches(sub: &clap::ArgMatches, selection: UastSelection) -> Option<Self> {
        let opts = history::HistoryOptions::from_matches(sub).ok()?;
        let shotness_dsl = if selection.shotness {
            ShotnessDsl::from_matches(sub).ok()?
        } else {
            ShotnessDsl::default()
        };
        Some(Self {
            path: run_repo_path(sub),
            head: opts.head,
            limit: opts.limit,
            first_parent: opts.first_parent,
            since: opts.since,
            tick_size_hours: opts.tick_size_hours,
            max_distance: history::typos::typos_max_distance(sub),
            min_comment_len: history::sentiment::sentiment_min_comment_len(sub),
            max_changes: opts.max_changes,
            exact_signatures: opts.exact_signatures,
            people_dict: opts.people_dict,
            anonymize: opts.anonymize,
            selection,
            filter: opts.filter,
            shotness_dsl,
        })
    }
}

/// The finished per-analyzer results of one pass — the per-commit records
/// every report builder reads, shared behind an [`Arc`]. `None` for an
/// unselected analyzer.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct UastResults {
    /// `history/quality` records, walk order.
    pub quality: Option<Arc<Vec<QualityCommit>>>,
    /// `history/sentiment` records, walk order.
    pub sentiment: Option<Arc<Vec<SentimentCommit>>>,
    /// `history/imports` records with the identity state naming the authors.
    pub imports: Option<Arc<ImportsWalk>>,
    /// `history/typos` records, walk order.
    pub typos: Option<Arc<Vec<TyposCommit>>>,
    /// `history/shotness` records, walk order.
    pub shotness: Option<Arc<Vec<ShotnessCommit>>>,
}

/// Process-wide memo: one CLI invocation has one walk parameter set, so the
/// five analyzers (and every format/section consumer above them) share ONE
/// computation. The inner `None` records a failed walk (repo unreadable) so
/// it is not retried. The lock is held across the computation: concurrent
/// analyzer tasks asking for the pass block until the first one finishes
/// it — the pass is one task, never N.
static STORE: Mutex<Option<(WalkKey, Option<HeldResults>)>> = Mutex::new(None);

/// Pre-computes the pass when two or more UAST analyzers are selected, so the
/// multi-analyzer dispatch paths can fan out their per-analyzer tasks AFTER
/// the one heavy pass completes, instead of having several tasks block on the
/// store lock. A single UAST analyzer keeps running concurrently with the
/// other selected history analyzers.
pub(crate) fn prewarm(sub: &clap::ArgMatches) {
    let selection = UastSelection::from_matches(sub);
    if selection.count() >= 2 {
        memoized(sub, selection);
    }
}

/// The pass results for this invocation's selection plus `want`, computed
/// once and memoized; only `want`'s field is filled. `None` when the walk
/// fails (the repository cannot be opened or walked) or spilled results
/// cannot be read back (the cause goes to stderr).
pub(crate) fn uast_results(sub: &clap::ArgMatches, want: UastAnalyzer) -> Option<UastResults> {
    let held = memoized(sub, UastSelection::from_matches(sub).with(want))?;
    held.view(want)
        .map_err(|e| eprintln!("Error: cannot read the spilled {}: {e}", want.id()))
        .ok()
}

/// Forgets the memoized pass results (and their spill files): a
/// long-running process (`codefang mcp`) must not hold one call's results
/// into the next.
pub(crate) fn forget_results() {
    *STORE.lock().expect("shared walk store poisoned") = None;
}

/// Looks up (computing if needed) the pass results for `selection`. The
/// lock is released before a consumer reads spilled results back.
fn memoized(sub: &clap::ArgMatches, selection: UastSelection) -> Option<HeldResults> {
    let key = WalkKey::from_matches(sub, selection)?;
    let mut guard = STORE.lock().expect("shared walk store poisoned");
    if let Some((have, results)) = guard.as_ref() {
        if *have == key {
            return results.clone();
        }
    }
    let results = compute_results(sub, &key).map(|results| hold(sub, results));
    *guard = Some((key, results.clone()));
    results
}

/// How the memo keeps finished results: on disk while the run's other
/// history walks run after the pass (they would otherwise share memory with
/// results nobody reads until rendering), in memory otherwise. A failed
/// spill keeps them in memory.
fn hold(sub: &clap::ArgMatches, results: UastResults) -> HeldResults {
    if !history::other_walks_follow_the_uast_pass(sub) {
        return HeldResults::in_memory(results);
    }
    match HeldResults::spilled(&results) {
        Ok(held) => {
            if sub.try_get_one::<bool>("verbose").ok().flatten() == Some(&true) {
                for (id, bytes) in held.spilled_bytes() {
                    eprintln!(
                        "progress: {id} results spilled to disk: {} MiB",
                        bytes.div_ceil(cf_units::MIB as u64)
                    );
                }
            }
            held
        }
        Err(e) => {
            eprintln!("warning: keeping the UAST results in memory: {e}");
            HeldResults::in_memory(results)
        }
    }
}

/// One analyzer's finished results: shared in memory, or spilled to disk.
enum Held<T> {
    Memory(Arc<T>),
    Disk(Arc<Spilled<T>>),
}

impl<T> Clone for Held<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Memory(value) => Self::Memory(Arc::clone(value)),
            Self::Disk(spilled) => Self::Disk(Arc::clone(spilled)),
        }
    }
}

impl<T: serde::Serialize + serde::de::DeserializeOwned + Default> Held<T> {
    /// `value` written to disk.
    fn spill(value: &T) -> Result<Self, SpillError> {
        Spilled::write(value).map(|spilled| Self::Disk(Arc::new(spilled)))
    }

    /// The results: the shared copy, or a fresh copy read back from disk.
    fn get(&self) -> Result<Arc<T>, SpillError> {
        match self {
            Self::Memory(value) => Ok(Arc::clone(value)),
            Self::Disk(spilled) => spilled.read().map(Arc::new),
        }
    }

    /// Bytes on disk, `None` when held in memory.
    fn disk_bytes(&self) -> Option<u64> {
        match self {
            Self::Memory(_) => None,
            Self::Disk(spilled) => Some(spilled.bytes()),
        }
    }
}

/// The pass results as the memo keeps them, one [`Held`] per selected
/// analyzer.
#[derive(Clone)]
pub(crate) struct HeldResults {
    quality: Option<Held<Vec<QualityCommit>>>,
    sentiment: Option<Held<Vec<SentimentCommit>>>,
    imports: Option<Held<ImportsWalk>>,
    typos: Option<Held<Vec<TyposCommit>>>,
    shotness: Option<Held<Vec<ShotnessCommit>>>,
}

impl HeldResults {
    /// `results`, kept in memory.
    fn in_memory(results: UastResults) -> Self {
        Self {
            quality: results.quality.map(Held::Memory),
            sentiment: results.sentiment.map(Held::Memory),
            imports: results.imports.map(Held::Memory),
            typos: results.typos.map(Held::Memory),
            shotness: results.shotness.map(Held::Memory),
        }
    }

    /// `results`, every selected analyzer written to its own spill file.
    fn spilled(results: &UastResults) -> Result<Self, SpillError> {
        Ok(Self {
            quality: results.quality.as_deref().map(Held::spill).transpose()?,
            sentiment: results.sentiment.as_deref().map(Held::spill).transpose()?,
            imports: results.imports.as_deref().map(Held::spill).transpose()?,
            typos: results.typos.as_deref().map(Held::spill).transpose()?,
            shotness: results.shotness.as_deref().map(Held::spill).transpose()?,
        })
    }

    /// `want`'s results (read back from disk when spilled); the other
    /// fields stay empty.
    fn view(&self, want: UastAnalyzer) -> Result<UastResults, SpillError> {
        let mut out = UastResults::default();
        match want {
            UastAnalyzer::Quality => {
                out.quality = self.quality.as_ref().map(Held::get).transpose()?
            }
            UastAnalyzer::Sentiment => {
                out.sentiment = self.sentiment.as_ref().map(Held::get).transpose()?;
            }
            UastAnalyzer::Imports => {
                out.imports = self.imports.as_ref().map(Held::get).transpose()?
            }
            UastAnalyzer::Typos => out.typos = self.typos.as_ref().map(Held::get).transpose()?,
            UastAnalyzer::Shotness => {
                out.shotness = self.shotness.as_ref().map(Held::get).transpose()?;
            }
        }
        Ok(out)
    }

    /// `(analyzer id, bytes)` of every spilled analyzer, in report order.
    fn spilled_bytes(&self) -> Vec<(&'static str, u64)> {
        [
            (
                "history/quality",
                self.quality.as_ref().and_then(Held::disk_bytes),
            ),
            (
                "history/sentiment",
                self.sentiment.as_ref().and_then(Held::disk_bytes),
            ),
            (
                "history/shotness",
                self.shotness.as_ref().and_then(Held::disk_bytes),
            ),
            (
                "history/imports",
                self.imports.as_ref().and_then(Held::disk_bytes),
            ),
            (
                "history/typos",
                self.typos.as_ref().and_then(Held::disk_bytes),
            ),
        ]
        .into_iter()
        .filter_map(|(id, bytes)| Some((id, bytes?)))
        .collect()
    }
}

/// The per-worker inputs of the pass's pure per-commit stage.
pub(crate) struct UastParams<'a> {
    /// The shared change filter.
    pub filter: &'a HistoryFilter,
    /// `--max-changes-per-commit`.
    pub max_changes: usize,
    /// The analyzers to compute.
    pub selection: UastSelection,
    /// `--min-comment-len` (sentiment).
    pub min_comment_len: i64,
    /// `--typos-max-distance` (typos).
    pub max_distance: i64,
    /// The shotness DSL (read when shotness is selected).
    pub shotness_dsl: &'a ShotnessDsl,
}

/// One commit's products across the selected analyzers — each field is the
/// analyzer's per-commit product over the commit's FILTERED changes, in
/// change order; an unselected analyzer's field stays empty.
#[derive(Default)]
pub(crate) struct UastCommitProduct {
    /// Per-file quality samples.
    pub quality: cf_quality::TickQuality,
    /// The comments the commit introduced or changed.
    pub sentiment: Vec<String>,
    /// The extracted `(lang, import)` entries.
    pub imports: Vec<cf_imports::history::ImportEntry>,
    /// The typo fixes.
    pub typos: Vec<cf_typos::Typo>,
    /// The per-change shotness inputs (entity lists and line diffs).
    pub shotness: Vec<ShotnessChangeProduct>,
}

/// The per-commit inputs of the selected analyzers besides the changes:
/// the compiled shotness DSL and the typos context (`None` when that
/// analyzer is not selected).
struct CommitScratch {
    shotness: Option<CompiledDsl>,
    typos: Option<TyposContext>,
}

impl UastCommitProduct {
    /// Computes the selected analyzers' products over `changes`, CHANGE-MAJOR:
    /// every selected analyzer reads one change's shared parses before the
    /// next change is parsed, and each product accumulates in change order.
    fn compute(
        repo: &cf_gitlib::Repository,
        parser: &cf_uast::Parser,
        changes: &[Change],
        params: &UastParams<'_>,
        scratch: &mut CommitScratch,
    ) -> Self {
        let mut product = Self::default();
        for_each_change(repo, parser, changes, |parses| {
            product.add_change(parses, params, scratch);
        });
        product
    }

    /// Adds one change's contribution to every selected analyzer's product.
    fn add_change(
        &mut self,
        parses: &ChangeParses<'_>,
        params: &UastParams<'_>,
        scratch: &mut CommitScratch,
    ) {
        let sel = params.selection;
        if sel.quality {
            history::quality::quality_change(parses, &mut self.quality);
        }
        if sel.sentiment {
            history::sentiment::sentiment_change(
                parses,
                params.min_comment_len,
                &mut self.sentiment,
            );
        }
        if sel.imports {
            history::imports::imports_change(parses, &mut self.imports);
        }
        if let Some(dsl) = &scratch.shotness {
            self.shotness
                .extend(shotness_run::shotness_change(parses, dsl));
        }
        if let Some(context) = &mut scratch.typos {
            history::typos::typos_change(parses, context, &mut self.typos);
        }
    }
}

/// The pure per-commit stage of the pass, run on a worker thread: the raw
/// tree diff behind the `--max-changes-per-commit` gate, the shared change
/// filter, then the selected analyzers' products. `None` on a fatal git
/// error (or a shotness DSL the preflight would have rejected).
pub(crate) fn uast_prepare(
    repo: &cf_gitlib::Repository,
    hash: Hash,
    params: &UastParams<'_>,
) -> Option<Gated<UastCommitProduct>> {
    let sel = params.selection;
    let mut scratch = CommitScratch {
        shotness: sel
            .shotness
            .then(|| params.shotness_dsl.compile())
            .transpose()
            .ok()?,
        typos: sel
            .typos
            .then(|| TyposContext::new(hash, params.max_distance)),
    };
    history::with_uast_parser(|parser| {
        gated_product(
            repo,
            hash,
            params.filter,
            params.max_changes,
            |_, changes| UastCommitProduct::compute(repo, parser, changes, params, &mut scratch),
        )
    })
}

/// The pass's reduce state: each kept commit's product is folded, in window
/// order, into the selected analyzers' records and dropped.
struct UastFold {
    quality: Option<Vec<QualityCommit>>,
    sentiment: Option<Vec<SentimentCommit>>,
    imports: Option<Vec<ImportsCommit>>,
    typos: Option<Vec<TyposCommit>>,
    shotness: Option<ShotnessFold>,
}

impl UastFold {
    /// An empty fold for `sel`; `first_parent` is the walk's first-parent
    /// mode (shotness skips merges otherwise).
    fn new(sel: UastSelection, first_parent: bool) -> Self {
        Self {
            quality: sel.quality.then(Vec::new),
            sentiment: sel.sentiment.then(Vec::new),
            imports: sel.imports.then(Vec::new),
            typos: sel.typos.then(Vec::new),
            shotness: sel.shotness.then(|| ShotnessFold::new(first_parent)),
        }
    }

    /// Folds one kept commit, in window order.
    fn consume(&mut self, stamp: &CommitStamp, product: UastCommitProduct) {
        if let Some(commits) = &mut self.quality {
            commits.push(QualityCommit::new(stamp, product.quality));
        }
        if let Some(commits) = &mut self.sentiment {
            commits.push(SentimentCommit::new(stamp, product.sentiment));
        }
        if let Some(commits) = &mut self.imports {
            commits.push(ImportsCommit::new(stamp, product.imports));
        }
        if let Some(commits) = &mut self.typos {
            commits.push(TyposCommit::new(stamp, product.typos));
        }
        if let Some(fold) = &mut self.shotness {
            fold.consume(stamp, &product.shotness);
        }
    }

    /// The finished results; imports keeps the identity state and the tick
    /// size its report names authors and ticks with.
    fn finish(self, summary: StampSummary, tick_size_secs: i64) -> UastResults {
        UastResults {
            quality: self.quality.map(Arc::new),
            sentiment: self.sentiment.map(Arc::new),
            imports: self.imports.map(|commits| {
                Arc::new(ImportsWalk {
                    commits,
                    summary,
                    tick_size_secs,
                })
            }),
            typos: self.typos.map(Arc::new),
            shotness: self.shotness.map(|fold| Arc::new(fold.finish())),
        }
    }
}

/// The shared UAST pass's measured cost (any selection: every selected
/// analyzer reads the same per-change parses). Kubernetes, 7,973 commits,
/// `history/sentiment`: one worker peaks at 166 MB, 24 workers at 839 MB
/// (28 MiB per added worker); all five co-selected peak at 664 MB and
/// 1046 MB (16 MiB per added worker). Products hold at most 24 bytes more
/// per commit over the whole window than over 50-commit chunks.
const UAST_COST: WalkCost = WalkCost {
    worker_bytes: 32 * MIB,
    product_bytes: 4 * KIB,
};

/// Runs the ONE pass over the run's chunk plan: the gated per-commit
/// products are computed in parallel one chunk at a time and folded in
/// window order ([`walk_chunks`]). `None` when the walk fails.
fn compute_results(sub: &clap::ArgMatches, key: &WalkKey) -> Option<UastResults> {
    let run = HistoryRun::open(sub)?;
    // Two or more UAST analyzers are pre-computed ([`prewarm`]) before the
    // other history walks start, so the pass has the budget to itself.
    let name = key.selection.walk_name();
    let plan = if key.selection.count() >= 2 {
        ChunkPlan::for_lone_walk(sub, &name, run.hashes.len(), UAST_COST)
    } else {
        ChunkPlan::for_walk(sub, &name, run.hashes.len(), UAST_COST)
    }
    .ok()?;
    let params = UastParams {
        filter: &run.filter,
        max_changes: run.max_changes,
        selection: key.selection,
        min_comment_len: key.min_comment_len,
        max_distance: key.max_distance,
        shotness_dsl: &key.shotness_dsl,
    };
    let mut fold = UastFold::new(key.selection, run.first_parent);
    let summary = walk_chunks(
        &run,
        plan,
        |repo, hash| uast_prepare(repo, hash, &params),
        |gated| gated.oversized(params.max_changes),
        |stamp, gated| fold.consume(stamp, gated.product),
    )?;
    Some(fold.finish(summary, run.tick_size_hours() * SECS_PER_HOUR))
}

#[cfg(test)]
mod tests {
    use cf_gitlib::testutil::{init_repo, CommitBuilder, TestRepo};

    use super::*;

    /// The `-v` plan line names the pass by its selected analyzers, in
    /// report order.
    #[test]
    fn the_pass_is_named_by_its_selection() {
        let one = UastSelection::default().with(UastAnalyzer::Sentiment);
        assert_eq!(one.walk_name(), "history/sentiment");
        let two = UastSelection::default()
            .with(UastAnalyzer::Typos)
            .with(UastAnalyzer::Quality);
        assert_eq!(two.walk_name(), "history/quality,history/typos");
    }

    /// 2021-01-01T00:00:00Z.
    const T0: i64 = 1_609_459_200;
    /// One day.
    const DAY: i64 = 86_400;
    /// The Go files every fixture commit writes.
    const FILES: [&str; 3] = ["a.go", "b.go", "c.go"];
    /// The two sides of one change: the most parses a worker may hold.
    const SIDES_PER_CHANGE: usize = 2;
    /// Every analyzer alone, then all five together (the shared pass).
    const SELECTIONS: [&[UastAnalyzer]; 6] = [
        &[UastAnalyzer::Quality],
        &[UastAnalyzer::Sentiment],
        &[UastAnalyzer::Imports],
        &[UastAnalyzer::Typos],
        &[UastAnalyzer::Shotness],
        &[
            UastAnalyzer::Quality,
            UastAnalyzer::Sentiment,
            UastAnalyzer::Imports,
            UastAnalyzer::Typos,
            UastAnalyzer::Shotness,
        ],
    ];

    /// A Go file with an import, a comment and one function, whose name
    /// carries a typo in the first version (`recieve`) that the second
    /// version fixes; the second version also adds a comment.
    fn go_file(file: &str, fixed: bool) -> Vec<u8> {
        let name = if fixed { "receive" } else { "recieve" };
        let extra = if fixed {
            "// wonderful helper that makes every caller happy\n"
        } else {
            ""
        };
        format!(
            "package p\n\nimport \"strings\"\n\n// the {file} helper keeps the happy path tidy\n{extra}func {name}(s string) string {{\n\treturn strings.TrimSpace(s)\n}}\n"
        )
        .into_bytes()
    }

    /// Two commits: the first adds [`FILES`], the second modifies all of
    /// them (three Modify changes whose both sides parse).
    fn multi_file_history() -> (TestRepo, Hash) {
        let test = init_repo().expect("init");
        let mut first = CommitBuilder::new(&test, "add", T0).author("alice", "alice@x");
        let mut second = CommitBuilder::new(&test, "fix", T0 + DAY).author("bob", "bob@x");
        for file in FILES {
            first = first.write(file, &go_file(file, false));
            second = second.write(file, &go_file(file, true));
        }
        first.commit().expect("first commit");
        let modified = second.commit().expect("second commit");
        (test, modified)
    }

    /// The parsed `run` matches over `test`.
    fn run_matches(test: &TestRepo) -> clap::ArgMatches {
        let path = test.path().to_str().expect("utf-8 path");
        crate::build_codefang_command()
            .try_get_matches_from(["codefang", "run", "-a", "history/quality", path])
            .expect("run args parse")
            .subcommand_matches("run")
            .expect("run")
            .clone()
    }

    impl UastResults {
        /// These results with only `analyzer`'s field kept.
        fn only(&self, analyzer: UastAnalyzer) -> Self {
            let mut out = Self::default();
            match analyzer {
                UastAnalyzer::Quality => out.quality.clone_from(&self.quality),
                UastAnalyzer::Sentiment => out.sentiment.clone_from(&self.sentiment),
                UastAnalyzer::Imports => out.imports.clone_from(&self.imports),
                UastAnalyzer::Typos => out.typos.clone_from(&self.typos),
                UastAnalyzer::Shotness => out.shotness.clone_from(&self.shotness),
            }
            out
        }
    }

    /// The five analyzers, in one selection.
    const ALL: [UastAnalyzer; 5] = [
        UastAnalyzer::Quality,
        UastAnalyzer::Sentiment,
        UastAnalyzer::Imports,
        UastAnalyzer::Typos,
        UastAnalyzer::Shotness,
    ];

    /// Every analyzer's results, spilled to disk, read back equal to what
    /// the pass computed; reading one analyzer loads only that analyzer.
    #[test]
    fn spilled_results_read_back_as_computed() {
        let (test, _) = multi_file_history();
        let sub = run_matches(&test);
        let selection = ALL
            .iter()
            .fold(UastSelection::default(), |sel, &a| sel.with(a));
        let key = WalkKey::from_matches(&sub, selection).expect("key");
        let computed = compute_results(&sub, &key).expect("pass");
        let held = HeldResults::spilled(&computed).expect("spill");
        for analyzer in ALL {
            assert_eq!(
                held.view(analyzer).expect("read back"),
                computed.only(analyzer),
                "{analyzer:?}"
            );
        }
        let memory = HeldResults::in_memory(computed.clone());
        assert_eq!(
            memory.view(UastAnalyzer::Typos).expect("view"),
            computed.only(UastAnalyzer::Typos)
        );
    }

    /// A worker computing a multi-file commit's products holds the parses of
    /// at most ONE change (its two sides) at a time — for every analyzer
    /// alone (a direct walk) and for all five together (the shared walk) —
    /// and still parses each side of each change exactly once, however many
    /// analyzers read it.
    #[test]
    fn a_worker_holds_the_parses_of_one_change_at_a_time() {
        let (test, modified) = multi_file_history();
        let run = HistoryRun::open(&run_matches(&test)).expect("open run");
        let dsl = ShotnessDsl::default();
        let one_side = FILES.len();
        let both_sides = SIDES_PER_CHANGE * FILES.len();
        let expected_parses = [
            one_side, both_sides, one_side, both_sides, both_sides, both_sides,
        ];
        let mut defects = Vec::new();
        for (analyzers, expected) in SELECTIONS.into_iter().zip(expected_parses) {
            let selection = analyzers
                .iter()
                .fold(UastSelection::default(), |sel, &a| sel.with(a));
            let params = UastParams {
                filter: &run.filter,
                max_changes: run.max_changes,
                selection,
                min_comment_len: cf_sentiment::DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH,
                max_distance: cf_typos::DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE as i64,
                shotness_dsl: &dsl,
            };
            live_parses::reset();
            let gated = uast_prepare(&run.repo, modified, &params).expect("prepare");
            assert_eq!(gated.raw_change_count, FILES.len());
            let (live, peak, born) = live_parses::snapshot();
            if live != 0 || peak > SIDES_PER_CHANGE || born != expected {
                defects.push(format!(
                    "{analyzers:?}: {peak} parses alive at once (at most {SIDES_PER_CHANGE}), \
                     {born} parsed (expected {expected}), {live} outlived the commit"
                ));
            }
        }
        assert!(defects.is_empty(), "{}", defects.join("\n"));
    }
}
