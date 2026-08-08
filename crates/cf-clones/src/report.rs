//! Clone-type classification and the [`ComputedMetrics`] machine-export
//! struct: the clone types, the [`ClonePair`] record, the similarity-threshold
//! classifier, the clone-type distribution counts, and the
//! [`ComputedMetrics`] projection used by the JSON/YAML/binary export path.

use cf_gojson::{GoMap, GoValue};

/// Exact clone (identical AST structure and tokens).
pub const CLONE_TYPE1: &str = "Type-1";
/// Renamed clone (identical AST structure, different tokens).
pub const CLONE_TYPE2: &str = "Type-2";
/// Near-miss clone (similar AST structure).
pub const CLONE_TYPE3: &str = "Type-3";

/// Threshold for Type-1 (exact) clones.
pub const SIMILARITY_EXACT: f64 = 1.0;
/// Minimum threshold for Type-2 (renamed) clones.
pub const SIMILARITY_TYPE2: f64 = 0.8;
/// Minimum threshold for Type-3 (near-miss) clones, and the minimum
/// similarity for a pair to be reported at all.
pub const SIMILARITY_TYPE3: f64 = 0.5;

/// Maximum number of clone pairs stored in the report detail. The
/// `total_clone_pairs` count remains exact (not capped). Zero means
/// unlimited.
pub const DEFAULT_MAX_CLONE_PAIRS: usize = 1000;

/// A detected clone relationship between two functions.
///
/// Field order is the JSON/YAML key order (report contract):
/// `func_a, func_b, similarity, clone_type`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClonePair {
    /// First function name (`func_a`).
    pub func_a: String,
    /// Second function name (`func_b`).
    pub func_b: String,
    /// Estimated `MinHash` similarity (`similarity`).
    pub similarity: f64,
    /// Classified clone type (`clone_type`).
    pub clone_type: String,
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
        m.push("clone_type", GoValue::Str(self.clone_type.clone()));
        GoValue::Object(m)
    }
}

/// Determines the clone type from a similarity score: descending thresholds
/// `{1.0 -> Type-1, 0.8 -> Type-2}` with default `Type-3`; the first threshold
/// whose limit `similarity` meets (`>=`) wins.
///
/// ```
/// use cf_clones::{classify_clone_type, CLONE_TYPE1, CLONE_TYPE2, CLONE_TYPE3};
/// assert_eq!(classify_clone_type(1.0), CLONE_TYPE1);
/// assert_eq!(classify_clone_type(0.8), CLONE_TYPE2);
/// assert_eq!(classify_clone_type(0.79), CLONE_TYPE3);
/// ```
#[must_use]
pub fn classify_clone_type(similarity: f64) -> &'static str {
    if similarity >= SIMILARITY_EXACT {
        CLONE_TYPE1
    } else if similarity >= SIMILARITY_TYPE2 {
        CLONE_TYPE2
    } else {
        CLONE_TYPE3
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
    pub fn increment(&mut self, clone_type: &str) {
        match clone_type {
            CLONE_TYPE1 => self.type1 += 1,
            CLONE_TYPE2 => self.type2 += 1,
            CLONE_TYPE3 => self.type3 += 1,
            _ => {}
        }
    }

    /// Total across all three types.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.type1 + self.type2 + self.type3
    }
}

/// Counts clone pairs by type.
#[must_use]
pub fn categorize_clone_pairs(pairs: &[ClonePair]) -> CloneTypeCounts {
    let mut counts = CloneTypeCounts::default();
    for p in pairs {
        counts.increment(&p.clone_type);
    }
    counts
}

/// Builds the `clone_type_distribution` map (map-origin: keys byte-sorted on
/// encode).
#[must_use]
pub fn clone_type_dist_map(c: CloneTypeCounts) -> GoValue {
    // Map-origin: keys byte-sort on encode (Type-1 < Type-2 < Type-3 anyway).
    let mut m = GoMap::new_map();
    m.push(CLONE_TYPE1, GoValue::Int(c.type1));
    m.push(CLONE_TYPE2, GoValue::Int(c.type2));
    m.push(CLONE_TYPE3, GoValue::Int(c.type3));
    GoValue::Object(m)
}

/// Computed clone-detection metrics for JSON/YAML/binary export.
///
/// Field declaration order (the emitted order, as `cf-gojson` treats structs
/// as declaration-ordered) is: `total_functions, total_clone_pairs,
/// clone_ratio, clone_type_distribution (omitempty), clone_pairs, message`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Total functions analyzed (`total_functions`).
    pub total_functions: i64,
    /// Total clone pairs (`total_clone_pairs`).
    pub total_clone_pairs: i64,
    /// Fraction of functions involved in a clone (`clone_ratio`).
    pub clone_ratio: f64,
    /// Clone-type distribution (`clone_type_distribution`, `omitempty`).
    ///
    /// `None` omits the key entirely (`omitempty` on an absent map).
    pub clone_type_dist: Option<CloneTypeCounts>,
    /// The detected clone pairs (`clone_pairs`).
    ///
    /// `None` mirrors a Go `nil` slice (the report carried no `clone_pairs`
    /// key — e.g. the analyzer's empty result) and renders `null`; `Some`
    /// mirrors the non-nil slice the aggregator extraction builds
    /// (`make([]ClonePair, 0, …)`), so an empty walk result renders `[]`.
    pub clone_pairs: Option<Vec<ClonePair>>,
    /// Human-readable summary (`message`).
    pub message: String,
}

