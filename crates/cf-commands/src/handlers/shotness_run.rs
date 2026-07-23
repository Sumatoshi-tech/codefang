//! Real `history/shotness` over the general history pipeline.
//!
//! Port of the reference streaming shotness pipeline (the reference `initHistoryPipeline` /
//! initHeadOnly → Runner → shotness `Analyzer.Consume`
//! (`handleInsertion`/`handleModification`/`handleDeletion`, diff-driven
//! line→node attribution) → aggregator (`accumulateNodes` /
//! `computeCouplingPairs`) → `ticksToReport` (`buildReportFromMerged`) →
//! `ComputeAllMetrics`).
//!
//! ## Node identity in the streaming pipeline (irreducible reference-binary nondeterminism)
//!
//! The reference analyzer keys touched nodes through `reverseNodeMap`, a
//! `map[node.ID]name`. In the **streaming** pipeline the UAST plumbing
//! (`parseBlob`) never calls `AssignStableIDs`, so every parsed node carries the
//! **empty** id `""`. `reverseNodeMap` therefore collapses to a single entry
//! `{"" : <one name>}` whose value is whichever entry the reference implementation's randomized map
//! iteration visits last. Consequently `recordTouchedNodes` attributes every
//! diff-touched line's node(s) to that one (random) name. This makes the reference implementation
//! shotness output genuinely nondeterministic at the *content* level — the
//! selected node SET differs run-to-run, not merely byte order (confirmed: two
//! the reference implementation runs at `--limit 20` select disjoint node sets of differing size). No
//! deterministic port can be byte-identical to the reference binary, and the recorded golden is
//! itself non-reproducible.
//!
//! This port reproduces the *algorithm* faithfully and resolves the empty-id
//! collapse **deterministically**: the `reverseNodeMap[""]` winner is the
//! maximum extracted name (the reference implementation picks a random one; we pick the well-defined
//! maximum so the output is stable). All accumulation, coupling, metric, and
//! serialization logic is the byte-exact cf-shotness port; only the
//! (intrinsically nondeterministic) node-name tiebreak is made deterministic.
//!
//! Output bytes route through `cf-gojson` (the reference `encoding/json` parity); never
//! `serde_json`.

use std::collections::{BTreeMap, BTreeSet};

use cf_analyzers_plumbing::identity_detector::IdentityDetector;
use cf_gitlib::changes::ChangeAction;
use cf_pathpolicy::Options as PathPolicyOptions;
use cf_shotness::aggregate::{
    accumulate_nodes, build_report_from_merged, compute_coupling_pairs, merge_nodes_into, TickNodes,
};
use cf_shotness::{compute_all_metrics, NodeSummary};
use cf_uast_node::Node;

use crate::handlers::{floor_tick_secs, run_repo_path};

/// `UASTChangesAnalyzer` spill threshold: a commit with more than this many file
/// changes streams zero UAST changes (the gated parses themselves, including
/// the 256 KiB blob cap, live in [`crate::handlers::uast_walk::CommitParseCache`]).
const SPILL_THRESHOLD: usize = 32;
/// Default DSL selecting structural nodes.
const DSL_STRUCT: &str = r#"filter(.roles has "Function")"#;
/// Default DSL extracting the node name (the reference `DefaultShotnessDSLName`,
/// `.props.name`): resolved directly from the node's `name` property — see
/// [`extract_nodes`].
const _DSL_NAME: &str = ".props.name";

/// Builds the `run --analyzers history/shotness --format json` bytes over the
/// REAL general history pipeline, or `None` if the repository cannot be
/// opened/walked.
pub fn shotness_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = shotness_run_metrics(sub)?;
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Computes the `history/shotness` [`ComputedMetrics`] over the REAL general
/// history pipeline (one report value shared by every output format), or `None`
/// if the repository cannot be opened/walked. Each `run --format` encoding is
/// just a serializer over this single value (the reference `ToJSON`/`ToYAML`/binary
/// envelope), routed at the handler layer.
pub fn shotness_run_metrics(sub: &clap::ArgMatches) -> Option<cf_shotness::ComputedMetrics> {
    let report = shotness_run_report_data(sub)?;
    Some(compute_all_metrics(&report))
}

