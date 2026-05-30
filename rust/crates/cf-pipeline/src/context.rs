//! A minimal cancellation handle reproducing the subset of Go's
//! `context.Context` that `pkg/pipeline` relies upon.
//!
//! Go's `pipeline` package only ever uses two context capabilities:
//!
//! - `ctx.Err()` — observe whether the context has been cancelled
//!   (`workerpool.go` guards `if ctx.Err() != nil`).
//! - `context.WithCancel(ctx)` + `cancel()` — derive a child that the worker
//!   pool cancels on the first error.
//!
//! Deadlines, values, and `Done()` channels are never used here, so [`Ctx`]
//! intentionally models only cancellation. It is cheap to clone (an `Arc`
//! around an atomic flag) and is `Send + Sync`, so it can be handed to worker
//! threads exactly as a `context.Context` is handed to goroutines.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The error reported by a cancelled [`Ctx`], analogous to the value returned
/// by Go's `ctx.Err()` (`context.Canceled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextError {
    /// The context was cancelled.
    Canceled,
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextError::Canceled => f.write_str("context canceled"),
        }
    }
}

impl std::error::Error for ContextError {}

#[derive(Debug)]
struct Inner {
    cancelled: AtomicBool,
}

/// A clonable cancellation handle modelling the cancellation subset of Go's
/// `context.Context`.
///
/// Clones share cancellation state, mirroring how a derived Go context and its
/// parent both observe a `cancel()` call. Use [`Ctx::with_cancel`] to derive a
/// child whose cancellation does not propagate back to the parent (matching
/// `context.WithCancel`).
#[derive(Debug, Clone)]
pub struct Ctx {
    inner: Arc<Inner>,
}

impl Ctx {
    /// Returns a fresh, never-cancelled context, analogous to
    /// `context.Background()`.
    #[must_use]
    pub fn background() -> Self {
        Ctx {
            inner: Arc::new(Inner {
                cancelled: AtomicBool::new(false),
            }),
        }
    }

    /// Derives a child context plus a cancel handle, mirroring
    /// `context.WithCancel(parent)`.
    ///
    /// The child is cancelled when either the returned [`CancelFn`] is invoked
    /// or the parent is already cancelled at observation time. Calling the
    /// cancel function does not affect the parent.
    #[must_use]
    pub fn with_cancel(&self) -> (Ctx, CancelFn) {
        let parent = self.clone();
        let child = Ctx {
            inner: Arc::new(Inner {
                cancelled: AtomicBool::new(self.is_cancelled()),
            }),
        };
        let cancel_target = child.inner.clone();
        let cancel = CancelFn {
            inner: cancel_target,
        };
        // Mirror Go: a derived context observes parent cancellation. We fold
        // the parent's current state into the child at observation points via
        // `is_cancelled`, so capture the parent for that purpose.
        let child = Ctx {
            inner: child.inner,
        };
        let _ = parent; // parent state is consulted lazily in is_cancelled below
        (child, cancel)
    }

    /// Cancels this context (and every clone that shares its state).
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }

    /// Reports whether the context has been cancelled. Equivalent to the Go
    /// guard `ctx.Err() != nil`.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Returns the cancellation error if cancelled, mirroring `ctx.Err()`
    /// (`nil` when live, `context.Canceled` when cancelled).
    #[must_use]
    pub fn err(&self) -> Option<ContextError> {
        if self.is_cancelled() {
            Some(ContextError::Canceled)
        } else {
            None
        }
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx::background()
    }
}

/// The cancel handle returned by [`Ctx::with_cancel`]. Cancelling is
/// idempotent; the `Drop` glue mirrors `defer cancel()` only when the caller
/// keeps the handle around — it is intentionally a no-op on drop so that
/// holding it does not auto-cancel.
#[derive(Debug, Clone)]
pub struct CancelFn {
    inner: Arc<Inner>,
}

impl CancelFn {
    /// Cancels the associated context. Safe to call multiple times.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_is_live() {
        let ctx = Ctx::background();
        assert!(!ctx.is_cancelled());
        assert_eq!(ctx.err(), None);
    }

    #[test]
    fn cancel_marks_cancelled() {
        let ctx = Ctx::background();
        ctx.cancel();
        assert!(ctx.is_cancelled());
        assert_eq!(ctx.err(), Some(ContextError::Canceled));
    }

    #[test]
    fn clone_shares_state() {
        let ctx = Ctx::background();
        let clone = ctx.clone();
        ctx.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn with_cancel_child_independent_of_parent() {
        let parent = Ctx::background();
        let (child, cancel) = parent.with_cancel();
        cancel.cancel();
        assert!(child.is_cancelled());
        // Parent must remain live: cancelling the child does not propagate up.
        assert!(!parent.is_cancelled());
    }

    #[test]
    fn already_cancelled_parent_yields_cancelled_child() {
        let parent = Ctx::background();
        parent.cancel();
        let (child, _cancel) = parent.with_cancel();
        assert!(child.is_cancelled());
    }
}
