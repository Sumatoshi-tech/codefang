//! Per-tick aggregation of per-commit anomaly data.

use std::collections::BTreeMap;

use crate::model::{CommitAnomalyData, TickMetrics};

/// Builds per-tick metrics from per-commit data grouped by the
/// `commits_by_tick` mapping (commit hash hex -> commit data;
/// tick -> ordered list of commit hashes).
///
/// Returns an empty map when either input is empty.
///
/// `commits_by_tick` preserves the per-tick hash ordering via a `Vec`; the
/// outer `BTreeMap` keys (ticks) are iterated in ascending order, matching
/// the deterministic sorted-key consumption downstream.
///
/// ```
/// use cf_anomaly::aggregate::aggregate_commits_to_ticks;
/// use cf_anomaly::model::CommitAnomalyData;
/// use std::collections::BTreeMap;
///
/// let commit_metrics = BTreeMap::from([
///     ("aaa".to_string(), CommitAnomalyData {
///         files_changed: 3, lines_added: 20, lines_removed: 5, author_id: 1,
///         ..Default::default()
///     }),
///     ("bbb".to_string(), CommitAnomalyData {
///         files_changed: 2, lines_added: 10, lines_removed: 3, author_id: 2,
///         ..Default::default()
///     }),
/// ]);
/// // Tick 0 references both commits plus a missing "ccc" (skipped).
/// let commits_by_tick = BTreeMap::from([(0i64, vec![
///     "aaa".to_string(), "bbb".to_string(), "ccc".to_string(),
/// ])]);
///
/// let ticks = aggregate_commits_to_ticks(&commit_metrics, &commits_by_tick);
/// let tm = &ticks[&0];
/// assert_eq!(tm.files_changed, 5);                 // additive merge
/// assert_eq!(tm.net_churn, 30 - 8);                // added - removed
/// assert_eq!(tm.author_ids.len(), 2);              // distinct authors
///
/// // Either input empty → empty result.
/// assert!(aggregate_commits_to_ticks(&BTreeMap::new(), &commits_by_tick).is_empty());
/// ```
#[must_use]
pub fn aggregate_commits_to_ticks(
    commit_metrics: &BTreeMap<String, CommitAnomalyData>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
) -> BTreeMap<i64, TickMetrics> {
    if commit_metrics.is_empty() || commits_by_tick.is_empty() {
        return BTreeMap::new();
    }

    let mut result = BTreeMap::new();

    for (tick, hashes) in commits_by_tick {
        if let Some(mut tm) = aggregate_tick_from_commits(hashes, commit_metrics) {
            tm.net_churn = tm.lines_added - tm.lines_removed;
            result.insert(*tick, tm);
        }
    }

    result
}

/// Merges commit-level anomaly data for a single tick. Returns `None` when no
/// listed commit hash is present in `commit_metrics` (the tick is then
/// dropped from the result map).
fn aggregate_tick_from_commits(
    hashes: &[String],
    commit_metrics: &BTreeMap<String, CommitAnomalyData>,
) -> Option<TickMetrics> {
    let mut tm: Option<TickMetrics> = None;

    for hash in hashes {
        let Some(cm) = commit_metrics.get(hash) else {
            continue;
        };

        let acc = tm.get_or_insert_with(TickMetrics::default);

        acc.files_changed += cm.files_changed;
        acc.lines_added += cm.lines_added;
        acc.lines_removed += cm.lines_removed;
        acc.files.extend(cm.files.iter().cloned());

        // Sum language counts additively across commits.
        for (lang, count) in &cm.languages {
            *acc.languages.entry(lang.clone()).or_insert(0) += count;
        }

        // Unique author IDs.
        acc.author_ids.insert(cm.author_id);
    }

    tm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(c: char) -> String {
        std::iter::repeat(c).take(40).collect()
    }

    #[test]
    fn basic_aggregation() {
        // Mirrors reference test TestAggregateCommitsToTicks_Basic.
        let mut commit_metrics = BTreeMap::new();
        commit_metrics.insert(
            hash('a'),
            CommitAnomalyData {
                files_changed: 3,
                lines_added: 20,
                lines_removed: 5,
                files: vec!["a.go".into(), "b.go".into(), "c.go".into()],
                languages: BTreeMap::from([("Go".to_string(), 3)]),
                author_id: 1,
                ..Default::default()
            },
        );
        commit_metrics.insert(
            hash('b'),
            CommitAnomalyData {
                files_changed: 2,
                lines_added: 10,
                lines_removed: 3,
                files: vec!["d.go".into(), "e.go".into()],
                languages: BTreeMap::from([("Go".to_string(), 1), ("Python".to_string(), 1)]),
                author_id: 2,
                ..Default::default()
            },
        );
        let commits_by_tick = BTreeMap::from([(0_i64, vec![hash('a'), hash('b')])]);

        let result = aggregate_commits_to_ticks(&commit_metrics, &commits_by_tick);
        assert_eq!(result.len(), 1);

        let tm = &result[&0];
        assert_eq!(tm.files_changed, 5);
        assert_eq!(tm.lines_added, 30);
        assert_eq!(tm.lines_removed, 8);
        assert_eq!(tm.net_churn, 22);
        assert_eq!(tm.files.len(), 5);
        assert_eq!(tm.languages["Go"], 4);
        assert_eq!(tm.languages["Python"], 1);
        assert_eq!(tm.author_ids.len(), 2);
    }

    #[test]
    fn multiple_ticks() {
        // Mirrors reference test TestAggregateCommitsToTicks_MultipleTicks.
        let mut commit_metrics = BTreeMap::new();
        commit_metrics.insert(
            hash('a'),
            CommitAnomalyData { files_changed: 3, lines_added: 20, lines_removed: 5, author_id: 1, ..Default::default() },
        );
        commit_metrics.insert(
            hash('b'),
            CommitAnomalyData { files_changed: 2, lines_added: 10, lines_removed: 3, author_id: 2, ..Default::default() },
        );
        let commits_by_tick = BTreeMap::from([(0_i64, vec![hash('a')]), (1_i64, vec![hash('b')])]);

        let result = aggregate_commits_to_ticks(&commit_metrics, &commits_by_tick);
        assert_eq!(result.len(), 2);
        assert_eq!(result[&0].files_changed, 3);
        assert_eq!(result[&1].files_changed, 2);
    }

    #[test]
    fn empty_inputs() {
        // Mirrors reference test TestAggregateCommitsToTicks_EmptyInputs.
        let empty_cm: BTreeMap<String, CommitAnomalyData> = BTreeMap::new();
        let some_tick = BTreeMap::from([(0_i64, Vec::<String>::new())]);
        assert!(aggregate_commits_to_ticks(&empty_cm, &some_tick).is_empty());

        let cm = BTreeMap::from([("a".to_string(), CommitAnomalyData::default())]);
        let empty_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        assert!(aggregate_commits_to_ticks(&cm, &empty_tick).is_empty());
    }

    #[test]
    fn missing_commit_is_skipped() {
        // Mirrors reference test TestAggregateCommitsToTicks_MissingCommit.
        let mut commit_metrics = BTreeMap::new();
        commit_metrics.insert(
            hash('a'),
            CommitAnomalyData { files_changed: 3, lines_added: 20, lines_removed: 5, author_id: 1, ..Default::default() },
        );
        let commits_by_tick = BTreeMap::from([(0_i64, vec![hash('a'), hash('c')])]);

        let result = aggregate_commits_to_ticks(&commit_metrics, &commits_by_tick);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0].files_changed, 3);
    }
}
