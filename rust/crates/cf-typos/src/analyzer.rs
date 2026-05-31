//! Typos history analyzer.
//!
//! Port of Go `internal/analyzers/typos/analyzer.go`. The analyzer scans each
//! commit's diffs for line pairs within a small Levenshtein distance (default
//! 4) and, where both the removed and added focused lines contain exactly one
//! UAST identifier, records a `(wrong -> correct)` typo-fix pair. Per-tick and
//! cross-tick the pairs are deduplicated by `"wrong|correct"` (first-seen
//! wins), and the final report exposes the deduplicated list under `"typos"`.
//!
//! ## Scope of this port
//!
//! The pure typo-extraction algorithm (diff scan, Levenshtein candidate
//! matching, identifier collection, dedup) is ported in full and unit-tested.
//! The `consume` glue that pulls per-commit UAST changes, blob cache, and file
//! diffs from the streaming plumbing pipeline (Go `t.UAST` / `t.BlobCache` /
//! `t.FileDiff`) depends on `cf-plumbing`'s change/diff types, which the
//! algorithm here is parameterized over via [`FileChange`] and [`DiffEdit`].
//! Once the plumbing pipeline is wired in, `consume` collects [`FileChange`]s
//! and calls [`detect_typos_in_change`].

use std::collections::{BTreeMap, HashMap};

use cf_alg_levenshtein::Context as LevenshteinContext;
use cf_analyze::context::Context;
use cf_analyze::history::{CommitContext, Descriptor, GoFact, HistoryAnalyzer, Mode};
use cf_analyze::report::Report;
use cf_analyze::tc::Tc;
use cf_analyze::tick::Tick;
use cf_gitlib::Hash;
use cf_gojson::GoValue;
use cf_uast_node::{Node, UAST_IDENTIFIER};

use crate::typos::{
    deduplicate_typos, TickData, Typo, CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE,
    DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE,
};

/// The typos analyzer ID (Go `history/typos`).
pub const ID: &str = "history/typos";

/// CLI flag for the maximum Levenshtein distance.
pub const FLAG_MAX_DISTANCE: &str = "typos-max-distance";

/// One diff edit, mirroring `diffmatchpatch.Diff`.
///
/// The Go analyzer consumes diffs produced upstream by
/// `github.com/sergi/go-diff/diffmatchpatch` (via the file-diff plumbing
/// analyzer). It only uses each edit's operation and the **rune count** of its
/// text (`utf8.RuneCountInString`), so this port carries exactly that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEdit {
    /// The edit operation.
    pub op: DiffOp,
    /// Number of runes (Unicode scalar values) in the edit text.
    pub rune_count: usize,
}

/// A diff edit operation (Go `diffmatchpatch.Operation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// Lines removed from the "before" side.
    Delete,
    /// Lines added on the "after" side.
    Insert,
    /// Lines unchanged.
    Equal,
}

/// A single file change within a commit, mirroring the inputs the Go `Consume`
/// pulls from the plumbing pipeline for one `uast.Change`.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// The "after" file name (Go `change.Change.To.Name`).
    pub file: String,
    /// The "before" UAST root (`None` if absent).
    pub before: Option<Node>,
    /// The "after" UAST root (`None` if absent).
    pub after: Option<Node>,
    /// "Before" blob split into lines (Go `bytes.Split(blob, '\n')`).
    pub lines_before: Vec<String>,
    /// "After" blob split into lines.
    pub lines_after: Vec<String>,
    /// The file's diff edits.
    pub diffs: Vec<DiffEdit>,
}

/// Typos history analyzer.
///
/// Port of Go `typos.Analyzer` (the per-commit and aggregation behavior; the
/// streaming-pipeline plumbing fields are supplied per call via [`FileChange`]).
#[derive(Debug, Default)]
pub struct Analyzer {
    descriptor: Option<Descriptor>,
    /// Maximum allowed Levenshtein distance (Go `MaximumAllowedDistance`).
    pub maximum_allowed_distance: i32,
    lcontext: LevenshteinContext,
}

impl Analyzer {
    /// Creates a new typos analyzer.
    ///
    /// Port of Go `NewAnalyzer`. The Levenshtein context is created eagerly so
    /// the analyzer is usable without a separate `Initialize` call; Go creates
    /// it in `Initialize`.
    pub fn new() -> Self {
        Analyzer {
            descriptor: Some(Descriptor {
                id: ID.to_string(),
                description: "Extracts typo-fix identifier pairs from source code in commit diffs."
                    .to_string(),
                mode: Mode::History,
            }),
            maximum_allowed_distance: 0,
            lcontext: LevenshteinContext::new(),
        }
    }

