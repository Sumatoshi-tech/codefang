//! `WorkerPool` — bounded-concurrency item processing.

use crate::context::{ContextError, Ctx};
use crossbeam_channel::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;

/// The error type returned by [`WorkerPool`] operations.
///
/// Either wraps a context error or surfaces the first work error verbatim.
/// The rendered strings are part of the CLI error-text contract.
#[derive(Debug, thiserror::Error)]
pub enum WorkerPoolError<E> {
    /// The supplied context was already cancelled. Stringifies as
    /// `"worker pool: <ctx err>"` (error-text contract).
    #[error("worker pool: {0}")]
    Context(ContextError),
    /// The first error returned by a [`WorkerPool::work`] call.
    #[error("{0}")]
    Work(E),
}

/// Runs `work` on each item with at most `max_parallel` worker threads.
///
/// Returns the first error encountered, or `Ok(())`. Remaining workers
/// observe cancellation on the first error (the derived context is
/// cancelled), so they skip their remaining items.
pub struct WorkerPool<T, E, F>
where
    F: Fn(&Ctx, T) -> Result<(), E> + Send + Sync,
{
    /// The maximum number of concurrent worker threads. Zero (or negative)
    /// defaults to the available parallelism.
    pub max_parallel: i64,
    /// Processes a single item.
    pub work: F,
    _marker: std::marker::PhantomData<fn(T) -> E>,
}

