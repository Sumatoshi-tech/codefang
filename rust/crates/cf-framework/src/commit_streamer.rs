//! Commit batching for the pipeline front-end.
//!
//! [`CommitStreamer`] takes a slice of commits and emits them as batches over
//! a bounded channel, with `lookahead` pre-fetched batches. It is generic over
//! the commit item type; pipeline callers typically instantiate it with
//! `Arc<Commit>`.
//!
//! Backpressure comes from the producer thread blocking on the bounded
//! channel of capacity `lookahead`. Cancellation is modeled with an
//! `AtomicBool` stop flag, so a consumer that stops reading and signals
//! cancellation lets the producer exit promptly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::Arc;
use std::thread;

/// Default number of commits per batch.
pub const DEFAULT_BATCH_SIZE: usize = 10;

/// Default number of batches to prefetch.
pub const DEFAULT_LOOKAHEAD: usize = 2;

/// A batch of commits for processing.
#[derive(Debug, Clone)]
pub struct CommitBatch<T> {
    /// Commits in this batch.
    pub commits: Vec<T>,
    /// Index of the first commit in the full sequence.
    pub start_index: usize,
    /// Identifies this batch for ordering.
    pub batch_id: usize,
}

/// Iterates commits and groups them into batches for efficient processing.
#[derive(Debug, Clone, Copy)]
pub struct CommitStreamer {
    /// Number of commits per batch.
    pub batch_size: usize,
    /// Number of batches to prefetch (the output channel's capacity).
    pub lookahead: usize,
}

impl Default for CommitStreamer {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            lookahead: DEFAULT_LOOKAHEAD,
        }
    }
}

impl CommitStreamer {
    /// Creates a streamer with the given batch size and lookahead.
    #[must_use]
    pub const fn new(batch_size: usize, lookahead: usize) -> Self {
        Self {
            batch_size,
            lookahead,
        }
    }

    /// Computes the batches for a slice without spawning a thread.
    ///
    /// This is the deterministic core of [`stream`](Self::stream), extracted
    /// for testing and for callers that prefer a pull model. Batch `i` covers
    /// `commits[i*batch_size .. min((i+1)*batch_size, len)]`, with
    /// `start_index` = the absolute index of the first commit and a monotonic
    /// `batch_id` starting at 0. A `batch_size` of 0 is treated as 1 to avoid
    /// a zero-stride loop (the coordinator clamps this upstream).
    #[must_use]
    pub fn batches<T: Clone>(&self, commits: &[T]) -> Vec<CommitBatch<T>> {
        let step = self.batch_size.max(1);
        let mut out = Vec::new();
        let mut batch_id = 0;
        let mut i = 0;
        while i < commits.len() {
            let end = (i + step).min(commits.len());
            out.push(CommitBatch {
                commits: commits[i..end].to_vec(),
                start_index: i,
                batch_id,
            });
            batch_id += 1;
            i += step;
        }
        out
    }

    /// Streams the commits as batches over a bounded channel.
    ///
    /// A background thread sends each batch, closing the channel (dropping
    /// the sender) when done. Set the returned `stop` flag to `true` to ask
    /// the producer to stop early. The channel capacity is `lookahead`, which
    /// provides the prefetch/backpressure behavior.
    ///
    /// Returns the receiver plus the shared stop flag and the producer's
    /// `JoinHandle` (so callers can join on shutdown).
    #[must_use]
    pub fn stream<T: Clone + Send + 'static>(
        &self,
        commits: Vec<T>,
    ) -> (Receiver<CommitBatch<T>>, Arc<AtomicBool>, thread::JoinHandle<()>) {
        let (tx, rx) = sync_channel::<CommitBatch<T>>(self.lookahead);
        let stop = Arc::new(AtomicBool::new(false));
        let streamer = *self;
        let stop_producer = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            for batch in streamer.batches(&commits) {
                if stop_producer.load(Ordering::SeqCst) {
                    return;
                }
                // A send error means the receiver was dropped (consumer
                // gone) — stop producing.
                if tx.send(batch).is_err() {
                    return;
                }
            }
        });

        (rx, stop, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_exact_multiple() {
        let s = CommitStreamer::new(2, 2);
        let batches = s.batches(&[1, 2, 3, 4]);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].commits, vec![1, 2]);
        assert_eq!(batches[0].start_index, 0);
        assert_eq!(batches[0].batch_id, 0);
        assert_eq!(batches[1].commits, vec![3, 4]);
        assert_eq!(batches[1].start_index, 2);
        assert_eq!(batches[1].batch_id, 1);
    }

    #[test]
    fn batches_with_remainder() {
        let s = CommitStreamer::new(3, 1);
        let batches = s.batches(&[1, 2, 3, 4, 5]);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].commits, vec![1, 2, 3]);
        assert_eq!(batches[1].commits, vec![4, 5]);
        assert_eq!(batches[1].start_index, 3);
    }

    #[test]
    fn batches_empty_input() {
        let s = CommitStreamer::new(10, 2);
        let batches: Vec<CommitBatch<i32>> = s.batches(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn batch_size_zero_is_treated_as_one() {
        let s = CommitStreamer::new(0, 1);
        let batches = s.batches(&[1, 2]);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].commits, vec![1]);
        assert_eq!(batches[1].commits, vec![2]);
    }

    #[test]
    fn stream_emits_all_batches_in_order() {
        let s = CommitStreamer::new(2, 2);
        let (rx, _stop, handle) = s.stream(vec![10, 20, 30, 40, 50]);
        let mut got = Vec::new();
        for batch in rx.iter() {
            got.push((batch.batch_id, batch.commits));
        }
        handle.join().unwrap();
        assert_eq!(
            got,
            vec![
                (0, vec![10, 20]),
                (1, vec![30, 40]),
                (2, vec![50]),
            ]
        );
    }

    #[test]
    fn stream_stops_when_flag_set() {
        // A tiny lookahead so the producer blocks after the first batch;
        // setting stop then draining must terminate cleanly.
        let s = CommitStreamer::new(1, 1);
        let (rx, stop, handle) = s.stream((0..1000).collect::<Vec<_>>());
        // Take a couple, then signal stop and drain.
        let _ = rx.recv().unwrap();
        stop.store(true, Ordering::SeqCst);
        // Drain whatever is buffered; the producer will exit at the next check.
        while rx.recv().is_ok() {}
        handle.join().unwrap();
    }
}
