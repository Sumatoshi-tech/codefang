//! `history/shotness` over the shared history core.
//!
//! Per commit, in the pure parallel pre-pass of the history core: the
//! filtered tree diff against the first parent and, change by change, the
//! gated UAST parse of the changed blobs through the change's [`ChangeParses`]
//! (the shared 256 KiB cap), the entity extraction (structural DSL → name
//! DSL → qualified name) and the line diff — the [`ShotnessChangeProduct`]s,
//! which keep only the extracted entities and the diff, never a tree. Then
//! ONE sequential fold in chronological order ([`ShotnessFold`]) turns
//! the products into each commit's [`CommitEffects`] over the live
//! [`ShotnessState`]: an entity is born on insertion, touched once per commit
//! whose hunks overlap its line span, moved by a file rename and ended when
//! its declaration or file disappears. Every format replays the SAME effect
//! records: the report is the state after the last commit, ndjson /
//! timeseries list the per-commit touches.
//!
//! Entity identity is `(type, qualified name, file)`: the name DSL result
//! prefixed by the names of the enclosing declarations
//! (`Outer.Inner.method`) and, for a method declared with a receiver
//! (`func (a *A) String()`), by the receiver type (`A.String`), so same-named
//! methods on different types are different entities.
//!
//! Merge commits are skipped (their diff against the first parent re-shows
//! the side line that was already walked); under `--first-parent` a merge is
//! an ordinary commit and counts once.
//!
//! Output bytes route through `cf-gojson`; never `serde_json`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cf_gitlib::changes::ChangeAction;
use cf_shotness::{
    compute_all_metrics, coupling_pair_count, touched_entities, CommitEffects, EntityEffect,
    NodeSummary, ShotnessState, DEFAULT_SHOTNESS_DSL_NAME, DEFAULT_SHOTNESS_DSL_STRUCT,
};
use cf_uast_node::dsl::Query;
use cf_uast_node::{
    Node, ROLE_NAME, ROLE_TYPE, UAST_CLASS, UAST_ENUM, UAST_FUNCTION, UAST_IDENTIFIER,
    UAST_INTERFACE, UAST_METHOD, UAST_PARAMETER, UAST_STRUCT,
};

use crate::handlers::history::{is_merge, CommitStamp};
use crate::handlers::uast_walk::{uast_results, ChangeParses, ParseOutcome, Side, UastAnalyzer};
use crate::pipeline::HandlerError;

/// The structural-selection flag.
pub(crate) const FLAG_DSL_STRUCT: &str = "shotness-dsl-struct";
/// The name-extraction flag.
pub(crate) const FLAG_DSL_NAME: &str = "shotness-dsl-name";
/// `--shotness-keep-ended`: ended entities stay in the report, marked.
pub(crate) const FLAG_KEEP_ENDED: &str = "shotness-keep-ended";

/// Declaration kinds whose names qualify the entities declared inside them.
const QUALIFYING_TYPES: [&str; 6] = [
    UAST_CLASS,
    UAST_STRUCT,
    UAST_INTERFACE,
    UAST_ENUM,
    UAST_FUNCTION,
    UAST_METHOD,
];
/// Separator between a qualifier and the name it qualifies.
const QUALIFIER_SEPARATOR: &str = ".";

/// The `history/shotness` DSL configuration as the user set it: the structural
/// selection (`--shotness-dsl-struct`) and the name extraction
/// (`--shotness-dsl-name`), both validated at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShotnessDsl {
    /// The structural query, run against each parsed file root.
    pub struct_query: String,
    /// The name query, run against each selected node.
    pub name_query: String,
}

impl Default for ShotnessDsl {
    fn default() -> Self {
        Self {
            struct_query: DEFAULT_SHOTNESS_DSL_STRUCT.to_string(),
            name_query: DEFAULT_SHOTNESS_DSL_NAME.to_string(),
        }
    }
}

