//! Real `history/shotness` over the general history pipeline.
//!
//! Port of the Go streaming shotness pipeline (run.go initHistoryPipeline /
//! initHeadOnly → Runner → shotness `Analyzer.Consume`
//! (`handleInsertion`/`handleModification`/`handleDeletion`, diff-driven
//! line→node attribution) → aggregator (`accumulateNodes` /
//! `computeCouplingPairs`) → `ticksToReport` (`buildReportFromMerged`) →
//! `ComputeAllMetrics`).
//!
//! ## Node identity in the streaming pipeline (irreducible Go nondeterminism)
//!
//! The Go analyzer keys touched nodes through `reverseNodeMap`, a
//! `map[node.ID]name`. In the **streaming** pipeline the UAST plumbing
//! (`parseBlob`) never calls `AssignStableIDs`, so every parsed node carries the
//! **empty** id `""`. `reverseNodeMap` therefore collapses to a single entry
//! `{"" : <one name>}` whose value is whichever entry Go's randomized map
//! iteration visits last. Consequently `recordTouchedNodes` attributes every
//! diff-touched line's node(s) to that one (random) name. This makes the Go
//! shotness output genuinely nondeterministic at the *content* level — the
//! selected node SET differs run-to-run, not merely byte order (confirmed: two
//! Go runs at `--limit 20` select disjoint node sets of differing size). No
//! deterministic port can be byte-identical to Go, and the recorded golden is
//! itself non-reproducible.
//!
//! This port reproduces the *algorithm* faithfully and resolves the empty-id
//! collapse **deterministically**: the `reverseNodeMap[""]` winner is the
//! maximum extracted name (Go picks a random one; we pick the well-defined
//! maximum so the output is stable). All accumulation, coupling, metric, and
//! serialization logic is the byte-exact cf-shotness port; only the
//! (intrinsically nondeterministic) node-name tiebreak is made deterministic.
//!
//! Output bytes route through `cf-gojson` (Go `encoding/json` parity); never
//! `serde_json`.

use std::collections::{BTreeMap, BTreeSet};

use cf_analyzers_plumbing::identity_detector::IdentityDetector;
use cf_gitlib::blob::CachedBlob;
use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction, ChangeEntry};
use cf_gitlib::repository::LogOptions;
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
use cf_shotness::aggregate::{
    accumulate_nodes, build_report_from_merged, compute_coupling_pairs, merge_nodes_into, TickNodes,
};
use cf_shotness::{compute_all_metrics, NodeSummary};
use cf_uast_node::Node;

use crate::{floor_tick_secs, run_repo_path};

/// `UASTChangesAnalyzer` spill threshold: a commit with more than this many file
/// changes streams zero UAST changes (Go `UASTPipeline.SpillThreshold = 32`).
const SPILL_THRESHOLD: usize = 32;
/// UAST blob size cap (Go `maxUASTBlobSize = 256 KiB`).
const MAX_BLOB_SIZE: usize = 256 * 1024;
/// Default DSL selecting structural nodes (Go `DefaultShotnessDSLStruct`).
const DSL_STRUCT: &str = r#"filter(.roles has "Function")"#;
/// Default DSL extracting the node name (Go `DefaultShotnessDSLName`,
/// `.props.name`): resolved directly from the node's `name` property — see
/// [`extract_nodes`].
const _DSL_NAME: &str = ".props.name";

