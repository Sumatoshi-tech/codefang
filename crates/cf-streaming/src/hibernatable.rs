//! Hibernation and spill-cleanup interfaces for streaming execution.

use cf_sigutil::SignalCleanupGuard;

/// Optional trait for analyzers that create spill files on disk during
/// hibernation. [`SpillCleaner::cleanup_spills`] removes all temp directories
/// and files. It is invoked by [`SpillCleanupGuard`] on normal exit, error
/// exit, and SIGTERM/SIGINT to prevent orphaned temp files.
pub trait SpillCleaner {
    /// Removes all spill temp directories and files created by this analyzer.
    fn cleanup_spills(&self);
}

/// Ensures that spill temp directories are removed when the streaming pipeline
/// exits, whether normally, on error, or via signal.
///
/// Construct via [`SpillCleanupGuard::new`] (or
/// [`from_boxed`](Self::from_boxed)) and call [`close`](Self::close) — or rely
/// on [`Drop`] — to run cleanup. Cleanup runs **exactly once**.
///
/// Wraps [`cf_sigutil::SignalCleanupGuard`], which owns the run-exactly-once
/// semantics, the [`Drop`] hook, and SIGTERM/SIGINT registration.
pub struct SpillCleanupGuard {
    inner: SignalCleanupGuard,
}

impl SpillCleanupGuard {
    /// Creates a guard that invokes [`SpillCleaner::cleanup_spills`] on every
    /// registered cleaner when [`close`](Self::close) (or [`Drop`]) runs.
    ///
    /// Cleaners must be `Send` because the underlying guard may run cleanup
    /// from a signal-handling thread.
    #[must_use]
    pub fn new<C>(cleaners: Vec<C>) -> Self
    where
        C: SpillCleaner + Send + 'static,
    {
        let cleanup = move || {
            for c in &cleaners {
                c.cleanup_spills();
            }
        };

        Self {
            inner: SignalCleanupGuard::new(Some(Box::new(cleanup)), None),
        }
    }

    /// Creates a guard from boxed trait objects, allowing a heterogeneous set
    /// of cleaners.
    #[must_use]
    pub fn from_boxed(cleaners: Vec<Box<dyn SpillCleaner + Send + 'static>>) -> Self {
        let cleanup = move || {
            for c in &cleaners {
                c.cleanup_spills();
            }
        };

        Self {
            inner: SignalCleanupGuard::new(Some(Box::new(cleanup)), None),
        }
    }

    /// Runs cleanup exactly once. Idempotent: subsequent calls are no-ops.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

/// Optional trait for analyzers that support hibernation. Implementors can have
/// their state compressed between chunks to reduce memory usage during
/// streaming execution.
pub trait Hibernatable {
    /// Compresses the analyzer's state to reduce memory usage. Called between
    /// chunks during streaming execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the analyzer's state cannot be compressed (for
    /// example, a spill write fails).
    fn hibernate(&mut self) -> Result<(), HibernateError>;

    /// Restores the analyzer from hibernated state. Called before processing a
    /// new chunk after hibernation.
    ///
    /// # Errors
    ///
    /// Returns an error if the analyzer's state cannot be restored.
    fn boot(&mut self) -> Result<(), HibernateError>;
}

/// Error returned by [`Hibernatable`] operations.
///
/// A concrete, inspectable error type so analyzers across crates can
/// interoperate. It carries a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HibernateError {
    /// Description of what went wrong.
    pub message: String,
}

