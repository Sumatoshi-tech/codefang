//! Lightweight metrics primitives.
//!
//! Rust port of the Go package `pkg/metrics`. It provides interfaces for defining
//! self-contained, reusable metrics.
//!
//! Each metric is a computation unit that:
//!   - Declares its input requirements,
//!   - Computes a typed output,
//!   - Provides metadata for documentation and serialization.
//!
//! This design allows metrics to be reused across analyzers and output formats.
//!
//! # Serialization parity
//!
//! The data types in this crate ([`TimeSeriesPoint`], [`RiskResult`]) appear in
//! MACHINE-format reports. Per the Rust-rewrite design (`specs/rust-rewrite/DESIGN.md`
//! rule (1) / section 3), machine-format bytes (json, yaml, ndjson, timeseries,
//! compact, bin) must be produced by the shared `cf-gojson` / `cf-goyaml`
//! Go-compatibility crates, **not** by raw `serde_json` / `serde_yaml` defaults.
//!
//! This crate therefore carries **no** serde dependency. Instead the report-bearing
//! types implement [`GoSerialize`], lowering themselves into the shared
//! [`cf_gojson::GoValue`] tree. Each is built as a **struct-origin** [`GoMap`]
//! ([`MapOrigin::Struct`]), so its fields are emitted in **Go struct declaration
//! order** — never byte-sorted — and `omitempty` fields are dropped (not `push`ed)
//! when empty/zero, exactly as Go's `encoding/json` does. The resulting `GoValue`
//! is then encoded by `cf_gojson::{marshal, marshal_indent, Encoder}` (and the
//! `cf-goyaml` emitter) to obtain byte-identical output.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;

pub use cf_gojson::{GoMap, GoValue, MapOrigin};

/// Types that can lower themselves into a [`GoValue`] for Go-compatible encoding.
///
/// Implementors build a **struct-origin** [`GoMap`] ([`MapOrigin::Struct`]) and
/// `push` fields in **Go declaration order**, honoring `omitempty` by skipping the
/// corresponding `push` when the value is empty/zero. The returned [`GoValue`] is
/// then encoded by the shared `cf-gojson` marshallers (and `cf-goyaml`) to produce
/// byte-identical machine output.
pub trait GoSerialize {
    /// Lower `self` into a [`GoValue`] with declaration-ordered fields.
    fn to_go_value(&self) -> GoValue;
}

/// The core trait that all metrics must implement.
///
/// Each metric is a self-contained computation with metadata. This is the Rust
/// port of the Go generic interface `Metric[In, Out any]`. The Go type parameters
/// `In` and `Out` map to the associated types [`Metric::In`] and [`Metric::Out`].
pub trait Metric {
    /// Input data type for [`Metric::compute`] (the Go `In` type parameter).
    type In;
    /// Output value type returned by [`Metric::compute`] (the Go `Out` type
    /// parameter).
    type Out;

    /// Returns the machine-readable identifier (snake_case, unique).
    fn name(&self) -> &str;

    /// Returns a human-readable name for UI/reports.
    fn display_name(&self) -> &str;

    /// Returns detailed documentation including:
    /// - What the metric measures.
    /// - How to interpret the value.
    /// - Units (if applicable).
    /// - Any caveats or limitations.
    fn description(&self) -> &str;

    /// Returns the metric category (e.g., `"aggregate"`, `"time_series"`, `"risk"`).
    fn metric_type(&self) -> &str;

    /// Calculates the metric value from input data.
    fn compute(&self, input: Self::In) -> Self::Out;
}

/// A single data point in a time series.
///
/// Port of Go `TimeSeriesPoint`. JSON shape: `{"tick": <int>, "value": <float>}`,
/// in declaration order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeSeriesPoint {
    /// The tick (discrete time index). Go field `Tick int` (`json:"tick"`).
    pub tick: i64,
    /// The value at this tick. Go field `Value float64` (`json:"value"`).
    pub value: f64,
}

