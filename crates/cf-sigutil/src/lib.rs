//! Signal-handling utilities for graceful cleanup.
//!
//! Provides [`SignalCleanupGuard`], which ensures a cleanup function runs
//! **exactly once** when the process exits normally, on error, or via
//! `SIGINT` / `SIGTERM`.
//!
//! # Guarantees
//!
//! * [`SignalCleanupGuard::new`] registers `SIGINT` and `SIGTERM` handlers and
//!   spawns a background thread that, on the first such signal, logs a warning
//!   ("signal received, running cleanup guard") and runs the cleanup.
//! * Passing [`None`] for the cleanup yields a no-op guard.
//! * [`SignalCleanupGuard::close`] runs the cleanup (if not already run),
//!   deregisters the signal handler, and stops the background thread.
//! * The cleanup runs **exactly once** regardless of how many signals arrive or
//!   how many times `close` is called, enforced by a [`std::sync::Once`].
//! * `close` is also invoked from [`Drop`], so a guard whose scope ends without
//!   an explicit `close()` still runs cleanup.
//!
//! # Logging
//!
//! Logging is optional and pluggable via the [`Logger`] trait. Pass [`None`]
//! for a discard/no-op logger.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicBool, Ordering};
//! use cf_sigutil::SignalCleanupGuard;
//!
//! let ran = Arc::new(AtomicBool::new(false));
//! let flag = Arc::clone(&ran);
//!
//! let mut guard = SignalCleanupGuard::new(
//!     Some(Box::new(move || {
//!         // ... release resources, flush state, remove temp dirs ...
//!         flag.store(true, Ordering::SeqCst);
//!     })),
//!     None, // discard logger
//! );
//!
//! // ... do work; if SIGINT/SIGTERM arrives the cleanup runs automatically ...
//!
//! // Run cleanup now (Drop would also do it). Cleanup runs exactly once.
//! guard.close();
//! assert!(ran.load(Ordering::SeqCst));
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use std::thread::JoinHandle;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

/// The cleanup closure type.
///
/// `Send` is required because the closure may be invoked from the background
/// signal-handling thread.
pub type CleanupFn = Box<dyn FnOnce() + Send + 'static>;

/// A minimal warning-level logging sink.
///
/// Implementations receive the warning message plus the signal name so they can
/// emit a structured log line such as
/// `signal received, running cleanup guard signal=interrupt`.
pub trait Logger: Send + Sync {
    /// Emit a warning. `signal` is the textual signal name (e.g. `"interrupt"`,
    /// `"terminated"`).
    fn warn(&self, msg: &str, signal: &str);
}

/// A [`Logger`] that discards everything.
struct DiscardLogger;

impl Logger for DiscardLogger {
    fn warn(&self, _msg: &str, _signal: &str) {}
}

/// Ensures a cleanup function runs exactly once when the process exits normally,
/// on error, or via `SIGINT` / `SIGTERM`.
///
/// Construct one via [`SignalCleanupGuard::new`] and call [`close`] on scope
/// exit; [`Drop`] also calls [`close`] as a safety net.
///
/// [`close`]: SignalCleanupGuard::close
pub struct SignalCleanupGuard {
    shared: Arc<Shared>,
    /// Handle to the registered signal source, used by [`close`] / [`Drop`] to
    /// deregister handlers and unblock the listener thread. `Option` so it is
    /// taken exactly once.
    ///
    /// [`close`]: SignalCleanupGuard::close
    handle: Option<signal_hook::iterator::Handle>,
    /// Handle to the background signal-listening thread. `Option` so [`close`]
    /// / [`Drop`] can `take` and join it exactly once.
    ///
    /// [`close`]: SignalCleanupGuard::close
    listener: Option<JoinHandle<()>>,
    /// Guards against running the [`close`](Self::close) logic twice when it is
    /// called both explicitly and from [`Drop`].
    closed: bool,
}