impl HibernateError {
    /// Creates a new error with the given message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    struct MockSpillCleaner {
        calls: Arc<AtomicI32>,
    }

    impl SpillCleaner for MockSpillCleaner {
        fn cleanup_spills(&self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Mirrors reference test TestSpillCleanupGuard_CleanupOnClose.
    #[test]
    fn cleanup_on_close() {
        let c1 = Arc::new(AtomicI32::new(0));
        let c2 = Arc::new(AtomicI32::new(0));

        let mut guard = SpillCleanupGuard::new(vec![
            MockSpillCleaner { calls: c1.clone() },
            MockSpillCleaner { calls: c2.clone() },
        ]);
        guard.close();

        assert_eq!(c1.load(Ordering::SeqCst), 1);
        assert_eq!(c2.load(Ordering::SeqCst), 1);
    }

    // Mirrors reference test TestSpillCleanupGuard_IdempotentClose.
    #[test]
    fn idempotent_close() {
        let calls = Arc::new(AtomicI32::new(0));

        let mut guard = SpillCleanupGuard::new(vec![MockSpillCleaner {
            calls: calls.clone(),
        }]);
        guard.close();
        guard.close();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // Mirrors reference test TestSpillCleanupGuard_NilCleaners (empty cleaner set).
    #[test]
    fn nil_cleaners() {
        let mut guard: SpillCleanupGuard = SpillCleanupGuard::from_boxed(Vec::new());
        guard.close();
    }

    // Drop runs cleanup exactly once even if close was not called explicitly.
    #[test]
    fn drop_runs_cleanup_once() {
        let calls = Arc::new(AtomicI32::new(0));
        {
            let _guard = SpillCleanupGuard::new(vec![MockSpillCleaner {
                calls: calls.clone(),
            }]);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // Mock hibernatable mirroring the reference suite's mock.
    struct MockHibernatable {
        hibernate_count: i32,
        boot_count: i32,
    }

    impl Hibernatable for MockHibernatable {
        fn hibernate(&mut self) -> Result<(), HibernateError> {
            self.hibernate_count += 1;
            Ok(())
        }

        fn boot(&mut self) -> Result<(), HibernateError> {
            self.boot_count += 1;
            Ok(())
        }
    }

    fn hibernate_all(analyzers: &mut [&mut dyn Hibernatable]) -> Result<(), HibernateError> {
        for h in analyzers.iter_mut() {
            h.hibernate()?;
        }
        Ok(())
    }

    fn boot_all(analyzers: &mut [&mut dyn Hibernatable]) -> Result<(), HibernateError> {
        for h in analyzers.iter_mut() {
            h.boot()?;
        }
        Ok(())
    }

    // Mirrors reference test TestHibernateAnalyzers_SingleChunk_NoHibernation.
    #[test]
    fn single_chunk_no_hibernation() {
        let mut mock = MockHibernatable {
            hibernate_count: 0,
            boot_count: 0,
        };
        let chunks = [crate::ChunkBounds { start: 0, end: 10 }];

        for (i, _chunk) in chunks.iter().enumerate() {
            if i > 0 {
                let mut analyzers: [&mut dyn Hibernatable; 1] = [&mut mock];
                hibernate_all(&mut analyzers).unwrap();
                boot_all(&mut analyzers).unwrap();
            }
        }

        assert_eq!(mock.hibernate_count, 0);
        assert_eq!(mock.boot_count, 0);
    }

    // Mirrors reference test TestHibernateAnalyzers_MultipleChunks_Hibernates.
    #[test]
    fn multiple_chunks_hibernates() {
        let mut mock = MockHibernatable {
            hibernate_count: 0,
            boot_count: 0,
        };
        let chunks = [
            crate::ChunkBounds { start: 0, end: 10 },
            crate::ChunkBounds { start: 10, end: 20 },
            crate::ChunkBounds { start: 20, end: 30 },
        ];

        for (i, _chunk) in chunks.iter().enumerate() {
            if i > 0 {
                let mut analyzers: [&mut dyn Hibernatable; 1] = [&mut mock];
                hibernate_all(&mut analyzers).unwrap();
                boot_all(&mut analyzers).unwrap();
            }
        }

        // 3 chunks means 2 transitions.
        assert_eq!(mock.hibernate_count, 2);
        assert_eq!(mock.boot_count, 2);
    }

    // Mirrors reference test TestCollectHibernatables_MixedAnalyzers.
    #[test]
    fn collect_hibernatables_mixed() {
        let mut h1 = MockHibernatable {
            hibernate_count: 0,
            boot_count: 0,
        };
        let mut h2 = MockHibernatable {
            hibernate_count: 0,
            boot_count: 0,
        };

        {
            let mut hibernatables: [&mut dyn Hibernatable; 2] = [&mut h1, &mut h2];
            hibernate_all(&mut hibernatables).unwrap();
        }

        assert_eq!(h1.hibernate_count, 1);
        assert_eq!(h2.hibernate_count, 1);
    }
}
