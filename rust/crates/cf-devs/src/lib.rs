//! `cf-devs` — developer activity / per-developer line statistics with
//! HyperLogLog developer-cardinality estimation.
//!
//! Port of the Go package `internal/analyzers/devs` (analyzer id
//! `history/devs`, Sequential). It computes, from per-commit developer data
//! grouped into time buckets ("ticks"):
//!
//! - per-developer commit / lines-added / removed / changed totals, language
//!   breakdowns, and activity span ([`metrics::compute_developers`]);
//! - per-language totals and per-developer contributions
//!   ([`metrics::compute_languages`]);
//! - CHAOSS bus-factor risk per language ([`metrics::compute_bus_factor`]);
//! - per-tick activity and churn time-series
//!   ([`metrics::compute_activity`], [`metrics::compute_churn`]);
//! - an aggregate summary that estimates total/active developer cardinality with
//!   an HLL sketch ([`metrics::compute_aggregate`]).
//!
//! ## Determinism and byte-identity
//!
//! Scoring reads **no wall clock** (`time.Now` is never consulted); the only
//! time input is the configured tick size. Merges are purely additive
//! ([`model::CommitDevData::merge`], [`aggregate::merge_dev_data`]). All ordered
//! results iterate sorted keys / sort explicitly so the output is reproducible.
//!
//! Machine-format report bytes (json/yaml/ndjson/timeseries/compact/bin) are
//! produced via [`cf_gojson`] in [`serialize`]; this crate never uses
//! `serde_json` for output. See `specs/rust-rewrite/DESIGN.md` §2.

#![forbid(unsafe_code)]

pub mod aggregate;
pub mod metrics;
pub mod model;
pub mod serialize;

/// Store record kind constants (`internal/analyzers/devs/store_writer.go`).
pub mod kinds {
    /// Per-developer records.
    pub const DEVELOPER: &str = "developer";
    /// Per-language records.
    pub const LANGUAGE: &str = "language";
    /// Per-language bus-factor records.
    pub const BUS_FACTOR: &str = "bus_factor";
    /// Per-tick activity records.
    pub const ACTIVITY: &str = "activity";
    /// Per-tick churn records.
    pub const CHURN: &str = "churn";
    /// Single aggregate record.
    pub const AGGREGATE: &str = "aggregate";
}

/// Analyzer descriptor constants (`NewAnalyzer`).
pub mod descriptor {
    /// Analyzer id.
    pub const ID: &str = "history/devs";
    /// Human-readable description.
    pub const DESCRIPTION: &str =
        "Calculates the number of commits, added, removed and changed lines per developer through time.";
    /// The analyzer is sequential-only.
    pub const SEQUENTIAL: bool = true;
}