/// Builds the `run --analyzers history/shotness --format json` bytes over the
/// REAL general history pipeline, or `None` if the repository cannot be
/// opened/walked.
pub fn shotness_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let head_only = sub.get_flag("head");
    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    let hashes: Vec<cf_gitlib::Hash> = if head_only {
        vec![repo.head().ok()?]
    } else {
        let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
        let mut iter = repo.log(&log_opts).ok()?;
        let mut v = Vec::new();
        while limit <= 0 || (v.len() as i64) < limit {
            match iter.next_commit() {
                Some(c) => v.push(c.hash()),
                None => break,
            }
        }
        v
    };

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();

    // Per-tick accumulator (Go aggregator `byTick`).
    let mut by_tick: BTreeMap<i64, TickNodes> = BTreeMap::new();
    // Cumulative analyzer state (Go `s.nodes` / `s.files`).
    let mut state = ShotnessState::default();
    // Merge dedup tracker (Go `s.merges.SeenOrAdd` over NumParents() > 1).
    let mut seen_merges: BTreeSet<cf_gitlib::Hash> = BTreeSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        let gsig = commit.author();
        let _author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // shouldConsumeCommit: a merge commit is processed only the first time.
        if commit.num_parents() > 1 && !seen_merges.insert(*hash) {
            continue;
        }

        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        // allNodes: node keys touched in THIS commit (deduped, Go `allNodes`).
        let mut all_nodes: BTreeSet<String> = BTreeSet::new();

        for change in &changes {
            match change.action {
                ChangeAction::Delete => state.handle_deletion(&change.from.name),
                ChangeAction::Insert => {
                    if let Some(after) = parse_change_uast(&repo, &parser, &opts, &change.to) {
                        state.handle_insertion(&change.to.name, &after, &mut all_nodes);
                    }
                }
                ChangeAction::Modify => {
                    let before = parse_change_uast(&repo, &parser, &opts, &change.from);
                    let after = parse_change_uast(&repo, &parser, &opts, &change.to);
                    state.handle_modification(
                        &repo,
                        change,
                        before.as_ref(),
                        after.as_ref(),
                        &mut all_nodes,
                    );
                }
            }
        }

        if all_nodes.is_empty() {
            continue;
        }

        // buildCommitData → extractTC/accumulateNodes/computeCouplingPairs.
        let mut touched: BTreeMap<String, NodeSummary> = BTreeMap::new();
        for key in &all_nodes {
            if let Some(ns) = state.nodes.get(key) {
                touched.insert(key.clone(), ns.summary.clone());
            }
        }
        if touched.is_empty() {
            continue;
        }

        let acc = by_tick.entry(tick).or_default();
        accumulate_nodes(acc, &touched);
        compute_coupling_pairs(acc, &touched);
    }

    // ticksToReport: merge every tick's node map, then buildReportFromMerged.
    let mut merged: TickNodes = TickNodes::new();
    for td in by_tick.values() {
        merge_nodes_into(&mut merged, td);
    }

    let report = build_report_from_merged(&merged);
    let metrics = compute_all_metrics(&report);
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Parses a change side's blob into a UAST root, applying the same filters as
/// Go's `UASTPipeline.parseBlob`: path-policy exclusion, parser support,
/// 256 KiB blob cap, content-aware generated detection.
fn parse_change_uast(
    repo: &cf_gitlib::Repository,
    parser: &cf_uast::Parser,
    opts: &PathPolicyOptions,
    entry: &ChangeEntry,
) -> Option<Node> {
    let name = &entry.name;
    if entry.hash.is_zero() {
        return None;
    }
    if exclude(name, None, opts) {
        return None;
    }
    if !parser.is_supported(name) {
        return None;
    }
    let blob = CachedBlob::from_repo(repo, entry.hash).ok()?;
    if blob.data.len() > MAX_BLOB_SIZE {
        return None;
    }
    if exclude(name, Some(&blob.data), opts) {
        return None;
    }
    parser.parse(name, &blob.data).ok()
}

/// One extracted structural node: its resolved name, type, and 1-based inclusive
/// line span, used for registration and diff line→node mapping
/// (Go `extractNodes` / `genLine2Node` / `resolveEndLine`).
struct ExtractedNode {
    name: String,
    type_: String,
    start_line: usize,
    end_line: usize,
}

