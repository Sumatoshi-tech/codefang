//! Framework integration sketch (NOT YET IMPLEMENTED).
//!
//! The streaming side of the analyzer — consume/fork/merge over the commit
//! walk, the aggregator and spill store, checkpointing, hibernation, and the
//! store writers/readers — depends on framework crates that are not yet stable
//! in this tree (`cf-analyze`, `cf-analyzers-plumbing`, `cf-spillstore`,
//! `cf-gitlib`, `cf-checkpoint`/`cf-persist`).
//!
//! Once those crates expose stable interfaces, the analyzer struct, aggregator,
//! checkpoint state and store writers can be completed here. The minimal
//! contracts that work will need are sketched below so downstream registration
//! code can reference them.

use crate::metrics::FileHistory;

/// Store record kinds written by the analyzer.
pub mod store_kinds {
    /// Per-file `FileChurnData` records (sorted by churn score).
    pub const FILE_CHURN: &str = "file_churn";
    /// Single `AggregateData` summary record.
    pub const SUMMARY: &str = "summary";
    /// Per-tick `CompositionTimeSeriesEntry` records.
    pub const COMPOSITION: &str = "composition";
}

/// Checkpoint basename.
pub const CHECKPOINT_BASENAME: &str = "file_history_state";

/// Estimated working-state bytes per commit.
pub const WORKING_STATE_SIZE: i64 = 2 * 1024;
/// Estimated TC payload bytes per commit.
pub const AVG_TC_SIZE: i64 = 10 * 1024;

/// Merges two [`FileHistory`] values: sums per-author line stats and appends
/// hash lists.
///
/// This is framework-adjacent but pure, so it is implemented here and unit
/// tested; the streaming aggregator will call it.
#[must_use]
pub fn merge_file_history(mut existing: FileHistory, incoming: FileHistory) -> FileHistory {
    for (author, stats) in incoming.people {
        let old = existing.people.entry(author).or_default();
        *old = *old + stats;
    }
    existing.hashes.extend(incoming.hashes);
    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tc::LineStats;
    use std::collections::BTreeMap;

    #[test]
    fn merge_sums_people_and_appends_hashes() {
        let a = FileHistory {
            people: BTreeMap::from([(1, LineStats { added: 10, ..Default::default() })]),
            hashes: vec!["h1".into()],
        };
        let b = FileHistory {
            people: BTreeMap::from([
                (1, LineStats { added: 5, removed: 2, ..Default::default() }),
                (2, LineStats { changed: 7, ..Default::default() }),
            ]),
            hashes: vec!["h2".into()],
        };
        let merged = merge_file_history(a, b);
        assert_eq!(merged.people[&1], LineStats { added: 15, removed: 2, changed: 0 });
        assert_eq!(merged.people[&2], LineStats { added: 0, removed: 0, changed: 7 });
        assert_eq!(merged.hashes, vec!["h1".to_string(), "h2".to_string()]);
    }
}
