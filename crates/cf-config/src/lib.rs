//! cf-config — the `codefang` configuration file.
//!
//! A `.codefang.yaml` file (explicit `--config`, else the working directory,
//! else `$HOME`) and the `CODEFANG_*` environment supply the DEFAULT of the
//! `run` flags listed in [`KEYS`]: the precedence is explicit flag >
//! environment > file > compiled default. Each field of [`Config`] is `None`
//! when no layer sets it, so the compiled defaults live in one place — the
//! flag registrations in `cf-commands`.
//!
//! Keys a file sets that nothing reads (the Go-era `pipeline:` tuning tree,
//! `checkpoint:`, analyzer thresholds without a flag, …) are returned in
//! [`Loaded::unsupported_keys`] for the caller to warn about; they never fail
//! the load.
//!
//! The crate emits no report bytes; `serde_yaml` only parses the input file.
//!
//! # Example
//!
//! ```
//! use cf_config::{keys, load, Value};
//!
//! let dir = tempfile::tempdir().unwrap();
//! let file = dir.path().join("codefang.yaml");
//! std::fs::write(&file, "history:\n  anomaly:\n    window_size: 3\npipeline:\n  workers: 8\n")
//!     .unwrap();
//! let env = |name: &str| (name == "CODEFANG_HISTORY_TICK_SIZE").then(|| "6".to_owned());
//!
//! let loaded = load(Some(&file), &[], env).unwrap();
//! assert_eq!(loaded.unsupported_keys, vec!["pipeline.workers"]);
//! let settings = loaded.config.settings();
//! assert_eq!(settings[0].key, keys::TICK_SIZE);
//! assert_eq!(settings[0].value, Value::Int(6));
//! assert_eq!(settings[1].key, keys::ANOMALY_WINDOW_SIZE);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod loader;
mod types;

pub use loader::{default_search_dirs, env_name, load, LoadError, Loaded, CONFIG_FILE_NAME};
pub use types::{
    keys, AnomalyConfig, BurndownConfig, Config, ConfigError, DevsConfig, HistoryConfig,
    PipelineConfig, SentimentConfig, Setting, ShotnessConfig, TyposConfig, Value, KEYS,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A document that sets every supported key.
    const EVERY_KEY: &str = "\
analyzers: [static/complexity, history/devs]
pipeline:
  max_changes_per_commit: 500
  memory_budget: 2GB
  chunk_size: 500
history:
  tick_size: 12
  burndown:
    granularity: 15
    sampling: 10
    track_files: true
    track_people: false
  devs:
    consider_empty_commits: true
    anonymize: true
  sentiment:
    min_comment_length: 0
  shotness:
    dsl_struct: 'rfilter(.roles has \"Class\")'
    dsl_name: .props.identifier
    keep_ended: true
  typos:
    max_distance: 3
  anomaly:
    threshold: 1.5
    window_size: 5
";

    fn parse(yaml: &str) -> Config {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn every_key_deserializes_and_is_a_setting_in_key_order() {
        let cfg = parse(EVERY_KEY);
        assert_eq!(cfg.validate(), Ok(()));
        let settings = cfg.settings();
        let keys: Vec<&str> = settings.iter().map(|s| s.key).collect();
        assert_eq!(keys, KEYS);
        assert_eq!(
            settings[0].value,
            Value::List(vec!["static/complexity".into(), "history/devs".into()])
        );
        assert_eq!(settings[2].value, Value::Text("2GB".into()));
        assert_eq!(settings[3].value, Value::Int(500));
        assert_eq!(settings[7].value, Value::Bool(true));
        assert_eq!(
            settings[12].value,
            Value::Text("rfilter(.roles has \"Class\")".into())
        );
        assert_eq!(settings[14].value, Value::Bool(true));
        assert_eq!(settings[16].value, Value::Float(1.5));
    }

    /// A budget written as a plain byte count is the same setting as its
    /// text form.
    #[test]
    fn a_numeric_memory_budget_is_a_byte_count() {
        let cfg = parse("pipeline:\n  memory_budget: 1073741824\n");
        assert_eq!(
            cfg.settings(),
            vec![Setting {
                key: keys::MEMORY_BUDGET,
                value: Value::Text("1073741824".into()),
            }]
        );
    }

    #[test]
    fn the_default_configuration_sets_nothing() {
        assert!(Config::default().settings().is_empty());
        assert_eq!(Config::default().validate(), Ok(()));
    }

    #[test]
    fn empty_lists_and_blank_queries_count_as_unset() {
        let cfg =
            parse("analyzers: []\nhistory:\n  shotness:\n    dsl_struct: ''\n    dsl_name: ' '\n");
        assert!(cfg.settings().is_empty());
    }

    #[test]
    fn null_values_count_as_unset() {
        let cfg = parse("history:\n  anomaly:\n    window_size:\n");
        assert_eq!(cfg.history.anomaly.window_size, None);
    }

    #[test]
    fn values_render_as_flag_arguments() {
        assert_eq!(Value::Int(-3).to_args(), vec!["-3"]);
        assert_eq!(Value::Float(2.0).to_args(), vec!["2"]);
        assert_eq!(Value::Float(0.25).to_args(), vec!["0.25"]);
        assert_eq!(Value::Bool(false).to_args(), vec!["false"]);
        assert_eq!(Value::Text("x y".into()).to_args(), vec!["x y"]);
        assert_eq!(
            Value::List(vec!["a".into(), "b".into()]).to_args(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn every_rule_is_enforced_with_the_key_named() {
        let cases = [
            (
                "pipeline:\n  max_changes_per_commit: -1\n",
                "pipeline.max_changes_per_commit must be >= 0, got -1",
            ),
            (
                "pipeline:\n  chunk_size: 0\n",
                "pipeline.chunk_size must be >= 1, got 0",
            ),
            (
                "history:\n  tick_size: 0\n",
                "history.tick_size must be > 0, got 0",
            ),
            (
                "history:\n  burndown:\n    granularity: 0\n",
                "history.burndown.granularity must be > 0, got 0",
            ),
            (
                "history:\n  burndown:\n    sampling: -2\n",
                "history.burndown.sampling must be > 0, got -2",
            ),
            (
                "history:\n  sentiment:\n    min_comment_length: -1\n",
                "history.sentiment.min_comment_length must be >= 0, got -1",
            ),
            (
                "history:\n  typos:\n    max_distance: 0\n",
                "history.typos.max_distance must be > 0, got 0",
            ),
            (
                "history:\n  anomaly:\n    threshold: 0\n",
                "history.anomaly.threshold must be > 0, got 0",
            ),
            (
                "history:\n  anomaly:\n    threshold: .nan\n",
                "history.anomaly.threshold must be > 0, got NaN",
            ),
            (
                "history:\n  anomaly:\n    window_size: 1\n",
                "history.anomaly.window_size must be >= 2, got 1",
            ),
        ];
        for (yaml, message) in cases {
            let err = parse(yaml).validate().unwrap_err();
            assert_eq!(err.to_string(), message, "{yaml}");
        }
    }

    #[test]
    fn the_first_violation_in_key_order_is_reported() {
        let cfg =
            parse("history:\n  anomaly:\n    window_size: 1\n    threshold: -1\n  tick_size: 0\n");
        assert_eq!(cfg.validate().unwrap_err().key, keys::TICK_SIZE);
    }
}
