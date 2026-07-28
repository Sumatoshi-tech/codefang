//! Shared UAST history walk for the five tree-sitter-heavy history analyzers
//! (`history/{imports,quality,sentiment,shotness,typos}`).
//!
//! Each of those analyzers consumes the SAME per-commit inputs: the tree diff
//! against the first parent, the common gates (spill threshold, 256 KiB blob
//! cap, path policy), and the UAST parse of the changed blobs — the dominant
//! cost of a history run. When a single `run` selects two or more of them, the
//! reference pipeline streams ONE commit walk into every leaf; running five
//! independent walks here multiplied the wall time by the selection size.
//!
//! This module restores the single-walk shape WITHOUT touching any analyzer
//! logic: one [`parallel_prepare`] pass computes the tree diff once per commit
//! and parses each needed `(file name, blob hash)` at most once per commit
//! through [`CommitParseCache`], then calls the SAME per-commit product
//! functions the single-analyzer walks call
//! ([`history::quality_commit_product`], [`history::sentiment_commit_product`],
//! [`history::imports_commit_product`], [`history::typos_commit_product`],
//! [`shotness_run::shotness_commit_product`]). The per-analyzer sequential
//! reduces (identity/tick stamping, shotness's cumulative state machine) run
//! unchanged over the shared walk's outputs, so byte identity follows from
//! shared code, not from re-verified duplication.
//!
//! The computed walks are memoized process-wide (one CLI invocation = one walk
//! parameter set), so every consumer — the combined render, the plot
//! orchestrators, the per-id pipeline loop, the merged ndjson/timeseries/text
//! documents, and the anomaly store enrichment that re-reads quality/sentiment
//! metrics — reuses the one walk instead of re-walking. Single-analyzer
//! invocations (fewer than two of the five selected) keep their existing
//! direct walks and never touch this store.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use cf_gitlib::blob::CachedBlob;
use cf_gitlib::Hash;
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

use crate::handlers::history::{self, ImportsCommit, QualityCommit, SentimentCommit, TyposCommit};
use crate::handlers::shotness_run::{self, ShotnessChangeProduct, ShotnessCommit, ShotnessReducer};
use crate::handlers::{
    effective_first_parent, expand_combined_ids, floor_tick_secs, load_history_commit_hashes,
    run_repo_path,
};

/// `UASTChangesAnalyzer` spill threshold, identical across the five analyzers:
/// a commit with more than this many file changes streams zero UAST changes.
pub(crate) const SPILL_THRESHOLD: usize = 32;
/// UAST blob size cap (`UASTPipeline.parseBlob`).
pub(crate) const MAX_BLOB_SIZE: usize = 256 * 1024;

/// Outcome of one gated `(file name, blob hash)` UAST parse, memoized per
/// commit by [`CommitParseCache`]. The three cases preserve the distinctions
/// the analyzers branch on: a gate-skipped file contributes nothing anywhere,
/// while a gates-passed-but-unparsable file still counts as an analyzed file
/// for quality and feeds sentiment's shell-comment fallback (which needs the
/// blob bytes).
pub(crate) enum ParseOutcome {
    /// A pre-parse gate rejected the file: zero hash, path policy, unsupported
    /// extension, blob read failure, the 256 KiB cap, or content-aware
    /// generated detection.
    Skipped,
    /// Every gate passed but the tree-sitter parse failed; the blob is kept
    /// for content fallbacks (sentiment's `.sh` comment extraction).
    Failed(Rc<CachedBlob>),
    /// The parsed UAST root.
    Parsed(cf_uast::Node),
}

/// Per-commit blob + UAST parse memo shared by every analyzer product function
/// run over that commit. Keyed by `(file name, blob hash)` so each blob parses
/// at most once per commit no matter how many of the five analyzers (or change
/// sides) need it. The gate sequence inside [`CommitParseCache::parse`] is the
/// exact `UASTPipeline.parseBlob` order every individual walk applied: zero
/// hash, `pathpolicy.Exclude(name, nil)`, parser language support, blob read,
/// the 256 KiB cap, `pathpolicy.Exclude(name, content)`, then the parse. All
/// gates are pure functions of `(name, blob)`, so memoizing them is
/// observation-free.
pub(crate) struct CommitParseCache<'a> {
    repo: &'a cf_gitlib::Repository,
    parser: &'a cf_uast::Parser,
    opts: &'a PathPolicyOptions,
    blobs: HashMap<Hash, Option<Rc<CachedBlob>>>,
    parses: HashMap<(String, Hash), Rc<ParseOutcome>>,
}