impl ComputedMetrics {
    /// Builds the struct-origin [`GoValue`] tree for this metrics struct
    /// (report-format contract): fields emit in declaration order,
    /// `clone_type_distribution` is omitted when `None` (`omitempty`), and
    /// `clone_pairs` — which has no `omitempty` — renders a Go `nil` slice
    /// (`None`) as `null` and a non-nil slice (`Some`) as a JSON array, so the
    /// aggregator's empty-but-present list renders `[]` exactly as the
    /// reference binary emits it.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("total_functions", GoValue::Int(self.total_functions));
        m.push("total_clone_pairs", GoValue::Int(self.total_clone_pairs));
        m.push("clone_ratio", GoValue::Float(self.clone_ratio));
        if let Some(counts) = self.clone_type_dist {
            // omitempty: a present map is emitted even if all counts are zero.
            m.push("clone_type_distribution", clone_type_dist_map(counts));
        }
        match &self.clone_pairs {
            None => m.push("clone_pairs", GoValue::Null),
            Some(pairs) => m.push(
                "clone_pairs",
                GoValue::Array(pairs.iter().map(ClonePair::to_go_value).collect()),
            ),
        }
        m.push("message", GoValue::Str(self.message.clone()));
        GoValue::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::Encoder;

    #[test]
    fn classify_matches_contract_thresholds() {
        assert_eq!(classify_clone_type(1.0), CLONE_TYPE1);
        assert_eq!(classify_clone_type(1.5), CLONE_TYPE1);
        assert_eq!(classify_clone_type(0.9), CLONE_TYPE2);
        assert_eq!(classify_clone_type(0.8), CLONE_TYPE2);
        assert_eq!(classify_clone_type(0.79), CLONE_TYPE3);
        assert_eq!(classify_clone_type(0.5), CLONE_TYPE3);
    }

    #[test]
    fn categorize_counts_by_type() {
        let pairs = vec![
            ClonePair {
                func_a: "a".into(),
                func_b: "b".into(),
                similarity: 1.0,
                clone_type: CLONE_TYPE1.into(),
            },
            ClonePair {
                func_a: "c".into(),
                func_b: "d".into(),
                similarity: 0.85,
                clone_type: CLONE_TYPE2.into(),
            },
            ClonePair {
                func_a: "e".into(),
                func_b: "f".into(),
                similarity: 0.6,
                clone_type: CLONE_TYPE3.into(),
            },
        ];
        let counts = categorize_clone_pairs(&pairs);
        assert_eq!(counts.type1, 1);
        assert_eq!(counts.type2, 1);
        assert_eq!(counts.type3, 1);
        assert_eq!(counts.total(), 3);
    }

    #[test]
    fn clone_pair_json_field_order() {
        let p = ClonePair {
            func_a: "A".into(),
            func_b: "B".into(),
            similarity: 0.75,
            clone_type: CLONE_TYPE3.into(),
        };
        let json = Encoder::marshal().encode_to_string(&p.to_go_value());
        assert_eq!(
            json,
            r#"{"func_a":"A","func_b":"B","similarity":0.75,"clone_type":"Type-3"}"#
        );
    }

    #[test]
    fn computed_metrics_omits_dist_emits_null_pairs() {
        let cm = ComputedMetrics {
            total_functions: 3,
            total_clone_pairs: 0,
            clone_ratio: 0.0,
            clone_type_dist: None,
            clone_pairs: None,
            message: "No code clones detected".into(),
        };
        let json = Encoder::marshal().encode_to_string(&cm.to_go_value());
        assert_eq!(
            json,
            r#"{"total_functions":3,"total_clone_pairs":0,"clone_ratio":0,"clone_pairs":null,"message":"No code clones detected"}"#
        );
    }

    #[test]
    fn computed_metrics_full_field_order() {
        let cm = ComputedMetrics {
            total_functions: 4,
            total_clone_pairs: 1,
            clone_ratio: 0.5,
            clone_type_dist: Some(CloneTypeCounts {
                type1: 0,
                type2: 1,
                type3: 0,
            }),
            clone_pairs: Some(vec![ClonePair {
                func_a: "A".into(),
                func_b: "B".into(),
                similarity: 0.9,
                clone_type: CLONE_TYPE2.into(),
            }]),
            message: "Low duplication - few clone pairs detected".into(),
        };
        let json = Encoder::marshal().encode_to_string(&cm.to_go_value());
        assert_eq!(
            json,
            concat!(
                r#"{"total_functions":4,"total_clone_pairs":1,"clone_ratio":0.5,"#,
                r#""clone_type_distribution":{"Type-1":0,"Type-2":1,"Type-3":0},"#,
                r#""clone_pairs":[{"func_a":"A","func_b":"B","similarity":0.9,"clone_type":"Type-2"}],"#,
                r#""message":"Low duplication - few clone pairs detected"}"#
            )
        );
    }
}