    /// Returns the analyzer name (last path segment of the ID).
    pub fn name(&self) -> &str {
        "typos"
    }

    /// Returns the CLI flag name.
    pub fn flag(&self) -> &str {
        FLAG_MAX_DISTANCE
    }

    /// Prepares the analyzer for processing commits.
    ///
    /// Port of Go `Initialize`: (re)creates the Levenshtein context and applies
    /// the default distance if unset.
    pub fn initialize(&mut self) {
        self.lcontext = LevenshteinContext::new();
        if self.maximum_allowed_distance <= 0 {
            self.maximum_allowed_distance = DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE;
        }
    }

    /// Whether the analyzer requires the UAST pipeline (Go `NeedsUAST`).
    pub fn needs_uast(&self) -> bool {
        true
    }

    /// Whether the analyzer is CPU-heavy (Go `CPUHeavy`).
    pub fn cpu_heavy(&self) -> bool {
        true
    }

    /// Whether the analyzer must run sequentially (Go `SequentialOnly`).
    pub fn sequential_only(&self) -> bool {
        false
    }

    /// Detects typo-fix pairs for one file change.
    ///
    /// Port of the per-`uast.Change` body of Go `Consume`: find candidate line
    /// pairs via the diff + Levenshtein scan, then match single-identifier line
    /// pairs into [`Typo`] records.
    pub fn detect_typos_in_change(&mut self, change: &FileChange, commit: Hash) -> Vec<Typo> {
        let result = self.find_typo_candidates(&change.diffs, &change.lines_before, &change.lines_after);
        if result.candidates.is_empty() {
            return Vec::new();
        }
        match_typo_identifiers(change, &result, commit)
    }

    /// Scans diff edits for before/after line pairs within the distance bound.
    ///
    /// Port of Go `findTypoCandidates`.
    fn find_typo_candidates(
        &mut self,
        diffs: &[DiffEdit],
        lines_before: &[String],
        lines_after: &[String],
    ) -> TypoCandidateResult {
        let mut line_num_before: i64 = 0;
        let mut line_num_after: i64 = 0;
        let mut removed_size: i64 = 0;
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut focused_lines_before: BTreeMap<i64, bool> = BTreeMap::new();
        let mut focused_lines_after: BTreeMap<i64, bool> = BTreeMap::new();

        for edit in diffs {
            let size = edit.rune_count as i64;
            match edit.op {
                DiffOp::Delete => {
                    line_num_before += size;
                    removed_size = size;
                }
                DiffOp::Insert => {
                    if size == removed_size {
                        self.match_delete_insert_pairs(
                            line_num_before,
                            line_num_after,
                            size,
                            lines_before,
                            lines_after,
                            &mut candidates,
                            &mut focused_lines_before,
                            &mut focused_lines_after,
                        );
                    }
                    line_num_after += size;
                    removed_size = 0;
                }
                DiffOp::Equal => {
                    line_num_before += size;
                    line_num_after += size;
                    removed_size = 0;
                }
            }
        }

        TypoCandidateResult {
            candidates,
            focused_lines_before,
            focused_lines_after,
        }
    }

    /// Checks each line pair in a delete/insert hunk for typo candidates.
    ///
    /// Port of Go `matchDeleteInsertPairs`.
    #[allow(clippy::too_many_arguments)]
    fn match_delete_insert_pairs(
        &mut self,
        line_num_before: i64,
        line_num_after: i64,
        size: i64,
        lines_before: &[String],
        lines_after: &[String],
        candidates: &mut Vec<Candidate>,
        focused_before: &mut BTreeMap<i64, bool>,
        focused_after: &mut BTreeMap<i64, bool>,
    ) {
        let max_dist = self.maximum_allowed_distance as i64;
        for i in 0..size {
            let lb = line_num_before - size + i;
            let la = line_num_after + i;

            if lb < 0 || la < 0 {
                continue;
            }
            let (lb_u, la_u) = (lb as usize, la as usize);
            if lb_u >= lines_before.len() || la_u >= lines_after.len() {
                continue;
            }

            // Go compares len() on []byte; do the same in bytes, not chars.
            let len_b = lines_before[lb_u].len() as i64;
            let len_a = lines_after[la_u].len() as i64;

            // Length difference alone exceeds threshold -> skip O(N*M) compute.
            if len_b - len_a > max_dist || len_a - len_b > max_dist {
                continue;
            }

            let dist = self.lcontext.distance(&lines_before[lb_u], &lines_after[la_u]) as i64;
            if dist <= max_dist {
                candidates.push(Candidate {
                    before: lb,
                    after: la,
                });
                focused_before.insert(lb, true);
                focused_after.insert(la, true);
            }
        }
    }
}

