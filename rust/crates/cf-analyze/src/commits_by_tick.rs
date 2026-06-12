//! Tick → commit-hashes index construction.
//!
//! The generic
//! [`build_commits_by_tick`] takes an `extract` callback that inspects
//! `tick.Data`; in Rust the extractor is a closure returning the per-tick
//! commit-keyed map (or `None` when the data is not the expected shape).

use crate::tc::{CommitHash, Tick};

/// Builds a map from tick index to the commit hashes recorded in that tick.
///
/// `extract` inspects a
/// tick's `Data` and returns the commit-hash-keyed entries, or `None`/empty to
/// skip the tick. Within a tick the hashes follow the iteration order the
/// extractor yields; ticks accumulate in input order.
///
/// The result is returned as ordered `(tick, hashes)` pairs (a `Vec`) rather
/// than a hash map, since the only consumer needs deterministic iteration.
#[must_use]
pub fn build_commits_by_tick<F>(ticks: &[Tick], extract: F) -> Vec<(i32, Vec<CommitHash>)>
where
    F: Fn(&Tick) -> Option<Vec<String>>,
{
    let mut out: Vec<(i32, Vec<CommitHash>)> = Vec::new();

    for tick in ticks {
        let Some(hashes) = extract(tick) else {
            continue;
        };
        if hashes.is_empty() {
            continue;
        }
        let converted: Vec<CommitHash> = hashes.into_iter().map(CommitHash::new).collect();

        if let Some(entry) = out.iter_mut().find(|(t, _)| *t == tick.tick) {
            entry.1.extend(converted);
        } else {
            out.push((tick.tick, converted));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(n: i32) -> Tick {
        Tick {
            tick: n,
            ..Default::default()
        }
    }

    #[test]
    fn collects_hashes_per_tick() {
        let ticks = vec![tick(0), tick(1)];
        let ct = build_commits_by_tick(&ticks, |t| {
            Some(vec![format!("hash{}", t.tick)])
        });
        assert_eq!(ct.len(), 2);
        assert_eq!(ct[0].0, 0);
        assert_eq!(ct[0].1[0].as_str(), "hash0");
        assert_eq!(ct[1].1[0].as_str(), "hash1");
    }

    #[test]
    fn skips_ticks_with_no_data() {
        let ticks = vec![tick(0), tick(1)];
        let ct = build_commits_by_tick(&ticks, |t| {
            if t.tick == 0 {
                Some(vec!["a".into()])
            } else {
                None
            }
        });
        assert_eq!(ct.len(), 1);
        assert_eq!(ct[0].0, 0);
    }

    #[test]
    fn skips_empty_maps() {
        let ticks = vec![tick(0)];
        let ct = build_commits_by_tick(&ticks, |_t| Some(vec![]));
        assert!(ct.is_empty());
    }

    #[test]
    fn merges_same_tick_index() {
        let ticks = vec![tick(0), tick(0)];
        let ct = build_commits_by_tick(&ticks, |_t| Some(vec!["x".into()]));
        assert_eq!(ct.len(), 1);
        assert_eq!(ct[0].1.len(), 2);
    }
}
