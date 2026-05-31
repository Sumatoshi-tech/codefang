//! Per-item data collection with deduplication.
//!
//! Port of `spillable_data_collector.go`. [`SpillableDataCollector`] collects
//! per-item data keyed by an identifier with last-write-wins deduplication, and
//! returns it sorted by the identifier key.
//!
//! # Spill-to-disk
//!
//! The Go collector transparently spills its in-memory buffer to numbered
//! `encoding/gob` files when it exceeds a threshold. Per DESIGN §3, **gob is
//! dropped** in the Rust port (it is Go-specific and not byte-portable). The
//! spill mechanism is reproduced here as a behaviour-equivalent **in-memory**
//! overflow store: when the threshold is crossed the live buffer is flushed into
//! an overflow list ([`SpillCount`] counts the flushes) and merged back in
//! [`SpillableDataCollector::get_sorted_data`] with the same last-write-wins
//! semantics. Spilling never affects report output bytes, so this is
//! behaviour-faithful. A future on-disk implementation should use a Rust-native
//! codec (bincode/postcard), not gob.
//!
//! [`SpillCount`]: SpillableDataCollector::spill_count

use crate::report::{AggregationMode, Item, Report, Value};
use std::collections::BTreeMap;

/// Default number of items before spilling, matching the Go
/// `defaultSpillThreshold`.
pub const DEFAULT_SPILL_THRESHOLD: usize = 10_000;

/// Average estimated per-item memory, matching the Go `estimatedItemBytes`.
const ESTIMATED_ITEM_BYTES: i64 = 512;

/// Separator joining composite identifier values, matching the Go
/// `compositeKeySeparator`.
const COMPOSITE_KEY_SEPARATOR: &str = ":";

/// Collects per-item data keyed by identifier with last-write-wins dedup.
///
/// Mirrors the Go `SpillableDataCollector`.
#[derive(Debug, Clone)]
pub struct SpillableDataCollector {
    buffer: BTreeMap<String, Item>,
    overflow: Vec<BTreeMap<String, Item>>,
    collection_key: String,
    identifier_key: String,
    identifier_keys: Vec<String>,
    mode: AggregationMode,
    spill_n: usize,
    spill_threshold: usize,
}

impl SpillableDataCollector {
    /// Creates a single-key collector that spills when the buffer reaches
    /// `threshold` items (0 disables spilling). Mirrors
    /// `NewSpillableDataCollector`.
    pub fn new(collection_key: &str, identifier_key: &str, threshold: usize) -> Self {
        SpillableDataCollector {
            buffer: BTreeMap::new(),
            overflow: Vec::new(),
            collection_key: collection_key.to_string(),
            identifier_key: identifier_key.to_string(),
            identifier_keys: Vec::new(),
            mode: AggregationMode::default(),
            spill_n: 0,
            spill_threshold: threshold,
        }
    }

    /// Creates a composite-key collector. The last key is the primary
    /// identifier (used for sorting and [`get_identifier_key`]); earlier keys
    /// are joined with `:` to form the dedup key. Mirrors
    /// `NewSpillableDataCollectorComposite`.
    ///
    /// [`get_identifier_key`]: SpillableDataCollector::get_identifier_key
    pub fn new_composite(collection_key: &str, identifier_keys: &[String], threshold: usize) -> Self {
        let primary_key = identifier_keys.last().cloned().unwrap_or_default();
        SpillableDataCollector {
            buffer: BTreeMap::new(),
            overflow: Vec::new(),
            collection_key: collection_key.to_string(),
            identifier_key: primary_key,
            identifier_keys: identifier_keys.to_vec(),
            mode: AggregationMode::default(),
            spill_n: 0,
            spill_threshold: threshold,
        }
    }

    /// Sets the aggregation mode. Mirrors `SetAggregationMode`.
    pub fn set_aggregation_mode(&mut self, mode: AggregationMode) {
        self.mode = mode;
    }

    /// Sets the spill threshold. Mirrors the field write done by
    /// `Aggregator.SetSpillThreshold`.
    pub fn set_spill_threshold(&mut self, threshold: usize) {
        self.spill_threshold = threshold;
    }