impl HistoryAnalyzer for Analyzer {
    fn descriptor(&self) -> &Descriptor {
        self.descriptor
            .as_ref()
            .expect("descriptor initialized in new()")
    }

    /// Applies configuration facts.
    ///
    /// Port of Go `Configure`: reads the max-distance int fact and falls back to
    /// the default when unset or non-positive.
    fn configure(&mut self, facts: &BTreeMap<String, GoFact>) -> anyhow::Result<()> {
        if let Some(GoFact::Int(v)) = facts.get(CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE) {
            self.maximum_allowed_distance = *v as i32;
        }
        if self.maximum_allowed_distance <= 0 {
            self.maximum_allowed_distance = DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE;
        }
        Ok(())
    }

    /// Processes a single commit.
    ///
    /// The streaming plumbing pipeline (UAST changes, blob cache, file diffs)
    /// that Go's `Consume` reads from is supplied through [`FileChange`] inputs
    /// rather than analyzer-held state; see [`Analyzer::detect_typos_in_change`]
    /// and [`Analyzer::consume_changes`]. This trait method has no per-call
    /// change source yet, so it emits no contribution, matching Go's behavior
    /// for a commit with no qualifying changes.
    fn consume(&mut self, _ctx: &Context, _ac: &CommitContext) -> anyhow::Result<Option<Tc>> {
        Ok(None)
    }

    /// Converts accumulated ticks into the final report.
    ///
    /// Port of Go `ticksToReport`: concatenate all per-tick typos, cross-tick
    /// deduplicate by `"wrong|correct"`, and expose under `"typos"`.
    fn ticks_to_report(&self, ticks: &[Tick]) -> Report {
        let mut all_typos: Vec<Typo> = Vec::new();
        for tick in ticks {
            if let Some(data) = &tick.data {
                if let Some(td) = data.downcast_ref::<TickData>() {
                    all_typos.extend(td.typos.iter().cloned());
                }
            }
        }

        let all_typos = deduplicate_typos(&all_typos);
        let mut report = Report::new();
        report.insert("typos".to_string(), typos_to_govalue(&all_typos));
        report
    }
}

impl Analyzer {
    /// Processes a commit's collected file changes into a [`Tc`].
    ///
    /// Port of the loop body of Go `Consume`: iterate qualifying changes, gather
    /// typos, and emit a contribution only when at least one typo was found
    /// (Go returns an empty `TC` otherwise).
    pub fn consume_changes(&mut self, tick: i64, commit: Hash, changes: &[FileChange]) -> Option<Tc> {
        let mut typos: Vec<Typo> = Vec::new();
        for change in changes {
            if change.before.is_none() || change.after.is_none() {
                continue;
            }
            typos.extend(self.detect_typos_in_change(change, commit));
        }

        if typos.is_empty() {
            return None;
        }

        Some(Tc {
            tick,
            commit_hash: commit,
            data: Box::new(TickData { typos }),
        })
    }
}

/// A candidate line pair (Go `candidate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Candidate {
    before: i64,
    after: i64,
}

/// Output of [`Analyzer::find_typo_candidates`] (Go `typoCandidateResult`).
struct TypoCandidateResult {
    candidates: Vec<Candidate>,
    focused_lines_before: BTreeMap<i64, bool>,
    focused_lines_after: BTreeMap<i64, bool>,
}

/// Matches single-identifier line pairs into [`Typo`] records.
///
/// Port of Go `matchTypoIdentifiers`: for each candidate, when the focused
/// before line has exactly one identifier and the focused after line has
/// exactly one, emit a typo pair.
fn match_typo_identifiers(change: &FileChange, result: &TypoCandidateResult, commit: Hash) -> Vec<Typo> {
    let removed = collect_identifiers_on_lines(change.before.as_ref(), &result.focused_lines_before);
    let added = collect_identifiers_on_lines(change.after.as_ref(), &result.focused_lines_after);

    let mut typos = Vec::new();
    for cand in &result.candidates {
        let nodes_before = removed.get(&cand.before);
        let nodes_after = added.get(&cand.after);
        if let (Some(nb), Some(na)) = (nodes_before, nodes_after) {
            if nb.len() == 1 && na.len() == 1 {
                typos.push(Typo {
                    wrong: nb[0].clone(),
                    correct: na[0].clone(),
                    commit,
                    file: change.file.clone(),
                    line: cand.after,
                });
            }
        }
    }
    typos
}

