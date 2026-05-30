//! Color-coded metric thresholds.
//!
//! Port of the Go `Thresholds = map[string]map[string]any` alias
//! (analyzer.go:75): a map from metric name to a map of band → value, e.g.
//! `{"complexity": {"red": 30, "yellow": 15, "green": 5}}`.

use std::collections::BTreeMap;

use cf_gojson::GoValue;

/// Color-coded thresholds for multiple metrics.
///
/// Port of Go `Thresholds`. A `BTreeMap` keyed by metric name reproduces Go's
/// `map[string]...` byte-sorted ordering when serialized.
pub type Thresholds = BTreeMap<String, BTreeMap<String, GoValue>>;