/// State shared between the guard handle and the background listener thread.
struct Shared {
    /// `Option` so we can `take` the closure and run it inside the `Once` —
    /// `FnOnce` can only be called by value. The `Once` guarantees the take
    /// happens exactly once even across threads.
    cleanup: std::sync::Mutex<Option<CleanupFn>>,
    once: Once,
    logger: Box<dyn Logger>,
    /// Set when the guard is closing so the listener thread, if it wakes for any
    /// reason other than a real signal, exits without further work.
    closing: AtomicBool,
}

impl Shared {
    /// Runs the cleanup exactly once. Subsequent calls are no-ops.
    fn run(&self) {
        self.once.call_once(|| {
            // Take the closure out; FnOnce must be moved to be called. Release
            // the lock before invoking the cleanup.
            let cleanup = self
                .cleanup
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(cleanup) = cleanup {
                cleanup();
            }
        });
    }
}

impl SignalCleanupGuard {
    /// Registers `SIGINT` and `SIGTERM` handlers that invoke `cleanup` exactly
    /// once. The caller should call [`close`](Self::close) (or drop the guard)
    /// to ensure cleanup runs on normal/error exit and the signal handler is
    /// deregistered.
    ///
    /// * `cleanup` — the work to run on signal/close. [`None`] is treated as a
    ///   no-op.
    /// * `logger` — where to emit the warning when a signal triggers cleanup.
    ///   [`None`] selects a discard logger.
    ///
    /// # Panics
    ///
    /// Panics only if the OS refuses to register the signal handlers (i.e.
    /// `signal_hook::iterator::Signals::new` fails). In practice
    /// `SIGINT`/`SIGTERM` registration does not fail on supported platforms.
    #[must_use]
    pub fn new(cleanup: Option<CleanupFn>, logger: Option<Box<dyn Logger>>) -> Self {
        let shared = Arc::new(Shared {
            cleanup: std::sync::Mutex::new(cleanup),
            once: Once::new(),
            logger: logger.unwrap_or_else(|| Box::new(DiscardLogger)),
            closing: AtomicBool::new(false),
        });

        let mut signals =
            Signals::new([SIGINT, SIGTERM]).expect("failed to register OS signal handler");
        let handle = signals.handle();

        let listener_shared = Arc::clone(&shared);
        let listener = std::thread::spawn(move || {
            // Block until the first signal, or until the handle is closed by
            // `close` (which makes `forever()` yield `None`).
            if let Some(sig) = signals.forever().next() {
                if listener_shared.closing.load(Ordering::SeqCst) {
                    // Woken by close (handle closed) rather than a genuine
                    // signal — do not log, do not duplicate cleanup; close
                    // already ran it.
                    return;
                }
                listener_shared
                    .logger
                    .warn("signal received, running cleanup guard", signal_name(sig));
                listener_shared.run();
            }
        });

        Self {
            shared,
            handle: Some(handle),
            listener: Some(listener),
            closed: false,
        }
    }

    /// Performs cleanup (if not already done) and deregisters the signal
    /// handler. Safe to call multiple times; cleanup still runs only once.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicUsize, Ordering};
    /// use cf_sigutil::SignalCleanupGuard;
    ///
    /// let calls = Arc::new(AtomicUsize::new(0));
    /// let c = Arc::clone(&calls);
    /// let mut guard =
    ///     SignalCleanupGuard::new(Some(Box::new(move || { c.fetch_add(1, Ordering::SeqCst); })), None);
    ///
    /// // Calling close repeatedly is safe; cleanup still runs exactly once.
    /// guard.close();
    /// guard.close();
    /// assert_eq!(calls.load(Ordering::SeqCst), 1);
    /// ```
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;

        // Run cleanup exactly once.
        self.shared.run();

        // Mark closing so the listener, if woken by the handle close (rather
        // than a real signal), takes the no-op path.
        self.shared.closing.store(true, Ordering::SeqCst);

        // Deregister handlers and unblock the listener's `forever()` so it
        // returns and the thread can exit.
        if let Some(handle) = self.handle.take() {
            handle.close();
        }

