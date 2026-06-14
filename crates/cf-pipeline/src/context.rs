//! A minimal cancellation handle for pipeline code.
//!
//! Pipeline code only ever needs two capabilities:
//!
//! - [`Ctx::err`] / [`Ctx::is_cancelled`] — observe whether the run has been
//!   cancelled.
//! - [`Ctx::with_cancel`] — derive a child that the worker pool cancels on
//!   the first error.
//!
//! Deadlines, attached values, and wakeup channels are never needed here, so
//! [`Ctx`] intentionally models only cancellation. It is cheap to clone (an
//! `Arc` around an atomic flag) and is `Send + Sync`, so it can be handed to
//! worker threads freely.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The error reported by a cancelled [`Ctx`].
///
/// The `"context canceled"` message is part of the CLI error-text contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContextError {
    /// The context was cancelled.
    #[error("context canceled")]
    Canceled,
}

#[derive(Debug)]
struct Inner {
    cancelled: AtomicBool,
}

/// A clonable cancellation handle.
///
/// Clones share cancellation state, so every holder observes a `cancel()`
/// call. Use [`Ctx::with_cancel`] to derive a child whose cancellation does
/// not propagate back to the parent.
#[derive(Debug, Clone)]
pub struct Ctx {
    inner: Arc<Inner>,
}

impl Ctx {
    /// Returns a fresh, never-cancelled context.
    #[must_use]
    pub fn background() -> Self {
        Ctx {
            inner: Arc::new(Inner {
                cancelled: AtomicBool::new(false),
            }),
        }
    }

    /// Derives a child context plus a cancel handle.
    ///
    /// The child starts cancelled if the parent is already cancelled when it
    /// is derived; afterwards it is cancelled by the returned [`CancelFn`].
    /// Calling the cancel function does not affect the parent.
    #[must_use]
    pub fn with_cancel(&self) -> (Ctx, CancelFn) {
        let child = Ctx {
            inner: Arc::new(Inner {
                cancelled: AtomicBool::new(self.is_cancelled()),
            }),
        };
        let cancel = CancelFn {
            inner: child.inner.clone(),
        };
        (child, cancel)
    }

    /// Cancels this context (and every clone that shares its state).
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }

    /// Reports whether the context has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Returns the cancellation error if cancelled, `None` while live.
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
/// idempotent. Dropping the handle is intentionally a no-op, so holding it
/// does not auto-cancel the context.
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
