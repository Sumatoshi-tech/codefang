//! `history/sentiment` over the shared history walk.
//!
//! Per commit: the filtered tree diff, then for every Insert/Modify change a
//! gated UAST parse of BOTH sides, `Comment` node collection and
//! `merge_comments` (group by line, merge adjacent, strip delimiters, drop
//! short / license / low-letter comments) per side, and the multiset
//! difference After − Before ([`cf_sentiment::new_or_changed`]): only the
//! comments the commit introduced or edited are its product. The shared
//! identity/tick stamps bucket them per tick for
//! `cf_sentiment::compute_all_metrics`, which scores only the ticks that
//! carry at least one comment.

use std::collections::BTreeMap;
use std::sync::Arc;

use cf_gitlib::changes::ChangeAction;
use cf_sentiment::analyzer::{merge_comments, CommentNode};
use cf_sentiment::{
    compute_all_metrics, new_or_changed, ReportData, TickBounds,
    DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH,
};
use cf_uast_node::UAST_COMMENT;

use super::super::uast_walk::{uast_results, ChangeParses, ParseOutcome, Side, UastAnalyzer};
use super::CommitStamp;
use crate::pipeline::HandlerError;

/// The `--min-comment-len` flag name.
const MIN_COMMENT_LEN_FLAG: &str = "min-comment-len";

/// Reads `--min-comment-len`; absent ⇒ the default.
fn min_comment_len_raw(sub: &clap::ArgMatches) -> i64 {
    sub.try_get_one::<i64>(MIN_COMMENT_LEN_FLAG)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH)
}

/// Validates `--min-comment-len` (`>= 0`; `0` disables the length filter).
///
/// # Errors
///
/// [`HandlerError::InvalidFlag`] for a negative value.
pub(crate) fn validate_sentiment_flags(sub: &clap::ArgMatches) -> Result<(), HandlerError> {
    let value = min_comment_len_raw(sub);
    if value < 0 {
        return Err(HandlerError::InvalidFlag(format!(
            "invalid --{MIN_COMMENT_LEN_FLAG}: must be >= 0, got {value}"
        )));
    }
    Ok(())
}

/// The effective minimum comment byte length: `--min-comment-len`, with the
/// default standing in for a value the preflight rejects.
pub(crate) fn sentiment_min_comment_len(sub: &clap::ArgMatches) -> i64 {
    let value = min_comment_len_raw(sub);
    if value < 0 {
        DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH
    } else {
        value
    }
}

/// Computes the typed sentiment [`cf_sentiment::ComputedMetrics`] — the single
/// report value behind every output format — or `None` if the repository
/// cannot be walked.
#[must_use]
pub fn sentiment_metrics(sub: &clap::ArgMatches) -> Option<cf_sentiment::ComputedMetrics> {
    let walk = sentiment_walk(sub)?;

    let mut comments_by_commit: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for c in walk.iter() {
        commits_by_tick
            .entry(c.tick)
            .or_default()
            .push(c.hash.clone());
        tick_when
            .entry(c.tick)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(c.when);
                *hi = (*hi).max(c.when);
            })
            .or_insert((c.when, c.when));
        comments_by_commit.insert(c.hash.clone(), c.comments.clone());
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

    let input = ReportData::from_commit_data(&comments_by_commit, commits_by_tick, tick_bounds);
    Some(compute_all_metrics(&input))
}

/// One walked commit's sentiment products plus the shared stamps.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SentimentCommit {
    /// Position in the ORIGINAL window (oversized-dropped commits count).
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// The merged+filtered comments this commit introduced or changed
    /// (possibly empty).
    pub comments: Vec<String>,
    /// Tick.
    pub tick: i64,
    /// Author id.
    pub author_id: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

impl SentimentCommit {
    /// The record of one kept commit: its stamps and its new or changed
    /// comments.
    pub(crate) fn new(stamp: &CommitStamp, comments: Vec<String>) -> Self {
        Self {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            comments,
            tick: stamp.tick,
            author_id: stamp.author_id,
            when: stamp.when,
            offset_min: stamp.offset_min,
        }
    }
}

/// The `history/sentiment` records of the run's UAST pass: every sentiment
/// format consumes THIS one walk.
pub(crate) fn sentiment_walk(sub: &clap::ArgMatches) -> Option<Arc<Vec<SentimentCommit>>> {
    uast_results(sub, UastAnalyzer::Sentiment)?.sentiment
}