impl ShotnessDsl {
    /// Reads and validates the two DSL flags. A flag the user did not set
    /// (or set empty) resolves to the crate default.
    ///
    /// # Errors
    ///
    /// [`HandlerError::InvalidFlag`] naming the flag whose expression does not
    /// compile.
    pub(crate) fn from_matches(sub: &clap::ArgMatches) -> Result<Self, HandlerError> {
        let dsl = Self {
            struct_query: dsl_flag(sub, FLAG_DSL_STRUCT, DEFAULT_SHOTNESS_DSL_STRUCT),
            name_query: dsl_flag(sub, FLAG_DSL_NAME, DEFAULT_SHOTNESS_DSL_NAME),
        };
        dsl.compile()?;
        Ok(dsl)
    }

    /// Compiles both queries.
    ///
    /// # Errors
    ///
    /// [`HandlerError::InvalidFlag`] naming the flag whose expression does not
    /// compile.
    pub(crate) fn compile(&self) -> Result<CompiledDsl, HandlerError> {
        Ok(CompiledDsl {
            structure: compile_flag(FLAG_DSL_STRUCT, &self.struct_query)?,
            name: compile_flag(FLAG_DSL_NAME, &self.name_query)?,
        })
    }
}

/// Compiles one flag's expression, naming the flag on failure.
fn compile_flag(flag: &str, query: &str) -> Result<Query, HandlerError> {
    Query::compile(query)
        .map_err(|e| HandlerError::InvalidFlag(format!("invalid --{flag} {query:?}: {e}")))
}

/// The flag's value when the user set it (command line or environment) to a
/// non-empty string, else `default` — the registered flag default is NOT
/// consulted, so the crate default stays the effective one.
fn dsl_flag(sub: &clap::ArgMatches, name: &str, default: &str) -> String {
    use clap::parser::ValueSource;
    let Ok(Some(value)) = sub.try_get_one::<String>(name) else {
        return default.to_string();
    };
    match sub.value_source(name) {
        Some(ValueSource::CommandLine | ValueSource::EnvVariable) if !value.is_empty() => {
            value.clone()
        }
        _ => default.to_string(),
    }
}

/// Validates the shotness DSL flags (the preflight leg of this analyzer).
///
/// # Errors
///
/// [`HandlerError::InvalidFlag`] naming the flag whose expression does not
/// compile.
pub(crate) fn validate_shotness_dsl(sub: &clap::ArgMatches) -> Result<(), HandlerError> {
    ShotnessDsl::from_matches(sub).map(|_| ())
}

/// Both DSL queries compiled once per worker commit (a [`Query`] is not
/// `Sync`, so the parallel pre-pass compiles from the [`ShotnessDsl`] strings).
pub(crate) struct CompiledDsl {
    structure: Query,
    name: Query,
}

/// Computes the `history/shotness` [`cf_shotness::ComputedMetrics`] over the
/// shared history core (one report value shared by every output format), or
/// `None` if the repository cannot be opened/walked.
pub fn shotness_run_metrics(sub: &clap::ArgMatches) -> Option<cf_shotness::ComputedMetrics> {
    let report = shotness_run_report_data(sub)?;
    Some(compute_all_metrics(&report))
}

/// The raw `history/shotness` report (key-sorted `Nodes` + index-keyed
/// `Counters`): the live state after replaying every walked commit's effects
/// in walk order. The plot path consumes this directly.
pub fn shotness_run_report_data(sub: &clap::ArgMatches) -> Option<cf_shotness::ReportData> {
    let walk = shotness_walk(sub)?;
    let keep_ended = sub
        .try_get_one::<bool>(FLAG_KEEP_ENDED)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let mut state = if keep_ended {
        ShotnessState::keeping_ended()
    } else {
        ShotnessState::new()
    };
    for c in walk.iter() {
        state.apply(&c.touched);
    }
    Some(state.report())
}

/// One walked commit's shotness record: its [`CommitEffects`] plus the shared
/// identity/tick stamps. `touched` empty ⇔ the commit affected no entity
/// (skipped merge / no parseable change).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShotnessCommit {
    /// Position in the ORIGINAL window (oversized-dropped commits count): the
    /// key the merged ndjson stream orders every analyzer's records by.
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// What the commit did to each entity, keyed by entity key.
    pub touched: CommitEffects,
    /// The tick.
    pub tick: i64,
    /// The author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

