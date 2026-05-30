//! Analyzer configuration: keys, defaults, validation, and descriptor.
//!
//! Ports the configuration constants and `validate` logic from
//! `internal/analyzers/anomaly/analyzer.go`.

/// Configuration key for the Z-score threshold (`TemporalAnomaly.Threshold`).
pub const CONFIG_ANOMALY_THRESHOLD: &str = "TemporalAnomaly.Threshold";
/// Configuration key for the window size (`TemporalAnomaly.WindowSize`).
pub const CONFIG_ANOMALY_WINDOW_SIZE: &str = "TemporalAnomaly.WindowSize";

/// CLI flag for the threshold option (`--anomaly-threshold`).
pub const FLAG_ANOMALY_THRESHOLD: &str = "anomaly-threshold";
/// CLI flag for the window option (`--anomaly-window`).
pub const FLAG_ANOMALY_WINDOW: &str = "anomaly-window";

/// Default Z-score threshold (`float32(2.0)`).
pub const DEFAULT_ANOMALY_THRESHOLD: f32 = 2.0;
/// Default sliding window size (20 ticks).
pub const DEFAULT_ANOMALY_WINDOW_SIZE: usize = 20;

/// Minimum valid sliding window size.
pub const MIN_WINDOW_SIZE: usize = 2;
/// Minimum valid Z-score threshold (`float32(0.1)`).
pub const MIN_THRESHOLD: f32 = 0.1;

/// Read-only analyzer configuration after validation.
///
/// Mirrors the `Threshold` / `WindowSize` fields of Go `Analyzer`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Z-score threshold (standard deviations).
    pub threshold: f32,
    /// Sliding window size in ticks.
    pub window_size: usize,
}

impl Default for Config {
    /// Returns the validated defaults (`threshold = 2.0`, `window = 20`).
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ANOMALY_THRESHOLD,
            window_size: DEFAULT_ANOMALY_WINDOW_SIZE,
        }
    }
}

impl Config {
    /// Clamps out-of-range values back to defaults.
    ///
    /// Mirrors Go `Analyzer.validate`: a threshold below [`MIN_THRESHOLD`] or a
    /// window below [`MIN_WINDOW_SIZE`] falls back to the default.
    pub fn validate(&mut self) {
        if self.threshold < MIN_THRESHOLD {
            self.threshold = DEFAULT_ANOMALY_THRESHOLD;
        }

        if self.window_size < MIN_WINDOW_SIZE {
            self.window_size = DEFAULT_ANOMALY_WINDOW_SIZE;
        }
    }

    /// Applies optional threshold/window overrides then validates.
    ///
    /// Mirrors Go `Analyzer.Configure`: only present facts override the current
    /// value (Go's `facts[key].(T)` type-assert success), then `validate` runs.
    pub fn apply(&mut self, threshold: Option<f32>, window_size: Option<usize>) {
        if let Some(t) = threshold {
            self.threshold = t;
        }
        if let Some(w) = window_size {
            self.window_size = w;
        }
        self.validate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_applies_valid_values() {
        // Mirrors Go TestAnalyzer_Configure.
        let mut cfg = Config::default();
        cfg.apply(Some(3.0), Some(30));
        assert!((cfg.threshold - 3.0).abs() < 0.001);
        assert_eq!(cfg.window_size, 30);
    }

    #[test]
    fn configure_clamps_invalid_to_defaults() {
        // Mirrors Go TestAnalyzer_Configure_Validation.
        let mut cfg = Config::default();
        cfg.apply(Some(-1.0), Some(0));
        assert!((cfg.threshold - DEFAULT_ANOMALY_THRESHOLD).abs() < 0.001);
        assert_eq!(cfg.window_size, DEFAULT_ANOMALY_WINDOW_SIZE);
    }

    #[test]
    fn default_is_validated() {
        // Mirrors Go TestAnalyzer_Initialize (defaults after validate).
        let cfg = Config::default();
        assert!((cfg.threshold - DEFAULT_ANOMALY_THRESHOLD).abs() < 0.001);
        assert_eq!(cfg.window_size, DEFAULT_ANOMALY_WINDOW_SIZE);
    }

    #[test]
    fn window_below_min_falls_back() {
        let mut cfg = Config {
            threshold: 2.0,
            window_size: 1,
        };
        cfg.validate();
        assert_eq!(cfg.window_size, DEFAULT_ANOMALY_WINDOW_SIZE);
    }
}
