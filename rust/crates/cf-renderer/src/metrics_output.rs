//! The metrics-first output pipeline. Port of the Go `renderer/metrics_output.go`.
//!
//! Analyzer "computed metrics" implement [`MetricsOutput`] to provide
//! serializable output for the JSON and YAML renderers. The Go interface
//! returns `any` from `ToJSON`/`ToYAML` (later passed to `json.Marshal` /
//! `yaml.Marshal`); the Rust port returns a [`GoValue`](crate::gocompat::GoValue)
//! so serialization routes through the Go-byte-compatible encoders rather than
//! `serde_json`.

use crate::gocompat::{Encoder, GoValue};

/// Returned when `None` is passed to the render functions. Mirrors Go's
/// `ErrNilMetricsOutput`.
#[derive(Debug, PartialEq, Eq)]
pub struct NilMetricsOutput;

impl std::fmt::Display for NilMetricsOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "metrics output is nil")
    }
}

impl std::error::Error for NilMetricsOutput {}

/// Implemented by analyzer computed-metrics types to provide serializable
/// output for JSON and YAML. Mirrors Go's `MetricsOutput` interface.
pub trait MetricsOutput {
    /// The analyzer identifier (e.g. "devs", "burndown"). Mirrors `AnalyzerName`.
    fn analyzer_name(&self) -> String;

    /// A value suitable for JSON marshaling. Mirrors `ToJSON`.
    fn to_json(&self) -> GoValue;

    /// A value suitable for YAML marshaling. Mirrors `ToYAML`. For most
    /// analyzers this returns the same value as [`MetricsOutput::to_json`].
    fn to_yaml(&self) -> GoValue;
}

/// Serializes metrics output to Go-compatible JSON bytes. Port of
/// `RenderMetricsJSON`. Returns [`NilMetricsOutput`] when `m` is `None`.
pub fn render_metrics_json(m: Option<&dyn MetricsOutput>) -> Result<String, NilMetricsOutput> {
    let m = m.ok_or(NilMetricsOutput)?;
    Ok(Encoder::default().encode(&m.to_json()))
}

/// Serializes metrics output to YAML bytes. Port of `RenderMetricsYAML`.
///
/// The design routes YAML through `cf-goyaml` (DESIGN.md §2.4) for byte
/// identity with `gopkg.in/yaml.v3`. That crate is still a scaffold, so this
/// returns the value's [`GoValue`] structure rendered via the JSON encoder's
/// data model as a stand-in until `cf-goyaml` is wired in; the function
/// signature and nil-handling match Go so call sites are stable.
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

    /// Port of `TestMetricsOutput_AnalyzerName/ToJSON/ToYAML`.
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

    /// Port of `TestRenderMetricsJSON` + `_NilInput`.
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

    /// Port of `TestRenderMetricsYAML` + `_NilInput`.
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
