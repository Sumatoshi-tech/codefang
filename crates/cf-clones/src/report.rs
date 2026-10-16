//! The clone-detection report model: clone types, the [`ClonePair`] record,
//! the per-type distribution, the per-file summaries, and the [`CloneReport`]
//! the aggregator produces — plus its two machine projections (the raw
//! `analyze.Report` map and the YAML/binary metrics struct).

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::{
    KEY_ANALYZER_NAME, KEY_CLONE_PAIRS, KEY_CLONE_RATIO, KEY_CLONE_TYPE_DISTRIBUTION, KEY_MESSAGE,
    KEY_TOTAL_CLONE_PAIRS, KEY_TOTAL_FUNCTIONS,
};

/// Wire string of Type-1 (exact) clones.
pub const CLONE_TYPE1: &str = "Type-1";
/// Wire string of Type-2 (renamed) clones.
pub const CLONE_TYPE2: &str = "Type-2";
/// Wire string of Type-3 (near-miss) clones.
pub const CLONE_TYPE3: &str = "Type-3";

/// Display label for Type-1 clones.
pub const DIST_LABEL_TYPE1: &str = "Type-1 (Exact)";
/// Display label for Type-2 clones.
pub const DIST_LABEL_TYPE2: &str = "Type-2 (Renamed)";
/// Display label for Type-3 clones.
pub const DIST_LABEL_TYPE3: &str = "Type-3 (Near-miss)";

/// The clone taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CloneType {
    /// Identical structure and identical tokens.
    Type1,
    /// Identical structure, renamed identifiers / literals.
    Type2,
    /// Modified structure above the similarity threshold.
    Type3,
}

impl CloneType {
    /// The wire string (`Type-1` / `Type-2` / `Type-3`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CloneType::Type1 => CLONE_TYPE1,
            CloneType::Type2 => CLONE_TYPE2,
            CloneType::Type3 => CLONE_TYPE3,
        }
    }

    /// The display label (`Type-1 (Exact)` / …).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            CloneType::Type1 => DIST_LABEL_TYPE1,
            CloneType::Type2 => DIST_LABEL_TYPE2,
            CloneType::Type3 => DIST_LABEL_TYPE3,
        }
    }
}

/// A detected clone relationship between two functions.
///
/// `func_a` sorts before `func_b` (lexicographic), so the pair is a canonical
/// unordered pair. Field order is the JSON/YAML key order:
/// `func_a, func_b, similarity, clone_type`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClonePair {
    /// First function name (`func_a`).
    pub func_a: String,
    /// Second function name (`func_b`).
    pub func_b: String,
    /// Exact shingle-set Jaccard of the pair (`1.0` for identical structure).
    pub similarity: f64,
    /// Classified clone type.
    pub clone_type: CloneType,
}

impl ClonePair {
    /// Builds the struct-origin [`GoValue`] for this pair, emitting fields in
    /// declaration order (`func_a, func_b, similarity, clone_type`).
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("func_a", GoValue::Str(self.func_a.clone()));
        m.push("func_b", GoValue::Str(self.func_b.clone()));
        m.push("similarity", GoValue::Float(self.similarity));
        m.push(
            "clone_type",
            GoValue::Str(self.clone_type.as_str().to_string()),
        );
        GoValue::Map(m)
    }

    /// `func_a <-> func_b`, the issue name shown in reports.
    #[must_use]
    pub fn display_name(&self) -> String {
        format!("{} <-> {}", self.func_a, self.func_b)
    }
}

/// Per-clone-type counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloneTypeCounts {
    /// Type-1 (exact) count.
    pub type1: i64,
    /// Type-2 (renamed) count.
    pub type2: i64,
    /// Type-3 (near-miss) count.
    pub type3: i64,
}

impl CloneTypeCounts {
    /// Increments the counter for `clone_type`.
    pub fn increment(&mut self, clone_type: CloneType) {
        match clone_type {
            CloneType::Type1 => self.type1 += 1,
            CloneType::Type2 => self.type2 += 1,
            CloneType::Type3 => self.type3 += 1,
        }
    }

    /// The count for one clone type.
    #[must_use]
    pub const fn get(&self, clone_type: CloneType) -> i64 {
        match clone_type {
            CloneType::Type1 => self.type1,
            CloneType::Type2 => self.type2,
            CloneType::Type3 => self.type3,
        }
    }

    /// Total across all three types.
    #[must_use]
    pub const fn total(&self) -> i64 {
        self.type1 + self.type2 + self.type3
    }

    /// Builds the `clone_type_distribution` map (map-origin: keys byte-sort
    /// on encode).
    #[must_use]
    pub fn to_go_value(self) -> GoValue {
        let mut m = GoMap::new_map();
        m.push(CLONE_TYPE1, GoValue::Int(self.type1));
        m.push(CLONE_TYPE2, GoValue::Int(self.type2));
        m.push(CLONE_TYPE3, GoValue::Int(self.type3));
        GoValue::Map(m)
    }
}

/// One analyzed file's share of the clone report (the `--per-file` entry
/// source).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSummary {
    /// Path relative to the analyzed root.
    pub path: String,
    /// Function-like nodes found in the file (closures included).
    pub total_functions: i64,
    /// Functions of this file that take part in at least one clone pair.
    pub cloned_functions: i64,
    /// Indices into [`CloneReport::pairs`] of the stored pairs that involve a
    /// function of this file.
    pub pairs: Vec<usize>,
}