impl ShotnessCommit {
    /// Whether the commit touched (counted a change on) any entity.
    fn touched_any(&self) -> bool {
        self.touched.values().any(|e| e.touched)
    }
}

/// The `history/shotness` records of the run's UAST pass: one entry per
/// walked commit, walk order. Every shotness format consumes THIS one walk.
pub(crate) fn shotness_walk(sub: &clap::ArgMatches) -> Option<Arc<Vec<ShotnessCommit>>> {
    uast_results(sub, UastAnalyzer::Shotness)?.shotness
}

/// One change's state-independent shotness inputs: the extracted entity lists
/// and the file diff — everything the reduce needs that does NOT depend on
/// the cumulative state. Computed per change by [`shotness_change`] and
/// folded per commit, sequentially, by [`ShotnessFold`] — which needs ALL of
/// a commit's products at once (every file-level delete and rename applies
/// before any entity touch), so a commit's products are kept until its fold.
pub(crate) enum ShotnessChangeProduct {
    /// A Delete change: every entity of the file ends.
    Delete {
        /// The deleted file path.
        from_name: String,
    },
    /// An Insert change whose After side parsed: every entity is born.
    Insert {
        /// The inserted file path.
        to_name: String,
        /// The extracted entities of the After tree.
        nodes: Vec<ExtractedNode>,
    },
    /// A Modify change. The rename bookkeeping applies unconditionally;
    /// `detail` is present only when BOTH sides parsed AND the line diff
    /// survived its preconditions.
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
    nodes_after: Vec<ExtractedNode>,
    diff: FileDiff,
}

/// Computes one change's [`ShotnessChangeProduct`] — the pure
/// (state-independent) half of the consume: the gated UAST parses of the
/// change's sides, entity extraction under `dsl`, and the line diff. `None`
/// for an Insert whose After side does not parse (it contributes nothing).
pub(crate) fn shotness_change(
    parses: &ChangeParses<'_>,
    dsl: &CompiledDsl,
) -> Option<ShotnessChangeProduct> {
    let change = parses.change();
    match change.action {
        ChangeAction::Delete => Some(ShotnessChangeProduct::Delete {
            from_name: change.from.name.clone(),
        }),
        ChangeAction::Insert => match parses.parse(Side::After) {
            ParseOutcome::Parsed(after) => Some(ShotnessChangeProduct::Insert {
                to_name: change.to.name.clone(),
                nodes: extract_nodes(after, dsl),
            }),
            ParseOutcome::Failed | ParseOutcome::Skipped => None,
        },
        ChangeAction::Modify => {
            let detail = match (parses.parse(Side::Before), parses.parse(Side::After)) {
                (ParseOutcome::Parsed(before), ParseOutcome::Parsed(after)) => file_diff(parses)
                    .map(|diff| ShotnessModifyDetail {
                        nodes_before: extract_nodes(before, dsl),
                        nodes_after: extract_nodes(after, dsl),
                        diff,
                    }),
                _ => None,
            };
            Some(ShotnessChangeProduct::Modify {
                from_name: change.from.name.clone(),
                to_name: change.to.name.clone(),
                detail,
            })
        }
    }
}

/// The sequential half of the walk: turns each commit's products into its
/// [`CommitEffects`] over the live state (file deletes and renames need the
/// entities currently declared in the file), applies them and records them,
/// in walk order. Merges are skipped (their record has no effects) unless the
/// walk is first-parent simplified.
pub(crate) struct ShotnessFold {
    /// The live entity state.
    state: ShotnessState,
    /// Whether the walk simplifies to the first parent (a merge is then an
    /// ordinary commit).
    first_parent: bool,
    /// One record per folded commit.
    commits: Vec<ShotnessCommit>,
}

impl ShotnessFold {
    /// An empty fold for a walk in the given first-parent mode.
    pub(crate) fn new(first_parent: bool) -> Self {
        Self {
            state: ShotnessState::new(),
            first_parent,
            commits: Vec::new(),
        }
    }

