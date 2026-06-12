//! Per-stage high-watermark counters for memory triage.
//!
//! All fields are updated atomically by pipeline stages and read by the
//! sampler. Following the playbook: "diff items queued", "bytes of blob
//! content held", "AST cache entries", "results map size".
//!
//! Every field is an [`AtomicI64`](std::sync::atomic::AtomicI64); the
//! high-watermark updates use a CAS loop so that concurrent `record_*`
//! callers never lose a peak.

use std::sync::atomic::{AtomicI64, Ordering};

/// Per-stage high-watermark counters for memory triage.
///
/// Each counter is atomic; high-watermark counters (`peak_*`) never decrease
/// within a chunk (see [`Self::reset`]).
#[derive(Debug, Default)]
pub struct StageMetrics {
    // Blob pipeline metrics.
    /// Number of file changes being processed.
    pub blob_changes_in_flight: AtomicI64,
    /// Total blob bytes loaded in current batch.
    pub blob_bytes_loaded: AtomicI64,
    /// Current global blob cache entry count.
    pub blob_cache_entries: AtomicI64,
    /// Current global blob cache byte size.
    pub blob_cache_bytes: AtomicI64,

    // Diff pipeline metrics.
    /// Diff requests pending in batcher.
    pub diff_items_queued: AtomicI64,
    /// Current diff cache entry count.
    pub diff_cache_entries: AtomicI64,

    // UAST pipeline metrics.
    /// UAST parse jobs pending.
    pub uast_items_queued: AtomicI64,

    // Runner / aggregator metrics.
    /// Estimated aggregator state size.
    pub aggregator_bytes: AtomicI64,
    /// Commits processed in current chunk.
    pub commits_processed: AtomicI64,
    /// File changes in most recent commit.
    pub last_change_count: AtomicI64,

    // High-watermarks (updated by Record* methods, never decrease within a chunk).
    /// Max changes seen in any single commit.
    pub peak_blob_changes: AtomicI64,
    /// Max blob bytes loaded in any single batch.
    pub peak_blob_bytes: AtomicI64,
    /// Max diff items queued at any point.
    pub peak_diff_queued: AtomicI64,
}

/// Lock-free `peak = max(peak, value)` via a CAS loop.
fn store_max(target: &AtomicI64, value: i64) {
    loop {
        let peak = target.load(Ordering::SeqCst);
        if value <= peak
            || target
                .compare_exchange(peak, value, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            break;
        }
    }
}

impl StageMetrics {
    /// Creates a fresh, all-zero metrics block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates blob metrics and high-watermarks for a batch.
    pub fn record_blob_batch(&self, changes: i64, blob_bytes: i64) {
        self.blob_changes_in_flight.store(changes, Ordering::SeqCst);
        self.blob_bytes_loaded.store(blob_bytes, Ordering::SeqCst);
        store_max(&self.peak_blob_changes, changes);
        store_max(&self.peak_blob_bytes, blob_bytes);
    }

    /// Updates diff queue depth and high-watermark.
    pub fn record_diff_queue(&self, queued: i64) {
        self.diff_items_queued.store(queued, Ordering::SeqCst);
        store_max(&self.peak_diff_queued, queued);
    }

    /// Updates per-commit metrics.
    pub fn record_commit(&self, change_count: i64) {
        self.commits_processed.fetch_add(1, Ordering::SeqCst);
        self.last_change_count.store(change_count, Ordering::SeqCst);
        store_max(&self.peak_blob_changes, change_count);
    }

    /// Clears all counters and watermarks for a new chunk.
    pub fn reset(&self) {
        self.blob_changes_in_flight.store(0, Ordering::SeqCst);
        self.blob_bytes_loaded.store(0, Ordering::SeqCst);
        self.blob_cache_entries.store(0, Ordering::SeqCst);
        self.blob_cache_bytes.store(0, Ordering::SeqCst);
        self.diff_items_queued.store(0, Ordering::SeqCst);
        self.diff_cache_entries.store(0, Ordering::SeqCst);
        self.uast_items_queued.store(0, Ordering::SeqCst);
        self.aggregator_bytes.store(0, Ordering::SeqCst);
        self.commits_processed.store(0, Ordering::SeqCst);
        self.last_change_count.store(0, Ordering::SeqCst);
        self.peak_blob_changes.store(0, Ordering::SeqCst);
        self.peak_blob_bytes.store(0, Ordering::SeqCst);
        self.peak_diff_queued.store(0, Ordering::SeqCst);
    }