pub use aggregate::{
    accumulate_line_stats, aggregate_commits_to_ticks, merge_dev_data, parse_tick_data,
    resolve_tick_size,
};
pub use metrics::{
    compute_activity, compute_aggregate, compute_all_metrics, compute_bus_factor,
    compute_bus_factor_from_sorted, compute_churn, compute_developers, compute_languages,
    compute_project_bus_factor, dev_id_bytes, dev_name_and_email, AggregateInput, BusFactorInput,
    MetricOptions, TickData, HLL_PRECISION,
};
pub use model::{
    ActivityData, AggregateData, BusFactorData, ChurnData, CommitDevData, ComputedMetrics,
    DeveloperCommits, DeveloperData, DevTick, LanguageData, LanguageStatsEntry, LineStats,
};

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::GoValue;
    use std::collections::BTreeMap;

    const TICK_SIZE: i64 = 24 * 3_600_000_000_000; // 24h in ns.
    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn ls(added: i64, removed: i64, changed: i64) -> LineStats {
        LineStats {
            added,
            removed,
            changed,
        }
    }

    fn dev_tick(commits: i64, line: LineStats, langs: &[(&str, LineStats)]) -> DevTick {
        let mut languages = BTreeMap::new();
        for (name, stats) in langs {
            languages.insert((*name).to_string(), *stats);
        }
        DevTick {
            line_stats: line,
            languages,
            commits,
        }
    }

    fn tick_data(ticks: BTreeMap<i64, BTreeMap<i64, DevTick>>, names: &[&str]) -> TickData {
        TickData {
            ticks,
            names: names.iter().map(|s| (*s).to_string()).collect(),
            tick_size: TICK_SIZE,
        }
    }

    fn cdd(
        commits: i64,
        a: i64,
        r: i64,
        c: i64,
        author: i64,
        langs: &[(&str, LineStats)],
    ) -> CommitDevData {
        let mut languages = BTreeMap::new();
        for (name, stats) in langs {
            languages.insert((*name).to_string(), *stats);
        }
        CommitDevData {
            commits,
            added: a,
            removed: r,
            changed: c,
            author_id: author,
            languages,
        }
    }

    // --- dev_id_bytes (hll_test.go) ---

    #[test]
    fn dev_id_bytes_deterministic() {
        assert_eq!(dev_id_bytes(0), dev_id_bytes(0));
    }

    #[test]
    fn dev_id_bytes_negative_id_maps_to_author_missing() {
        // AuthorMissing is the large positive sentinel 262142.
        let result = dev_id_bytes(i64::from(cf_identity::AUTHOR_MISSING));
        assert!(!result.is_empty());
        assert_eq!(result, b"262142");
    }

    #[test]
    fn dev_id_bytes_unique() {
        let b0 = dev_id_bytes(0);
        let b1 = dev_id_bytes(1);
        let b2 = dev_id_bytes(2);
        assert_ne!(b0, b1);
        assert_ne!(b1, b2);
        assert_ne!(b0, b2);
    }

    // --- aggregate_commits_to_ticks (analyzer_test.go) ---

    #[test]
    fn aggregate_commits_to_ticks_basic() {
        let mut commit_dev_data = BTreeMap::new();
        commit_dev_data.insert(HASH_A.to_string(), cdd(1, 20, 5, 3, 1, &[("Go", ls(20, 5, 3))]));
        commit_dev_data.insert(
            HASH_B.to_string(),
            cdd(1, 10, 3, 2, 2, &[("Python", ls(10, 3, 2))]),
        );
        let mut commits_by_tick = BTreeMap::new();
        commits_by_tick.insert(0, vec![HASH_A.to_string(), HASH_B.to_string()]);

        let result = aggregate_commits_to_ticks(&commit_dev_data, &commits_by_tick);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0].len(), 2);

        let dt1 = &result[&0][&1];
        assert_eq!(dt1.commits, 1);
        assert_eq!(dt1.line_stats.added, 20);
        assert_eq!(dt1.line_stats.removed, 5);

        let dt2 = &result[&0][&2];
        assert_eq!(dt2.commits, 1);
        assert_eq!(dt2.line_stats.added, 10);
    }

    #[test]
    fn aggregate_commits_to_ticks_same_author_multiple_commits() {
        let mut commit_dev_data = BTreeMap::new();
        commit_dev_data.insert(HASH_A.to_string(), cdd(1, 20, 5, 0, 1, &[]));
        commit_dev_data.insert(HASH_B.to_string(), cdd(1, 10, 3, 0, 1, &[]));
        let mut commits_by_tick = BTreeMap::new();
        commits_by_tick.insert(0, vec![HASH_A.to_string(), HASH_B.to_string()]);

        let result = aggregate_commits_to_ticks(&commit_dev_data, &commits_by_tick);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0].len(), 1);

        let dt = &result[&0][&1];
        assert_eq!(dt.commits, 2);
        assert_eq!(dt.line_stats.added, 30);
        assert_eq!(dt.line_stats.removed, 8);
    }

    #[test]
    fn aggregate_commits_to_ticks_empty_inputs() {
        let empty_cdd: BTreeMap<String, CommitDevData> = BTreeMap::new();
        let mut one_tick = BTreeMap::new();
        one_tick.insert(0i64, Vec::<String>::new());
        assert!(aggregate_commits_to_ticks(&empty_cdd, &one_tick).is_empty());

        let mut one_cdd = BTreeMap::new();
        one_cdd.insert("a".to_string(), CommitDevData::default());
        let empty_cbt: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        assert!(aggregate_commits_to_ticks(&one_cdd, &empty_cbt).is_empty());
    }

    // --- merge_dev_data / CommitDevData::merge (analyzer_test.go) ---

    #[test]
    fn merge_commit_dev_data() {
        let mut existing = cdd(1, 10, 2, 1, 0, &[("Go", ls(10, 2, 0))]);
        let incoming = cdd(2, 20, 5, 3, 0, &[("Go", ls(15, 3, 0)), ("Python", ls(5, 2, 0))]);
        existing.merge(&incoming);
        assert_eq!(existing.commits, 3);
        assert_eq!(existing.added, 30);
        assert_eq!(existing.removed, 7);
        assert_eq!(existing.languages["Go"].added, 25);
        assert_eq!(existing.languages["Python"].added, 5);
    }

    #[test]
    fn merge_state_combines_maps() {
        let mut s1 = BTreeMap::new();
        s1.insert("aaa".to_string(), cdd(1, 10, 0, 0, 0, &[]));
        let mut s2 = BTreeMap::new();
        s2.insert("bbb".to_string(), cdd(2, 20, 0, 0, 0, &[]));
        merge_dev_data(&mut s1, &s2);
        assert_eq!(s1.len(), 2);
    }

    // --- DevelopersMetric (metrics_test.go) ---

    #[test]
    fn developers_metric_single_developer() {
        let mut t0 = BTreeMap::new();
        t0.insert(0, dev_tick(10, ls(100, 50, 0), &[("Go", ls(100, 0, 0))]));
        let mut ticks = BTreeMap::new();
        ticks.insert(0, t0);

        let input = tick_data(ticks, &["Alice"]);
        let result = compute_developers(&input);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 0);
        assert_eq!(result[0].name, "Alice");
        assert_eq!(result[0].commits, 10);
        assert_eq!(result[0].added, 100);
        assert_eq!(result[0].removed, 50);
        assert_eq!(result[0].net_lines, 50);
        assert_eq!(result[0].first_tick, 0);
        assert_eq!(result[0].last_tick, 0);
        assert_eq!(result[0].active_ticks, 1);
    }

    #[test]
    fn developers_metric_sorted_by_commits() {
        let mut t0 = BTreeMap::new();
        t0.insert(0, dev_tick(5, ls(0, 0, 0), &[]));
        t0.insert(1, dev_tick(15, ls(0, 0, 0), &[]));
        let mut ticks = BTreeMap::new();
        ticks.insert(0, t0);

        let input = tick_data(ticks, &["Alice", "Bob"]);
        let result = compute_developers(&input);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "Bob");
        assert_eq!(result[0].commits, 15);
        assert_eq!(result[1].name, "Alice");
        assert_eq!(result[1].commits, 5);
    }

    #[test]
    fn developers_metric_multiple_ticks() {
        let mut ticks = BTreeMap::new();
        for (t, c, a) in [(0, 5, 50), (5, 3, 30), (10, 2, 20)] {
            let mut tm = BTreeMap::new();
            tm.insert(0, dev_tick(c, ls(a, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        let input = tick_data(ticks, &["Alice"]);
        let result = compute_developers(&input);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].commits, 10);
        assert_eq!(result[0].added, 100);
        assert_eq!(result[0].first_tick, 0);
        assert_eq!(result[0].last_tick, 10);
        assert_eq!(result[0].active_ticks, 3);
    }

    #[test]
    fn developers_metric_language_aggregation() {
        let mut t0 = BTreeMap::new();
        t0.insert(
            0,
            dev_tick(10, ls(0, 0, 0), &[("Go", ls(50, 10, 5)), ("Python", ls(30, 5, 2))]),
        );
        let mut t1 = BTreeMap::new();
        t1.insert(0, dev_tick(10, ls(0, 0, 0), &[("Go", ls(20, 5, 3))]));
        let mut ticks = BTreeMap::new();
        ticks.insert(0, t0);
        ticks.insert(1, t1);

        let input = tick_data(ticks, &["Alice"]);
        let result = compute_developers(&input);
        assert_eq!(result.len(), 1);
        let go = result[0].languages.iter().find(|l| l.language == "Go").unwrap();
        assert_eq!(go.added, 70);
        assert_eq!(go.removed, 15);
        assert_eq!(go.changed, 8);
        let py = result[0]
            .languages
            .iter()
            .find(|l| l.language == "Python")
            .unwrap();
        assert_eq!(py.added, 30);
        assert_eq!(py.removed, 5);
        assert_eq!(py.changed, 2);
    }

    // --- LanguagesMetric (metrics_test.go) ---

    #[test]
    fn languages_metric_contribution_includes_removed() {
        let developers = vec![
            DeveloperData {
                id: 0,
                languages: vec![LanguageStatsEntry {
                    language: "Go".to_string(),
                    added: 60,
                    removed: 40,
                    changed: 0,
                }],
                ..DeveloperData::default()
            },
            DeveloperData {
                id: 1,
                languages: vec![LanguageStatsEntry {
                    language: "Go".to_string(),
                    added: 10,
                    removed: 90,
                    changed: 0,
                }],
                ..DeveloperData::default()
            },
        ];
        let result = compute_languages(&developers);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_lines, 70);
        assert_eq!(result[0].total_contribution, 200);
        assert_eq!(result[0].contributors[&0], 100);
        assert_eq!(result[0].contributors[&1], 100);
    }

    #[test]
    fn languages_metric_empty_name_becomes_other() {
        let developers = vec![DeveloperData {
            id: 0,
            languages: vec![LanguageStatsEntry {
                language: String::new(),
                added: 100,
                removed: 0,
                changed: 0,
            }],
            ..DeveloperData::default()
        }];
        let result = compute_languages(&developers);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Other");
    }

    // --- BusFactorMetric (metrics_test.go) ---

    fn lang_with_contribs(name: &str, total: i64, contribs: &[(i64, i64)]) -> LanguageData {
        let mut c = BTreeMap::new();
        for (id, v) in contribs {
            c.insert(*id, *v);
        }
        LanguageData {
            name: name.to_string(),
            total_lines: total,
            total_contribution: total,
            contributors: c,
        }
    }

    #[test]
    fn bus_factor_single_contributor_critical() {
        let langs = vec![lang_with_contribs("Go", 100, &[(0, 100)])];
        let names = vec!["Alice".to_string()];
        let result = compute_bus_factor(
            &BusFactorInput {
                languages: &langs,
                names: &names,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].language, "Go");
        assert_eq!(result[0].primary_dev_id, 0);
        assert_eq!(result[0].primary_dev_name, "Alice");
        assert!((result[0].primary_pct - 100.0).abs() < 0.01);
        assert_eq!(result[0].risk_level, cf_metrics::RISK_CRITICAL);
        assert_eq!(result[0].bus_factor, 1);
        assert_eq!(result[0].total_contributors, 1);
    }

    #[test]
    fn bus_factor_risk_levels() {
        let cases = [
            (95, cf_metrics::RISK_CRITICAL),
            (90, cf_metrics::RISK_CRITICAL),
            (85, cf_metrics::RISK_HIGH),
            (80, cf_metrics::RISK_HIGH),
            (70, cf_metrics::RISK_MEDIUM),
            (60, cf_metrics::RISK_MEDIUM),
            (55, cf_metrics::RISK_LOW),
            (50, cf_metrics::RISK_LOW),
        ];
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        for (pct, want) in cases {
            let langs = vec![lang_with_contribs("Go", 100, &[(0, pct), (1, 100 - pct)])];
            let result = compute_bus_factor(
                &BusFactorInput {
                    languages: &langs,
                    names: &names,
                },
                &MetricOptions::default(),
            );
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].risk_level, want, "pct={pct}");
        }
    }

    #[test]
    fn bus_factor_zero_contribution_skipped() {
        let langs = vec![LanguageData {
            name: "Go".to_string(),
            total_contribution: 0,
            ..LanguageData::default()
        }];
        let names = vec!["Alice".to_string()];
        let result = compute_bus_factor(
            &BusFactorInput {
                languages: &langs,
                names: &names,
            },
            &MetricOptions::default(),
        );
        assert!(result.is_empty());
    }

    #[test]
    fn bus_factor_chaoss_number() {
        let langs = vec![lang_with_contribs(
            "Go",
            100,
            &[(0, 30), (1, 25), (2, 20), (3, 15), (4, 10)],
        )];
        let names: Vec<String> = ["Alice", "Bob", "Charlie", "Dave", "Eve"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let result = compute_bus_factor(
            &BusFactorInput {
                languages: &langs,
                names: &names,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bus_factor, 2);
        assert_eq!(result[0].total_contributors, 5);
        assert_eq!(result[0].risk_level, cf_metrics::RISK_LOW);
    }

    #[test]
    fn bus_factor_sorted_by_risk_priority() {
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        let langs = vec![
            lang_with_contribs("Go", 100, &[(0, 50), (1, 50)]), // LOW
            lang_with_contribs("Python", 100, &[(0, 95), (1, 5)]), // CRITICAL
            lang_with_contribs("JavaScript", 100, &[(0, 70), (1, 30)]), // MEDIUM
        ];
        let result = compute_bus_factor(
            &BusFactorInput {
                languages: &langs,
                names: &names,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].risk_level, cf_metrics::RISK_CRITICAL);
        assert_eq!(result[1].risk_level, cf_metrics::RISK_MEDIUM);
        assert_eq!(result[2].risk_level, cf_metrics::RISK_LOW);
    }

    // --- ActivityMetric / ChurnMetric (metrics_test.go) ---

    #[test]
    fn activity_metric_single_tick() {
        let mut t0 = BTreeMap::new();
        t0.insert(0, dev_tick(5, ls(0, 0, 0), &[]));
        t0.insert(1, dev_tick(3, ls(0, 0, 0), &[]));
        let mut ticks = BTreeMap::new();
        ticks.insert(0, t0);
        let input = tick_data(ticks, &[]);
        let result = compute_activity(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].tick, 0);
        assert_eq!(result[0].total_commits, 8);
        assert_eq!(result[0].by_developer.len(), 2);
        assert_eq!(result[0].by_developer[0].dev_id, 0);
        assert_eq!(result[0].by_developer[0].commits, 5);
        assert_eq!(result[0].by_developer[1].dev_id, 1);
        assert_eq!(result[0].by_developer[1].commits, 3);
    }

    #[test]
    fn activity_metric_multiple_ticks_sorted() {
        let mut ticks = BTreeMap::new();
        for (t, dev, c) in [(0, 0, 5), (5, 0, 3), (10, 1, 2)] {
            let mut tm = BTreeMap::new();
            tm.insert(dev, dev_tick(c, ls(0, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        let input = tick_data(ticks, &[]);
        let result = compute_activity(&input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].tick, 0);
        assert_eq!(result[1].tick, 5);
        assert_eq!(result[2].tick, 10);
    }

    #[test]
    fn churn_metric_single_tick() {
        let mut t0 = BTreeMap::new();
        t0.insert(0, dev_tick(0, ls(100, 30, 0), &[]));
        t0.insert(1, dev_tick(0, ls(50, 20, 0), &[]));
        let mut ticks = BTreeMap::new();
        ticks.insert(0, t0);
        let input = tick_data(ticks, &[]);
        let result = compute_churn(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].tick, 0);
        assert_eq!(result[0].added, 150);
        assert_eq!(result[0].removed, 50);
        assert_eq!(result[0].net, 100);
    }

    // --- AggregateMetric (metrics_test.go, hll integration) ---

    #[test]
    fn aggregate_metric_compute() {
        let developers = vec![
            DeveloperData {
                commits: 10,
                added: 100,
                removed: 30,
                ..DeveloperData::default()
            },
            DeveloperData {
                commits: 5,
                added: 50,
                removed: 20,
                ..DeveloperData::default()
            },
        ];
        let mut ticks = BTreeMap::new();
        for (t, dev, c) in [(0, 0, 5), (5, 0, 5), (8, 1, 5), (10, 0, 3)] {
            let mut tm = BTreeMap::new();
            tm.insert(dev, dev_tick(c, ls(0, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        let result = compute_aggregate(
            &AggregateInput {
                developers: &developers,
                languages: &[],
                ticks: &ticks,
                tick_size: TICK_SIZE,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.total_commits, 15);
        assert_eq!(result.total_lines_added, 150);
        assert_eq!(result.total_lines_removed, 50);
        assert_eq!(result.total_developers, 2);
        assert_eq!(result.analysis_period_ticks, 10);
        // Active window covers the whole period here, so both devs are active.
        assert_eq!(result.active_developers, 2);
        // Dev0 contributes 130 >= 100 (half of 200) → project bus factor 1.
        assert_eq!(result.project_bus_factor, 1);
    }

    #[test]
    fn aggregate_active_developers_time_based() {
        let developers = vec![
            DeveloperData {
                id: 0,
                commits: 5,
                added: 100,
                removed: 30,
                ..DeveloperData::default()
            },
            DeveloperData {
                id: 1,
                commits: 3,
                added: 50,
                removed: 10,
                ..DeveloperData::default()
            },
        ];
        let mut ticks = BTreeMap::new();
        for (t, dev, c) in [(0, 0, 5), (180, 1, 3)] {
            let mut tm = BTreeMap::new();
            tm.insert(dev, dev_tick(c, ls(0, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        let result = compute_aggregate(
            &AggregateInput {
                developers: &developers,
                languages: &[],
                ticks: &ticks,
                tick_size: TICK_SIZE,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.analysis_period_ticks, 180);
        // threshold = 180 - 90 = 90; only dev1 (tick 180) is active.
        assert_eq!(result.active_developers, 1);
    }

    #[test]
    fn aggregate_active_developers_ratio_fallback() {
        let developers = vec![
            DeveloperData {
                id: 0,
                commits: 5,
                added: 100,
                removed: 30,
                ..DeveloperData::default()
            },
            DeveloperData {
                id: 1,
                commits: 3,
                added: 50,
                removed: 10,
                ..DeveloperData::default()
            },
        ];
        let mut ticks = BTreeMap::new();
        for (t, dev, c) in [(0, 0, 5), (5, 0, 5), (8, 1, 5), (10, 0, 3)] {
            let mut tm = BTreeMap::new();
            tm.insert(dev, dev_tick(c, ls(0, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        // tick_size = 0 → ratio fallback (threshold = 10 * 0.7 = 7).
        let result = compute_aggregate(
            &AggregateInput {
                developers: &developers,
                languages: &[],
                ticks: &ticks,
                tick_size: 0,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.active_developers, 2);
    }

    #[test]
    fn aggregate_empty_estimates_zero() {
        let ticks = BTreeMap::new();
        let result = compute_aggregate(
            &AggregateInput {
                developers: &[],
                languages: &[],
                ticks: &ticks,
                tick_size: TICK_SIZE,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.estimated_total_developers, 0);
        assert_eq!(result.estimated_active_developers, 0);
    }

    #[test]
    fn aggregate_hll_estimated_fields_small() {
        // 2 distinct developers → HLL estimates exactly 2 (small-range correction).
        let developers = vec![
            DeveloperData {
                id: 0,
                ..DeveloperData::default()
            },
            DeveloperData {
                id: 1,
                ..DeveloperData::default()
            },
        ];
        let mut ticks = BTreeMap::new();
        for (t, dev) in [(0, 0), (5, 0), (8, 1)] {
            let mut tm = BTreeMap::new();
            tm.insert(dev, dev_tick(3, ls(0, 0, 0), &[]));
            ticks.insert(t, tm);
        }
        let result = compute_aggregate(
            &AggregateInput {
                developers: &developers,
                languages: &[],
                ticks: &ticks,
                tick_size: TICK_SIZE,
            },
            &MetricOptions::default(),
        );
        assert_eq!(result.estimated_total_developers, 2);
        assert_eq!(result.estimated_active_developers, 2);
    }

    #[test]
    fn hll_accuracy_1000_devs() {
        let developers: Vec<DeveloperData> = (0..1000)
            .map(|i| DeveloperData {
                id: i,
                ..DeveloperData::default()
            })
            .collect();
        let result = compute_aggregate(
            &AggregateInput {
                developers: &developers,
                languages: &[],
                ticks: &BTreeMap::new(),
                tick_size: TICK_SIZE,
            },
            &MetricOptions::default(),
        );
        let err = metrics::relative_error(result.estimated_total_developers, 1000);
        assert!(err <= 0.03, "HLL error {err} exceeds bound");
    }

    // --- dev_name_and_email (metrics_test.go) ---

    #[test]
    fn dev_name_and_email_variants() {
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        assert_eq!(dev_name_and_email(0, &names).0, "Alice");
        assert_eq!(dev_name_and_email(1, &names).0, "Bob");
        assert!(dev_name_and_email(99, &names).0.contains("dev_99"));
    }

    // --- compute_all_metrics end-to-end (analyzer_test.go) ---

    #[test]
    fn compute_all_metrics_from_commit_data() {
        let mut commit_dev_data = BTreeMap::new();
        commit_dev_data.insert(HASH_A.to_string(), cdd(1, 20, 5, 3, 0, &[("Go", ls(20, 5, 3))]));
        commit_dev_data.insert(
            HASH_B.to_string(),
            cdd(1, 10, 3, 2, 1, &[("Python", ls(10, 3, 2))]),
        );
        let mut commits_by_tick = BTreeMap::new();
        commits_by_tick.insert(0, vec![HASH_A.to_string()]);
        commits_by_tick.insert(1, vec![HASH_B.to_string()]);

        let input = parse_tick_data(
            &commit_dev_data,
            &commits_by_tick,
            vec!["Alice".to_string(), "Bob".to_string()],
            TICK_SIZE,
        );
        let computed = compute_all_metrics(&input, &MetricOptions::default());
        assert_eq!(computed.developers.len(), 2);
        assert_eq!(computed.aggregate.total_commits, 2);
        assert_eq!(computed.analyzer_name(), "devs");
    }

    // --- serialize: byte-shape sanity (struct/map origin) ---

    #[test]
    fn serialize_computed_metrics_is_object() {
        let m = ComputedMetrics {
            aggregate: AggregateData {
                total_commits: 10,
                ..AggregateData::default()
            },
            ..ComputedMetrics::default()
        };
        let gv = serialize::computed_metrics_to_go(&m);
        assert!(matches!(gv, GoValue::Map(_)));
    }

    #[test]
    fn serialize_language_contributors_decimal_string_keys_sorted() {
        // Go encodes map[int]int keys as decimal strings sorted lexically:
        // "10" < "2". Verify the JSON bytes reflect that.
        let mut contributors = BTreeMap::new();
        contributors.insert(2i64, 5i64);
        contributors.insert(10i64, 7i64);
        let ld = LanguageData {
            name: "Go".to_string(),
            total_lines: 12,
            total_contribution: 12,
            contributors,
        };
        let json = cf_gojson::marshal(&serialize::language_data_to_go(&ld));
        let s = String::from_utf8(json).unwrap();
        // "10" precedes "2" by byte order.
        let pos10 = s.find("\"10\"").unwrap();
        let pos2 = s.find("\"2\"").unwrap();
        assert!(pos10 < pos2, "got: {s}");
    }
}
