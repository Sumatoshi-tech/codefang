//! `history/typos` over the shared history walk.
//!
//! Per commit: the filtered tree diff; for every Modify change whose blobs
//! differ and are text, the libgit2 line-diff op stream feeds
//! [`cf_typos::candidate_line_pairs`] (equal-sized Delete/Insert runs whose
//! lines pass the Levenshtein prefilter), both UAST sides are parsed, and
//! [`cf_typos::identifier_pair`] turns the identifiers of each focused line
//! pair into the `(wrong → correct)` fix the commit made. The walk's
//! occurrences, in chronological order, are the report input
//! ([`cf_typos::ReportData`]); the crate derives the metrics and every
//! machine format encodes that ONE value (`typo_list` keeps the first fix of
//! each pair, `patterns` counts every occurrence, `line` is 1-based).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cf_alg_levenshtein::Context as LevenshteinContext;
use cf_gitlib::changes::ChangeAction;
use cf_gitlib::diff::LineOp;
use cf_typos::{DiffOp, Hash as TypoHash, ReportData, Typo};

use super::super::uast_walk::{uast_results, ChangeParses, ParseOutcome, Side, UastAnalyzer};
use super::CommitStamp;

/// The analyzer id.
const ID: &str = "history/typos";
/// The yaml section name.
const NAME: &str = "history/typos";

/// `history/typos --format json` bytes (compact, no trailing newline).
#[must_use]
pub fn typos_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    typos_report_bytes(sub, crate::formats::FORMAT_JSON)
}

/// `history/typos --format yaml` bytes: the `codefang (v2)` header, the
/// `history/typos:` section name and the report value.
#[must_use]
pub fn typos_run_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    typos_report_bytes(sub, crate::formats::FORMAT_YAML)
}

/// `history/typos --format bin` bytes: the CFB1 envelope over the json
/// value.
#[must_use]
pub fn typos_run_report_bin(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    typos_report_bytes(sub, crate::formats::FORMAT_BIN_ALIAS)
}

/// The one report value in `format`, or `None` when the repository cannot be
/// walked.
fn typos_report_bytes(sub: &clap::ArgMatches, format: &str) -> Option<Vec<u8>> {
    let report = typos_report_data(sub)?;
    let value = cf_typos::metrics_report_value(&report);
    super::super::serialize_history_metrics(format, ID, NAME, &value, &value).ok()
}

/// Builds the `history/typos` [`cf_typos::ReportData`]: every typo-fix
/// occurrence of the walk, chronological (the single report input every
/// output format derives its metrics from). Returns `None` when the
/// repository cannot be walked.
#[must_use]
pub fn typos_report_data(sub: &clap::ArgMatches) -> Option<ReportData> {
    let walk = typos_walk(sub)?;
    Some(ReportData {
        typos: walk.iter().flat_map(|c| c.typos.iter().cloned()).collect(),
    })
}

/// One walked commit's typos products plus the shared stamps. One entry per
/// walked commit, in walk order.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TyposCommit {
    /// Position in the ORIGINAL window (oversized-dropped commits count).
    pub pos: usize,
    /// This commit's detected typos, in detection order.
    pub typos: Vec<Typo>,
    /// Tick.
    pub tick: i64,
    /// Author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

impl TyposCommit {
    /// The record of one kept commit: its stamps and its typo fixes.
    pub(crate) fn new(stamp: &CommitStamp, typos: Vec<Typo>) -> Self {
        Self {
            pos: stamp.pos,
            typos,
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        }
    }
}

/// The `history/typos` records of the run's UAST pass: every typos format
/// consumes THIS walk.
pub(crate) fn typos_walk(sub: &clap::ArgMatches) -> Option<Arc<Vec<TyposCommit>>> {
    uast_results(sub, UastAnalyzer::Typos)?.typos
}

/// The effective `--typos-max-distance`: the flag when set (the preflight
/// rejects a non-positive value), else the crate default.
pub(crate) fn typos_max_distance(sub: &clap::ArgMatches) -> i64 {
    let v = sub
        .try_get_one::<i64>("typos-max-distance")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(0);
    if v <= 0 {
        cf_typos::DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE as i64
    } else {
        v
    }
}

/// The per-commit `history/typos` context: the commit every fix names, the
/// distance cap, and the Levenshtein scratch (pure scratch: results are
/// state-independent, so one context per commit equals one per walk).
pub(crate) struct TyposContext {
    commit: TypoHash,
    max_distance: usize,
    lctx: LevenshteinContext,
}

impl TyposContext {
    /// The context of commit `hash` under `--typos-max-distance`.
    pub(crate) fn new(hash: cf_gitlib::Hash, max_distance: i64) -> Self {
        Self {
            commit: TypoHash(hash.0),
            max_distance: usize::try_from(max_distance)
                .unwrap_or(cf_typos::DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE),
            lctx: LevenshteinContext::new(),
        }
    }
}