    /// Folds one kept commit's per-change products, in window order.
    pub(crate) fn consume(&mut self, stamp: &CommitStamp, products: &[ShotnessChangeProduct]) {
        let touched = if is_merge(stamp.num_parents, self.first_parent) {
            CommitEffects::new()
        } else {
            self.effects(products)
        };
        self.commits.push(ShotnessCommit {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            touched,
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        });
    }

    /// The folded records, walk order.
    pub(crate) fn finish(self) -> Vec<ShotnessCommit> {
        self.commits
    }

    /// Builds and applies one commit's effects, returning them.
    fn effects(&mut self, products: &[ShotnessChangeProduct]) -> CommitEffects {
        let mut effects = CommitEffects::new();
        // File-level events first (a rename onto a deleted path must see the
        // delete), then the per-entity touches and ends.
        for product in products {
            match product {
                ShotnessChangeProduct::Delete { from_name } => {
                    self.end_file(&mut effects, from_name);
                }
                ShotnessChangeProduct::Modify {
                    from_name, to_name, ..
                } if from_name != to_name => {
                    self.move_file(&mut effects, from_name, to_name);
                }
                _ => {}
            }
        }
        for product in products {
            match product {
                ShotnessChangeProduct::Insert { to_name, nodes } => {
                    for n in nodes {
                        touch(&mut effects, n.summary(to_name));
                    }
                }
                ShotnessChangeProduct::Modify {
                    to_name,
                    detail: Some(d),
                    ..
                } => {
                    apply_diff_edits(&mut effects, to_name, d);
                    for removed in removed_declarations(&d.nodes_before, &d.nodes_after) {
                        end(&mut effects, removed.summary(to_name));
                    }
                }
                _ => {}
            }
        }
        self.state.apply(&effects);
        effects
    }

    /// Records the end of every entity declared in `file`.
    fn end_file(&self, effects: &mut CommitEffects, file: &str) {
        for node in self.state.entities_in(file) {
            end(effects, node.summary.clone());
        }
    }

    /// Records the move of every entity of `from` under `to`.
    fn move_file(&self, effects: &mut CommitEffects, from: &str, to: &str) {
        for node in self.state.entities_in(from) {
            let moved = NodeSummary::new(&node.summary.type_, &node.summary.name, to);
            let key = moved.key();
            let effect = effects.entry(key).or_insert_with(|| EntityEffect {
                summary: moved,
                touched: false,
                ended: false,
                moved_from: None,
            });
            effect.moved_from = Some(node.summary.key());
            // The destination key's previous occupant (a deleted file's
            // entity) is replaced by the move, never ended after it.
            effect.ended = false;
        }
    }
}

/// Records a touch of `summary` in the commit's effects.
fn touch(effects: &mut CommitEffects, summary: NodeSummary) {
    effects
        .entry(summary.key())
        .or_insert_with(|| EntityEffect {
            summary,
            touched: false,
            ended: false,
            moved_from: None,
        })
        .touched = true;
}

/// Records the end of `summary` in the commit's effects.
fn end(effects: &mut CommitEffects, summary: NodeSummary) {
    effects
        .entry(summary.key())
        .or_insert_with(|| EntityEffect {
            summary,
            touched: false,
            ended: false,
            moved_from: None,
        })
        .ended = true;
}

/// The Before-side entities whose `(type, name)` no longer exists on the
/// After side: their declarations were removed in this change.
fn removed_declarations<'a>(
    before: &'a [ExtractedNode],
    after: &[ExtractedNode],
) -> Vec<&'a ExtractedNode> {
    let kept: BTreeSet<(&str, &str)> = after
        .iter()
        .map(|n| (n.type_.as_str(), n.name.as_str()))
        .collect();
    before
        .iter()
        .filter(|n| !kept.contains(&(n.type_.as_str(), n.name.as_str())))
        .collect()
}