    /// Extracts per-item data from a report. No-op in
    /// [`AggregationMode::SummaryOnly`]. Mirrors `CollectFromReport`.
    pub fn collect_from_report(&mut self, report: &Report) {
        if self.mode == AggregationMode::SummaryOnly {
            return;
        }

        let collection = match self.extract_collection(report) {
            Some(c) => c,
            None => return,
        };

        for item in collection {
            let identifier = self.extract_identifier(&item);
            if identifier.is_empty() {
                continue;
            }
            self.buffer.insert(identifier, item);
        }

        self.spill_if_needed();
    }

    /// Extracts the collection slice for this collector's key from a report.
    fn extract_collection(&self, report: &Report) -> Option<Vec<Item>> {
        match report.get(&self.collection_key) {
            Some(Value::Collection(items)) => Some(items.clone()),
            _ => None,
        }
    }

    /// Returns all collected items (buffer + overflow) merged with last-write-
    /// wins and sorted by the identifier key. Resets the collector afterwards.
    /// Mirrors `GetSortedData`.
    pub fn get_sorted_data(&mut self) -> Vec<Item> {
        let merged = self.merge_all();
        let mut data: Vec<Item> = merged.into_values().collect();
        data.sort_by(|a, b| {
            let na = extract_string_key(a, &self.identifier_key);
            let nb = extract_string_key(b, &self.identifier_key);
            na.cmp(&nb)
        });
        self.buffer = BTreeMap::new();
        self.overflow.clear();
        self.spill_n = 0;
        data
    }

    /// Returns the number of items in the live in-memory buffer (excludes
    /// spilled items). Mirrors `GetDataCount`.
    pub fn get_data_count(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the collection key. Mirrors `GetCollectionKey`.
    pub fn get_collection_key(&self) -> &str {
        &self.collection_key
    }

    /// Returns the identifier key. Mirrors `GetIdentifierKey`.
    pub fn get_identifier_key(&self) -> &str {
        &self.identifier_key
    }

    /// Returns the number of spill flushes. Mirrors `SpillCount`.
    pub fn spill_count(&self) -> usize {
        self.spill_n
    }

    /// Estimates the in-memory buffer size in bytes. Mirrors
    /// `EstimatedBufferBytes`.
    pub fn estimated_buffer_bytes(&self) -> i64 {
        self.buffer.len() as i64 * ESTIMATED_ITEM_BYTES
    }

    /// Flushes the buffer into the overflow store if it exceeds the threshold.
    fn spill_if_needed(&mut self) {
        if self.spill_threshold == 0 || self.buffer.len() < self.spill_threshold {
            return;
        }
        if self.buffer.is_empty() {
            return;
        }
        self.overflow.push(std::mem::take(&mut self.buffer));
        self.spill_n += 1;
    }

    /// Merges overflow flushes and the live buffer, last-write-wins.
    fn merge_all(&self) -> BTreeMap<String, Item> {
        let mut result: BTreeMap<String, Item> = BTreeMap::new();
        for chunk in &self.overflow {
            for (k, v) in chunk {
                result.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &self.buffer {
            result.insert(k.clone(), v.clone());
        }
        result
    }

    /// Builds the dedup key for an item (single or composite). Mirrors
    /// `extractIdentifier`.
    fn extract_identifier(&self, item: &Item) -> String {
        if !self.identifier_keys.is_empty() {
            return self.build_composite_key(item);
        }
        match item.get(&self.identifier_key) {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        }
    }

    /// Joins composite identifier values with `:`; the last key is required and
    /// earlier keys are optional. Mirrors `buildCompositeKey`.
    fn build_composite_key(&self, item: &Item) -> String {
        let last_idx = self.identifier_keys.len() - 1;
        let last_val = match item.get(&self.identifier_keys[last_idx]) {
            Some(Value::String(s)) => s.clone(),
            _ => return String::new(),
        };
        if last_idx == 0 {
            return last_val;
        }
        let mut b = String::new();
        for k in &self.identifier_keys[..last_idx] {
            if let Some(Value::String(v)) = item.get(k) {
                b.push_str(v);
                b.push_str(COMPOSITE_KEY_SEPARATOR);
            }
        }
        b.push_str(&last_val);
        b
    }
}

/// Safely extracts a string value from an item, returning `""` if absent or not
/// a string. Mirrors the Go `extractStringKey`.
fn extract_string_key(item: &Item, key: &str) -> String {
    match item.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(items: Vec<Item>) -> Report {
        let mut r = Report::new();
        r.insert("items".into(), Value::Collection(items));
        r
    }

    fn item(pairs: &[(&str, Value)]) -> Item {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // Ported from spillable_data_collector_test.go: TestSpillableDataCollector_Basic
    #[test]
    fn basic() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.collect_from_report(&report(vec![
            item(&[("name", Value::String("item1".into())), ("value", Value::Int(10))]),
            item(&[("name", Value::String("item2".into())), ("value", Value::Int(20))]),
        ]));
        assert_eq!(dc.get_data_count(), 2);
    }

    // Ported from: TestSpillableDataCollector_Dedup
    #[test]
    fn dedup_last_write_wins() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.collect_from_report(&report(vec![
            item(&[("name", Value::String("item1".into())), ("value", Value::Int(10))]),
            item(&[("name", Value::String("item1".into())), ("value", Value::Int(20))]),
        ]));
        assert_eq!(dc.get_data_count(), 1);
        let data = dc.get_sorted_data();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].get("value"), Some(&Value::Int(20)));
    }