impl GoSerialize for TimeSeriesPoint {
    fn to_go_value(&self) -> GoValue {
        // Struct-origin map: Go declaration order tick, value (never byte-sorted).
        let mut m = GoMap::new_struct();
        m.push(TICK_FIELD, GoValue::Int(self.tick));
        m.push(VALUE_FIELD, GoValue::Float(self.value));
        GoValue::Map(m)
    }
}

/// JSON field name for [`TimeSeriesPoint::tick`].
const TICK_FIELD: &str = "tick";
/// JSON field name shared by [`TimeSeriesPoint::value`] and [`RiskResult::value`].
const VALUE_FIELD: &str = "value";
/// JSON field name for [`RiskResult::level`] (`json:"risk_level"`).
const RISK_LEVEL_FIELD: &str = "risk_level";
/// JSON field name for [`RiskResult::threshold`].
const THRESHOLD_FIELD: &str = "threshold";
/// JSON field name for [`RiskResult::message`].
const MESSAGE_FIELD: &str = "message";

/// Severity level for a [`RiskResult`].
///
/// Port of Go `RiskLevel` (`type RiskLevel string`). The wire representation is the
/// uppercase string token (`"CRITICAL"`, `"HIGH"`, `"MEDIUM"`, `"LOW"`). Use the
/// [`RISK_CRITICAL`] etc. constants or the convenience constructors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RiskLevel(pub String);

impl RiskLevel {
    /// `RiskLevel("CRITICAL")`.
    pub fn critical() -> RiskLevel {
        RiskLevel(RISK_CRITICAL.to_string())
    }
    /// `RiskLevel("HIGH")`.
    pub fn high() -> RiskLevel {
        RiskLevel(RISK_HIGH.to_string())
    }
    /// `RiskLevel("MEDIUM")`.
    pub fn medium() -> RiskLevel {
        RiskLevel(RISK_MEDIUM.to_string())
    }
    /// `RiskLevel("LOW")`.
    pub fn low() -> RiskLevel {
        RiskLevel(RISK_LOW.to_string())
    }

    /// Returns the underlying string token.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RiskLevel {
    fn from(s: &str) -> Self {
        RiskLevel(s.to_string())
    }
}