/// The raw `history/shotness` report (the reference `ticksToReport`: key-sorted `Nodes` +
/// index-keyed `Counters`) over the same walk as [`shotness_run_metrics`]. The
/// plot path consumes this directly — the reference implementation's store writer streams exactly these
/// `(NodeSummary, Counter)` records (`WriteToStore` →
/// `extractShotnessData(report)`).
pub fn shotness_run_report_data(sub: &clap::ArgMatches) -> Option<cf_shotness::ReportData> {
    let walk = shotness_walk(sub)?;

    // Per-tick accumulator (reference: aggregator `byTick`).
    let mut by_tick: BTreeMap<i64, TickNodes> = BTreeMap::new();
    for c in &walk {
        if c.touched.is_empty() {
            continue;
        }
        let acc = by_tick.entry(c.tick).or_default();
        accumulate_nodes(acc, &c.touched);
        compute_coupling_pairs(acc, &c.touched);
    }

    // ticksToReport: merge every tick's node map, then buildReportFromMerged.
    let mut merged: TickNodes = TickNodes::new();
    for td in by_tick.values() {
        merge_nodes_into(&mut merged, td);
    }

    Some(build_report_from_merged(&merged))
}

/// One walked commit's shotness products (the reference `shotness.Consume` TC + runner
/// stamps). `touched` empty ⇔ nil-Data TC (skipped merge / spill / no nodes).
#[derive(Clone)]
pub(crate) struct ShotnessCommit {
    /// Full hex hash.
    pub hash: String,
    /// Node key → summary for the nodes this commit touched (reference:
    /// `CommitData.NodesTouched` keys + summaries; `CountDelta` is always 1).
    pub touched: BTreeMap<String, NodeSummary>,
    /// TicksSinceStart tick.
    pub tick: i64,
    /// Loose-identity author id (walk order).
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

/// The shared `history/shotness` revwalk: one entry per walked commit, walk
/// order. Every shotness format consumes THIS one walk.
pub(crate) fn shotness_walk(sub: &clap::ArgMatches) -> Option<Vec<ShotnessCommit>> {
    // Multi-analyzer runs route through the ONE shared UAST walk (same code,
    // one tree diff + one parse per blob per commit across the co-selected
    // analyzers); single-analyzer runs keep this direct walk. Only the
    // per-commit PARSES/extracts are shared — the cumulative state machine
    // below ([`ShotnessReducer`]) runs sequentially in walk order either way.
    if let Some(shared) = crate::handlers::uast_walk::shared_shotness_walk(sub) {
        return shared;
    }

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let head_only = sub.get_flag("head");
    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = crate::handlers::effective_first_parent(sub);

    let hashes: Vec<cf_gitlib::Hash> = if head_only {
        vec![repo.head().ok()?]
    } else {
        // Window: `limit` NEWEST commits oldest-first.
        crate::handlers::load_history_commit_hashes(&repo, limit, first_parent)?
    };

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    // Cumulative analyzer state + merge dedup (sequential, walk order).
    let mut reducer = ShotnessReducer::default();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    let mut commits: Vec<ShotnessCommit> = Vec::with_capacity(hashes.len());

    for hash in &hashes {
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

        let mut entry = ShotnessCommit {
            hash: hash.to_string(),
            touched: BTreeMap::new(),
            tick,
            author_id,
            when,
            offset_min: committer_when.offset_minutes(),
        };

        // shouldConsumeCommit: a merge commit is processed only the first time.
        if !reducer.should_consume(*hash, commit.num_parents()) {
            commits.push(entry);
            continue;
        }

        // Tree diff against the first parent (root → full initial tree), then
        // the SAME per-commit parse/extract/diff product the shared
        // multi-analyzer walk computes, fed to the sequential state machine.
        let changes = crate::handlers::history::commit_tree_changes(&repo, &commit)?;
        let mut cache = crate::handlers::uast_walk::CommitParseCache::new(&repo, &parser, &opts);
        let products = shotness_commit_product(&changes, &mut cache);
        entry.touched = reducer.consume(&products);

        commits.push(entry);
    }

    Some(commits)
}

/// One change's state-independent shotness inputs: the parsed/extracted node
/// lists and the file diff — everything `Consume`'s `handle*` calls need that
/// does NOT depend on the cumulative analyzer state. Computed per commit by
/// [`shotness_commit_product`] (in the direct walk AND the shared
/// multi-analyzer walk) and consumed sequentially by [`ShotnessReducer`].
pub(crate) enum ShotnessChangeProduct {
    /// A Delete change (`handleDeletion` input).
    Delete {
        /// The deleted file path.
        from_name: String,
    },
    /// An Insert change whose After side parsed (`handleInsertion` input);
    /// a failed parse emits no product, exactly as the direct walk skipped
    /// the `handle_insertion` call.
    Insert {
        /// The inserted file path.
        to_name: String,
        /// The extracted structural nodes of the After tree.
        nodes: Vec<ExtractedNode>,
    },
    /// A Modify change (`handleModification` input). The rename bookkeeping
    /// applies unconditionally; `detail` is present only when BOTH sides
    /// parsed AND the FileDiff survived its preconditions.
    Modify {
        /// The old file path.
        from_name: String,
        /// The new file path.
        to_name: String,
        /// The diff-driven touch inputs (absent ⇒ rename bookkeeping only).
        detail: Option<ShotnessModifyDetail>,
    },
}

/// The diff-driven inputs of one surviving Modify change.
pub(crate) struct ShotnessModifyDetail {
    nodes_before: Vec<ExtractedNode>,
    winner_before: Option<String>,
    nodes_after: Vec<ExtractedNode>,
    winner_after: Option<String>,
    diff: FileDiff,
}

/// Computes one commit's [`ShotnessChangeProduct`]s — the pure
/// (state-independent) half of the reference `shotness.Consume`: gated UAST
/// parses through the per-commit cache, node extraction, and the FileDiff.
/// The spill rule (> 32 changes ⇒ zero UAST changes, NO products — not even
/// deletions) matches the direct walk's pre-loop skip.
pub(crate) fn shotness_commit_product(
    changes: &[cf_gitlib::changes::Change],
    cache: &mut crate::handlers::uast_walk::CommitParseCache<'_>,
) -> Vec<ShotnessChangeProduct> {
    use crate::handlers::uast_walk::ParseOutcome;

    // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
    if changes.len() > SPILL_THRESHOLD {
        return Vec::new();
    }

    let mut products: Vec<ShotnessChangeProduct> = Vec::with_capacity(changes.len());
    for change in changes {
        match change.action {
            ChangeAction::Delete => products.push(ShotnessChangeProduct::Delete {
                from_name: change.from.name.clone(),
            }),
            ChangeAction::Insert => {
                if let ParseOutcome::Parsed(after) = &*cache.parse(&change.to.name, change.to.hash)
                {
                    let (nodes, _winner) = extract_nodes(after);
                    products.push(ShotnessChangeProduct::Insert {
                        to_name: change.to.name.clone(),
                        nodes,
                    });
                }
            }
            ChangeAction::Modify => {
                let before = cache.parse(&change.from.name, change.from.hash);
                let after = cache.parse(&change.to.name, change.to.hash);
                let detail = match (&*before, &*after) {
                    (ParseOutcome::Parsed(before), ParseOutcome::Parsed(after)) => {
                        file_diff(cache, change).map(|diff| {
                            let (nodes_before, winner_before) = extract_nodes(before);
                            let (nodes_after, winner_after) = extract_nodes(after);
                            ShotnessModifyDetail {
                                nodes_before,
                                winner_before,
                                nodes_after,
                                winner_after,
                                diff,
                            }
                        })
                    }
                    _ => None,
                };
                products.push(ShotnessChangeProduct::Modify {
                    from_name: change.from.name.clone(),
                    to_name: change.to.name.clone(),
                    detail,
                });
            }
        }
    }
    products
}

/// The sequential, cumulative half of the reference `shotness.Consume`: the
/// state machine over commits in walk order. Both the direct walk and the
/// shared multi-analyzer walk drive THIS reducer over identical per-commit
/// products, so the cumulative node/file state — and therefore every report —
/// is byte-identical between the two.
#[derive(Default)]
pub(crate) struct ShotnessReducer {
    /// Cumulative analyzer state.
    state: ShotnessState,
    /// Merge dedup tracker (the reference `s.merges.SeenOrAdd` over
    /// `NumParents() > 1`).
    seen_merges: BTreeSet<cf_gitlib::Hash>,
}

impl ShotnessReducer {
    /// shouldConsumeCommit: a merge commit is processed only the first time.
    pub(crate) fn should_consume(&mut self, hash: cf_gitlib::Hash, num_parents: usize) -> bool {
        num_parents <= 1 || self.seen_merges.insert(hash)
    }