    // Ported from: TestSpillableDataCollector_SortedData
    #[test]
    fn sorted_data() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.collect_from_report(&report(vec![
            item(&[("name", Value::String("zebra".into()))]),
            item(&[("name", Value::String("alpha".into()))]),
            item(&[("name", Value::String("mango".into()))]),
        ]));
        let data = dc.get_sorted_data();
        assert_eq!(data[0].get("name"), Some(&Value::String("alpha".into())));
        assert_eq!(data[1].get("name"), Some(&Value::String("mango".into())));
        assert_eq!(data[2].get("name"), Some(&Value::String("zebra".into())));
    }

    // Ported from: TestSpillableDataCollector_Spilling
    #[test]
    fn spilling() {
        let mut dc = SpillableDataCollector::new("items", "name", 2);
        dc.collect_from_report(&report(vec![
            item(&[("name", Value::String("item1".into()))]),
            item(&[("name", Value::String("item2".into()))]),
            item(&[("name", Value::String("item3".into()))]),
        ]));
        assert!(dc.spill_count() > 0);
        let data = dc.get_sorted_data();
        assert_eq!(data.len(), 3);
    }

    // Ported from: TestSpillableDataCollector_CompositeKey
    #[test]
    fn composite_key() {
        let mut dc = SpillableDataCollector::new_composite(
            "items",
            &["_source_file".to_string(), "name".to_string()],
            0,
        );
        dc.collect_from_report(&report(vec![
            item(&[
                ("_source_file", Value::String("a.go".into())),
                ("name", Value::String("foo".into())),
            ]),
            item(&[
                ("_source_file", Value::String("b.go".into())),
                ("name", Value::String("foo".into())),
            ]),
        ]));
        assert_eq!(dc.get_data_count(), 2);
    }

    // Ported from: TestSpillableDataCollector_GetCollectionKey / GetIdentifierKey
    #[test]
    fn keys() {
        let dc = SpillableDataCollector::new("mykey", "myid", 0);
        assert_eq!(dc.get_collection_key(), "mykey");
        assert_eq!(dc.get_identifier_key(), "myid");
    }

    // Ported from: TestSpillableDataCollector_EmptyReport
    #[test]
    fn empty_report() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.collect_from_report(&Report::new());
        assert_eq!(dc.get_data_count(), 0);
    }

    // Ported from: TestSpillableDataCollector_MissingIdentifier
    #[test]
    fn missing_identifier() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.collect_from_report(&report(vec![item(&[("value", Value::Int(10))])]));
        assert_eq!(dc.get_data_count(), 0);
    }

    // Ported from: TestSpillableDataCollector_AggregationModeSummaryOnly
    #[test]
    fn summary_only_mode() {
        let mut dc = SpillableDataCollector::new("items", "name", 0);
        dc.set_aggregation_mode(AggregationMode::SummaryOnly);
        dc.collect_from_report(&report(vec![item(&[("name", Value::String("item1".into()))])]));
        assert_eq!(dc.get_data_count(), 0);
    }
}