    /// Reads all counters atomically (each field individually).
    ///
    /// This is NOT a consistent cut across all fields; each load is
    /// independent.
    #[must_use]
    pub fn snapshot(&self) -> StageMetricsSnapshot {
        StageMetricsSnapshot {
            blob_changes_in_flight: self.blob_changes_in_flight.load(Ordering::SeqCst),
            blob_bytes_loaded: self.blob_bytes_loaded.load(Ordering::SeqCst),
            blob_cache_entries: self.blob_cache_entries.load(Ordering::SeqCst),
            blob_cache_bytes: self.blob_cache_bytes.load(Ordering::SeqCst),
            diff_items_queued: self.diff_items_queued.load(Ordering::SeqCst),
            diff_cache_entries: self.diff_cache_entries.load(Ordering::SeqCst),
            uast_items_queued: self.uast_items_queued.load(Ordering::SeqCst),
            aggregator_bytes: self.aggregator_bytes.load(Ordering::SeqCst),
            commits_processed: self.commits_processed.load(Ordering::SeqCst),
            last_change_count: self.last_change_count.load(Ordering::SeqCst),
            peak_blob_changes: self.peak_blob_changes.load(Ordering::SeqCst),
            peak_blob_bytes: self.peak_blob_bytes.load(Ordering::SeqCst),
            peak_diff_queued: self.peak_diff_queued.load(Ordering::SeqCst),
        }
    }
}

/// A point-in-time copy of all stage metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageMetricsSnapshot {
    /// See [`StageMetrics::blob_changes_in_flight`].
    pub blob_changes_in_flight: i64,
    /// See [`StageMetrics::blob_bytes_loaded`].
    pub blob_bytes_loaded: i64,
    /// See [`StageMetrics::blob_cache_entries`].
    pub blob_cache_entries: i64,
    /// See [`StageMetrics::blob_cache_bytes`].
    pub blob_cache_bytes: i64,
    /// See [`StageMetrics::diff_items_queued`].
    pub diff_items_queued: i64,
    /// See [`StageMetrics::diff_cache_entries`].
    pub diff_cache_entries: i64,
    /// See [`StageMetrics::uast_items_queued`].
    pub uast_items_queued: i64,
    /// See [`StageMetrics::aggregator_bytes`].
    pub aggregator_bytes: i64,
    /// See [`StageMetrics::commits_processed`].
    pub commits_processed: i64,
    /// See [`StageMetrics::last_change_count`].
    pub last_change_count: i64,
    /// See [`StageMetrics::peak_blob_changes`].
    pub peak_blob_changes: i64,
    /// See [`StageMetrics::peak_blob_bytes`].
    pub peak_blob_bytes: i64,
    /// See [`StageMetrics::peak_diff_queued`].
    pub peak_diff_queued: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn record_blob_batch_updates_peaks() {
        let m = StageMetrics::new();
        m.record_blob_batch(10, 100);
        m.record_blob_batch(5, 50); // lower — must not lower peaks.

        let s = m.snapshot();
        assert_eq!(s.blob_changes_in_flight, 5);
        assert_eq!(s.blob_bytes_loaded, 50);
        assert_eq!(s.peak_blob_changes, 10);
        assert_eq!(s.peak_blob_bytes, 100);
    }

    #[test]
    fn record_commit_increments_and_peaks() {
        let m = StageMetrics::new();
        m.record_commit(3);
        m.record_commit(7);
        m.record_commit(2);

        let s = m.snapshot();
        assert_eq!(s.commits_processed, 3);
        assert_eq!(s.last_change_count, 2);
        assert_eq!(s.peak_blob_changes, 7);
    }

    #[test]
    fn record_diff_queue_peak() {
        let m = StageMetrics::new();
        m.record_diff_queue(4);
        m.record_diff_queue(9);
        m.record_diff_queue(1);

        let s = m.snapshot();
        assert_eq!(s.diff_items_queued, 1);
        assert_eq!(s.peak_diff_queued, 9);
    }

    #[test]
    fn reset_clears_everything() {
        let m = StageMetrics::new();
        m.record_blob_batch(10, 100);
        m.record_commit(5);
        m.record_diff_queue(8);
        m.reset();
        assert_eq!(m.snapshot(), StageMetricsSnapshot::default());
    }

    #[test]
    fn concurrent_peak_never_lost() {
        let m = Arc::new(StageMetrics::new());
        let mut handles = Vec::new();
        for t in 0..8 {
            let m = Arc::clone(&m);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    m.record_blob_batch((t * 1000 + i) as i64, 0);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Highest value across all threads is 7*1000 + 999 = 7999.
        assert_eq!(m.snapshot().peak_blob_changes, 7999);
    }
}