impl<'a> CommitParseCache<'a> {
    /// Builds an empty per-commit cache over this worker thread's repository
    /// handle, UAST parser, and path-policy options.
    pub(crate) fn new(
        repo: &'a cf_gitlib::Repository,
        parser: &'a cf_uast::Parser,
        opts: &'a PathPolicyOptions,
    ) -> Self {
        Self {
            repo,
            parser,
            opts,
            blobs: HashMap::new(),
            parses: HashMap::new(),
        }
    }

    /// Reads (and memoizes) a blob; `None` mirrors the `CachedBlob::from_repo`
    /// error every walk treated as "skip this change".
    pub(crate) fn blob(&mut self, hash: Hash) -> Option<Rc<CachedBlob>> {
        if let Some(cached) = self.blobs.get(&hash) {
            return cached.clone();
        }
        let blob = CachedBlob::from_repo(self.repo, hash).ok().map(Rc::new);
        self.blobs.insert(hash, blob.clone());
        blob
    }

    /// Runs the gated UAST parse for `(name, hash)`, memoized per commit.
    pub(crate) fn parse(&mut self, name: &str, hash: Hash) -> Rc<ParseOutcome> {
        let key = (name.to_string(), hash);
        if let Some(cached) = self.parses.get(&key) {
            return Rc::clone(cached);
        }
        let outcome = Rc::new(self.parse_uncached(name, hash));
        self.parses.insert(key, Rc::clone(&outcome));
        outcome
    }

    /// The one gated parse (`UASTPipeline.parseBlob` gate order).
    fn parse_uncached(&mut self, name: &str, hash: Hash) -> ParseOutcome {
        if hash.is_zero() {
            return ParseOutcome::Skipped;
        }
        if exclude(name, None, self.opts) {
            return ParseOutcome::Skipped;
        }
        if !self.parser.is_supported(name) {
            return ParseOutcome::Skipped;
        }
        let Some(blob) = self.blob(hash) else {
            return ParseOutcome::Skipped;
        };
        if blob.data.len() > MAX_BLOB_SIZE {
            return ParseOutcome::Skipped;
        }
        if exclude(name, Some(&blob.data), self.opts) {
            return ParseOutcome::Skipped;
        }
        match self.parser.parse(name, &blob.data) {
            Ok(root) => ParseOutcome::Parsed(root),
            Err(_) => ParseOutcome::Failed(blob),
        }
    }
}

/// Which of the five UAST-heavy history analyzers the current `run` selected
/// (resolved over the registry exactly like the dispatch paths do).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
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

    /// How many of the five are selected.
    fn count(self) -> usize {
        usize::from(self.quality)
            + usize::from(self.sentiment)
            + usize::from(self.imports)
            + usize::from(self.typos)
            + usize::from(self.shotness)
    }

    /// The shared walk only pays off (and only changes the execution shape)
    /// when at least two of the five run in one process; single-analyzer
    /// invocations keep their direct walks.
    fn shared_applies(self) -> bool {
        self.count() >= 2
    }
}

/// Everything that determines the walk outputs; a key mismatch (impossible
/// within one CLI invocation, possible across library tests) recomputes.
#[derive(Clone, PartialEq, Eq)]
struct WalkKey {
    path: String,
    head: bool,
    limit: i64,
    first_parent: bool,
    since: crate::handlers::SinceSpec,
    max_distance: i64,
    max_changes: usize,
    selection: UastSelection,
}

impl WalkKey {
    fn from_matches(sub: &clap::ArgMatches, selection: UastSelection) -> Self {
        Self {
            path: run_repo_path(sub),
            head: sub.get_flag("head"),
            limit: sub.get_one::<i64>("limit").copied().unwrap_or(0),
            first_parent: effective_first_parent(sub),
            since: crate::handlers::history_since_spec(sub),
            max_distance: history::typos_max_distance(sub),
            max_changes: history::max_changes_per_commit_cap(sub),
            selection,
        }
    }
}

