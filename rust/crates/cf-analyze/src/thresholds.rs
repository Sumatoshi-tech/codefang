//! Color-coded metric thresholds.
//!
//! The `Thresholds` alias
//!: a map from metric name to a map of band → value, e.g.
//! `{"complexity": {"red": 30, "yellow": 15, "green": 5}}`.

use std::collections::BTreeMap;

use cf_gojson::GoValue;

/// Color-coded thresholds for multiple metrics.
///
/// A `BTreeMap` keyed by metric name reproduces the contract's
/// `map[string]...` byte-sorted ordering when serialized.
pub type Thresholds = BTreeMap<String, BTreeMap<String, GoValue>>;
