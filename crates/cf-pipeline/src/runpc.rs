//! `RunPc` — a producer-consumer micro-skeleton.

use crate::context::Ctx;
use crossbeam_channel::{Receiver, Sender};
use std::thread;

/// The minimum capacity for the internal jobs channel.
const MIN_BUFFER: usize = 1;

/// The producer closure: reads the input and sends work items on the jobs
/// sender.
pub type ProduceFn<In, Job> = Box<dyn FnOnce(&Ctx, In, &Sender<Job>) + Send>;

/// The consumer closure: reads work items from the jobs receiver and sends
/// results on the out sender.
pub type ConsumeFn<Out, Job> = Box<dyn FnOnce(&Ctx, &Receiver<Job>, &Sender<Out>) + Send>;

/// A producer-consumer micro-skeleton that owns the thread topology: channel
/// creation, thread spawning, and orderly shutdown.
///
/// Type parameters:
///
/// - `In`:  the input consumed by the producer.
/// - `Out`: the output emitted by the consumer.
/// - `Job`: the internal work item flowing from producer to consumer.
///
/// The `produce` function reads the input and writes jobs to the jobs
/// channel. The `consume` function reads jobs and writes results to the out
/// channel. Neither function should close its output channel; [`RunPc::run`]
/// handles that by dropping the corresponding [`Sender`]. The closures
/// receive crossbeam [`Sender`] / [`Receiver`] handles with blocking
/// send/recv and close-on-drop semantics.
pub struct RunPc<In, Out, Job> {
    /// Capacity of the internal jobs channel. Values below 1 are clamped to 1.
    pub buffer: usize,

    /// Reads the input and sends work items on the jobs sender.
    pub produce: ProduceFn<In, Job>,

    /// Reads work items from the jobs receiver and sends results on `out`.
    pub consume: ConsumeFn<Out, Job>,
}

impl<In, Out, Job> RunPc<In, Out, Job>
where
    In: Send + 'static,
    Out: Send + 'static,
    Job: Send + 'static,
{
    /// Constructs a [`RunPc`] from a buffer size and the produce/consume
    /// closures. `buffer` values below 1 are clamped to 1 by [`RunPc::run`].
    pub fn new<P, C>(buffer: i64, produce: P, consume: C) -> Self
    where
        P: FnOnce(&Ctx, In, &Sender<Job>) + Send + 'static,
        C: FnOnce(&Ctx, &Receiver<Job>, &Sender<Out>) + Send + 'static,
    {
        // Clamp here as well as in `run` so the stored field is already
        // normalized for callers that construct via `new`.
        let buffer = if buffer < MIN_BUFFER as i64 {
            MIN_BUFFER
        } else {
            buffer as usize
        };
        RunPc {
            buffer,
            produce: Box::new(produce),
            consume: Box::new(consume),
        }
    }

    /// Starts the producer and consumer threads and returns the output
    /// receiver. The jobs channel is closed after `produce` returns (its
    /// [`Sender`] is dropped). The output channel is closed after `consume`
    /// returns (its [`Sender`] is dropped).
    #[must_use]
    pub fn run(self, ctx: &Ctx, input: In) -> Receiver<Out> {
        let buf = self.buffer.max(MIN_BUFFER);

        let (jobs_tx, jobs_rx): (Sender<Job>, Receiver<Job>) = crossbeam_channel::bounded(buf);
        // The output channel is unbuffered (rendezvous), so consumers exert
        // backpressure on the consumer thread.
        let (out_tx, out_rx): (Sender<Out>, Receiver<Out>) = crossbeam_channel::bounded(0);

        let produce = self.produce;
        let consume = self.consume;

        let ctx_p = ctx.clone();
        thread::spawn(move || {
            // jobs_tx dropped at scope exit → jobs channel closes.
            produce(&ctx_p, input, &jobs_tx);
        });

        let ctx_c = ctx.clone();
        thread::spawn(move || {
            // out_tx dropped at scope exit → out channel closes.
            consume(&ctx_c, &jobs_rx, &out_tx);
        });

        out_rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_and_consumes() {
        let ctx = Ctx::background();
        let pc = RunPc::<i32, i32, i32>::new(
            4,
            |_ctx, n, jobs| {
                for i in 0..n {
                    jobs.send(i).unwrap();
                }
            },
            |_ctx, jobs, out| {
                while let Ok(job) = jobs.recv() {
                    out.send(job * 2).unwrap();
                }
            },
        );

        let out = pc.run(&ctx, 5);
        let mut got: Vec<i32> = Vec::new();
        while let Ok(v) = out.recv() {
            got.push(v);
        }
        got.sort_unstable();
        assert_eq!(got, vec![0, 2, 4, 6, 8]);
    }

    #[test]
    fn buffer_clamped_to_min() {
        let pc = RunPc::<(), (), ()>::new(0, |_c, _i, _j| {}, |_c, _j, _o| {});
        assert_eq!(pc.buffer, MIN_BUFFER);
        let pc = RunPc::<(), (), ()>::new(-10, |_c, _i, _j| {}, |_c, _j, _o| {});
        assert_eq!(pc.buffer, MIN_BUFFER);
    }

    #[test]
    fn out_channel_closes_after_consume() {
        let ctx = Ctx::background();
        let pc = RunPc::<i32, i32, i32>::new(
            1,
            |_ctx, _n, _jobs| { /* produce nothing */ },
            |_ctx, jobs, out| {
                while let Ok(job) = jobs.recv() {
                    out.send(job).unwrap();
                }
            },
        );
        let out = pc.run(&ctx, 0);
        // No items produced → out closes immediately.
        assert!(out.recv().is_err());
    }

    #[test]
    fn jobs_channel_closes_so_consumer_terminates() {
        let ctx = Ctx::background();
        let pc = RunPc::<i32, usize, i32>::new(
            2,
            |_ctx, n, jobs| {
                for i in 0..n {
                    jobs.send(i).unwrap();
                }
            },
            |_ctx, jobs, out| {
                // If the jobs channel did not close, the loop would block here
                // forever. Counting the drained items proves orderly shutdown.
                let mut count = 0usize;
                while jobs.recv().is_ok() {
                    count += 1;
                }
                out.send(count).unwrap();
            },
        );
        let out = pc.run(&ctx, 3);
        assert_eq!(out.recv(), Ok(3));
        assert!(out.recv().is_err());
    }
}