/// The shared walk's per-analyzer outputs — exactly what each direct walk
/// returns. `None` for an unselected analyzer.
struct SharedWalks {
    quality: Option<Vec<QualityCommit>>,
    sentiment: Option<Vec<SentimentCommit>>,
    imports: Option<Vec<ImportsCommit>>,
    typos: Option<Vec<TyposCommit>>,
    shotness: Option<Vec<ShotnessCommit>>,
}

/// Process-wide memo: one CLI invocation has one walk parameter set, so the
/// five walk functions (and every format/section consumer above them) share
/// ONE computation. The inner `Option<SharedWalks>` records a failed walk
/// (repo unreadable) so it is not retried. The lock is held across the
/// computation: concurrent analyzer tasks asking for the walk block until the
/// first one finishes it — the shared walk is one task, never N.
static STORE: Mutex<Option<(WalkKey, Option<SharedWalks>)>> = Mutex::new(None);

/// Pre-computes the shared walk (when applicable) so the multi-analyzer
/// dispatch paths can fan out their per-analyzer tasks AFTER the one heavy
/// walk completes, instead of having several tasks block on the store lock.
pub(crate) fn prewarm(sub: &clap::ArgMatches) {
    let selection = UastSelection::from_matches(sub);
    if !selection.shared_applies() {
        return;
    }
    let key = WalkKey::from_matches(sub, selection);
    let mut guard = STORE.lock().expect("shared walk store poisoned");
    if matches!(guard.as_ref(), Some((have, _)) if *have == key) {
        return;
    }
    let walks = compute_shared_walks(selection, &key);
    *guard = Some((key, walks));
}

/// Looks up (computing if needed) one analyzer's shared-walk output. Returns
/// `None` when the shared walk does not apply to this invocation — the caller
/// then runs its existing direct walk. `Some(inner)` is the walk result
/// (`inner == None` ⇔ the walk failed, exactly like the direct walk's `None`).
fn shared_component<T>(
    sub: &clap::ArgMatches,
    selected: impl Fn(UastSelection) -> bool,
    extract: impl Fn(&SharedWalks) -> Option<T>,
) -> Option<Option<T>> {
    let selection = UastSelection::from_matches(sub);
    if !selection.shared_applies() || !selected(selection) {
        return None;
    }
    let key = WalkKey::from_matches(sub, selection);
    let mut guard = STORE.lock().expect("shared walk store poisoned");
    if let Some((have, walks)) = guard.as_ref() {
        if *have == key {
            return Some(walks.as_ref().and_then(&extract));
        }
    }
    let walks = compute_shared_walks(selection, &key);
    let result = walks.as_ref().and_then(&extract);
    *guard = Some((key, walks));
    Some(result)
}

/// The `history/quality` view of the shared walk (see [`shared_component`]).
pub(crate) fn shared_quality_walk(sub: &clap::ArgMatches) -> Option<Option<Vec<QualityCommit>>> {
    shared_component(sub, |sel| sel.quality, |w| w.quality.clone())
}

/// The `history/sentiment` view of the shared walk.
pub(crate) fn shared_sentiment_walk(
    sub: &clap::ArgMatches,
) -> Option<Option<Vec<SentimentCommit>>> {
    shared_component(sub, |sel| sel.sentiment, |w| w.sentiment.clone())
}

/// The `history/imports` view of the shared walk.
pub(crate) fn shared_imports_walk(sub: &clap::ArgMatches) -> Option<Option<Vec<ImportsCommit>>> {
    shared_component(sub, |sel| sel.imports, |w| w.imports.clone())
}

/// The `history/typos` view of the shared walk.
pub(crate) fn shared_typos_walk(sub: &clap::ArgMatches) -> Option<Option<Vec<TyposCommit>>> {
    shared_component(sub, |sel| sel.typos, |w| w.typos.clone())
}