/// One change's `history/typos` contribution: the typo fixes a Modify change
/// made, in detection order, appended to the commit's fixes (so they follow
/// its change order). Only a Modify whose two text blobs differ and whose
/// two sides parse can fix a typo.
pub(crate) fn typos_change(
    parses: &ChangeParses<'_>,
    context: &mut TyposContext,
    typos: &mut Vec<Typo>,
) {
    let change = parses.change();
    if !matches!(change.action, ChangeAction::Modify) || change.from.hash == change.to.hash {
        return;
    }
    let Some(blob_before) = parses.blob(Side::Before) else {
        return;
    };
    let Some(blob_after) = parses.blob(Side::After) else {
        return;
    };
    if blob_before.is_binary() || blob_after.is_binary() || blob_before.data == blob_after.data {
        return;
    }
    let ParseOutcome::Parsed(before) = parses.parse(Side::Before) else {
        return;
    };
    let ParseOutcome::Parsed(after) = parses.parse(Side::After) else {
        return;
    };
    let old_lines = blob_before.count_lines().map_or(0, |n| n as i64);
    let ops: Vec<DiffOp> = cf_gitlib::diff::diff_blob_line_ops(
        parses.repo().native(),
        change.from.hash,
        change.to.hash,
        old_lines,
    )
    .unwrap_or_default()
    .iter()
    .map(diff_op)
    .collect();
    let sides = ModifySides {
        file: &change.to.name,
        before,
        after,
        lines_before: split_lines(&blob_before.data),
        lines_after: split_lines(&blob_after.data),
    };
    push_line_pair_fixes(&sides, &ops, context, typos);
}

/// The two parsed sides of one Modify change.
struct ModifySides<'a> {
    /// The After path (the file a fix is reported in).
    file: &'a str,
    /// The Before UAST.
    before: &'a cf_uast_node::Node,
    /// The After UAST.
    after: &'a cf_uast_node::Node,
    /// The Before blob's raw (unstripped) lines; candidate line indices index
    /// into them.
    lines_before: Vec<&'a [u8]>,
    /// The After blob's raw lines.
    lines_after: Vec<&'a [u8]>,
}

/// Appends the fixes of one Modify change: the equal-sized Delete/Insert line
/// pairs of `ops` that pass the Levenshtein prefilter, each turned into the
/// `(wrong → correct)` identifier fix its two lines make.
fn push_line_pair_fixes(
    sides: &ModifySides<'_>,
    ops: &[DiffOp],
    context: &mut TyposContext,
    typos: &mut Vec<Typo>,
) {
    let pairs = cf_typos::candidate_line_pairs(
        ops,
        &sides.lines_before,
        &sides.lines_after,
        context.max_distance,
        &mut context.lctx,
    );
    if pairs.is_empty() {
        return;
    }
    let focused_before: HashSet<usize> = pairs.iter().map(|p| p.before).collect();
    let focused_after: HashSet<usize> = pairs.iter().map(|p| p.after).collect();
    let removed = collect_identifiers_on_lines(sides.before, &focused_before);
    let added = collect_identifiers_on_lines(sides.after, &focused_after);

    for pair in &pairs {
        let (Some(nb), Some(na)) = (removed.get(&pair.before), added.get(&pair.after)) else {
            continue;
        };
        if let Some(fix) =
            cf_typos::identifier_pair(nb, na, context.max_distance, &mut context.lctx)
        {
            typos.push(Typo {
                wrong: fix.wrong,
                correct: fix.correct,
                file: sides.file.to_string(),
                commit: context.commit,
                line: pair.after as i64 + 1,
            });
        }
    }
}

/// Maps a libgit2 line-diff op onto the crate's diff op.
fn diff_op(op: &LineOp) -> DiffOp {
    let count = |n: i64| usize::try_from(n).unwrap_or(0);
    match *op {
        LineOp::Equal(n) => DiffOp::Equal(count(n)),
        LineOp::Insert(n) => DiffOp::Insert(count(n)),
        LineOp::Delete(n) => DiffOp::Delete(count(n)),
    }
}

/// Per-commit typos NDJSON records: only commits with detected typos emit a
/// line; `data` is the typo list (each `{Wrong, Correct, File, Commit, Line}`
/// with `Commit` as 20 bytes and `Line` 1-based).
#[must_use]
pub fn typos_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = typos_walk(sub)?;
    let mut records = Vec::new();
    for c in walk.iter() {
        if c.typos.is_empty() {
            continue;
        }
        let items: Vec<GoValue> = c
            .typos
            .iter()
            .map(|t| {
                let mut m = GoMap::new_struct();
                m.insert("Wrong".to_string(), GoValue::Str(t.wrong.clone()));
                m.insert("Correct".to_string(), GoValue::Str(t.correct.clone()));
                m.insert("File".to_string(), GoValue::Str(t.file.clone()));
                m.insert(
                    "Commit".to_string(),
                    GoValue::Array(
                        t.commit
                            .0
                            .iter()
                            .map(|b| GoValue::Int(i64::from(*b)))
                            .collect(),
                    ),
                );
                m.insert("Line".to_string(), GoValue::Int(t.line));
                GoValue::Object(m)
            })
            .collect();
        records.push(super::super::history_formats::NdjsonRecord {
            pos: c.pos,
            hash: c.typos[0].commit.string(),
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: GoValue::Array(items),
        });
    }
    Some(records)
}

/// Splits on `\n`, dropping the newline; a trailing newline yields a final
/// empty element.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n').collect()
}

/// Groups the identifier tokens of `root` by their 0-based start line,
/// keeping only the focused lines.
fn collect_identifiers_on_lines(
    root: &cf_uast_node::Node,
    focused: &HashSet<usize>,
) -> HashMap<usize, Vec<String>> {
    use cf_uast_node::UAST_IDENTIFIER;
    let mut result: HashMap<usize, Vec<String>> = HashMap::new();
    root.visit_pre_order(|n| {
        if n.node_type != UAST_IDENTIFIER {
            return;
        }
        let Some(line) = n
            .pos
            .as_ref()
            .and_then(|pos| (pos.start_line as usize).checked_sub(1))
        else {
            return;
        };
        if focused.contains(&line) {
            result.entry(line).or_default().push(n.token.clone());
        }
    });
    result
}