impl From<String> for RiskLevel {
    fn from(s: String) -> Self {
        RiskLevel(s)
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// String token for the critical risk level (Go `RiskCritical`).
pub const RISK_CRITICAL: &str = "CRITICAL";
/// String token for the high risk level (Go `RiskHigh`).
pub const RISK_HIGH: &str = "HIGH";
/// String token for the medium risk level (Go `RiskMedium`).
pub const RISK_MEDIUM: &str = "MEDIUM";
/// String token for the low risk level (Go `RiskLow`).
pub const RISK_LOW: &str = "LOW";

// Risk priority values for sorting (lower = higher priority). Ported from the
// unexported Go constants priorityCritical/priorityHigh/priorityMedium/priorityDefault.
const PRIORITY_CRITICAL: i64 = 0;
const PRIORITY_HIGH: i64 = 1;
const PRIORITY_MEDIUM: i64 = 2;
const PRIORITY_DEFAULT: i64 = 3;

/// Returns a sortable integer for a risk level.
///
/// Lower values indicate higher priority:
/// `CRITICAL < HIGH < MEDIUM < LOW/unknown`.
///
/// Port of Go `RiskPriority`. Unrecognized levels fold into the default priority
/// `3`, matching the Go `switch` which maps both `RiskLow` and `default` to the
/// same value.
pub fn risk_priority(level: &RiskLevel) -> i64 {
    match level.as_str() {
        RISK_CRITICAL => PRIORITY_CRITICAL,
        RISK_HIGH => PRIORITY_HIGH,
        RISK_MEDIUM => PRIORITY_MEDIUM,
        RISK_LOW => PRIORITY_DEFAULT,
        _ => PRIORITY_DEFAULT,
    }
}

/// The output of a risk metric.
///
/// Port of Go `RiskResult`. JSON field order (declaration order) is
/// `value, risk_level, threshold, message`, with `threshold` (Go `omitempty` on a
/// `float64`) and `message` (Go `omitempty` on a `string`) dropped when zero/empty.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskResult {
    /// The metric value (Go `Value any`, `json:"value"`). Modeled as a [`GoValue`]
    /// so the `any`-typed field can carry ints, floats, strings, etc. with correct
    /// downstream encoding.
    pub value: GoValue,

    /// The assessed risk level. Go `Level RiskLevel` (`json:"risk_level"`).
    pub level: RiskLevel,

    /// The threshold that triggered this risk level. Go
    /// `Threshold float64` (`json:"threshold,omitempty"`); omitted when `0.0`.
    pub threshold: f64,

    /// A human-readable message. Go `Message string` (`json:"message,omitempty"`);
    /// omitted when empty.
    pub message: String,
}

impl RiskResult {
    /// Creates a [`RiskResult`] with the given value and level, and no threshold
    /// or message.
    pub fn new(value: GoValue, level: RiskLevel) -> RiskResult {
        RiskResult {
            value,
            level,
            threshold: 0.0,
            message: String::new(),
        }
    }
}

impl GoSerialize for RiskResult {
    fn to_go_value(&self) -> GoValue {
        // Struct-origin map: Go declaration order value, risk_level,
        // threshold (omitempty), message (omitempty). omitempty fields are simply
        // not pushed when zero/empty, matching Go's encoding/json behavior.
        let mut m = GoMap::new_struct();
        m.push(VALUE_FIELD, self.value.clone());
        m.push(RISK_LEVEL_FIELD, GoValue::Str(self.level.0.clone()));
        if self.threshold != 0.0 {
            m.push(THRESHOLD_FIELD, GoValue::Float(self.threshold));
        }
        if !self.message.is_empty() {
            m.push(MESSAGE_FIELD, GoValue::Str(self.message.clone()));
        }
        GoValue::Map(m)
    }
}

/// Common metadata for a metric.
///
/// Embed this in metric implementations to satisfy the metadata methods of the
/// [`Metric`] trait. Port of Go `MetricMeta` and its `Name`/`DisplayName`/
/// `Description`/`Type` methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricMeta {
    /// Machine-readable identifier. Go field `MetricName`.
    pub metric_name: String,
    /// Human-readable name. Go field `MetricDisplayName`.
    pub metric_display_name: String,
    /// Detailed documentation. Go field `MetricDescription`.
    pub metric_description: String,
    /// Metric category. Go field `MetricType`.
    pub metric_type: String,
}

impl MetricMeta {
    /// Returns the machine-readable identifier (Go `Name()`).
    pub fn name(&self) -> &str {
        &self.metric_name
    }

    /// Returns a human-readable name for UI/reports (Go `DisplayName()`).
    pub fn display_name(&self) -> &str {
        &self.metric_display_name
    }

    /// Returns detailed documentation (Go `Description()`).
    pub fn description(&self) -> &str {
        &self.metric_description
    }

    /// Returns the metric category (Go `Type()`).
    pub fn metric_type(&self) -> &str {
        &self.metric_type
    }
}

/// A type-erased metric handle stored in a [`Registry`].
///
/// The Go registry stores `map[string]any` (heterogeneous `Metric[any, any]`).
/// Rust cannot store heterogeneous generic trait objects directly, so the registry
/// stores boxed, type-erased values keyed by name. Retrieve the concrete type via
/// [`Any::downcast_ref`].
pub type AnyMetric = Box<dyn Any + Send + Sync>;

/// A collection of metrics that can be computed together.
///
/// Port of Go `Registry`. Stores metrics keyed by name. As in Go, [`Registry::names`]
/// returns names in unspecified order (Go iterates a map); callers that need a
/// stable order must sort.
#[derive(Default)]
pub struct Registry {
    metrics: HashMap<String, AnyMetric>,
}

impl Registry {
    /// Creates an empty metric registry.
    ///
    /// Port of Go `NewRegistry`.
    pub fn new() -> Registry {
        Registry {
            metrics: HashMap::new(),
        }
    }

