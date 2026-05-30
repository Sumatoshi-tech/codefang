//! `WorkerPool` — bounded-concurrency item processing (`workerpool.go`).

use crate::context::{ContextError, Ctx};
use crossbeam_channel::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;

/// The error type returned by [`WorkerPool`] operations.
///
/// Mirrors Go where `Run`/`RunChan` either wrap a context error
/// (`fmt.Errorf("worker pool: %w", ctx.Err())`) or surface the first `Work`
/// error verbatim.
#[derive(Debug)]
pub enum WorkerPoolError<E> {
    /// The supplied context was already cancelled. Stringifies as
    /// `"worker pool: <ctx err>"`, matching Go's wrapping.
    Context(ContextError),
    /// The first non-`nil` error returned by a [`WorkerPool::work`] call.
    Work(E),
}

impl<E: std::fmt::Display> std::fmt::Display for WorkerPoolError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerPoolError::Context(e) => write!(f, "worker pool: {e}"),
            WorkerPoolError::Work(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::fmt::Display + std::fmt::Debug> std::error::Error for WorkerPoolError<E> {}

/// Runs `work` on each item with at most `max_parallel` worker threads.
///
/// Returns the first non-`nil` error encountered, or `Ok(())`. Remaining
/// workers observe cancellation on the first error (the derived context is
/// cancelled), so they skip their remaining items.
///
/// Mirrors Go's `WorkerPool[T]` struct.
pub struct WorkerPool<T, E, F>
where
    F: Fn(&Ctx, T) -> Result<(), E> + Send + Sync,
{
    /// The maximum number of concurrent worker threads. Zero (or negative)
    /// defaults to the available parallelism (the analogue of
    /// `runtime.NumCPU()`).
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
    /// Constructs a worker pool. `max_parallel` of `0` (or negative) defaults to
    /// the available parallelism, matching Go's `runtime.NumCPU()` fallback.
    pub fn new(max_parallel: i64, work: F) -> Self {
        WorkerPool {
            max_parallel,
            work,
            _marker: std::marker::PhantomData,
        }
    }

    /// Processes all items with bounded concurrency.
    ///
    /// If any `work` call returns an error, the derived context is cancelled and
    /// `run` returns that error after all workers finish. Mirrors Go's
    /// `(WorkerPool).Run`:
    ///
    /// - empty `items` → `Ok(())`;
    /// - already-cancelled `ctx` → `Err(WorkerPoolError::Context(..))`;
    /// - worker count is clamped to `items.len()`.
    pub fn run(&self, ctx: &Ctx, items: Vec<T>) -> Result<(), WorkerPoolError<E>> {
        if items.is_empty() {
            return Ok(());
        }
        if let Some(e) = ctx.err() {
            return Err(WorkerPoolError::Context(e));
        }

        let workers = self.resolve_workers(items.len());
        // Buffered work channel of capacity `workers`, matching
        // `make(chan T, workers)`.
        let (work_tx, work_rx) = crossbeam_channel::bounded::<T>(workers);
        let (child_ctx, cancel) = ctx.with_cancel();

        let first_err: Arc<Mutex<Option<E>>> = Arc::new(Mutex::new(None));

        let result = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let rx = work_rx.clone();
                let ctx_w = child_ctx.clone();
                let cancel_w = cancel.clone();
                let first_err_w = first_err.clone();
                let work = &self.work;
                handles.push(scope.spawn(move || {
                    while let Ok(item) = rx.recv() {
                        // `if ctx.Err() != nil { continue }`: drain remaining
                        // items without working once cancelled.
                        if ctx_w.is_cancelled() {
                            continue;
                        }
                        if let Err(e) = work(&ctx_w, item) {
                            // `errOnce.Do`: record only the first error, then
                            // cancel so peers stop working.
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

            // Feed all items, then close the channel so workers' `iter()` ends.
            for item in items {
                // A send error means every receiver was dropped, which cannot
                // happen here because the workers live until the scope ends; but
                // handle it defensively to avoid a panic.
                if work_tx.send(item).is_err() {
                    break;
                }
            }
            drop(work_tx); // `close(workCh)`

            for h in handles {
                // Worker closures never panic on their own; propagate if they do.
                h.join().expect("worker pool thread panicked");
            }
        });
        // thread::scope returns () on success here.
        let () = result;

        // `defer cancel()` — release the derived context regardless of outcome.
        cancel.cancel();

        match Arc::try_unwrap(first_err)
            .map(Mutex::into_inner)
            .map(|r| r.expect("worker pool first_err mutex poisoned"))
        {
            Ok(Some(e)) => Err(WorkerPoolError::Work(e)),
            Ok(None) => Ok(()),
            // If somehow still shared, fall back to a lock-and-clone path is not
            // possible without E: Clone; by construction all worker threads have
            // joined, so the Arc is unique. Treat the impossible case as success.
            Err(_) => Ok(()),
        }
    }

    /// Processes items arriving on a channel with bounded concurrency.
    ///
    /// Semantics match [`WorkerPool::run`] but items arrive via a [`Receiver`]
    /// instead of a `Vec`. An already-cancelled context returns an error; the
    /// worker count is *not* clamped (item count is unknown). Mirrors Go's
    /// `(WorkerPool).RunChan`.
    ///
    /// Go's "nil or already-closed channel returns nil immediately" splits into
    /// two Rust cases: there is no nil [`Receiver`], so the closest analogue —
    /// an already-closed/empty channel — naturally returns `Ok(())` because the
    /// workers' `iter()` ends immediately.
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

        match Arc::try_unwrap(first_err)
            .map(Mutex::into_inner)
            .map(|r| r.expect("worker pool first_err mutex poisoned"))
        {
            Ok(Some(e)) => Err(WorkerPoolError::Work(e)),
            Ok(None) => Ok(()),
            Err(_) => Ok(()),
        }
    }

    /// Returns the effective worker count, clamped to `item_count`.
    ///
    /// When `item_count` is 0 (unknown, e.g. a channel source), no clamping is
    /// applied. Mirrors Go's `resolveWorkers`.
    fn resolve_workers(&self, item_count: usize) -> usize {
        let mut workers = if self.max_parallel <= 0 {
            num_cpus()
        } else {
            self.max_parallel as usize
        };
        if item_count > 0 && workers > item_count {
            workers = item_count;
        }
        // A pool with zero workers can never drain its channel; Go never reaches
        // zero (NumCPU >= 1, MaxParallel clamp only lowers toward item_count
        // which is >= 1 when nonzero). Guard defensively.
        workers.max(1)
    }
}

/// The analogue of Go's `runtime.NumCPU()`: the available parallelism, or 1 if
/// it cannot be determined.
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
        // message under test comes from the Context variant's wrapping, which
        // mirrors Go's `fmt.Errorf("worker pool: %w", ctx.Err())`.
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
