//! `history/quality` over the shared history walk.
//!
//! Per commit: the filtered tree diff, then for every Insert/Modify change's
//! After blob a gated UAST parse ([`super::super::uast_walk::ChangeParses`])
//! and one [`cf_quality::analyze_file`] sample per PARSED file (the four
//! component analyzers over the UAST and the blob bytes — the static
//! analyzers' numbers). A failed parse contributes nothing. The shared
//! identity/tick stamps bucket the samples per tick for
//! `cf_quality::compute_all_metrics`.

use std::collections::BTreeMap;
use std::sync::Arc;

use cf_gitlib::changes::ChangeAction;
use cf_quality::{compute_all_metrics, ReportData, TickBounds, TickQuality};

use super::super::uast_walk::{uast_results, ChangeParses, ParseOutcome, Side, UastAnalyzer};
use super::CommitStamp;

/// Runs the `history/quality` walk and returns the typed
/// [`cf_quality::ComputedMetrics`] — the single report value behind every
/// output format — or `None` if the repository cannot be walked.
#[must_use]
pub fn quality_metrics(sub: &clap::ArgMatches) -> Option<cf_quality::ComputedMetrics> {
    let walk = quality_walk(sub)?;

    // Per-tick merged quality + bounds (committer-time min/max), folding each
    // commit's per-file samples into its tick in walk order.
    let mut tick_quality: BTreeMap<i64, TickQuality> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for c in walk.iter() {
        tick_when
            .entry(c.tick)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(c.when);
                *hi = (*hi).max(c.when);
            })
            .or_insert((c.when, c.when));
        tick_quality.entry(c.tick).or_default().merge(&c.tq);
    }

    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    let input = ReportData {
        tick_quality,
        tick_bounds,
    };
    Some(compute_all_metrics(&input))
}

/// One walked commit's quality products plus the shared stamps.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct QualityCommit {
    /// Position in the ORIGINAL window (oversized-dropped commits count).
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// This commit's per-file quality samples.
    pub tq: TickQuality,
    /// Tick.
    pub tick: i64,
    /// Author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

impl QualityCommit {
    /// The record of one kept commit: its stamps and its per-file samples.
    pub(crate) fn new(stamp: &CommitStamp, tq: TickQuality) -> Self {
        Self {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            tq,
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        }
    }
}

/// The `history/quality` records of the run's UAST pass: per-commit
/// `TickQuality` plus the stamps, in walk order. Every quality format
/// consumes THIS one walk.
pub(crate) fn quality_walk(sub: &clap::ArgMatches) -> Option<Arc<Vec<QualityCommit>>> {
    uast_results(sub, UastAnalyzer::Quality)?.quality
}

/// One change's `history/quality` contribution: a [`cf_quality::FileSample`]
/// of the PARSED After tree, pushed onto the commit's samples (so a commit's
/// samples follow its change order). A deleted, gate-skipped or unparsable
/// file contributes nothing: `files_analyzed` counts parsed files only.
pub(crate) fn quality_change(parses: &ChangeParses<'_>, tq: &mut TickQuality) {
    let change = parses.change();
    if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
        return;
    }
    let ParseOutcome::Parsed(root) = parses.parse(Side::After) else {
        return;
    };
    // The blob was read by the parse that just succeeded.
    let Some(blob) = parses.blob(Side::After) else {
        return;
    };
    tq.push(&cf_quality::analyze_file(root, &blob.data));
}

/// Serializes one per-commit [`TickQuality`] as the per-commit record struct
/// (declaration field order, empty slices → null).
fn quality_tq_value(tq: &TickQuality) -> cf_gojson::GoValue {
    use cf_gojson::{GoMap, GoValue};
    fn floats(v: &[f64]) -> GoValue {
        if v.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(v.iter().map(|f| GoValue::Float(*f)).collect())
        }
    }
    fn ints(v: &[i64]) -> GoValue {
        if v.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(v.iter().map(|i| GoValue::Int(*i)).collect())
        }
    }
    let mut m = GoMap::new_struct();
    m.insert("Complexities".to_string(), floats(&tq.complexities));
    m.insert("Cognitives".to_string(), floats(&tq.cognitives));
    m.insert("MaxComplexities".to_string(), ints(&tq.max_complexities));
    m.insert("Functions".to_string(), ints(&tq.functions));
    m.insert("HalsteadVolumes".to_string(), floats(&tq.halstead_volumes));
    m.insert("HalsteadEfforts".to_string(), floats(&tq.halstead_efforts));
    m.insert("DeliveredBugs".to_string(), floats(&tq.delivered_bugs));
    m.insert("CommentScores".to_string(), floats(&tq.comment_scores));
    m.insert("DocCoverages".to_string(), floats(&tq.doc_coverages));
    m.insert("CohesionScores".to_string(), floats(&tq.cohesion_scores));
    GoValue::Object(m)
}

/// Per-commit quality NDJSON records: every commit emits a line.
#[must_use]
pub fn quality_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    let walk = quality_walk(sub)?;
    Some(
        walk.iter()
            .map(|c| super::super::history_formats::NdjsonRecord {
                pos: c.pos,
                hash: c.hash.clone(),
                tick: c.tick,
                author_id: c.author_id,
                time_secs: c.when,
                tz_offset_min: c.offset_min,
                data: quality_tq_value(&c.tq),
            })
            .collect(),
    )
}

/// The quality contribution to the merged `--format timeseries` document: per
/// commit the summary map (`cf_quality::commit_summary`).
#[must_use]
pub fn quality_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    let walk = quality_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in walk.iter() {
        let summary = cf_quality::commit_summary(&c.tq);
        per_commit.push((
            c.hash.clone(),
            cf_quality::serialize::commit_summary_value(&summary),
        ));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "quality",
        per_commit,
        commit_meta,
    })
}