    /// Applies one commit's products to the cumulative state and returns the
    /// touched-node map (the reference `buildCommitData`: nil unless a known
    /// node was touched).
    pub(crate) fn consume(
        &mut self,
        products: &[ShotnessChangeProduct],
    ) -> BTreeMap<String, NodeSummary> {
        // allNodes: node keys touched in THIS commit (deduped, the reference `allNodes`).
        let mut all_nodes: BTreeSet<String> = BTreeSet::new();

        for product in products {
            match product {
                ShotnessChangeProduct::Delete { from_name } => {
                    self.state.handle_deletion(from_name);
                }
                ShotnessChangeProduct::Insert { to_name, nodes } => {
                    // Insertion has no diff, so every extracted node is touched
                    // under its OWN name (the reference implementation iterates
                    // `res` (name→node) directly).
                    for n in nodes {
                        self.state
                            .add_node(&n.name, &n.type_, to_name, &mut all_nodes);
                    }
                }
                ShotnessChangeProduct::Modify {
                    from_name,
                    to_name,
                    detail,
                } => {
                    if from_name != to_name {
                        self.state.apply_rename(from_name, to_name);
                    }
                    if let Some(d) = detail {
                        self.state.apply_diff_edits(
                            to_name,
                            &d.nodes_before,
                            d.winner_before.as_deref(),
                            &d.nodes_after,
                            d.winner_after.as_deref(),
                            &d.diff,
                            &mut all_nodes,
                        );
                    }
                }
            }
        }

        // buildCommitData: nil unless a known node was touched.
        let mut touched: BTreeMap<String, NodeSummary> = BTreeMap::new();
        for key in &all_nodes {
            if let Some(ns) = self.state.nodes.get(key) {
                touched.insert(key.clone(), ns.summary.clone());
            }
        }
        touched
    }
}

/// Per-commit shotness NDJSON records (forked leaf): only commits that touched
/// known nodes emit a line; `data` is the reference implementation's `*CommitData` — one `NodesTouched`
/// map (key-sorted) of `NodeDelta{Summary{Type,Name,File}, CountDelta: 1}`.
pub fn shotness_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<crate::handlers::history_formats::NdjsonRecord>> {
    use cf_gojson::value::{GoMap, GoValue};

    let walk = shotness_walk(sub)?;
    let mut records = Vec::new();
    for (pos, c) in walk.iter().enumerate() {
        if c.touched.is_empty() {
            continue;
        }
        let mut nodes = GoMap::new_map();
        for (key, ns) in &c.touched {
            let mut summary = GoMap::new_struct();
            summary.insert("Type".to_string(), GoValue::Str(ns.type_.clone()));
            summary.insert("Name".to_string(), GoValue::Str(ns.name.clone()));
            summary.insert("File".to_string(), GoValue::Str(ns.file.clone()));
            let mut delta = GoMap::new_struct();
            delta.insert("Summary".to_string(), GoValue::Map(summary));
            delta.insert("CountDelta".to_string(), GoValue::Int(1));
            nodes.insert(key.clone(), GoValue::Map(delta));
        }
        let mut data = GoMap::new_struct();
        data.insert("NodesTouched".to_string(), GoValue::Map(nodes));
        records.push(crate::handlers::history_formats::NdjsonRecord {
            pos,
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

/// The shotness contribution to the merged `--format timeseries` document (reference:
/// `shotness.ExtractCommitTimeSeries` over `report["commit_stats"]`): per
/// node-touching commit `{"nodes_touched": n, "coupling_pairs": n*(n-1)/2}`.
pub fn shotness_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<crate::handlers::history_formats::TimeSeriesContribution> {
    use cf_gojson::value::{GoMap, GoValue};

    let walk = shotness_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &walk {
        if c.touched.is_empty() {
            continue;
        }
        let n = c.touched.len() as i64;
        // The reference `computeCouplingPairs`: 0 below minCouplingNodes (2), else C(n,2).
        let pairs = if n < 2 { 0 } else { n * (n - 1) / 2 };
        let mut entry = GoMap::new_map();
        entry.insert("nodes_touched".to_string(), GoValue::Int(n));
        entry.insert("coupling_pairs".to_string(), GoValue::Int(pairs));
        per_commit.push((c.hash.clone(), GoValue::Map(entry)));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(crate::handlers::history_formats::TimeSeriesContribution {
        flag: "shotness",
        per_commit,
        commit_meta,
    })
}

/// One extracted structural node: its resolved name, type, and 1-based inclusive
/// line span, used for registration and diff line→node mapping
/// (the reference `extractNodes` / `genLine2Node` / `resolveEndLine`).
pub(crate) struct ExtractedNode {
    name: String,
    type_: String,
    start_line: usize,
    end_line: usize,
}

/// Extracts structural nodes via the struct DSL and resolves each node's name
/// via the name DSL. Returns one entry per distinct name
/// (last-wins on name collision over the deterministic `FindDSL` slice order)
/// plus the deterministic `reverseNodeMap[""]` winner name (the maximum name).
///
/// The reference implementation builds `res map[name]*Node` and then `reverseNodeMap` maps every node's
/// (empty) ID to a single name, picking one at random. We return the maximum
/// name as the deterministic winner.
fn extract_nodes(root: &Node) -> (Vec<ExtractedNode>, Option<String>) {
    let Ok(structs) = root.find_dsl(DSL_STRUCT) else {
        return (Vec::new(), None);
    };

    // name → node (last-wins over the deterministic FindDSL slice order).
    let mut named: BTreeMap<String, ExtractedNode> = BTreeMap::new();

    for struct_node in &structs {
        // Reference: `nameNodes, err := structNode.FindDSL(".props.name")`. The
        // `.props.<key>` field access yields one literal node holding the value of
        // the `name` property (the Rust DSL engine lacks the nested-props
        // processor, so resolve it directly — identical semantics). the reference implementation's name
        // selection:
        //   if err==nil && len(nameNodes)>0 { name=nameNodes[0].Token;
        //                                      if name!="" { use name } }      // empty ⇒ skip
        //   else if structNode.Token!="" { use structNode.Token }              // missing key ⇒ fall back
        let name = match struct_node.props.get("name") {
            Some(prop) => {
                if prop.is_empty() {
                    continue; // present-but-empty ⇒ no fallback, skip
                }
                prop.clone()
            }
            None if !struct_node.token.is_empty() => struct_node.token.clone(),
            None => continue,
        };

        let (start_line, end_line) = node_span(struct_node);
        named.insert(
            name.clone(),
            ExtractedNode {
                name,
                type_: struct_node.node_type.as_str().to_string(),
                start_line,
                end_line,
            },
        );
    }

    // reverseNodeMap[""] winner: deterministic stand-in for the reference implementation's random map pick.
    let winner = named.keys().next_back().cloned();
    let nodes: Vec<ExtractedNode> = named.into_values().collect();
    (nodes, winner)
}

/// Resolves a node's 1-based inclusive line span (the reference `pos.StartLine` /
/// `resolveEndLine`: explicit end line if greater, else the max descendant line).
fn node_span(n: &Node) -> (usize, usize) {
    let Some(pos) = &n.pos else { return (0, 0) };
    let start = pos.start_line as usize;
    if pos.end_line > pos.start_line {
        return (start, pos.end_line as usize);
    }
    let mut end = start;
    n.visit_pre_order(|child| {
        if let Some(cp) = &child.pos {
            let candidate = if cp.end_line > cp.start_line {
                cp.end_line
            } else {
                cp.start_line
            };
            if candidate as usize > end {
                end = candidate as usize;
            }
        }
    });
    (start, end)
}

/// A line→spanning-node-indices map; `line2node[l-1]` holds
/// the indices (into the `nodes` slice) of nodes whose span covers line `l`.
fn gen_line2node(nodes: &[ExtractedNode], lines: usize) -> Vec<Vec<usize>> {
    let mut res: Vec<Vec<usize>> = vec![Vec::new(); lines];
    for (i, n) in nodes.iter().enumerate() {
        if n.start_line == 0 {
            continue;
        }
        for line in n.start_line..=n.end_line {
            if line > 0 && line <= res.len() {
                res[line - 1].push(i);
            }
        }
    }
    res
}

/// Diff operation kind (the reference `diffmatchpatch.Operation` as used by FileDiff).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffOp {
    Delete,
    Insert,
    Equal,
}

/// One diff edit: its op and the number of lines it spans
/// (`utf8.RuneCountInString(edit.Text)` over the line-encoded diff text).
struct DiffEdit {
    op: DiffOp,
    line_count: usize,
}

/// Per-file diff: the rune-encoded edits plus the old/new LOC counts that the reference implementation's
/// FileDiff reports as `len(src)` / `len(dst)`.
struct FileDiff {
    old_lines_of_code: usize,
    new_lines_of_code: usize,
    edits: Vec<DiffEdit>,
}

/// Computes the FileDiff for a Modify change, mirroring the reference implementation's `processChange`
/// (skip same-hash / binary, line-diff via the diffmatchpatch port). Returns
/// `None` when the change is not diffed (reference `FileDiff` only diffs Modify changes
/// and skips binary blobs / missing blobs). Blob reads go through the
/// per-commit cache so the parse path and the diff path fetch each blob once.
fn file_diff(
    cache: &mut crate::handlers::uast_walk::CommitParseCache<'_>,
    change: &cf_gitlib::changes::Change,
) -> Option<FileDiff> {
    use cf_godiff::{line_diff, Op};

    if change.action != ChangeAction::Modify {
        return None;
    }
    if change.from.hash == change.to.hash || change.from.hash.is_zero() || change.to.hash.is_zero()
    {
        return None;
    }
    let blob_from = cache.blob(change.from.hash)?;
    let blob_to = cache.blob(change.to.hash)?;
    if is_binary(&blob_from.data) || is_binary(&blob_to.data) {
        return None;
    }

    // The reference implementation decodes via string([]byte) (no whitespace stripping at default config).
    let from = String::from_utf8_lossy(&blob_from.data).into_owned();
    let to = String::from_utf8_lossy(&blob_to.data).into_owned();

    // Identical-string fast path (reference: single DiffEqual of "L"*lineCount).
    if from == to {
        let lc = count_lines(&from);
        return Some(FileDiff {
            old_lines_of_code: lc,
            new_lines_of_code: lc,
            edits: vec![DiffEdit {
                op: DiffOp::Equal,
                line_count: lc,
            }],
        });
    }

    // FileDiff's DiffTimeout default is 1000ms (>0) ⇒ timeout_active = true.
    let segments = line_diff(from.as_bytes(), to.as_bytes(), true);

    let mut edits = Vec::with_capacity(segments.len());
    let mut old_loc = 0usize;
    let mut new_loc = 0usize;
    for seg in segments {
        let n = seg.lines.len();
        let op = match seg.op {
            Op::Delete => {
                old_loc += n;
                DiffOp::Delete
            }
            Op::Insert => {
                new_loc += n;
                DiffOp::Insert
            }
            Op::Equal => {
                old_loc += n;
                new_loc += n;
                DiffOp::Equal
            }
        };
        edits.push(DiffEdit { op, line_count: n });
    }

    Some(FileDiff {
        old_lines_of_code: old_loc,
        new_lines_of_code: new_loc,
        edits,
    })
}

/// Counts lines the way the reference implementation's identical-string fast path does
/// (`strings.Count(s,"\n")` plus one if non-empty without a trailing newline).
fn count_lines(s: &str) -> usize {
    let mut count = s.bytes().filter(|&b| b == b'\n').count();
    if !s.is_empty() && s.as_bytes().last() != Some(&b'\n') {
        count += 1;
    }
    count
}

/// The reference implementation's `CachedBlob.IsBinary`: a NUL byte in the first 8000 bytes.
fn is_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(8000)];
    head.contains(&0)
}

/// Cumulative shotness analyzer state.
#[derive(Default)]
struct ShotnessState {
    /// key → node hotness.
    nodes: BTreeMap<String, StateNode>,
    /// file → set of keys belonging to that file.
    files: BTreeMap<String, BTreeSet<String>>,
}

/// Per-node cumulative state.
struct StateNode {
    summary: NodeSummary,
    count: i64,
}

impl ShotnessState {
    /// Registers or increments a node.
    fn add_node(&mut self, name: &str, type_: &str, file: &str, all_nodes: &mut BTreeSet<String>) {
        let summary = NodeSummary::new(type_, name, file);
        let key = summary.key();
        let exists = all_nodes.contains(&key);
        all_nodes.insert(key.clone());

        let count = self.nodes.get(&key).map_or(0, |n| n.count);

        if count == 0 {
            self.nodes
                .insert(key.clone(), StateNode { summary, count: 1 });
            self.files.entry(file.to_string()).or_default().insert(key);
        } else if !exists {
            if let Some(n) = self.nodes.get_mut(&key) {
                n.count = count + 1;
            }
        }
    }

    /// Removes all nodes associated with a deleted file.
    fn handle_deletion(&mut self, from_name: &str) {
        if let Some(keys) = self.files.remove(from_name) {
            for key in keys {
                self.nodes.remove(&key);
            }
        }
    }

    /// Walks the diff edits and records touched nodes (the reference `applyDiffEdits` +
    /// `recordTouchedNodes`). With the empty-id `reverseNodeMap` collapse, a
    /// Delete hunk attributes the Before-winner name and an Insert hunk the
    /// After-winner name; the node Type comes from each line-spanning node `n`.
    #[allow(clippy::too_many_arguments)]
    fn apply_diff_edits(
        &mut self,
        to_name: &str,
        nodes_before: &[ExtractedNode],
        winner_before: Option<&str>,
        nodes_after: &[ExtractedNode],
        winner_after: Option<&str>,
        diff: &FileDiff,
        all_nodes: &mut BTreeSet<String>,
    ) {
        let line2node_before = gen_line2node(nodes_before, diff.old_lines_of_code);
        let line2node_after = gen_line2node(nodes_after, diff.new_lines_of_code);

        let mut line_before: usize = 0;
        let mut line_after: usize = 0;

        for edit in &diff.edits {
            let size = edit.line_count;
            match edit.op {
                DiffOp::Delete => {
                    self.record_touched(
                        &line2node_before,
                        nodes_before,
                        winner_before,
                        line_before,
                        size,
                        to_name,
                        all_nodes,
                    );
                    line_before += size;
                }
                DiffOp::Insert => {
                    self.record_touched(
                        &line2node_after,
                        nodes_after,
                        winner_after,
                        line_after,
                        size,
                        to_name,
                        all_nodes,
                    );
                    line_after += size;
                }
                DiffOp::Equal => {
                    line_before += size;
                    line_after += size;
                }
            }
        }
    }

    /// Records nodes touched by a hunk spanning `[start, start+size)`
    ///. For each line-spanning node, `addNode` is
    /// called with the winner name and that node's type.
    #[allow(clippy::too_many_arguments)]
    fn record_touched(
        &mut self,
        line2node: &[Vec<usize>],
        nodes: &[ExtractedNode],
        winner: Option<&str>,
        start: usize,
        size: usize,
        file: &str,
        all_nodes: &mut BTreeSet<String>,
    ) {
        let Some(winner) = winner else { return };
        for l in start..start + size {
            if l < line2node.len() {
                for &idx in &line2node[l] {
                    let type_ = nodes[idx].type_.clone();
                    self.add_node(winner, &type_, file, all_nodes);
                }
            }
        }
    }

    /// Updates state when a file is renamed.
    fn apply_rename(&mut self, old_name: &str, new_name: &str) {
        let Some(old_keys) = self.files.remove(old_name) else {
            return;
        };
        let mut new_keys: BTreeSet<String> = BTreeSet::new();
        for old_key in old_keys {
            if let Some(mut node) = self.nodes.remove(&old_key) {
                node.summary.file = new_name.to_string();
                let new_key = node.summary.key();
                self.nodes.insert(new_key.clone(), node);
                new_keys.insert(new_key);
            }
        }
        self.files.insert(new_name.to_string(), new_keys);
    }
}
