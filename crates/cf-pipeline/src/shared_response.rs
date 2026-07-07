//! `SharedResponse` — evaluate a computation exactly once.

use crate::context::Ctx;
use std::sync::Mutex;
use std::sync::Once;

/// The deferred computation a [`SharedResponse`] evaluates on first access.
type ComputeFn<T, E> = Box<dyn FnOnce(&Ctx) -> Result<T, E> + Send>;

/// Evaluates a computation exactly once and caches the result for concurrent
/// access by multiple threads. The computation receives a [`Ctx`] for
/// cancellation support.
///
/// The context passed to [`SharedResponse::get`] calls after the first is
/// ignored: only the first caller's context drives the computation.
///
/// The result and error are required to be [`Clone`] so that each caller gets
/// the cached outcome by value.
pub struct SharedResponse<T, E> {
    once: Once,
    // `Mutex<Option<...>>` holds the memoized outcome. It is written exactly
    // once (inside `once.call_once`) and only read afterwards, so contention is
    // limited to the cloning copy-out.
    result: Mutex<Option<Result<T, E>>>,
    compute: Mutex<Option<ComputeFn<T, E>>>,
}

impl<T, E> SharedResponse<T, E>
where
    T: Clone + Send,
    E: Clone + Send,
{
    /// Creates a [`SharedResponse`] that will evaluate `compute` on the first
    /// call to [`SharedResponse::get`].
    pub fn new<F>(compute: F) -> Self
    where
        F: FnOnce(&Ctx) -> Result<T, E> + Send + 'static,
    {
        SharedResponse {
            once: Once::new(),
            result: Mutex::new(None),
            compute: Mutex::new(Some(Box::new(compute))),
        }
    }

    /// Evaluates the compute function exactly once (via [`Once`]) and returns
    /// a clone of the cached outcome. Subsequent calls return the same value
    /// without re-evaluation, regardless of the context passed.
    ///
    /// # Errors
    ///
    /// Returns a clone of the cached error when the computation failed.
    ///
    /// # Panics
    ///
    /// Panics if an internal mutex was poisoned, which can only happen when a
    /// previous `compute` call panicked.
    pub fn get(&self, ctx: &Ctx) -> Result<T, E> {
        self.once.call_once(|| {
            // `take` the closure so it is consumed exactly once.
            let compute = self
                .compute
                .lock()
                .expect("SharedResponse compute mutex poisoned")
                .take()
                .expect("SharedResponse compute already consumed");
            let outcome = compute(ctx);
            *self
                .result
                .lock()
                .expect("SharedResponse result mutex poisoned") = Some(outcome);
        });

        self.result
            .lock()
            .expect("SharedResponse result mutex poisoned")
            .clone()
            .expect("SharedResponse result populated by call_once")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn computes_once_and_caches() {
        let calls = Arc::new(AtomicI32::new(0));
        let calls_clone = calls.clone();
        let shared = SharedResponse::<i32, ()>::new(move |_ctx| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        });

        let ctx = Ctx::background();
        assert_eq!(shared.get(&ctx), Ok(42));
        assert_eq!(shared.get(&ctx), Ok(42));
        assert_eq!(shared.get(&ctx), Ok(42));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "compute runs exactly once");
    }

    #[test]
    fn caches_error() {
        let calls = Arc::new(AtomicI32::new(0));
        let calls_clone = calls.clone();
        let shared = SharedResponse::<i32, String>::new(move |_ctx| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            Err("nope".to_string())
        });

        let ctx = Ctx::background();
        assert_eq!(shared.get(&ctx), Err("nope".to_string()));
        assert_eq!(shared.get(&ctx), Err("nope".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_get_computes_once() {
        let calls = Arc::new(AtomicI32::new(0));
        let calls_clone = calls.clone();
        let shared = Arc::new(SharedResponse::<i32, ()>::new(move |_ctx| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            // Small spin to widen the race window.
            for _ in 0..1000 {
                std::hint::spin_loop();
            }
            Ok(7)
        }));

        let mut handles = Vec::new();
        for _ in 0..16 {
            let s = shared.clone();
            handles.push(thread::spawn(move || {
                let ctx = Ctx::background();
                s.get(&ctx)
            }));
        }
        for h in handles {
            assert_eq!(h.join().unwrap(), Ok(7));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