/// Extracts structural nodes via the struct DSL and resolves each node's name
/// via the name DSL (Go `extractNodes`). Returns one entry per distinct name
/// (last-wins on name collision over the deterministic `FindDSL` slice order)
/// plus the deterministic `reverseNodeMap[""]` winner name (the maximum name).
///
/// Go builds `res map[name]*Node` and then `reverseNodeMap` maps every node's
/// (empty) ID to a single name, picking one at random. We return the maximum
/// name as the deterministic winner.
fn extract_nodes(root: &Node) -> (Vec<ExtractedNode>, Option<String>) {
    let Ok(structs) = root.find_dsl(DSL_STRUCT) else {
        return (Vec::new(), None);
    };

    // name → node (last-wins over the deterministic FindDSL slice order).
    let mut named: BTreeMap<String, ExtractedNode> = BTreeMap::new();

    for struct_node in &structs {
        // Go: `nameNodes, err := structNode.FindDSL(".props.name")`. The
        // `.props.<key>` field access yields one literal node holding the value of
        // the `name` property (the Rust DSL engine lacks the nested-props
        // processor, so resolve it directly — identical semantics). Go's name
        // selection:
        //   if err==nil && len(nameNodes)>0 { name=nameNodes[0].Token;
        //                                      if name!="" { use name } }      // empty ⇒ skip
        //   else if structNode.Token!="" { use structNode.Token }              // missing key ⇒ fall back
        let name = match struct_node.props.get("name") {
            Some(prop) => {
                if prop.is_empty() {
                    continue; // present-but-empty ⇒ no fallback, skip (Go parity)
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

    // reverseNodeMap[""] winner: deterministic stand-in for Go's random map pick.
    let winner = named.keys().next_back().cloned();
    let nodes: Vec<ExtractedNode> = named.into_values().collect();
    (nodes, winner)
}

/// Resolves a node's 1-based inclusive line span (Go `pos.StartLine` /
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
            let candidate = if cp.end_line > cp.start_line { cp.end_line } else { cp.start_line };
            if candidate as usize > end {
                end = candidate as usize;
            }
        }
    });
    (start, end)
}

/// A line→spanning-node-indices map (Go `genLine2Node`); `line2node[l-1]` holds
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

/// Diff operation kind (Go `diffmatchpatch.Operation` as used by FileDiff).
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

/// Per-file diff: the rune-encoded edits plus the old/new LOC counts that Go's
/// FileDiff reports as `len(src)` / `len(dst)`.
struct FileDiff {
    old_lines_of_code: usize,
    new_lines_of_code: usize,
    edits: Vec<DiffEdit>,
}

