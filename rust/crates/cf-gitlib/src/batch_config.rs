//! Batch processing configuration, ported from `pkg/gitlib/batch_config.go`.

/// Default number of blobs to load per batch (Go `defaultBlobBatchSize`).
const DEFAULT_BLOB_BATCH_SIZE: i32 = 100;

/// Default number of diffs to compute per batch (Go `defaultDiffBatchSize`).
const DEFAULT_DIFF_BATCH_SIZE: i32 = 50;

/// Batch processing parameters (Go `gitlib.BatchConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchConfig {
    /// Number of blobs to load per batch (default 100).
    pub blob_batch_size: i32,
    /// Number of diffs to compute per batch (default 50).
    pub diff_batch_size: i32,
    /// Number of parallel workers (default 1; sequential within gitlib).
    pub workers: i32,
}

impl Default for BatchConfig {
    /// Returns the default batch configuration (Go `DefaultBatchConfig`).
    fn default() -> Self {
        BatchConfig {
            blob_batch_size: DEFAULT_BLOB_BATCH_SIZE,
            diff_batch_size: DEFAULT_DIFF_BATCH_SIZE,
            workers: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from benchmark_test.go::BenchmarkBatchConfig (the default values).
    #[test]
    fn default_batch_config() {
        let c = BatchConfig::default();
        assert_eq!(c.blob_batch_size, 100);
        assert_eq!(c.diff_batch_size, 50);
        assert_eq!(c.workers, 1);
    }
}
