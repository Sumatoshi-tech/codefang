//! Analyzer configuration: the Z-score threshold and the trailing window,
//! validated once at the boundary.

use std::fmt;

/// CLI flag for the threshold option (`--anomaly-threshold`).
pub const FLAG_ANOMALY_THRESHOLD: &str = "anomaly-threshold";
/// CLI flag for the window option (`--anomaly-window`).
pub const FLAG_ANOMALY_WINDOW: &str = "anomaly-window";

/// Default Z-score threshold (standard deviations).
pub const DEFAULT_ANOMALY_THRESHOLD: f32 = 2.0;
/// Default trailing window size in ticks.
pub const DEFAULT_ANOMALY_WINDOW_SIZE: usize = 20;

/// Minimum valid window size: a Z-score needs at least two observations.
pub const MIN_WINDOW_SIZE: usize = 2;

/// A rejected configuration value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    /// `--anomaly-window` below [`MIN_WINDOW_SIZE`].
    WindowTooSmall(usize),
    /// `--anomaly-threshold` not strictly positive (or not a number).
    ThresholdNotPositive(f32),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::WindowTooSmall(got) => write!(
                f,
                "invalid --{FLAG_ANOMALY_WINDOW}: must be >= {MIN_WINDOW_SIZE}, got {got}"
            ),
            ConfigError::ThresholdNotPositive(got) => {
                write!(
                    f,
                    "invalid --{FLAG_ANOMALY_THRESHOLD}: must be > 0, got {got}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Validated analyzer configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Z-score threshold (standard deviations); a tick is flagged when any
    /// metric's `|Z|` strictly exceeds it.
    pub threshold: f32,
    /// Trailing window size in ticks.
    pub window_size: usize,
}

impl Default for Config {
    /// The defaults: `threshold = 2.0`, `window_size = 20`.
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ANOMALY_THRESHOLD,
            window_size: DEFAULT_ANOMALY_WINDOW_SIZE,
        }
    }
}

impl Config {
    /// Builds a configuration, rejecting a window below [`MIN_WINDOW_SIZE`]
    /// and a threshold that is not strictly positive.
    ///
    /// # Errors
    ///
    /// The [`ConfigError`] naming the rejected value.
    ///
    /// ```
    /// use cf_anomaly::config::{Config, ConfigError};
    ///
    /// assert_eq!(Config::new(3.0, 30), Ok(Config { threshold: 3.0, window_size: 30 }));
    /// assert_eq!(Config::new(2.0, 1), Err(ConfigError::WindowTooSmall(1)));
    /// assert_eq!(Config::new(0.0, 20), Err(ConfigError::ThresholdNotPositive(0.0)));
    /// assert_eq!(
    ///     Config::new(2.0, 1).unwrap_err().to_string(),
    ///     "invalid --anomaly-window: must be >= 2, got 1"
    /// );
    /// ```
    pub fn new(threshold: f32, window_size: usize) -> Result<Self, ConfigError> {
        if window_size < MIN_WINDOW_SIZE {
            return Err(ConfigError::WindowTooSmall(window_size));
        }
        if threshold.is_nan() || threshold <= 0.0 {
            return Err(ConfigError::ThresholdNotPositive(threshold));
        }
        Ok(Self {
            threshold,
            window_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        let cfg = Config::default();
        assert_eq!(
            Config::new(cfg.threshold, cfg.window_size),
            Ok(Config::default())
        );
        assert!((cfg.threshold - DEFAULT_ANOMALY_THRESHOLD).abs() < 0.001);
        assert_eq!(cfg.window_size, DEFAULT_ANOMALY_WINDOW_SIZE);
    }

    #[test]
    fn rejects_window_below_two() {
        assert_eq!(Config::new(2.0, 0), Err(ConfigError::WindowTooSmall(0)));
        assert_eq!(Config::new(2.0, 1), Err(ConfigError::WindowTooSmall(1)));
        assert!(Config::new(2.0, 2).is_ok());
    }

    #[test]
    fn rejects_non_positive_or_nan_threshold() {
        assert_eq!(
            Config::new(-1.0, 20),
            Err(ConfigError::ThresholdNotPositive(-1.0))
        );
        assert!(matches!(
            Config::new(f32::NAN, 20),
            Err(ConfigError::ThresholdNotPositive(_))
        ));
        assert!(Config::new(0.5, 20).is_ok());
    }

    #[test]
    fn errors_name_the_flag() {
        assert_eq!(
            ConfigError::ThresholdNotPositive(0.0).to_string(),
            "invalid --anomaly-threshold: must be > 0, got 0"
        );
    }
}