    /// Adds a metric to the registry under the given name.
    ///
    /// This corresponds to the Go free function `Register[In, Out]`, which keys the
    /// metric by `m.Name()`. The name is supplied explicitly here so the registry
    /// can store fully type-erased values. As in Go (map assignment), registering a
    /// metric under an existing name overwrites the previous entry.
    pub fn register<M>(&mut self, name: impl Into<String>, metric: M)
    where
        M: Any + Send + Sync,
    {
        self.metrics.insert(name.into(), Box::new(metric));
    }

    /// Retrieves a metric by name.
    ///
    /// Returns the type-erased value if present, mirroring Go's `(any, bool)`
    /// (here `Option`). The returned reference is `&dyn Any`; downcast it with
    /// [`Any::downcast_ref`] to recover the concrete type.
    pub fn get(&self, name: &str) -> Option<&(dyn Any + Send + Sync)> {
        self.metrics.get(name).map(|b| b.as_ref())
    }

    /// Returns all registered metric names.
    ///
    /// Order is unspecified (matches Go map iteration). Sort the result if a stable
    /// order is required.
    pub fn names(&self) -> Vec<String> {
        self.metrics.keys().cloned().collect()
    }

    /// Returns the number of registered metrics.
    pub fn len(&self) -> usize {
        self.metrics.len()
    }

    /// Returns `true` if no metrics are registered.
    pub fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test constants mirroring the Go test file's constants.
    const TEST_METRIC_NAME: &str = "test_metric";
    const TEST_METRIC_NAME_2: &str = "test_metric_2";
    const TEST_METRIC_DISPLAY_NAME: &str = "Test Metric";
    const TEST_METRIC_DESCRIPTION: &str = "A test metric for unit testing";
    const TEST_METRIC_TYPE: &str = "aggregate";
    const TEST_INPUT_VALUE: i64 = 42;
    const TEST_OUTPUT_MULTIPLIER: i64 = 2;

    // testMetric is a concrete implementation for testing the Metric trait,
    // embedding MetricMeta (Go: struct testMetric { MetricMeta }).
    struct TestMetric {
        meta: MetricMeta,
    }

    impl TestMetric {
        // newTestMetric: standard metadata.
        fn new() -> TestMetric {
            TestMetric {
                meta: MetricMeta {
                    metric_name: TEST_METRIC_NAME.to_string(),
                    metric_display_name: TEST_METRIC_DISPLAY_NAME.to_string(),
                    metric_description: TEST_METRIC_DESCRIPTION.to_string(),
                    metric_type: TEST_METRIC_TYPE.to_string(),
                },
            }
        }
    }

    impl Metric for TestMetric {
        type In = i64;
        type Out = i64;

        fn name(&self) -> &str {
            self.meta.name()
        }
        fn display_name(&self) -> &str {
            self.meta.display_name()
        }
        fn description(&self) -> &str {
            self.meta.description()
        }
        fn metric_type(&self) -> &str {
            self.meta.metric_type()
        }
        // Compute doubles the input value.
        fn compute(&self, input: i64) -> i64 {
            input * TEST_OUTPUT_MULTIPLIER
        }
    }

    // ---- MetricMeta (ported from TestMetricMeta_*) --------------------------

    #[test]
    fn metric_meta_name() {
        let meta = MetricMeta {
            metric_name: TEST_METRIC_NAME.to_string(),
            ..Default::default()
        };
        assert_eq!(meta.name(), TEST_METRIC_NAME);
    }

    #[test]
    fn metric_meta_display_name() {
        let meta = MetricMeta {
            metric_display_name: TEST_METRIC_DISPLAY_NAME.to_string(),
            ..Default::default()
        };
        assert_eq!(meta.display_name(), TEST_METRIC_DISPLAY_NAME);
    }

    #[test]
    fn metric_meta_description() {
        let meta = MetricMeta {
            metric_description: TEST_METRIC_DESCRIPTION.to_string(),
            ..Default::default()
        };
        assert_eq!(meta.description(), TEST_METRIC_DESCRIPTION);
    }