impl<T, E, F> WorkerPool<T, E, F>
where
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(&Ctx, T) -> Result<(), E> + Send + Sync + 'static,
{
    /// Constructs a worker pool. `max_parallel` of `0` (or negative) defaults
    /// to the available parallelism.
    pub fn new(max_parallel: i64, work: F) -> Self {
        WorkerPool {
            max_parallel,
            work,
            _marker: std::marker::PhantomData,
        }
    }

    /// Processes all items with bounded concurrency.
    ///
    /// If any `work` call returns an error, the derived context is cancelled
    /// and `run` returns that error after all workers finish.
    ///
    /// - empty `items` → `Ok(())`;
    /// - already-cancelled `ctx` → `Err(WorkerPoolError::Context(..))`;
    /// - worker count is clamped to `items.len()`.
    ///
    /// # Errors
    ///
    /// Returns the context error when `ctx` is already cancelled, or the
    /// first error produced by a `work` call.
    ///
    /// # Panics
    ///
    /// Panics if a worker thread panicked (propagated) or the internal error
    /// mutex was poisoned by such a panic.
    pub fn run(&self, ctx: &Ctx, items: Vec<T>) -> Result<(), WorkerPoolError<E>> {
        if items.is_empty() {
            return Ok(());
        }
        if let Some(e) = ctx.err() {
            return Err(WorkerPoolError::Context(e));
        }

        let workers = self.resolve_workers(items.len());
        // Buffered work channel of capacity `workers`.
        let (work_tx, work_rx) = crossbeam_channel::bounded::<T>(workers);
        let (child_ctx, cancel) = ctx.with_cancel();

        let first_err: Arc<Mutex<Option<E>>> = Arc::new(Mutex::new(None));

        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let rx = work_rx.clone();
                let ctx_w = child_ctx.clone();
                let cancel_w = cancel.clone();
                let first_err_w = first_err.clone();
                let work = &self.work;
                handles.push(scope.spawn(move || {
                    while let Ok(item) = rx.recv() {
                        // Once cancelled, drain remaining items without
                        // working them.
                        if ctx_w.is_cancelled() {
                            continue;
                        }
                        if let Err(e) = work(&ctx_w, item) {
                            // Record only the first error, then cancel so
                            // peers stop working.
                            let mut slot = first_err_w
                                .lock()
                                .expect("worker pool first_err mutex poisoned");
                            if slot.is_none() {
                                *slot = Some(e);
                                cancel_w.cancel();
                            }
                        }
                    }
                }));
            }

            // Feed all items, then close the channel so the workers' recv
            // loops end.
            for item in items {
                // A send error means every receiver was dropped, which cannot
                // happen here because the workers live until the scope ends; but
                // handle it defensively to avoid a panic.
                if work_tx.send(item).is_err() {
                    break;
                }
            }
            drop(work_tx); // close the work channel

            for h in handles {
                // Worker closures never panic on their own; propagate if they do.
                h.join().expect("worker pool thread panicked");
            }
        });

        // Release the derived context regardless of outcome.
        cancel.cancel();

        Self::take_first_err(first_err)
    }

    /// Processes items arriving on a channel with bounded concurrency.
    ///
    /// Semantics match [`WorkerPool::run`] but items arrive via a
    /// [`Receiver`] instead of a `Vec`. An already-cancelled context returns
    /// an error; the worker count is *not* clamped (item count is unknown).
    /// An already-closed, empty channel returns `Ok(())` because the workers'
    /// recv loops end immediately.
    ///
    /// # Errors
    ///
    /// Returns the context error when `ctx` is already cancelled, or the
    /// first error produced by a `work` call.
    ///
    /// # Panics
    ///
    /// Panics if a worker thread panicked (propagated) or the internal error
    /// mutex was poisoned by such a panic.
    pub fn run_chan(&self, ctx: &Ctx, ch: Receiver<T>) -> Result<(), WorkerPoolError<E>> {
        if let Some(e) = ctx.err() {
            return Err(WorkerPoolError::Context(e));
        }

        let workers = self.resolve_workers(0);
        let (child_ctx, cancel) = ctx.with_cancel();
        let first_err: Arc<Mutex<Option<E>>> = Arc::new(Mutex::new(None));

        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let rx = ch.clone();
                let ctx_w = child_ctx.clone();
                let cancel_w = cancel.clone();
                let first_err_w = first_err.clone();
                let work = &self.work;
                handles.push(scope.spawn(move || {
                    while let Ok(item) = rx.recv() {
                        if ctx_w.is_cancelled() {
                            continue;
                        }
                        if let Err(e) = work(&ctx_w, item) {
                            let mut slot = first_err_w
                                .lock()
                                .expect("worker pool first_err mutex poisoned");
                            if slot.is_none() {
                                *slot = Some(e);
                                cancel_w.cancel();
                            }
                        }
                    }
                }));
            }
            for h in handles {
                h.join().expect("worker pool thread panicked");
            }
        });

        cancel.cancel();

        Self::take_first_err(first_err)
    }

    /// Extracts the recorded first work error after all workers have joined.
    ///
    /// By construction every worker thread has been joined when this runs, so
    /// the `Arc` is unique; a lock-and-clone fallback would need `E: Clone`,
    /// and the impossible outstanding-clone case reads as success.
    fn take_first_err(first_err: Arc<Mutex<Option<E>>>) -> Result<(), WorkerPoolError<E>> {
        match Arc::try_unwrap(first_err)
            .map(Mutex::into_inner)
            .map(|r| r.expect("worker pool first_err mutex poisoned"))
        {
            Ok(Some(e)) => Err(WorkerPoolError::Work(e)),
            Ok(None) | Err(_) => Ok(()),
        }
    }

    /// Returns the effective worker count, clamped to `item_count`.
    ///
    /// When `item_count` is 0 (unknown, e.g. a channel source), no clamping
    /// is applied.
    fn resolve_workers(&self, item_count: usize) -> usize {
        let mut workers = if self.max_parallel <= 0 {
            num_cpus()
        } else {
            self.max_parallel as usize
        };
        if item_count > 0 && workers > item_count {
            workers = item_count;
        }
        // A pool with zero workers can never drain its channel. The count
        // never legitimately reaches zero (available parallelism >= 1; the
        // clamp only lowers toward item_count, which is >= 1 when nonzero) —
        // guard defensively.
        workers.max(1)
    }
}