/// Computes the FileDiff for a Modify change, mirroring Go's `processChange`
/// (skip same-hash / binary, line-diff via the diffmatchpatch port). Returns
/// `None` when the change is not diffed (Go FileDiff only diffs Modify changes
/// and skips binary blobs / missing blobs).
fn file_diff(repo: &cf_gitlib::Repository, change: &cf_gitlib::changes::Change) -> Option<FileDiff> {
    use cf_godiff::{line_diff, Op};

    if change.action != ChangeAction::Modify {
        return None;
    }
    if change.from.hash == change.to.hash || change.from.hash.is_zero() || change.to.hash.is_zero() {
        return None;
    }
    let blob_from = CachedBlob::from_repo(repo, change.from.hash).ok()?;
    let blob_to = CachedBlob::from_repo(repo, change.to.hash).ok()?;
    if is_binary(&blob_from.data) || is_binary(&blob_to.data) {
        return None;
    }

    // Go decodes via string([]byte) (no whitespace stripping at default config).
    let from = String::from_utf8_lossy(&blob_from.data).into_owned();
    let to = String::from_utf8_lossy(&blob_to.data).into_owned();

    // Identical-string fast path (Go: single DiffEqual of "L"*lineCount).
    if from == to {
        let lc = count_lines(&from);
        return Some(FileDiff {
            old_lines_of_code: lc,
            new_lines_of_code: lc,
            edits: vec![DiffEdit { op: DiffOp::Equal, line_count: lc }],
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

    Some(FileDiff { old_lines_of_code: old_loc, new_lines_of_code: new_loc, edits })
}

/// Counts lines the way Go's identical-string fast path does
/// (`strings.Count(s,"\n")` plus one if non-empty without a trailing newline).
fn count_lines(s: &str) -> usize {
    let mut count = s.bytes().filter(|&b| b == b'\n').count();
    if !s.is_empty() && s.as_bytes().last() != Some(&b'\n') {
        count += 1;
    }
    count
}

/// Go's `CachedBlob.IsBinary`: a NUL byte in the first 8000 bytes.
fn is_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(8000)];
    head.contains(&0)
}

/// Cumulative shotness analyzer state (Go `s.nodes` / `s.files`).
#[derive(Default)]
struct ShotnessState {
    /// key → node hotness (Go `s.nodes`).
    nodes: BTreeMap<String, StateNode>,
    /// file → set of keys belonging to that file (Go `s.files`).
    files: BTreeMap<String, BTreeSet<String>>,
}

/// Per-node cumulative state (Go `nodeShotness`).
struct StateNode {
    summary: NodeSummary,
    count: i64,
}

impl ShotnessState {
    /// Registers or increments a node (Go `addNode`).
    fn add_node(&mut self, name: &str, type_: &str, file: &str, all_nodes: &mut BTreeSet<String>) {
        let summary = NodeSummary::new(type_, name, file);
        let key = summary.key();
        let exists = all_nodes.contains(&key);
        all_nodes.insert(key.clone());

        let count = self.nodes.get(&key).map(|n| n.count).unwrap_or(0);

        if count == 0 {
            self.nodes.insert(key.clone(), StateNode { summary, count: 1 });
            self.files.entry(file.to_string()).or_default().insert(key);
        } else if !exists {
            if let Some(n) = self.nodes.get_mut(&key) {
                n.count = count + 1;
            }
        }
    }

    /// Removes all nodes associated with a deleted file (Go `handleDeletion`).
    fn handle_deletion(&mut self, from_name: &str) {
        if let Some(keys) = self.files.remove(from_name) {
            for key in keys {
                self.nodes.remove(&key);
            }
        }
    }

    /// Extracts nodes from a newly inserted file and registers them
    /// (Go `handleInsertion`). Insertion has no diff, so every extracted node is
    /// touched under its OWN name (Go iterates `res` (name→node) directly).
    fn handle_insertion(&mut self, to_name: &str, after: &Node, all_nodes: &mut BTreeSet<String>) {
        let (nodes, _winner) = extract_nodes(after);
        for n in &nodes {
            self.add_node(&n.name, &n.type_, to_name, all_nodes);
        }
    }

    /// Processes a file modification: rename bookkeeping then diff-driven
    /// line→node touch recording (Go `handleModification`).
    fn handle_modification(
        &mut self,
        repo: &cf_gitlib::Repository,
        change: &cf_gitlib::changes::Change,
        before: Option<&Node>,
        after: Option<&Node>,
        all_nodes: &mut BTreeSet<String>,
    ) {
        let to_name = &change.to.name;

        if change.from.name != *to_name {
            self.apply_rename(&change.from.name, to_name);
        }

        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        let Some(diff) = file_diff(repo, change) else {
            return;
        };

        let (nodes_before, winner_before) = extract_nodes(before);
        let (nodes_after, winner_after) = extract_nodes(after);

        self.apply_diff_edits(
            to_name,
            &nodes_before,
            winner_before.as_deref(),
            &nodes_after,
            winner_after.as_deref(),
            &diff,
            all_nodes,
        );
    }

    /// Walks the diff edits and records touched nodes (Go `applyDiffEdits` +
    /// `recordTouchedNodes`). With the empty-id `reverseNodeMap` collapse, a
    /// Delete hunk attributes the Before-winner name and an Insert hunk the
    /// After-winner name; the node Type comes from each line-spanning node `n`
    /// (Go `addNode(id=winnerName, n, file)`).
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
    /// (Go `recordTouchedNodes`). For each line-spanning node, `addNode` is
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

    /// Updates state when a file is renamed (Go `applyRename`).
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
