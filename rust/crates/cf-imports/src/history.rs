//! Per-developer import history (`ImportsPerDeveloper`, id `history/imports`).
//!
//! The history analyzer tracks import usage across commit history, attributing
//! each import to the commit author, language, and tick. The accumulated state
//! is a 4-level map ([`ImportsMap`]: author -> lang -> import -> tick -> count)
//! that merges **additively** (counts are summed) — the analyzer's defining
//! behaviour.
//!
//! This module owns the self-contained data model and the
//! merge/aggregation/attribution logic, and defines the analyzer surface
//! (name/flag/extract-commit-timeseries) over it. The pipeline wiring
//! (consume/fork/snapshotting) lives in the framework crates.

use std::collections::BTreeMap;

/// Default tick length, in hours.
pub const DEFAULT_TICK_HOURS: u64 = 24;

/// 4-level import-usage map: author -> lang -> import -> tick -> count.
///
/// Every level uses a [`BTreeMap`] so iteration is deterministic (sorted),
/// which matters for reproducible aggregation/serialization.
pub type ImportsMap = BTreeMap<i64, BTreeMap<String, BTreeMap<String, BTreeMap<i64, i64>>>>;

/// A single import extracted from a commit, carrying its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    /// The language the import was found in (e.g. `go`, `uast`).
    pub lang: String,
    /// The import path.
    pub import: String,
}

/// Per-commit summary for timeseries output.
///
/// `languages` maps language -> import count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitSummary {
    /// Total number of imports in the commit.
    pub import_count: i64,
    /// Per-language import counts.
    pub languages: BTreeMap<String, i64>,
}

/// Per-tick aggregated payload.
#[derive(Debug, Clone, Default)]
pub struct TickData {
    /// The accumulated 4-level imports map for the tick.
    pub imports: ImportsMap,
    /// Per-commit summaries keyed by commit hash string.
    pub commit_stats: BTreeMap<String, CommitSummary>,
}

/// The per-developer import history analyzer.
///
/// Stateless config holder; mutable per-tick state lives in
/// [`TickData`]/[`ImportsMap`].
#[derive(Debug, Clone)]
pub struct HistoryAnalyzer {
    /// Tick size in hours (defaults to [`DEFAULT_TICK_HOURS`]).
    pub tick_hours: u64,
    /// Author index: author id -> name.
    pub reversed_people_dict: Vec<String>,
}

impl Default for HistoryAnalyzer {
    fn default() -> Self {
        HistoryAnalyzer {
            tick_hours: DEFAULT_TICK_HOURS,
            reversed_people_dict: Vec::new(),
        }
    }
}

impl HistoryAnalyzer {
    /// Creates a new history analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the analyzer name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        "ImportsPerDeveloper"
    }

    /// Returns the CLI flag.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "imports-per-dev"
    }

    /// Returns the stable analyzer id.
    #[must_use]
    pub fn id(&self) -> &'static str {
        "history/imports"
    }

    /// Extracts per-commit timeseries data from a report's `commit_stats`:
    /// `None` when there are no commit stats, otherwise a map of commit hash ->
    /// `{import_count, languages}`.
    #[must_use]
    pub fn extract_commit_time_series(
        &self,
        commit_stats: &BTreeMap<String, CommitSummary>,
    ) -> Option<BTreeMap<String, CommitSummary>> {
        if commit_stats.is_empty() {
            return None;
        }
        Some(commit_stats.clone())
    }
}

/// Adds extracted entries to the 4-level map under one author/tick: each entry
/// increments `map[author][lang][import][tick]` by one.
pub fn add_entries_to_map(m: &mut ImportsMap, entries: &[ImportEntry], author_id: i64, tick: i64) {
    let langs = m.entry(author_id).or_default();
    for entry in entries {
        let imps = langs.entry(entry.lang.clone()).or_default();
        let timps = imps.entry(entry.import.clone()).or_default();
        *timps.entry(tick).or_insert(0) += 1;
    }
}

/// Merges `src` into `dst` additively (summing counts) across all four levels.
///
/// This is the additive-merge behaviour central to the analyzer.
pub fn merge_import_maps(dst: &mut ImportsMap, src: &ImportsMap) {
    for (auth, src_langs) in src {
        let dst_langs = dst.entry(*auth).or_default();
        for (lang, src_imps) in src_langs {
            let dst_imps = dst_langs.entry(lang.clone()).or_default();
            for (imp, src_ticks) in src_imps {
                let dst_ticks = dst_imps.entry(imp.clone()).or_default();
                for (tick, count) in src_ticks {
                    *dst_ticks.entry(*tick).or_insert(0) += *count;
                }
            }
        }
    }
}

/// Aggregates the 4-level map into per-import total counts: sums counts across
/// all authors, languages, and ticks for each import path.
#[must_use]
pub fn aggregate_import_counts(imports: &ImportsMap) -> BTreeMap<String, i64> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for lang_map in imports.values() {
        for imp_map in lang_map.values() {
            for (name, tick_map) in imp_map {
                let total: i64 = tick_map.values().sum();
                *counts.entry(name.clone()).or_insert(0) += total;
            }
        }
    }
    counts
}