/// The `history/shotness` view of the shared walk.
pub(crate) fn shared_shotness_walk(sub: &clap::ArgMatches) -> Option<Option<Vec<ShotnessCommit>>> {
    shared_component(sub, |sel| sel.shotness, |w| w.shotness.clone())
}

/// One commit's products across the selected analyzers — each field is the
/// exact value the analyzer's direct walk computes per commit.
#[derive(Default)]
struct SharedCommitProduct {
    /// RAW (pre-filter) tree-diff change count, for the oversized-commit gate.
    raw_change_count: usize,
    quality: Option<cf_quality::TickQuality>,
    sentiment: Option<Option<Vec<String>>>,
    imports: Option<Vec<cf_imports::history::ImportEntry>>,
    typos: Option<Vec<cf_typos::Typo>>,
    shotness: Option<Vec<ShotnessChangeProduct>>,
}

/// Runs the ONE shared walk: the same window selection as every direct walk,
/// one parallel pure-compute stage (tree diff once, every parse through the
/// per-commit cache), then the SAME sequential ordered reduces (identity/tick
/// stamping computed once — it is identical across the five walks — plus
/// shotness's cumulative state machine). `None` mirrors the direct walks'
/// failure (`.ok()?` on a fatal git error).
fn compute_shared_walks(sel: UastSelection, key: &WalkKey) -> Option<SharedWalks> {
    use cf_alg_levenshtein::Context as LevenshteinContext;
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;

    let path = &key.path;
    let repo = cf_gitlib::Repository::open(path).ok()?;

    // Window: identical to every direct walk (`--head` ⇒ exactly the HEAD
    // commit; otherwise the `limit` commits oldest-first).
    let hashes = if key.head {
        vec![repo.head().ok()?]
    } else {
        load_history_commit_hashes(&repo, key.limit, key.first_parent, key.since)?
    };

    let max_distance = key.max_distance;
    // Oversized-commit gate: commits whose RAW tree diff exceeds the cap are
    // silently dropped from history BEFORE any analyzer (reference framework
    // behaviour; flag `--max-changes-per-commit`, 0 = default 10000).
    let max_changes = key.max_changes;
    let opts = PathPolicyOptions::default();
    let opts_ref = &opts;

    // ---- parallel pure-compute stage -----------------------------------------
    // Per commit: ONE tree diff, ONE gated parse per (name, hash) via the
    // cache, then each selected analyzer's per-commit product function — the
    // same functions the direct walks call.
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let prepared = history::parallel_prepare(path, &hashes, workers, move |repo, hash| {
        history::with_uast_parser(|parser| {
            let commit = repo.lookup_commit(hash).ok()?;
            let changes = history::commit_tree_changes(repo, &commit)?;
            let mut product = SharedCommitProduct {
                raw_change_count: changes.len(),
                ..SharedCommitProduct::default()
            };
            // Oversized commits are dropped before any analyzer — no per-file
            // work; the reduce below removes them from the walk entirely.
            if product.raw_change_count > max_changes {
                return Some(product);
            }
            let mut cache = CommitParseCache::new(repo, parser, opts_ref);
            if sel.quality {
                product.quality = Some(history::quality_commit_product(&changes, &mut cache));
            }
            if sel.sentiment {
                product.sentiment = Some(history::sentiment_commit_product(&changes, &mut cache));
            }
            if sel.imports {
                product.imports = Some(history::imports_commit_product(&changes, &mut cache));
            }
            if sel.shotness {
                product.shotness =
                    Some(shotness_run::shotness_commit_product(&changes, &mut cache));
            }
            if sel.typos {
                // The Levenshtein context is pure scratch buffers (results are
                // state-independent), so a per-commit context is byte-identical
                // to the direct walk's long-lived one.
                let mut lctx = LevenshteinContext::new();
                product.typos = Some(history::typos_commit_product(
                    repo,
                    hash,
                    &changes,
                    max_distance,
                    &mut lctx,
                    &mut cache,
                ));
            }
            Some(product)
        })
    })?;

    // Oversized-commit skip: dropped from history before identity/tick
    // stamping (the reference framework never shows the commit to any
    // analyzer, core or leaf), exactly like each direct walk's gate. The
    // ORIGINAL walk position of each surviving commit is preserved: the
    // reference runner numbers consume positions before the drop suppresses a
    // record, and the forked-leaf NDJSON drain order keys on that position.
    let mut kept_pos: Vec<usize> = Vec::new();
    let (hashes, prepared): (Vec<_>, Vec<_>) = hashes
        .into_iter()
        .zip(prepared)
        .enumerate()
        .filter(|(_, (_, p))| p.raw_change_count <= max_changes)
        .map(|(i, hp)| {
            kept_pos.push(i);
            hp
        })
        .unzip();

    // ---- sequential ordered identity/tick stamping ----------------------------
    // Identical across the five walks, so computed once here.
    struct Stamp {
        pos: usize,
        hash_str: String,
        tick: i64,
        author_id: i64,
        when: i64,
        offset_min: i32,
        num_parents: usize,
    }
    let mut identity = IdentityDetector::new();
    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;
    let mut stamps: Vec<Stamp> = Vec::with_capacity(hashes.len());
    for (j, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;
        let committer_when = commit.committer().when;
        let when = committer_when.seconds();

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

        stamps.push(Stamp {
            pos: kept_pos[j],
            hash_str: hash.to_string(),
            tick,
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
            num_parents: commit.num_parents(),
        });
    }

    // Zips the shared stamps with each selected analyzer's per-commit product
    // into that analyzer's commit-struct vec (the direct walks' exact output).
    fn assemble<T>(
        selected: bool,
        stamps: &[Stamp],
        prepared: &[SharedCommitProduct],
        build: impl Fn(&Stamp, &SharedCommitProduct) -> T,
    ) -> Option<Vec<T>> {
        selected.then(|| {
            stamps
                .iter()
                .zip(prepared)
                .map(|(s, p)| build(s, p))
                .collect()
        })
    }

    let quality = assemble(sel.quality, &stamps, &prepared, |s, p| QualityCommit {
        pos: s.pos,
        hash: s.hash_str.clone(),
        tq: p
            .quality
            .clone()
            .expect("quality product for selected analyzer"),
        tick: s.tick,
        author_id: s.author_id,
        when: s.when,
        offset_min: s.offset_min,
    });
    let sentiment = assemble(sel.sentiment, &stamps, &prepared, |s, p| SentimentCommit {
        hash: s.hash_str.clone(),
        comments: p
            .sentiment
            .clone()
            .expect("sentiment product for selected analyzer"),
        tick: s.tick,
        author_id: s.author_id,
        when: s.when,
        offset_min: s.offset_min,
    });
    let imports = assemble(sel.imports, &stamps, &prepared, |s, p| ImportsCommit {
        hash: s.hash_str.clone(),
        entries: p
            .imports
            .clone()
            .expect("imports product for selected analyzer"),
        author_id: s.author_id,
        tick: s.tick,
        when: s.when,
        offset_min: s.offset_min,
    });
    let typos = assemble(sel.typos, &stamps, &prepared, |s, p| TyposCommit {
        typos: p
            .typos
            .clone()
            .expect("typos product for selected analyzer"),
        tick: s.tick,
        author_id: s.author_id,
        when: s.when,
        offset_min: s.offset_min,
    });

    // ---- shotness sequential cumulative reduce --------------------------------
    // The state machine consumes the precomputed parse/extract/diff products in
    // walk order — the same `ShotnessReducer` the direct walk drives.
    let shotness = sel.shotness.then(|| {
        let mut reducer = ShotnessReducer::default();
        stamps
            .iter()
            .zip(&prepared)
            .zip(&hashes)
            .map(|((s, p), hash)| {
                let mut entry = ShotnessCommit {
                    hash: s.hash_str.clone(),
                    touched: std::collections::BTreeMap::new(),
                    tick: s.tick,
                    author_id: s.author_id,
                    when: s.when,
                    offset_min: s.offset_min,
                };
                if reducer.should_consume(*hash, s.num_parents) {
                    let products = p
                        .shotness
                        .as_ref()
                        .expect("shotness product for selected analyzer");
                    entry.touched = reducer.consume(products);
                }
                entry
            })
            .collect()
    });

    Some(SharedWalks {
        quality,
        sentiment,
        imports,
        typos,
        shotness,
    })
}
