//! Per-developer import history (`ImportsPerDeveloper`, id `history/imports`).
//!
//! The history analyzer tracks import usage across commit history, attributing
//! each import to the commit author, the file's language and the commit's
//! tick. The accumulated state is a 4-level map ([`ImportsMap`]: author id ->
//! language -> import -> tick -> count) that merges **additively** (counts
//! are summed). [`HistoryReport`] is the report the machine formats encode:
//! the tick size, the author index and the map keyed by developer name.

use std::collections::BTreeMap;

use cf_gojson::{GoMap, GoValue};

/// 4-level import-usage map: author id -> language -> import -> tick -> count.
///
/// Every level uses a [`BTreeMap`] so iteration is deterministic (sorted),
/// which matters for reproducible aggregation/serialization.
pub type ImportsMap = BTreeMap<i64, LanguageImports>;

/// language -> import -> tick -> count: one developer's usage.
pub type LanguageImports = BTreeMap<String, BTreeMap<String, BTreeMap<i64, i64>>>;

/// A single import extracted from a commit, carrying the language of the file
/// it was found in (the parser's language name, e.g. `go`, `python`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportEntry {
    /// The language of the importing file.
    pub lang: String,
    /// The import path.
    pub import: String,
}

/// Adds extracted entries to the 4-level map under one author/tick: each entry
/// increments `map[author][lang][import][tick]` by one. An empty entry list
/// adds nothing — not even the author.
pub fn add_entries_to_map(m: &mut ImportsMap, entries: &[ImportEntry], author_id: i64, tick: i64) {
    if entries.is_empty() {
        return;
    }
    let langs = m.entry(author_id).or_default();
    for entry in entries {
        let imps = langs.entry(entry.lang.clone()).or_default();
        let timps = imps.entry(entry.import.clone()).or_default();
        *timps.entry(tick).or_insert(0) += 1;
    }
}

/// Merges `src` into `dst` additively (summing counts) across all four levels.
///
/// This is the additive-merge behaviour central to the analyzer: the same
/// `author/lang/import/tick` cell in both maps sums its counts.
///
/// ```
/// use cf_imports::history::{add_entries_to_map, merge_import_maps, ImportEntry, ImportsMap};
///
/// let entry = ImportEntry { lang: "go".into(), import: "fmt".into() };
///
/// // Two ticks each record `fmt` once for author 1.
/// let mut a: ImportsMap = ImportsMap::new();
/// add_entries_to_map(&mut a, &[entry.clone()], 1, 0);
/// let mut b: ImportsMap = ImportsMap::new();
/// add_entries_to_map(&mut b, &[entry.clone()], 1, 0);
///
/// merge_import_maps(&mut a, &b);
/// // Same author/lang/import/tick -> counts add to 2.
/// assert_eq!(a[&1]["go"]["fmt"][&0], 2);
/// ```
pub fn merge_import_maps(dst: &mut ImportsMap, src: &ImportsMap) {
    for (auth, src_langs) in src {
        merge_language_imports(dst.entry(*auth).or_default(), src_langs);
    }
}

/// Merges one developer's `language -> import -> tick -> count` map into
/// `dst` additively.
fn merge_language_imports(dst: &mut LanguageImports, src: &LanguageImports) {
    for (lang, src_imps) in src {
        let dst_imps = dst.entry(lang.clone()).or_default();
        for (imp, src_ticks) in src_imps {
            let dst_ticks = dst_imps.entry(imp.clone()).or_default();
            for (tick, count) in src_ticks {
                *dst_ticks.entry(*tick).or_insert(0) += *count;
            }
        }
    }
}

/// Aggregates the 4-level map into per-import total counts: sums counts across
/// all authors, languages, and ticks for each import path.
///
/// ```
/// use cf_imports::history::{add_entries_to_map, aggregate_import_counts, ImportEntry, ImportsMap};
///
/// let mut m: ImportsMap = ImportsMap::new();
/// let fmt = ImportEntry { lang: "go".into(), import: "fmt".into() };
/// // `fmt` recorded by author 1 (tick 0) and author 2 (tick 1).
/// add_entries_to_map(&mut m, &[fmt.clone()], 1, 0);
/// add_entries_to_map(&mut m, &[fmt], 2, 1);
///
/// let counts = aggregate_import_counts(&m);
/// assert_eq!(counts["fmt"], 2);
/// ```
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

