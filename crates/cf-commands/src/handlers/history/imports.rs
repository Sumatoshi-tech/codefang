//! `history/imports` over the shared history walk.
//!
//! Per commit: the filtered tree diff, then for every Insert/Modify change's
//! After blob a gated UAST parse and [`cf_imports::extract_imports`] with
//! the parser's language for the file and the blob bytes (Kotlin reads its
//! import statements back from the source). The shared identity/tick stamps
//! key the 4-level `author → language → import → tick → count` map; the
//! [`cf_imports::HistoryReport`] (tick size, author index, the map keyed by
//! developer name) is the ONE value json/yaml/bin encode.

use std::collections::BTreeMap;
use std::sync::Arc;

use cf_analyzers_plumbing::author_missing_id;
use cf_gitlib::changes::ChangeAction;
use cf_imports::history::{add_entries_to_map, aggregate_import_counts, ImportEntry, ImportsMap};
use cf_imports::HistoryReport;

use super::super::uast_walk::{uast_results, ChangeParses, ParseOutcome, Side, UastAnalyzer};
use super::{CommitStamp, StampSummary};

/// Computes the `history/imports` [`cf_imports::HistoryReport`] over the
/// commit stream (the format-independent report value).
#[must_use]
pub fn imports_run_metrics(sub: &clap::ArgMatches) -> Option<HistoryReport> {
    let walk = imports_walk(sub)?;
    let unmatched = walk.summary.unmatched_slot as i64;
    let mut imports = ImportsMap::new();
    for c in &walk.commits {
        let author = if c.author_id == author_missing_id() {
            unmatched
        } else {
            c.author_id
        };
        add_entries_to_map(&mut imports, &c.entries, author, c.tick);
    }
    Some(HistoryReport {
        tick_size_secs: walk.tick_size_secs,
        author_index: walk.summary.names.clone(),
        imports,
    })
}

/// Aggregated per-import usage counts over the same report as
/// [`imports_run_metrics`]: total count per import path across every
/// developer, language and tick (the plot's input).
#[must_use]
pub fn imports_run_usage_counts(sub: &clap::ArgMatches) -> Option<BTreeMap<String, i64>> {
    let report = imports_run_metrics(sub)?;
    Some(aggregate_import_counts(&report.imports))
}

/// One walked commit's imports products plus the shared stamps.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImportsCommit {
    /// Position in the ORIGINAL window (oversized-dropped commits count).
    pub pos: usize,
    /// Full hex hash.
    pub hash: String,
    /// Extracted `(lang, import)` entries (empty ⇒ no ndjson line, no
    /// timeseries entry).
    pub entries: Vec<ImportEntry>,
    /// Author id.
    pub author_id: i64,
    /// Tick.
    pub tick: i64,
    /// Committer time, Unix seconds.
    pub when: i64,
    /// Committer UTC-offset minutes.
    pub offset_min: i32,
}

impl ImportsCommit {
    /// The record of one kept commit: its stamps and its import entries.
    pub(crate) fn new(stamp: &CommitStamp, entries: Vec<ImportEntry>) -> Self {
        Self {
            pos: stamp.pos,
            hash: stamp.hex.clone(),
            entries,
            author_id: stamp.author_id,
            tick: stamp.tick,
            when: stamp.when,
            offset_min: stamp.offset_min,
        }
    }
}

/// The `history/imports` walk: the per-commit records plus the identity
/// state that names the authors and the configured tick size.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImportsWalk {
    /// One entry per walked commit, oldest first.
    pub commits: Vec<ImportsCommit>,
    /// The identity state the commits were stamped with.
    pub summary: StampSummary,
    /// `--tick-size` in seconds.
    pub tick_size_secs: i64,
}

/// The `history/imports` walk of the run's UAST pass: all imports formats
/// consume THIS one walk.
pub(crate) fn imports_walk(sub: &clap::ArgMatches) -> Option<Arc<ImportsWalk>> {
    uast_results(sub, UastAnalyzer::Imports)?.imports
}

/// One change's `history/imports` contribution: the `(lang, import)` entries
/// extracted from a surviving After tree, each tagged with the parser's
/// language for the file, appended to the commit's entries (so they follow
/// its change order).
pub(crate) fn imports_change(parses: &ChangeParses<'_>, entries: &mut Vec<ImportEntry>) {
    let change = parses.change();
    if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
        return;
    }
    let ParseOutcome::Parsed(root) = parses.parse(Side::After) else {
        return;
    };
    let Some(blob) = parses.blob(Side::After) else {
        return;
    };
    let lang = parses.parser().get_language(&change.to.name);
    for imp in cf_imports::extract_imports(root, &lang, &blob.data) {
        entries.push(ImportEntry {
            lang: lang.clone(),
            import: imp.path,
        });
    }
}

/// Per-commit imports NDJSON records: `data` is `{"authorID", "entries":
/// [{Lang, Import}]}`; commits with no entries emit no line.
#[must_use]
pub fn imports_ndjson_records(
    sub: &clap::ArgMatches,
) -> Option<Vec<super::super::history_formats::NdjsonRecord>> {
    use cf_gojson::{GoMap, GoValue};

    let walk = imports_walk(sub)?;
    let mut records = Vec::new();
    for c in &walk.commits {
        if c.entries.is_empty() {
            continue;
        }
        let mut data = GoMap::new_map();
        data.insert("authorID".to_string(), GoValue::Int(c.author_id));
        let entries: Vec<GoValue> = c
            .entries
            .iter()
            .map(|e| {
                let mut m = GoMap::new_struct();
                m.insert("Lang".to_string(), GoValue::Str(e.lang.clone()));
                m.insert("Import".to_string(), GoValue::Str(e.import.clone()));
                GoValue::Object(m)
            })
            .collect();
        data.insert("entries".to_string(), GoValue::Array(entries));
        records.push(super::super::history_formats::NdjsonRecord {
            pos: c.pos,
            hash: c.hash.clone(),
            tick: c.tick,
            author_id: c.author_id,
            time_secs: c.when,
            tz_offset_min: c.offset_min,
            data: GoValue::Object(data),
        });
    }
    Some(records)
}

/// The imports contribution to the merged `--format timeseries` document: per
/// entry-bearing commit `{"import_count": n, "languages": {lang: count}}`.
#[must_use]
pub fn imports_timeseries_contribution(
    sub: &clap::ArgMatches,
) -> Option<super::super::history_formats::TimeSeriesContribution> {
    use cf_gojson::{GoMap, GoValue};

    let walk = imports_walk(sub)?;
    let mut per_commit = Vec::new();
    let mut commit_meta = Vec::new();
    for c in &walk.commits {
        if c.entries.is_empty() {
            continue;
        }
        let mut languages: BTreeMap<&str, i64> = BTreeMap::new();
        for e in &c.entries {
            *languages.entry(e.lang.as_str()).or_insert(0) += 1;
        }
        let mut entry = GoMap::new_map();
        entry.insert(
            "import_count".to_string(),
            GoValue::Int(c.entries.len() as i64),
        );
        let mut langs = GoMap::new_map();
        for (lang, n) in &languages {
            langs.insert((*lang).to_string(), GoValue::Int(*n));
        }
        entry.insert("languages".to_string(), GoValue::Object(langs));
        per_commit.push((c.hash.clone(), GoValue::Object(entry)));
        commit_meta.push((
            c.hash.clone(),
            c.tick,
            crate::handlers::format_rfc3339_offset(c.when, c.offset_min),
            String::new(),
        ));
    }
    Some(super::super::history_formats::TimeSeriesContribution {
        flag: "imports-per-dev",
        per_commit,
        commit_meta,
    })
}