/// Maximum number of imports returned by [`top_imports`].
pub const TOP_IMPORTS_LIMIT: usize = 20;

/// Returns the top imports by count (descending), capped at [`TOP_IMPORTS_LIMIT`].
///
/// The reference implementation sorts unstably by count descending with
/// unspecified tie order; this implementation breaks ties by import name
/// ascending for determinism. Returns parallel `(labels, data)` vectors.
#[must_use]
pub fn top_imports(counts: &BTreeMap<String, i64>) -> (Vec<String>, Vec<i64>) {
    let mut items: Vec<(String, i64)> =
        counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if items.len() > TOP_IMPORTS_LIMIT {
        items.truncate(TOP_IMPORTS_LIMIT);
    }
    let labels = items.iter().map(|(k, _)| k.clone()).collect();
    let data = items.iter().map(|(_, v)| *v).collect();
    (labels, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_analyzer_name() {
        let h = HistoryAnalyzer::new();
        assert!(!h.name().is_empty());
        assert_eq!(h.name(), "ImportsPerDeveloper");
    }

    #[test]
    fn test_history_analyzer_flag() {
        let h = HistoryAnalyzer::new();
        assert!(!h.flag().is_empty());
        assert_eq!(h.flag(), "imports-per-dev");
    }

    #[test]
    fn test_history_analyzer_id() {
        let h = HistoryAnalyzer::new();
        assert_eq!(h.id(), "history/imports");
    }

    #[test]
    fn test_extract_commit_time_series() {
        let h = HistoryAnalyzer::new();
        let hash = "aabbccdd00112233445566778899aabbccddeeff".to_string();
        let mut langs = BTreeMap::new();
        langs.insert("go".to_string(), 3);
        langs.insert("python".to_string(), 2);
        let mut stats = BTreeMap::new();
        stats.insert(
            hash.clone(),
            CommitSummary {
                import_count: 5,
                languages: langs,
            },
        );

        let result = h.extract_commit_time_series(&stats).expect("not nil");
        let entry = result.get(&hash).expect("hash present");
        assert_eq!(entry.import_count, 5);
        assert_eq!(entry.languages["go"], 3);
        assert_eq!(entry.languages["python"], 2);
    }

    #[test]
    fn test_extract_commit_time_series_empty() {
        let h = HistoryAnalyzer::new();
        assert!(h.extract_commit_time_series(&BTreeMap::new()).is_none());
    }

    // --- additive-merge / aggregation coverage (the core behaviour) ---

    fn build_test_import_ticks() -> Vec<TickData> {
        let mk = |pairs: &[(i64, &str, &str, i64, i64)]| -> ImportsMap {
            let mut m: ImportsMap = BTreeMap::new();
            for (author, lang, imp, tick, count) in pairs {
                m.entry(*author)
                    .or_default()
                    .entry(lang.to_string())
                    .or_default()
                    .entry(imp.to_string())
                    .or_default()
                    .insert(*tick, *count);
            }
            m
        };

        vec![
            TickData {
                imports: mk(&[
                    (0, "go", "fmt", 0, 3),
                    (0, "go", "os", 0, 2),
                    (0, "go", "strings", 0, 1),
                ]),
                commit_stats: BTreeMap::new(),
            },
            TickData {
                imports: mk(&[
                    (0, "go", "fmt", 1, 5),
                    (0, "go", "io", 1, 1),
                    (1, "go", "fmt", 1, 2),
                    (1, "go", "os", 1, 3),
                ]),
                commit_stats: BTreeMap::new(),
            },
        ]
    }

    #[test]
    fn test_merge_and_aggregate_counts() {
        let ticks = build_test_import_ticks();
        let mut merged: ImportsMap = BTreeMap::new();
        for td in &ticks {
            merge_import_maps(&mut merged, &td.imports);
        }
        let counts = aggregate_import_counts(&merged);
        // fmt: 3 + 5 + 2 = 10; os: 2 + 3 = 5; strings: 1; io: 1.
        assert_eq!(counts["fmt"], 10);
        assert_eq!(counts["os"], 5);
        assert_eq!(counts["strings"], 1);
        assert_eq!(counts["io"], 1);
    }

    #[test]
    fn test_top_imports_orders_by_count_desc() {
        let ticks = build_test_import_ticks();
        let mut merged: ImportsMap = BTreeMap::new();
        for td in &ticks {
            merge_import_maps(&mut merged, &td.imports);
        }
        let counts = aggregate_import_counts(&merged);
        let (labels, data) = top_imports(&counts);
        assert_eq!(labels[0], "fmt");
        assert_eq!(data[0], 10);
    }

    #[test]
    fn test_add_entries_to_map_increments() {
        let mut m: ImportsMap = BTreeMap::new();
        let entries = vec![
            ImportEntry {
                lang: "go".to_string(),
                import: "fmt".to_string(),
            },
            ImportEntry {
                lang: "go".to_string(),
                import: "fmt".to_string(),
            },
            ImportEntry {
                lang: "go".to_string(),
                import: "os".to_string(),
            },
        ];
        add_entries_to_map(&mut m, &entries, 0, 0);
        assert_eq!(m[&0]["go"]["fmt"][&0], 2);
        assert_eq!(m[&0]["go"]["os"][&0], 1);
    }
}