/// Returns the top imports by count (descending, ties by import name
/// ascending), capped at [`TOP_IMPORTS_LIMIT`], as parallel `(labels, data)`
/// vectors — the plot's bar chart input.
///
/// ```
/// use std::collections::BTreeMap;
/// use cf_imports::history::top_imports;
///
/// let mut counts = BTreeMap::new();
/// counts.insert("os".to_string(), 1);
/// counts.insert("sys".to_string(), 3);
/// counts.insert("io".to_string(), 1); // ties with `os`; name breaks the tie.
///
/// let (labels, data) = top_imports(&counts);
/// assert_eq!(labels, vec!["sys", "io", "os"]); // count desc, then name asc
/// assert_eq!(data, vec![3, 1, 1]);
/// ```
#[must_use]
pub fn top_imports(counts: &BTreeMap<String, i64>) -> (Vec<String>, Vec<i64>) {
    let mut items: Vec<(String, i64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(TOP_IMPORTS_LIMIT);
    let labels = items.iter().map(|(k, _)| k.clone()).collect();
    let data = items.iter().map(|(_, v)| *v).collect();
    (labels, data)
}

/// Report key: the tick length in seconds.
pub const KEY_TICK_SIZE: &str = "tick_size";
/// Report key: developer names by author id.
pub const KEY_AUTHOR_INDEX: &str = "author_index";
/// Report key: the developer → language → import → tick → count map.
pub const KEY_IMPORTS: &str = "imports";

/// The `history/imports` report: the tick size, the author index and the
/// per-developer import usage — the ONE value json/yaml/bin encode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryReport {
    /// Tick length in seconds (`--tick-size` hours × 3600).
    pub tick_size_secs: i64,
    /// Developer names by author id.
    pub author_index: Vec<String>,
    /// author id -> language -> import -> tick -> count.
    pub imports: ImportsMap,
}

impl HistoryReport {
    /// The developer name of `author_id`: its `author_index` entry, or the
    /// id itself when it lies outside the index.
    fn developer_name(&self, author_id: i64) -> String {
        usize::try_from(author_id)
            .ok()
            .and_then(|i| self.author_index.get(i))
            .map_or_else(|| author_id.to_string(), Clone::clone)
    }

    /// The map keyed by developer name: two author ids that resolve to the
    /// same name merge additively.
    fn imports_by_developer(&self) -> BTreeMap<String, LanguageImports> {
        let mut by_name: BTreeMap<String, LanguageImports> = BTreeMap::new();
        for (author_id, langs) in &self.imports {
            merge_language_imports(
                by_name.entry(self.developer_name(*author_id)).or_default(),
                langs,
            );
        }
        by_name
    }

    /// The report value: `tick_size`, `author_index` and `imports`
    /// (developer → language → import → tick → count; tick keys are
    /// integers, rendered as decimal strings in json).
    ///
    /// ```
    /// use cf_imports::history::{add_entries_to_map, HistoryReport, ImportEntry, ImportsMap};
    ///
    /// let mut imports = ImportsMap::new();
    /// add_entries_to_map(&mut imports, &[ImportEntry { lang: "go".into(), import: "fmt".into() }], 0, 3);
    /// let report = HistoryReport { tick_size_secs: 86_400, author_index: vec!["alice".into()], imports };
    /// let json = String::from_utf8(cf_gojson::marshal(&report.to_go_value())).unwrap();
    /// assert_eq!(
    ///     json,
    ///     r#"{"tick_size":86400,"author_index":["alice"],"imports":{"alice":{"go":{"fmt":{"3":1}}}}}"#
    /// );
    /// ```
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut developers = GoMap::new_map();
        for (name, langs) in self.imports_by_developer() {
            let mut lang_map = GoMap::new_map();
            for (lang, imps) in &langs {
                let mut imp_map = GoMap::new_map();
                for (imp, ticks) in imps {
                    let mut tick_map = GoMap::new_int_map();
                    for (tick, count) in ticks {
                        tick_map.push(tick.to_string(), GoValue::Int(*count));
                    }
                    imp_map.push(imp.clone(), GoValue::Map(tick_map));
                }
                lang_map.push(lang.clone(), GoValue::Map(imp_map));
            }
            developers.push(name, GoValue::Map(lang_map));
        }