/// The available parallelism, or 1 if it cannot be determined.
fn num_cpus() -> usize {
    thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn empty_items_returns_ok() {
        let pool = WorkerPool::<i32, (), _>::new(4, |_ctx, _item| Ok(()));
        let ctx = Ctx::background();
        assert!(pool.run(&ctx, vec![]).is_ok());
    }

    #[test]
    fn processes_all_items() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let pool = WorkerPool::<i32, (), _>::new(4, move |_ctx, _item| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        let ctx = Ctx::background();
        let items: Vec<i32> = (0..100).collect();
        assert!(pool.run(&ctx, items).is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn returns_first_error_and_cancels() {
        let pool = WorkerPool::<i32, String, _>::new(2, |_ctx, item| {
            if item == 7 {
                Err(format!("bad item {item}"))
            } else {
                Ok(())
            }
        });
        let ctx = Ctx::background();
        let items: Vec<i32> = (0..50).collect();
        let res = pool.run(&ctx, items);
        match res {
            Err(WorkerPoolError::Work(msg)) => assert_eq!(msg, "bad item 7"),
            other => panic!("expected work error, got {other:?}"),
        }
    }

    #[test]
    fn already_cancelled_context_errors() {
        let pool = WorkerPool::<i32, (), _>::new(2, |_ctx, _item| Ok(()));
        let ctx = Ctx::background();
        ctx.cancel();
        let res = pool.run(&ctx, vec![1, 2, 3]);
        match res {
            Err(WorkerPoolError::Context(ContextError::Canceled)) => {}
            other => panic!("expected context error, got {other:?}"),
        }
    }

    #[test]
    fn cancelled_context_error_message() {
        // Use a Display-able error type so `to_string()` is available; the
        // message under test comes from the Context variant's wrapping
        // (error-text contract).
        let pool = WorkerPool::<i32, String, _>::new(1, |_ctx, _item| Ok(()));
        let ctx = Ctx::background();
        ctx.cancel();
        let err = pool.run(&ctx, vec![1]).unwrap_err();
        assert_eq!(err.to_string(), "worker pool: context canceled");
    }

    #[test]
    fn run_chan_processes_items() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let pool = WorkerPool::<i32, (), _>::new(3, move |_ctx, _item| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        let (tx, rx) = crossbeam_channel::unbounded();
        for i in 0..20 {
            tx.send(i).unwrap();
        }
        drop(tx);
        let ctx = Ctx::background();
        assert!(pool.run_chan(&ctx, rx).is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn run_chan_closed_channel_returns_ok() {
        let pool = WorkerPool::<i32, (), _>::new(2, |_ctx, _item| Ok(()));
        let (tx, rx) = crossbeam_channel::unbounded::<i32>();
        drop(tx);
        let ctx = Ctx::background();
        assert!(pool.run_chan(&ctx, rx).is_ok());
    }

    #[test]
    fn run_chan_already_cancelled_errors() {
        let pool = WorkerPool::<i32, (), _>::new(2, |_ctx, _item| Ok(()));
        let (_tx, rx) = crossbeam_channel::unbounded::<i32>();
        let ctx = Ctx::background();
        ctx.cancel();
        match pool.run_chan(&ctx, rx) {
            Err(WorkerPoolError::Context(ContextError::Canceled)) => {}
            other => panic!("expected context error, got {other:?}"),
        }
    }

    #[test]
    fn resolve_workers_clamps_to_item_count() {
        let pool = WorkerPool::<i32, (), _>::new(8, |_ctx, _item| Ok(()));
        assert_eq!(pool.resolve_workers(3), 3, "clamp down to item count");
        assert_eq!(pool.resolve_workers(20), 8, "max_parallel honoured");
        assert_eq!(pool.resolve_workers(0), 8, "no clamp when count unknown");
    }

    #[test]
    fn resolve_workers_defaults_to_num_cpus() {
        let pool = WorkerPool::<i32, (), _>::new(0, |_ctx, _item| Ok(()));
        // Unknown count → NumCPU (>= 1). Clamped to item count when smaller.
        assert!(pool.resolve_workers(0) >= 1);
        assert_eq!(pool.resolve_workers(1), 1);
    }
}
