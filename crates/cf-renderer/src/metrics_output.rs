//! The metrics-first output pipeline.
//!
//! Analyzer "computed metrics" implement [`MetricsOutput`] to provide
//! serializable output for the JSON and YAML renderers. The trait returns a
//! [`GoValue`](crate::gocompat::GoValue) so serialization routes through the
//! report-format byte-compatible encoders rather than `serde_json` (output
//! bytes are pinned by `tests/compat`).

use crate::gocompat::{Encoder, GoValue};

/// Returned when `None` is passed to the render functions.
///
/// The error text is part of the CLI contract; keep it byte-identical.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("metrics output is nil")]
pub struct NilMetricsOutput;

/// Implemented by analyzer computed-metrics types to provide serializable
/// output for JSON and YAML.
pub trait MetricsOutput {
    /// The analyzer identifier (e.g. "devs", "burndown").
    fn analyzer_name(&self) -> String;

    /// A value suitable for JSON marshaling.
    fn to_json(&self) -> GoValue;

    /// A value suitable for YAML marshaling. For most analyzers this returns
    /// the same value as [`MetricsOutput::to_json`].
    fn to_yaml(&self) -> GoValue;
}

/// Serializes metrics output to report-contract JSON bytes.
///
/// # Errors
///
/// Returns [`NilMetricsOutput`] when `m` is `None`.
///
/// ```
/// use cf_renderer::{render_metrics_json, MetricsOutput, NilMetricsOutput};
/// use cf_renderer::gocompat::GoValue;
///
/// struct Devs;
/// impl MetricsOutput for Devs {
///     fn analyzer_name(&self) -> String { "devs".to_string() }
///     fn to_json(&self) -> GoValue {
///         GoValue::Object(vec![("count".to_string(), GoValue::Int(42))])
///     }
///     fn to_yaml(&self) -> GoValue { self.to_json() }
/// }
///
/// assert_eq!(render_metrics_json(Some(&Devs)).unwrap(), r#"{"count":42}"#);
/// // A nil metrics output is the documented error.
/// assert_eq!(render_metrics_json(None), Err(NilMetricsOutput));
/// ```
pub fn render_metrics_json(m: Option<&dyn MetricsOutput>) -> Result<String, NilMetricsOutput> {
    let m = m.ok_or(NilMetricsOutput)?;
    Ok(Encoder::default().encode(&m.to_json()))
}

/// Produces the metrics output's YAML-marshalable value.
///
/// The design routes YAML bytes through `cf-goyaml` (DESIGN.md §2.4) for
/// report-format byte identity. This returns the value's [`GoValue`] structure
/// for that emitter to consume; the signature and nil-handling are stable for
/// call sites.
///
/// # Errors
///
/// Returns [`NilMetricsOutput`] when `m` is `None`.
pub fn render_metrics_yaml(m: Option<&dyn MetricsOutput>) -> Result<GoValue, NilMetricsOutput> {
    let m = m.ok_or(NilMetricsOutput)?;
    Ok(m.to_yaml())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMetrics {
        name: String,
        json_value: i64,
        yaml_text: String,
    }

    impl MetricsOutput for MockMetrics {
        fn analyzer_name(&self) -> String {
            self.name.clone()
        }
        fn to_json(&self) -> GoValue {
            GoValue::Object(vec![("value".to_string(), GoValue::Int(self.json_value))])
        }
        fn to_yaml(&self) -> GoValue {
            GoValue::Object(vec![("text".to_string(), GoValue::Str(self.yaml_text.clone()))])
        }
    }

    /// Mirrors reference test `TestMetricsOutput_AnalyzerName/ToJSON/ToYAML`.
    #[test]
    fn metrics_output_accessors() {
        let m = MockMetrics {
            name: "test-analyzer".into(),
            json_value: 42,
            yaml_text: "test-output-value".into(),
        };
        assert_eq!(m.analyzer_name(), "test-analyzer");
        assert_eq!(
            m.to_json(),
            GoValue::Object(vec![("value".to_string(), GoValue::Int(42))])
        );
        assert_eq!(
            m.to_yaml(),
            GoValue::Object(vec![(
                "text".to_string(),
                GoValue::Str("test-output-value".into())
            )])
        );
    }

    /// Mirrors reference test `TestRenderMetricsJSON` + `_NilInput`.
    #[test]
    fn render_metrics_json_cases() {
        let m = MockMetrics {
            name: "test-analyzer".into(),
            json_value: 42,
            yaml_text: String::new(),
        };
        let out = render_metrics_json(Some(&m)).unwrap();
        assert!(out.contains(r#""value":42"#));

        assert_eq!(render_metrics_json(None), Err(NilMetricsOutput));
    }

    /// Mirrors reference test `TestRenderMetricsYAML` + `_NilInput`.
    #[test]
    fn render_metrics_yaml_cases() {
        let m = MockMetrics {
            name: "test-analyzer".into(),
            json_value: 0,
            yaml_text: "test-output-value".into(),
        };
        let out = render_metrics_yaml(Some(&m)).unwrap();
        assert_eq!(
            out,
            GoValue::Object(vec![(
                "text".to_string(),
                GoValue::Str("test-output-value".into())
            )])
        );

        assert_eq!(render_metrics_yaml(None), Err(NilMetricsOutput));
    }
}