        // Join the background thread so it does not outlive the guard.
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

impl Drop for SignalCleanupGuard {
    fn drop(&mut self) {
        // Safety net: ensure cleanup runs and the listener thread is torn down
        // even if `close` was never called.
        self.close();
    }
}

/// Best-effort textual name for a raw signal number, for the two signals this
/// crate registers.
const fn signal_name(sig: i32) -> &'static str {
    match sig {
        SIGINT => "interrupt",
        SIGTERM => "terminated",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    /// Mirrors reference test `TestSignalCleanupGuard_CleanupOnClose`.
    #[test]
    fn cleanup_on_close() {
        let called = Arc::new(AtomicBool::new(false));
        let c = Arc::clone(&called);
        let mut guard = SignalCleanupGuard::new(
            Some(Box::new(move || {
                c.store(true, Ordering::SeqCst);
            })),
            None,
        );
        guard.close();
        assert!(
            called.load(Ordering::SeqCst),
            "cleanup must be called on Close"
        );
    }

    /// Mirrors reference test `TestSignalCleanupGuard_IdempotentClose`.
    #[test]
    fn idempotent_close() {
        let count = Arc::new(AtomicI32::new(0));
        let c = Arc::clone(&count);
        let mut guard = SignalCleanupGuard::new(
            Some(Box::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            })),
            None,
        );
        guard.close();
        // Close is safe to call multiple times but cleanup runs once.
        guard.close();
        guard.close();
        assert_eq!(1, count.load(Ordering::SeqCst));
    }

    /// Mirrors reference test `TestSignalCleanupGuard_NilCleanup`.
    #[test]
    fn nil_cleanup() {
        let mut guard = SignalCleanupGuard::new(None, None);
        guard.close(); // Must not panic.
    }

    /// Mirrors reference test `TestSignalCleanupGuard_MultipleCleanersViaClosure`.
    #[test]
    fn multiple_cleaners_via_closure() {
        let c1 = Arc::new(AtomicI32::new(0));
        let c2 = Arc::new(AtomicI32::new(0));
        let c1c = Arc::clone(&c1);
        let c2c = Arc::clone(&c2);
        let mut guard = SignalCleanupGuard::new(
            Some(Box::new(move || {
                c1c.fetch_add(1, Ordering::SeqCst);
                c2c.fetch_add(1, Ordering::SeqCst);
            })),
            None,
        );
        guard.close();
        assert_eq!(1, c1.load(Ordering::SeqCst));
        assert_eq!(1, c2.load(Ordering::SeqCst));
    }

    /// Drop runs cleanup as a safety net (beyond the reference suite).
    #[test]
    fn drop_runs_cleanup() {
        let called = Arc::new(AtomicBool::new(false));
        let c = Arc::clone(&called);
        {
            let _guard = SignalCleanupGuard::new(
                Some(Box::new(move || {
                    c.store(true, Ordering::SeqCst);
                })),
                None,
            );
            // No explicit close(); Drop must run cleanup.
        }
        assert!(called.load(Ordering::SeqCst), "Drop must run cleanup once");
    }

    /// A custom logger is invoked only when a real signal triggers cleanup; on a
    /// plain close it is not called. We can only assert the close path here
    /// (sending a real signal to the test process is unsafe under the runner).
    #[test]
    fn close_path_does_not_log() {
        struct Counting(Arc<AtomicI32>);
        impl Logger for Counting {
            fn warn(&self, _m: &str, _s: &str) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let n = Arc::new(AtomicI32::new(0));
        let mut guard = SignalCleanupGuard::new(None, Some(Box::new(Counting(Arc::clone(&n)))));
        guard.close();
        assert_eq!(0, n.load(Ordering::SeqCst), "close path must not log");
    }

    /// `signal_name` maps the two registered signals to their textual names.
    #[test]
    fn signal_names() {
        assert_eq!(signal_name(SIGINT), "interrupt");
        assert_eq!(signal_name(SIGTERM), "terminated");
    }
}