/// One change's `history/sentiment` contribution: for a surviving
/// Insert/Modify change, the merged+filtered comments of the After side that
/// are NEW or CHANGED relative to the Before side (an Insert has no Before
/// side), appended to the commit's comments (so they follow its change
/// order).
pub(crate) fn sentiment_change(
    parses: &ChangeParses<'_>,
    min_len: i64,
    comments: &mut Vec<String>,
) {
    let change = parses.change();
    if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
        return;
    }
    let after = side_comments(parses, Side::After, min_len);
    if after.is_empty() {
        return;
    }
    let before = side_comments(parses, Side::Before, min_len);
    comments.extend(new_or_changed(&after, &before));
}

/// The merged+filtered comments of one change side (empty for the missing
/// side of an Insert, a gate-skipped blob, or a failed non-shell parse).
fn side_comments(parses: &ChangeParses<'_>, side: Side, min_len: i64) -> Vec<String> {
    let entry = parses.entry(side);
    if entry.hash.is_zero() {
        return Vec::new();
    }
    let mut comment_nodes: Vec<CommentNode> = Vec::new();
    match parses.parse(side) {
        ParseOutcome::Parsed(root) => {
            collect_comment_nodes(root, UAST_COMMENT, &mut comment_nodes);
        }
        // Shell scripts register for a grammar the UAST loader does not
        // vendor, so they pass `is_supported` but fail to parse; their
        // `#`-comments are extracted line-wise instead (the failed parse
        // read the blob).
        ParseOutcome::Failed if is_shell_path(&entry.name) => {
            if let Some(blob) = parses.blob(side) {
                extract_shell_comment_nodes(&blob.data, &mut comment_nodes);
            }
        }
        ParseOutcome::Failed | ParseOutcome::Skipped => {}
    }
    merge_comments(&comment_nodes, min_len)
}

/// Per-commit sentiment NDJSON records: EVERY commit emits a line whose `data`
/// is a `Comments` array (possibly empty, never null).
#[must_use]
pub fn sentiment_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = sentiment_walk(sub)?;
    Some(
        walk.iter()
            .map(|c| {
                let comments: Vec<GoValue> =
                    c.comments.iter().map(|s| GoValue::Str(s.clone())).collect();
                let mut data = GoMap::new_struct();
                data.insert("Comments".to_string(), GoValue::Array(comments));
                super::super::history_formats::NdjsonRecord {
                    pos: c.pos,
                    hash: c.hash.clone(),
                    tick: c.tick,
                    author_id: c.author_id,
                    time_secs: c.when,
                    tz_offset_min: c.offset_min,
                    data: GoValue::Object(data),
                }
            })
            .collect(),
    )
}

/// The sentiment contribution to the merged `--format timeseries` document:
/// per commit `{"comment_count": n, "sentiment"?: f32}` (`sentiment` only when
/// the commit introduced or changed a comment).
#[must_use]
pub fn sentiment_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};
    use cf_sentiment::model::f32_float;

    let walk = sentiment_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in walk.iter() {
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
        let mut entry = GoMap::new_map();
        entry.insert(
            "comment_count".to_string(),
            GoValue::Int(c.comments.len() as i64),
        );
        if !c.comments.is_empty() {
            entry.insert(
                "sentiment".to_string(),
                f32_float(cf_sentiment::scorer::compute_sentiment(&c.comments)),
            );
        }
        per_commit.push((c.hash.clone(), GoValue::Object(entry)));
    }
    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "sentiment",
        per_commit,
        commit_meta,
    })
}

/// Recursively collects UAST nodes whose type is `Comment` into `out`
/// (preorder: the node itself before its children).
fn collect_comment_nodes(
    node: &cf_uast_node::Node,
    comment_type: &str,
    out: &mut Vec<CommentNode>,
) {
    if node.node_type == comment_type {
        let (start_line, end_line) = match &node.pos {
            Some(p) => (p.start_line as i64, p.end_line as i64),
            // Nodes without a position cannot be grouped by line.
            None => (-1, -1),
        };
        if start_line >= 0 {
            out.push(CommentNode {
                start_line,
                end_line,
                token: node.token.clone(),
            });
        }
    }
    for child in &node.children {
        collect_comment_nodes(child, comment_type, out);
    }
}

/// Whether `name` is a shell-script path handled by the bash-comment fallback.
fn is_shell_path(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
        && name.contains('.')
}

/// Extracts `#`-comment nodes from shell-script `content`: one node per
/// `#`-introduced line (including shebangs), spanning the `#` to end-of-line,
/// with `start_line == end_line` (1-based).
fn extract_shell_comment_nodes(content: &[u8], out: &mut Vec<CommentNode>) {
    let text = String::from_utf8_lossy(content);
    for (idx, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let lineno = (idx + 1) as i64;
        out.push(CommentNode {
            start_line: lineno,
            end_line: lineno,
            token: trimmed.to_string(),
        });
    }
}