/// Per-commit shotness NDJSON records: only commits that touched an entity
/// emit a line; `data` is one `NodesTouched` map (key-sorted) of
/// `{Summary{Type,Name,File}, CountDelta: 1}`.
pub fn shotness_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<crate::handlers::history_formats::NdjsonRecord>> {
    use cf_gojson::value::{GoMap, GoValue};

    let walk = shotness_walk(sub)?;
    let mut records = Vec::new();
    for c in walk.iter() {
        if !c.touched_any() {
            continue;
        }
        let mut nodes = GoMap::new_map();
        for (key, ns) in touched_entities(&c.touched) {
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

/// The shotness contribution to the merged `--format timeseries` document:
/// per entity-touching commit `{"nodes_touched": n, "coupling_pairs": C(n,2)}`.
pub fn shotness_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<crate::handlers::history_formats::TimeSeriesContribution> {
    use cf_gojson::value::{GoMap, GoValue};

    let walk = shotness_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in walk.iter() {
        let n = touched_entities(&c.touched).count();
        if n == 0 {
            continue;
        }
        let mut entry = GoMap::new_map();
        entry.insert("nodes_touched".to_string(), GoValue::Int(n as i64));
        entry.insert(
            "coupling_pairs".to_string(),
            GoValue::Int(coupling_pair_count(n)),
        );
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

/// One extracted entity: its qualified name, type, and 1-based inclusive line
/// span (for the diff line → entity mapping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtractedNode {
    name: String,
    type_: String,
    start_line: usize,
    end_line: usize,
}

impl ExtractedNode {
    /// The entity's identity in `file`.
    fn summary(&self, file: &str) -> NodeSummary {
        NodeSummary::new(&self.type_, &self.name, file)
    }
}

/// A byte span `[start, end)` of a node with position information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: u64,
    end: u64,
}

impl Span {
    fn of(n: &Node) -> Option<Self> {
        n.pos.as_ref().map(|p| Self {
            start: p.start_offset,
            end: p.end_offset,
        })
    }

    /// Whether `self` covers `other`.
    fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

/// A named declaration that qualifies what is declared inside it.
struct Container {
    span: Span,
    type_: String,
    qualified: String,
}

/// Extracts the entities of a parsed file: the structural query selects the
/// nodes, the name query names each one, and the enclosing declarations (plus
/// a receiver type) qualify the name. A node without a resolvable name is not
/// an entity. Two selected nodes resolving to the same `(type, name)` are one
/// entity spanning the last one (deterministic over the query's pre-order).
fn extract_nodes(root: &Node, dsl: &CompiledDsl) -> Vec<ExtractedNode> {
    let containers = containers(root, &dsl.name);
    let mut named: BTreeMap<(String, String), ExtractedNode> = BTreeMap::new();
    for node in dsl.structure.run(root) {
        let Some(own) = own_name(&node, &dsl.name) else {
            continue;
        };
        let name = match Span::of(&node).and_then(|span| enclosing(&containers, span, &node)) {
            Some(prefix) => format!("{prefix}{QUALIFIER_SEPARATOR}{own}"),
            None => own,
        };
        let (start_line, end_line) = node_span(&node);
        let type_ = node.node_type.clone();
        named.insert(
            (type_.clone(), name.clone()),
            ExtractedNode {
                name,
                type_,
                start_line,
                end_line,
            },
        );
    }
    named.into_values().collect()
}

/// The named qualifying declarations of the tree in pre-order, each with its
/// own qualified name.
fn containers(root: &Node, name_query: &Query) -> Vec<Container> {
    let mut out: Vec<Container> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    root.visit_pre_order(|n| {
        let Some(span) = Span::of(n) else {
            return;
        };
        while open.last().is_some_and(|&i| !out[i].span.contains(span)) {
            open.pop();
        }
        if !QUALIFYING_TYPES.contains(&n.node_type.as_str()) {
            return;
        }
        let Some(own) = own_name(n, name_query) else {
            return;
        };
        let qualified = match open.last() {
            Some(&i) => format!("{}{QUALIFIER_SEPARATOR}{own}", out[i].qualified),
            None => own,
        };
        out.push(Container {
            span,
            type_: n.node_type.clone(),
            qualified,
        });
        open.push(out.len() - 1);
    });
    out
}

/// The qualified name of the deepest container enclosing `node` (a container
/// with the node's own span and type is the node itself).
fn enclosing(containers: &[Container], span: Span, node: &Node) -> Option<String> {
    containers
        .iter()
        .rev()
        .find(|c| c.span.contains(span) && !(c.span == span && c.type_ == node.node_type))
        .map(|c| c.qualified.clone())
}

/// A node's own name: the receiver type (when declared with one) and the
/// name query's first non-empty result; `None` when the query yields nothing.
fn own_name(node: &Node, name_query: &Query) -> Option<String> {
    let name = name_query
        .run(node)
        .into_iter()
        .map(|n| n.token)
        .find(|t| !t.is_empty())?;
    Some(match receiver_type(node) {
        Some(receiver) => format!("{receiver}{QUALIFIER_SEPARATOR}{name}"),
        None => name,
    })
}

/// The receiver type of a method declared as `func (a *A) M()`: a `Parameter`
/// child preceding the name identifier carries the receiver, and the first
/// type-role identifier inside it is its type. Methods declared inside their
/// type (`class A { m() }`) have no receiver parameter and are qualified by
/// the enclosing declaration instead.
fn receiver_type(node: &Node) -> Option<String> {
    let name_index = node
        .children
        .iter()
        .position(|c| c.node_type == UAST_IDENTIFIER && c.roles.iter().any(|r| r == ROLE_NAME))?;
    let receiver = node.children[..name_index]
        .iter()
        .find(|c| c.node_type == UAST_PARAMETER)?;
    receiver
        .find(|d| d.node_type == UAST_IDENTIFIER && d.roles.iter().any(|r| r == ROLE_TYPE))
        .first()
        .map(|d| d.token.clone())
        .filter(|t| !t.is_empty())
}

/// Resolves a node's 1-based inclusive line span: the explicit end line when
/// greater than the start, else the maximum descendant line.
fn node_span(n: &Node) -> (usize, usize) {
    let Some(pos) = &n.pos else { return (0, 0) };
    let start = pos.start_line as usize;
    if pos.end_line > pos.start_line {
        return (start, pos.end_line as usize);
    }
    let mut end = start;
    n.visit_pre_order(|child| {
        if let Some(cp) = &child.pos {
            let candidate = cp.end_line.max(cp.start_line) as usize;
            if candidate > end {
                end = candidate;
            }
        }
    });
    (start, end)
}

/// A line → entity-indices map; `line2node[l-1]` holds the indices (into
/// `nodes`) of the entities whose span covers line `l`.
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

/// Diff operation kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffOp {
    Delete,
    Insert,
    Equal,
}

/// One diff edit: its op and the number of lines it spans.
struct DiffEdit {
    op: DiffOp,
    line_count: usize,
}

/// Per-file line diff plus the old/new line counts.
struct FileDiff {
    old_lines_of_code: usize,
    new_lines_of_code: usize,
    edits: Vec<DiffEdit>,
}

/// Computes the line diff of a Modify change (same-hash / zero-hash / binary
/// changes are not diffed). Blob reads go through the change's parse scope so
/// the parse path and the diff path fetch each blob once.
fn file_diff(parses: &ChangeParses<'_>) -> Option<FileDiff> {
    use cf_godiff::{line_diff, Op};

    let change = parses.change();
    if change.action != ChangeAction::Modify {
        return None;
    }
    if change.from.hash == change.to.hash || change.from.hash.is_zero() || change.to.hash.is_zero()
    {
        return None;
    }
    let blob_from = parses.blob(Side::Before)?;
    let blob_to = parses.blob(Side::After)?;
    if is_binary(&blob_from.data) || is_binary(&blob_to.data) {
        return None;
    }

    let from = String::from_utf8_lossy(&blob_from.data).into_owned();
    let to = String::from_utf8_lossy(&blob_to.data).into_owned();

    // Identical-string fast path: one Equal run.
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

/// Counts lines (`\n` count, plus one for an unterminated last line).
fn count_lines(s: &str) -> usize {
    let mut count = s.bytes().filter(|&b| b == b'\n').count();
    if !s.is_empty() && s.as_bytes().last() != Some(&b'\n') {
        count += 1;
    }
    count
}

/// Binary sniff: a NUL byte in the first 8000 bytes.
fn is_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(8000)];
    head.contains(&0)
}

/// Walks the diff edits and records the touched entities: a Delete hunk
/// touches the Before-side entities spanning its lines, an Insert hunk the
/// After-side ones.
fn apply_diff_edits(effects: &mut CommitEffects, file: &str, d: &ShotnessModifyDetail) {
    let line2node_before = gen_line2node(&d.nodes_before, d.diff.old_lines_of_code);
    let line2node_after = gen_line2node(&d.nodes_after, d.diff.new_lines_of_code);

    let mut line_before: usize = 0;
    let mut line_after: usize = 0;
    for edit in &d.diff.edits {
        let size = edit.line_count;
        match edit.op {
            DiffOp::Delete => {
                record_touched(
                    effects,
                    &line2node_before,
                    &d.nodes_before,
                    line_before,
                    size,
                    file,
                );
                line_before += size;
            }
            DiffOp::Insert => {
                record_touched(
                    effects,
                    &line2node_after,
                    &d.nodes_after,
                    line_after,
                    size,
                    file,
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

/// Records the entities a hunk spanning `[start, start+size)` (0-based
/// lines) touches.
fn record_touched(
    effects: &mut CommitEffects,
    line2node: &[Vec<usize>],
    nodes: &[ExtractedNode],
    start: usize,
    size: usize,
    file: &str,
) {
    for indices in line2node.iter().skip(start).take(size) {
        for &idx in indices {
            touch(effects, nodes[idx].summary(file));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str, source: &str) -> Node {
        cf_uast::Parser::new()
            .parse(name, source.as_bytes())
            .expect("parse")
    }

    /// `(type, qualified name, start line, end line)` of every entity.
    fn extracted(
        name: &str,
        source: &str,
        dsl: &ShotnessDsl,
    ) -> Vec<(String, String, usize, usize)> {
        extract_nodes(&parse(name, source), &dsl.compile().expect("compile"))
            .into_iter()
            .map(|n| (n.type_, n.name, n.start_line, n.end_line))
            .collect()
    }

    fn row(type_: &str, name: &str, start: usize, end: usize) -> (String, String, usize, usize) {
        (type_.to_string(), name.to_string(), start, end)
    }

    #[test]
    fn the_default_dsl_compiles() {
        ShotnessDsl::default()
            .compile()
            .expect("the default shotness DSL compiles");
    }

    #[test]
    fn go_receivers_qualify_methods() {
        let src = "package r\n\ntype A struct{}\ntype B struct{}\n\nfunc (a *A) String() string { return \"a\" }\n\nfunc (B) String() string { return \"b\" }\n\nfunc plain() int { return 1 }\n";
        assert_eq!(
            extracted("r.go", src, &ShotnessDsl::default()),
            vec![
                row("Function", "plain", 10, 10),
                row("Method", "A.String", 6, 6),
                row("Method", "B.String", 8, 8),
            ]
        );
    }

    #[test]
    fn enclosing_declarations_qualify_nested_entities() {
        let src = "def outer():\n    def inner():\n        return 1\n    return inner\n\nclass K:\n    def km(self):\n        return 3\n    class Inner:\n        def im(self):\n            return 4\n";
        assert_eq!(
            extracted("n.py", src, &ShotnessDsl::default()),
            vec![
                row("Function", "K.Inner.im", 10, 11),
                row("Function", "K.km", 7, 8),
                row("Function", "outer", 1, 4),
                row("Function", "outer.inner", 2, 3),
            ]
        );
    }

    #[test]
    fn java_methods_are_qualified_by_their_class() {
        let src = "public class A {\n    public int f() {\n        return 1;\n    }\n    public int g() {\n        return 2;\n    }\n}\n";
        assert_eq!(
            extracted("A.java", src, &ShotnessDsl::default()),
            vec![row("Method", "A.f", 2, 4), row("Method", "A.g", 5, 7)]
        );
        let classes = ShotnessDsl {
            struct_query: r#"rfilter(.roles has "Class")"#.to_string(),
            ..ShotnessDsl::default()
        };
        assert_eq!(
            extracted("A.java", src, &classes),
            vec![row("Class", "A", 1, 8)]
        );
    }

    #[test]
    fn the_name_dsl_is_honored_and_unnamed_nodes_are_skipped() {
        let src =
            "package l\n\nfunc top() int {\n\tf := func() int { return 1 }\n\treturn f()\n}\n";
        assert_eq!(
            extracted("l.go", src, &ShotnessDsl::default()),
            vec![row("Function", "top", 3, 6)]
        );
        let by_type = ShotnessDsl {
            name_query: ".type".to_string(),
            ..ShotnessDsl::default()
        };
        assert_eq!(
            extracted("l.go", src, &by_type),
            vec![row("Function", "Function", 3, 6)]
        );
        let lambdas = ShotnessDsl {
            struct_query: r#"rfilter(.type == "Lambda")"#.to_string(),
            ..ShotnessDsl::default()
        };
        assert!(extracted("l.go", src, &lambdas).is_empty());
    }

    #[test]
    fn removed_declarations_are_the_before_names_missing_after() {
        let before = vec![
            ExtractedNode {
                name: "f".into(),
                type_: "Function".into(),
                start_line: 1,
                end_line: 2,
            },
            ExtractedNode {
                name: "g".into(),
                type_: "Function".into(),
                start_line: 3,
                end_line: 4,
            },
        ];
        let after = vec![ExtractedNode {
            name: "g".into(),
            type_: "Function".into(),
            start_line: 1,
            end_line: 2,
        }];
        let removed: Vec<&str> = removed_declarations(&before, &after)
            .into_iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(removed, vec!["f"]);
    }

    #[test]
    fn hunks_touch_the_entities_spanning_their_lines() {
        let nodes = vec![
            ExtractedNode {
                name: "f".into(),
                type_: "Function".into(),
                start_line: 1,
                end_line: 3,
            },
            ExtractedNode {
                name: "g".into(),
                type_: "Function".into(),
                start_line: 5,
                end_line: 7,
            },
        ];
        // Lines 1-3 equal, line 4 deleted, lines 5-7 kept, then two lines inserted into g.
        let detail = ShotnessModifyDetail {
            nodes_before: nodes.clone(),
            nodes_after: nodes,
            diff: FileDiff {
                old_lines_of_code: 7,
                new_lines_of_code: 8,
                edits: vec![
                    DiffEdit {
                        op: DiffOp::Equal,
                        line_count: 5,
                    },
                    DiffEdit {
                        op: DiffOp::Insert,
                        line_count: 1,
                    },
                    DiffEdit {
                        op: DiffOp::Equal,
                        line_count: 2,
                    },
                ],
            },
        };
        let mut effects = CommitEffects::new();
        apply_diff_edits(&mut effects, "m.go", &detail);
        let touched: Vec<&String> = touched_entities(&effects).map(|(k, _)| k).collect();
        assert_eq!(touched, vec!["Function_g_m.go"]);
    }

    #[test]
    fn dsl_flags_default_unless_the_user_sets_them() {
        let cmd = crate::flags::build_run_command();
        let defaults = cmd.clone().try_get_matches_from(["run"]).unwrap();
        assert_eq!(
            ShotnessDsl::from_matches(&defaults).unwrap(),
            ShotnessDsl::default()
        );
        let set = cmd
            .clone()
            .try_get_matches_from([
                "run",
                "--shotness-dsl-struct",
                r#"rfilter(.roles has "Class")"#,
                "--shotness-dsl-name",
                ".token",
            ])
            .unwrap();
        assert_eq!(
            ShotnessDsl::from_matches(&set).unwrap(),
            ShotnessDsl {
                struct_query: r#"rfilter(.roles has "Class")"#.to_string(),
                name_query: ".token".to_string(),
            }
        );
        let empty = cmd
            .clone()
            .try_get_matches_from(["run", "--shotness-dsl-struct", ""])
            .unwrap();
        assert_eq!(
            ShotnessDsl::from_matches(&empty).unwrap(),
            ShotnessDsl::default()
        );
        let bad = cmd
            .try_get_matches_from(["run", "--shotness-dsl-name", "map("])
            .unwrap();
        let err = validate_shotness_dsl(&bad).unwrap_err();
        assert!(err.to_string().contains("--shotness-dsl-name"), "{err}");
    }
}