        let mut root = GoMap::new_struct();
        root.push(KEY_TICK_SIZE, GoValue::Int(self.tick_size_secs));
        root.push(
            KEY_AUTHOR_INDEX,
            GoValue::Array(
                self.author_index
                    .iter()
                    .cloned()
                    .map(GoValue::Str)
                    .collect(),
            ),
        );
        root.push(KEY_IMPORTS, GoValue::Map(developers));
        GoValue::Map(root)
    }

    /// The yaml value: the same tree as [`HistoryReport::to_go_value`] (the
    /// yaml encoder renders the integer tick keys as plain scalars).
    #[must_use]
    pub fn to_go_value_yaml(&self) -> GoValue {
        self.to_go_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(pairs: &[(i64, &str, &str, i64, i64)]) -> ImportsMap {
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
    }

    #[test]
    fn merge_and_aggregate_counts() {
        let tick0 = mk(&[
            (0, "go", "fmt", 0, 3),
            (0, "go", "os", 0, 2),
            (0, "go", "strings", 0, 1),
        ]);
        let tick1 = mk(&[
            (0, "go", "fmt", 1, 5),
            (0, "go", "io", 1, 1),
            (1, "go", "fmt", 1, 2),
            (1, "go", "os", 1, 3),
        ]);
        let mut merged: ImportsMap = BTreeMap::new();
        merge_import_maps(&mut merged, &tick0);
        merge_import_maps(&mut merged, &tick1);
        let counts = aggregate_import_counts(&merged);
        // fmt: 3 + 5 + 2 = 10; os: 2 + 3 = 5; strings: 1; io: 1.
        assert_eq!(counts["fmt"], 10);
        assert_eq!(counts["os"], 5);
        assert_eq!(counts["strings"], 1);
        assert_eq!(counts["io"], 1);
        let (labels, data) = top_imports(&counts);
        assert_eq!(labels[0], "fmt");
        assert_eq!(data[0], 10);
    }

    #[test]
    fn top_imports_is_capped() {
        let counts: BTreeMap<String, i64> = (0..30).map(|i| (format!("m{i:02}"), i)).collect();
        let (labels, data) = top_imports(&counts);
        assert_eq!(labels.len(), TOP_IMPORTS_LIMIT);
        assert_eq!(labels[0], "m29");
        assert_eq!(data[0], 29);
    }

    #[test]
    fn add_entries_to_map_increments() {
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
                lang: "python".to_string(),
                import: "os".to_string(),
            },
        ];
        add_entries_to_map(&mut m, &entries, 0, 0);
        assert_eq!(m[&0]["go"]["fmt"][&0], 2);
        assert_eq!(m[&0]["python"]["os"][&0], 1);
        // A commit without imports leaves no trace of its author.
        add_entries_to_map(&mut m, &[], 1, 0);
        assert!(!m.contains_key(&1));
    }

    /// The documented shape: developer → language → import → tick → count,
    /// plus `tick_size` and the author index.
    #[test]
    fn report_value_is_the_documented_four_level_map() {
        let report = HistoryReport {
            tick_size_secs: 86_400,
            author_index: vec!["alice".into(), "bob".into()],
            imports: mk(&[
                (0, "go", "fmt", 0, 1),
                (0, "go", "fmt", 2, 1),
                (0, "go", "net/http", 2, 1),
                (1, "python", "os", 1, 1),
            ]),
        };
        let json = String::from_utf8(cf_gojson::marshal(&report.to_go_value())).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"tick_size":86400,"author_index":["alice","bob"],"#,
                r#""imports":{"alice":{"go":{"fmt":{"0":1,"2":1},"net/http":{"2":1}}},"bob":{"python":{"os":{"1":1}}}}}"#
            )
        );
        let yaml = String::from_utf8(cf_goyaml::marshal(&report.to_go_value_yaml())).unwrap();
        assert!(yaml.contains("tick_size: 86400"), "{yaml}");
        assert!(yaml.contains("net/http:"), "{yaml}");
        assert!(yaml.contains("python:"), "{yaml}");
    }

    #[test]
    fn empty_report_keeps_every_key() {
        let report = HistoryReport {
            tick_size_secs: 3_600,
            ..HistoryReport::default()
        };
        let json = String::from_utf8(cf_gojson::marshal(&report.to_go_value())).unwrap();
        assert_eq!(json, r#"{"tick_size":3600,"author_index":[],"imports":{}}"#);
    }

    #[test]
    fn same_name_authors_merge_and_unknown_ids_keep_their_number() {
        let report = HistoryReport {
            tick_size_secs: 86_400,
            author_index: vec!["alice".into(), "alice".into()],
            imports: mk(&[
                (0, "go", "fmt", 0, 1),
                (1, "go", "fmt", 0, 2),
                (7, "go", "os", 0, 1),
            ]),
        };
        let json = String::from_utf8(cf_gojson::marshal(&report.to_go_value())).unwrap();
        assert!(json.contains(r#""alice":{"go":{"fmt":{"0":3}}}"#), "{json}");
        assert!(json.contains(r#""7":{"go":{"os":{"0":1}}}"#), "{json}");
    }
}