    #[test]
    fn metric_meta_type() {
        let meta = MetricMeta {
            metric_type: TEST_METRIC_TYPE.to_string(),
            ..Default::default()
        };
        assert_eq!(meta.metric_type(), TEST_METRIC_TYPE);
    }

    // ---- Registry (ported from TestNewRegistry / TestRegistry_*) -----------

    #[test]
    fn new_registry() {
        // Go asserts registry and registry.metrics are non-nil; the Rust analogue
        // is an empty, usable registry.
        let registry = Registry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_names_empty() {
        let registry = Registry::new();
        let names = registry.names();
        assert!(names.is_empty());
    }

    #[test]
    fn registry_register() {
        let mut registry = Registry::new();
        let metric = TestMetric::new();
        registry.register(metric.name().to_string(), metric);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn registry_get_found() {
        let mut registry = Registry::new();
        let metric = TestMetric::new();
        registry.register(metric.name().to_string(), metric);

        let retrieved = registry.get(TEST_METRIC_NAME);
        assert!(retrieved.is_some());
        let downcast = retrieved.unwrap().downcast_ref::<TestMetric>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().name(), TEST_METRIC_NAME);
    }

    #[test]
    fn registry_get_not_found() {
        let registry = Registry::new();
        let retrieved = registry.get("nonexistent_metric");
        assert!(retrieved.is_none());
    }

    #[test]
    fn registry_names() {
        let mut registry = Registry::new();
        let metric1 = TestMetric::new();
        let metric2 = TestMetric {
            meta: MetricMeta {
                metric_name: TEST_METRIC_NAME_2.to_string(),
                ..Default::default()
            },
        };
        registry.register(metric1.name().to_string(), metric1);
        registry.register(metric2.name().to_string(), metric2);

        let names = registry.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&TEST_METRIC_NAME.to_string()));
        assert!(names.contains(&TEST_METRIC_NAME_2.to_string()));
    }

    #[test]
    fn registry_register_overwrites_same_name() {
        // Go map semantics: re-registering under the same name overwrites.
        let mut registry = Registry::new();
        registry.register("dup", 1_i64);
        registry.register("dup", 2_i64);
        assert_eq!(registry.len(), 1);
        let v = registry.get("dup").unwrap().downcast_ref::<i64>().copied();
        assert_eq!(v, Some(2));
    }

    // ---- Metric.compute (ported from TestMetric_Compute) -------------------

    #[test]
    fn metric_compute() {
        let metric = TestMetric::new();
        let result = metric.compute(TEST_INPUT_VALUE);
        let expected = TEST_INPUT_VALUE * TEST_OUTPUT_MULTIPLIER;
        assert_eq!(result, expected);
    }

    #[test]
    fn metric_delegates_metadata() {
        let metric = TestMetric::new();
        assert_eq!(metric.name(), TEST_METRIC_NAME);
        assert_eq!(metric.display_name(), TEST_METRIC_DISPLAY_NAME);
        assert_eq!(metric.description(), TEST_METRIC_DESCRIPTION);
        assert_eq!(metric.metric_type(), TEST_METRIC_TYPE);
    }

    // ---- RiskLevel constants (ported from TestRiskLevel_Constants) ---------

    #[test]
    fn risk_level_constants() {
        assert_eq!(RiskLevel::critical(), RiskLevel::from("CRITICAL"));
        assert_eq!(RiskLevel::high(), RiskLevel::from("HIGH"));
        assert_eq!(RiskLevel::medium(), RiskLevel::from("MEDIUM"));
        assert_eq!(RiskLevel::low(), RiskLevel::from("LOW"));
        assert_eq!(RISK_CRITICAL, "CRITICAL");
        assert_eq!(RISK_HIGH, "HIGH");
        assert_eq!(RISK_MEDIUM, "MEDIUM");
        assert_eq!(RISK_LOW, "LOW");
    }

    #[test]
    fn risk_level_display_is_token() {
        assert_eq!(RiskLevel::critical().to_string(), "CRITICAL");
    }

    // ---- TimeSeriesPoint (ported from TestTimeSeriesPoint_Fields) ----------

    #[test]
    fn time_series_point_fields() {
        let point = TimeSeriesPoint {
            tick: TEST_INPUT_VALUE,
            value: TEST_INPUT_VALUE as f64,
        };
        assert_eq!(point.tick, TEST_INPUT_VALUE);
        assert!((point.value - TEST_INPUT_VALUE as f64).abs() < 0.001);
    }

    #[test]
    fn time_series_point_to_go_value_order() {
        let p = TimeSeriesPoint {
            tick: 7,
            value: 2.5,
        };
        match p.to_go_value() {
            GoValue::Map(m) => {
                let fields = m.entries();
                assert_eq!(fields[0].0, "tick");
                assert_eq!(fields[0].1, GoValue::Int(7));
                assert_eq!(fields[1].0, "value");
                assert_eq!(fields[1].1, GoValue::Float(2.5));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    // ---- RiskPriority (ported from TestRiskPriority_*) ----------------------

    #[test]
    fn risk_priority_all_levels() {
        // Table-driven, mirroring the Go test: level, want_pri, want_less
        // (must sort before). None want_less means "no next level".
        let cases: &[(RiskLevel, i64, Option<RiskLevel>)] = &[
            (RiskLevel::critical(), PRIORITY_CRITICAL, Some(RiskLevel::high())),
            (RiskLevel::high(), PRIORITY_HIGH, Some(RiskLevel::medium())),
            (RiskLevel::medium(), PRIORITY_MEDIUM, Some(RiskLevel::low())),
            (RiskLevel::low(), PRIORITY_DEFAULT, None),
        ];
        for (level, want_pri, want_less) in cases {
            assert_eq!(risk_priority(level), *want_pri, "level={level}");
            if let Some(next) = want_less {
                assert!(
                    risk_priority(level) < risk_priority(next),
                    "{level} should sort before {next}"
                );
            }
        }
    }

    #[test]
    fn risk_priority_unknown_level() {
        assert_eq!(risk_priority(&RiskLevel::from("UNKNOWN")), PRIORITY_DEFAULT);
        assert_eq!(risk_priority(&RiskLevel::from("")), PRIORITY_DEFAULT);
    }

    // ---- RiskResult (ported from TestRiskResult_Fields) --------------------

    #[test]
    fn risk_result_fields() {
        let result = RiskResult {
            value: GoValue::Int(TEST_INPUT_VALUE),
            level: RiskLevel::high(),
            threshold: TEST_INPUT_VALUE as f64,
            message: TEST_METRIC_DESCRIPTION.to_string(),
        };
        assert_eq!(result.value, GoValue::Int(TEST_INPUT_VALUE));
        assert_eq!(result.level, RiskLevel::high());
        assert!((result.threshold - TEST_INPUT_VALUE as f64).abs() < 0.001);
        assert_eq!(result.message, TEST_METRIC_DESCRIPTION);
    }

    #[test]
    fn risk_result_to_go_value_omits_empty_threshold_and_message() {
        let r = RiskResult::new(GoValue::Int(42), RiskLevel::high());
        match r.to_go_value() {
            GoValue::Map(m) => {
                let fields = m.entries();
                let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, vec!["value", "risk_level"]);
                assert_eq!(fields[0].1, GoValue::Int(42));
                assert_eq!(fields[1].1, GoValue::Str("HIGH".to_string()));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn risk_result_to_go_value_includes_present_threshold_and_message() {
        let r = RiskResult {
            value: GoValue::Str("v".to_string()),
            level: RiskLevel::critical(),
            threshold: 0.75,
            message: "too high".to_string(),
        };
        match r.to_go_value() {
            GoValue::Map(m) => {
                let fields = m.entries();
                let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
                // Declaration order: value, risk_level, threshold, message.
                assert_eq!(keys, vec!["value", "risk_level", "threshold", "message"]);
                assert_eq!(fields[2].1, GoValue::Float(0.75));
                assert_eq!(fields[3].1, GoValue::Str("too high".to_string()));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