/// Collects identifier tokens grouped by 0-based start line, limited to focused
/// lines.
///
/// Port of Go `collectIdentifiersOnLines`. Go converts the node's 1-based
/// `Pos.StartLine` to 0-based (`StartLine - 1`) and keeps only lines present in
/// the focused set. Returns tokens (not nodes) since downstream only reads the
/// token.
fn collect_identifiers_on_lines(
    root: Option<&Node>,
    focused_lines: &BTreeMap<i64, bool>,
) -> HashMap<i64, Vec<String>> {
    let mut result: HashMap<i64, Vec<String>> = HashMap::new();
    let Some(root) = root else {
        return result;
    };

    root.visit_pre_order(&mut |n: &Node| {
        if n.node_type != UAST_IDENTIFIER {
            return;
        }
        let Some(pos) = n.pos.as_ref() else {
            return;
        };
        // StartLine is 1-based; Go subtracts 1 for the 0-based line key.
        let line = pos.start_line as i64 - 1;
        if *focused_lines.get(&line).unwrap_or(&false) {
            result.entry(line).or_default().push(n.token.clone());
        }
    });

    result
}

/// Encodes a slice of [`Typo`] as a `GoValue` array of struct-origin objects.
///
/// Each typo serializes with fields in declaration order: wrong, correct, file,
/// commit, line (matching the Go `Typo` struct field order, which is how
/// `json.Marshal` of `[]Typo` renders).
fn typos_to_govalue(typos: &[Typo]) -> GoValue {
    GoValue::Array(
        typos
            .iter()
            .map(|t| {
                GoValue::Struct(vec![
                    ("wrong".to_string(), GoValue::Str(t.wrong.clone())),
                    ("correct".to_string(), GoValue::Str(t.correct.clone())),
                    ("file".to_string(), GoValue::Str(t.file.clone())),
                    ("commit".to_string(), GoValue::Str(t.commit.string())),
                    ("line".to_string(), GoValue::Int(t.line)),
                ])
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Positions;

    fn ident(token: &str, line: u32) -> Node {
        let mut n = Node::new(UAST_IDENTIFIER);
        n.token = token.to_string();
        n.pos = Some(Positions {
            start_line: line,
            ..Default::default()
        });
        n
    }

    fn root_with(children: Vec<Node>) -> Node {
        let mut r = Node::new("File");
        r.children = children;
        r
    }

    #[test]
    fn new_analyzer_sets_descriptor() {
        let a = Analyzer::new();
        assert_eq!(a.descriptor().id, "history/typos");
        assert_eq!(a.descriptor().mode, Mode::History);
        assert_eq!(a.name(), "typos");
        assert_eq!(a.flag(), "typos-max-distance");
    }

    #[test]
    fn initialize_applies_default_distance() {
        let mut a = Analyzer::new();
        a.initialize();
        assert_eq!(a.maximum_allowed_distance, DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE);
    }

    #[test]
    fn configure_reads_int_fact() {
        let mut a = Analyzer::new();
        let mut facts = BTreeMap::new();
        facts.insert(
            CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE.to_string(),
            GoFact::Int(3),
        );
        a.configure(&facts).unwrap();
        assert_eq!(a.maximum_allowed_distance, 3);
    }

    #[test]
    fn configure_default_when_absent() {
        let mut a = Analyzer::new();
        a.configure(&BTreeMap::new()).unwrap();
        assert_eq!(a.maximum_allowed_distance, DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE);
    }

    #[test]
    fn configure_default_when_non_positive() {
        let mut a = Analyzer::new();
        let mut facts = BTreeMap::new();
        facts.insert(
            CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE.to_string(),
            GoFact::Int(0),
        );
        a.configure(&facts).unwrap();
        assert_eq!(a.maximum_allowed_distance, DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE);
    }

    #[test]
    fn flags_match_go() {
        let a = Analyzer::new();
        assert!(a.needs_uast());
        assert!(a.cpu_heavy());
        assert!(!a.sequential_only());
    }

    // A delete-then-insert hunk of one line each, within distance 4, with one
    // identifier on each focused line, yields exactly one typo.
    #[test]
    fn detects_single_line_typo_fix() {
        let mut a = Analyzer::new();
        a.initialize();

        let change = FileChange {
            file: "main.go".to_string(),
            before: Some(root_with(vec![ident("recieve", 1)])),
            after: Some(root_with(vec![ident("receive", 1)])),
            lines_before: vec!["recieve".to_string()],
            lines_after: vec!["receive".to_string()],
            diffs: vec![
                DiffEdit { op: DiffOp::Delete, rune_count: 1 },
                DiffEdit { op: DiffOp::Insert, rune_count: 1 },
            ],
        };

        let typos = a.detect_typos_in_change(&change, Hash::default());
        assert_eq!(typos.len(), 1);
        assert_eq!(typos[0].wrong, "recieve");
        assert_eq!(typos[0].correct, "receive");
        assert_eq!(typos[0].file, "main.go");
        assert_eq!(typos[0].line, 0); // 0-based after line
    }

    // Distance beyond the bound yields no candidates.
    #[test]
    fn rejects_distant_lines() {
        let mut a = Analyzer::new();
        a.maximum_allowed_distance = 1;
        a.initialize();
        a.maximum_allowed_distance = 1;

        let change = FileChange {
            file: "main.go".to_string(),
            before: Some(root_with(vec![ident("foo", 1)])),
            after: Some(root_with(vec![ident("xxxxxx", 1)])),
            lines_before: vec!["foo".to_string()],
            lines_after: vec!["xxxxxx".to_string()],
            diffs: vec![
                DiffEdit { op: DiffOp::Delete, rune_count: 1 },
                DiffEdit { op: DiffOp::Insert, rune_count: 1 },
            ],
        };

        let typos = a.detect_typos_in_change(&change, Hash::default());
        assert!(typos.is_empty());
    }

    // Multiple identifiers on a focused line disqualify the candidate.
    #[test]
    fn requires_exactly_one_identifier_per_side() {
        let mut a = Analyzer::new();
        a.initialize();

        let change = FileChange {
            file: "main.go".to_string(),
            before: Some(root_with(vec![ident("a", 1), ident("b", 1)])),
            after: Some(root_with(vec![ident("c", 1)])),
            lines_before: vec!["a b".to_string()],
            lines_after: vec!["a c".to_string()],
            diffs: vec![
                DiffEdit { op: DiffOp::Delete, rune_count: 1 },
                DiffEdit { op: DiffOp::Insert, rune_count: 1 },
            ],
        };

        let typos = a.detect_typos_in_change(&change, Hash::default());
        assert!(typos.is_empty());
    }

    #[test]
    fn consume_changes_emits_tc_when_typos_found() {
        let mut a = Analyzer::new();
        a.initialize();
        let change = FileChange {
            file: "main.go".to_string(),
            before: Some(root_with(vec![ident("recieve", 1)])),
            after: Some(root_with(vec![ident("receive", 1)])),
            lines_before: vec!["recieve".to_string()],
            lines_after: vec!["receive".to_string()],
            diffs: vec![
                DiffEdit { op: DiffOp::Delete, rune_count: 1 },
                DiffEdit { op: DiffOp::Insert, rune_count: 1 },
            ],
        };
        let tc = a.consume_changes(0, Hash::default(), &[change]);
        assert!(tc.is_some());
        let tc = tc.unwrap();
        let td = tc.data.downcast_ref::<TickData>().unwrap();
        assert_eq!(td.typos.len(), 1);
    }

    #[test]
    fn consume_changes_none_when_no_typos() {
        let mut a = Analyzer::new();
        a.initialize();
        let change = FileChange {
            file: "main.go".to_string(),
            before: None, // missing UAST -> skipped
            after: None,
            lines_before: vec![],
            lines_after: vec![],
            diffs: vec![],
        };
        assert!(a.consume_changes(0, Hash::default(), &[change]).is_none());
    }

    #[test]
    fn ticks_to_report_dedups_cross_tick() {
        let a = Analyzer::new();

        let mk_tick = |wrong: &str, correct: &str, file: &str| {
            let mut t = Tick::default();
            t.data = Some(Box::new(TickData {
                typos: vec![Typo {
                    wrong: wrong.to_string(),
                    correct: correct.to_string(),
                    file: file.to_string(),
                    commit: Hash::default(),
                    line: 0,
                }],
            }));
            t
        };

        let ticks = vec![
            mk_tick("recieve", "receive", "a.go"),
            mk_tick("recieve", "receive", "b.go"), // dup pair across ticks
            mk_tick("seperate", "separate", "c.go"),
        ];

        let report = a.ticks_to_report(&ticks);
        let typos = report.get("typos").expect("typos key");
        let GoValue::Array(arr) = typos else {
            panic!("expected array");
        };
        assert_eq!(arr.len(), 2); // recieve|receive deduped to one
    }

    #[test]
    fn ticks_to_report_empty() {
        let a = Analyzer::new();
        let report = a.ticks_to_report(&[]);
        let typos = report.get("typos").expect("typos key");
        assert_eq!(*typos, GoValue::Array(vec![]));
    }
}