/// The aggregated clone-detection report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloneReport {
    /// Total function-like nodes analyzed (closures included).
    pub total_functions: i64,
    /// Exact number of clone pairs found (not capped).
    pub total_clone_pairs: i64,
    /// Share of functions in at least one pair, in `[0, 1]`.
    pub clone_ratio: f64,
    /// Exact per-type distribution over every pair found.
    pub distribution: CloneTypeCounts,
    /// The stored pairs, ordered by similarity descending then by name
    /// (capped at the aggregator's `max_clone_pairs`).
    pub pairs: Vec<ClonePair>,
    /// Status message keyed off `clone_ratio`.
    pub message: String,
    /// Per-file summaries in walk order.
    pub files: Vec<FileSummary>,
}

impl CloneReport {
    /// Builds the raw `analyze.Report` map (map-origin: keys byte-sort on
    /// encode) that `report.json` / the plot pages consume.
    #[must_use]
    pub fn to_raw_report(&self) -> GoMap {
        let mut report = GoMap::new(MapOrigin::Map);
        report.push(
            KEY_ANALYZER_NAME,
            GoValue::Str(crate::ANALYZER_NAME.to_string()),
        );
        report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(self.total_functions));
        report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(self.total_clone_pairs));
        report.push(KEY_CLONE_RATIO, GoValue::Float(self.clone_ratio));
        report.push(KEY_CLONE_PAIRS, self.pairs_value());
        report.push(KEY_CLONE_TYPE_DISTRIBUTION, self.distribution.to_go_value());
        report.push(KEY_MESSAGE, GoValue::Str(self.message.clone()));
        report
    }

    /// Builds the struct-origin metrics value used by the YAML and binary
    /// formats, in declaration order: `total_functions, total_clone_pairs,
    /// clone_ratio, clone_type_distribution, clone_pairs, message`.
    #[must_use]
    pub fn to_metrics_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(self.total_functions));
        m.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(self.total_clone_pairs));
        m.push(KEY_CLONE_RATIO, GoValue::Float(self.clone_ratio));
        m.push(KEY_CLONE_TYPE_DISTRIBUTION, self.distribution.to_go_value());
        m.push(KEY_CLONE_PAIRS, self.pairs_value());
        m.push(KEY_MESSAGE, GoValue::Str(self.message.clone()));
        GoValue::Map(m)
    }

    fn pairs_value(&self) -> GoValue {
        GoValue::Array(self.pairs.iter().map(ClonePair::to_go_value).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::Encoder;

    fn pair(a: &str, b: &str, similarity: f64, clone_type: CloneType) -> ClonePair {
        ClonePair {
            func_a: a.into(),
            func_b: b.into(),
            similarity,
            clone_type,
        }
    }

    #[test]
    fn clone_type_strings_and_labels() {
        assert_eq!(CloneType::Type1.as_str(), "Type-1");
        assert_eq!(CloneType::Type2.label(), "Type-2 (Renamed)");
        assert_eq!(CloneType::Type3.label(), "Type-3 (Near-miss)");
    }

    #[test]
    fn counts_increment_and_total() {
        let mut c = CloneTypeCounts::default();
        c.increment(CloneType::Type1);
        c.increment(CloneType::Type3);
        c.increment(CloneType::Type3);
        assert_eq!(c.get(CloneType::Type1), 1);
        assert_eq!(c.get(CloneType::Type2), 0);
        assert_eq!(c.get(CloneType::Type3), 2);
        assert_eq!(c.total(), 3);
    }

    #[test]
    fn clone_pair_json_field_order() {
        let p = pair("A", "B", 0.75, CloneType::Type3);
        let json = Encoder::marshal().encode_to_string(&p.to_go_value());
        assert_eq!(
            json,
            r#"{"func_a":"A","func_b":"B","similarity":0.75,"clone_type":"Type-3"}"#
        );
        assert_eq!(p.display_name(), "A <-> B");
    }

    #[test]
    fn metrics_value_field_order() {
        let mut distribution = CloneTypeCounts::default();
        distribution.increment(CloneType::Type2);
        let report = CloneReport {
            total_functions: 4,
            total_clone_pairs: 1,
            clone_ratio: 0.5,
            distribution,
            pairs: vec![pair("A", "B", 1.0, CloneType::Type2)],
            message: crate::MSG_HIGH_CLONES.into(),
            files: Vec::new(),
        };
        let json = Encoder::marshal().encode_to_string(&report.to_metrics_value());
        assert_eq!(
            json,
            concat!(
                r#"{"total_functions":4,"total_clone_pairs":1,"clone_ratio":0.5,"#,
                r#""clone_type_distribution":{"Type-1":0,"Type-2":1,"Type-3":0},"#,
                r#""clone_pairs":[{"func_a":"A","func_b":"B","similarity":1,"clone_type":"Type-2"}],"#,
                r#""message":"High duplication - significant refactoring recommended"}"#
            )
        );
    }

    #[test]
    fn raw_report_sorts_keys_and_names_the_analyzer() {
        let report = CloneReport {
            message: crate::MSG_NO_FUNCTIONS.into(),
            ..CloneReport::default()
        };
        let json = Encoder::marshal().encode_to_string(&GoValue::Map(report.to_raw_report()));
        assert_eq!(
            json,
            concat!(
                r#"{"analyzer_name":"clones","clone_pairs":[],"clone_ratio":0,"#,
                r#""clone_type_distribution":{"Type-1":0,"Type-2":0,"Type-3":0},"#,
                r#""message":"No functions found for clone analysis","#,
                r#""total_clone_pairs":0,"total_functions":0}"#
            )
        );
    }
}
