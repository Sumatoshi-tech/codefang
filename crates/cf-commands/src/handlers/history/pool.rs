//! The history worker pool: one fixed set of worker threads prepares a
//! walk's per-commit products while the calling thread folds them in window
//! order.
//!
//! Workers claim short runs of consecutive commits from a shared cursor
//! instead of owning a fixed slice: commit costs are heavily skewed (one
//! vendoring or rename-heavy commit can outweigh thousands of others, and such
//! commits cluster), so a static split makes the whole window wait for its
//! unluckiest slice. A run keeps the claiming worker's libgit2 object cache
//! warm (consecutive commits share most of their trees), and a worker that
//! finds nothing left to claim takes the unstarted back half of another
//! worker's run, so a cluster of slow commits is shared out instead of
//! serialized. A worker may claim a commit only while it is less than
//! `window` positions ahead of the fold, so at most `window` products are
//! alive at once, and the window slides one commit at a time: there is no
//! barrier where every worker waits for the slowest commit of a chunk. Each
//! worker opens its repository once and keeps its thread-local state
//! (parsers, caches) warm for the whole walk.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use cf_gitlib::Hash;

/// The largest run of consecutive commits one claim takes. Measured on the
/// 31,923-commit kubernetes prefix (`history/anomaly`, 24 workers, 3000-commit
/// window): claiming one commit at a time ran 65 s, runs of 8 ran 43 s, runs
/// of 32 ran 31 s; consecutive commits share most of their trees, which the
/// claiming worker's libgit2 object cache then still holds.
pub(crate) const MAX_BLOCK: usize = 32;

/// Claims per worker that fit in one window, at least: smaller runs keep
/// every worker fed near the end of the window.
const BLOCKS_PER_WORKER: usize = 4;

/// The commits one claim takes for `workers` workers sharing a `window`:
/// a quarter of each worker's share of the window, between one commit and
/// [`MAX_BLOCK`].
pub(crate) fn block_size(window: usize, workers: usize) -> usize {
    (window / workers.max(1).saturating_mul(BLOCKS_PER_WORKER)).clamp(1, MAX_BLOCK)
}

/// The pool state shared by the workers and the folding thread.
struct Flight<T> {
    /// The next window position a worker may claim.
    next: usize,
    /// Every position below this one has been folded.
    folded: usize,
    /// Prepared products waiting for their turn, by window position.
    ready: BTreeMap<usize, T>,
    /// The walk can no longer succeed (a repository that does not open, a
    /// failed or panicking commit, a failed fold): workers stop claiming and
    /// the fold stops waiting.
    failed: bool,
    /// Per worker, the claimed commits it has not started yet.
    queued: Vec<Range<usize>>,
}

/// One walk's pool: the window it prepares and the state guarding it.
struct Pool<'h, T> {
    hashes: &'h [Hash],
    /// Products alive at once, at most.
    window: usize,
    /// Commits one claim takes, at most ([`block_size`]).
    block: usize,
    flight: Mutex<Flight<T>>,
    /// Signalled when the fold advances or the walk fails (workers wait for
    /// room in the window).
    room: Condvar,
    /// Signalled when a product arrives or the walk fails (the fold waits for
    /// its next position).
    arrived: Condvar,
}

impl<'h, T> Pool<'h, T> {
    fn new(hashes: &'h [Hash], window: usize, workers: usize) -> Self {
        Self {
            hashes,
            window,
            block: block_size(window, workers),
            flight: Mutex::new(Flight {
                next: 0,
                folded: 0,
                ready: BTreeMap::new(),
                failed: false,
                queued: vec![0..0; workers],
            }),
            room: Condvar::new(),
            arrived: Condvar::new(),
        }
    }

    /// The state lock. A poisoned lock still holds consistent state: every
    /// critical section is a few field updates that cannot panic halfway.
    fn lock(&self) -> MutexGuard<'_, Flight<T>> {
        self.flight.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Marks the walk failed and wakes everyone waiting on it.
    fn fail(&self) {
        self.lock().failed = true;
        self.room.notify_all();
        self.arrived.notify_all();
    }

    /// The next commit for worker `me`: the front of its own run; else a new
    /// run from the cursor, inside the window; else the unstarted back half
    /// of the longest run another worker holds. Waits while the window is
    /// full and nothing can be taken; `None` once nothing is left to claim or
    /// the walk failed.
    fn claim(&self, me: usize) -> Option<usize> {
        let mut flight = self.lock();
        loop {
            if flight.failed {
                return None;
            }
            if let Some(pos) = flight.queued[me].next() {
                return Some(pos);
            }
            let limit = flight
                .folded
                .saturating_add(self.window)
                .min(self.hashes.len());
            if flight.next < limit {
                let start = flight.next;
                let end = start.saturating_add(self.block).min(limit);
                flight.next = end;
                flight.queued[me] = start + 1..end;
                if end - start > 1 {
                    // The rest of this run is work an idle worker may share.
                    self.room.notify_one();
                }
                return Some(start);
            }
            if let Some(pos) = steal(&mut flight.queued, me) {
                return Some(pos);
            }
            if flight.next >= self.hashes.len() {
                return None;
            }
            flight = self
                .room
                .wait(flight)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Hands a prepared product to the fold; a failed preparation fails the
    /// walk.
    fn deliver(&self, pos: usize, product: Option<T>) {
        match product {
            Some(product) => {
                self.lock().ready.insert(pos, product);
                self.arrived.notify_one();
            }
            None => self.fail(),
        }
    }

    /// The product at `pos` once it is prepared; `None` when the walk failed.
    fn take(&self, pos: usize) -> Option<T> {
        let mut flight = self.lock();
        loop {
            if let Some(product) = flight.ready.remove(&pos) {
                return Some(product);
            }
            if flight.failed {
                return None;
            }
            flight = self
                .arrived
                .wait(flight)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Records that `pos` is folded, making room for one more claim.
    fn advance(&self, pos: usize) {
        self.lock().folded = pos + 1;
        self.room.notify_one();
    }
}

/// Takes the unstarted back half of the longest run another worker holds
/// (its owner keeps the front, whose trees its cache is about to hold) and
/// makes it `me`'s run; returns its first commit, or `None` when no other
/// worker holds an unstarted commit.
fn steal(queued: &mut [Range<usize>], me: usize) -> Option<usize> {
    let victim = (0..queued.len())
        .filter(|&w| w != me)
        .max_by_key(|&w| queued[w].len())
        .filter(|&w| !queued[w].is_empty())?;
    let run = &mut queued[victim];
    let mid = run.start + run.len() / 2;
    let stolen = mid..run.end;
    run.end = mid;
    queued[me] = stolen.start + 1..stolen.end;
    Some(stolen.start)
}

/// Fails the walk when a worker unwinds, so neither the fold nor the other
/// workers wait for a product that will never arrive.
struct UnwindGuard<'p, 'h, T>(&'p Pool<'h, T>);

impl<T> Drop for UnwindGuard<'_, '_, T> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.fail();
        }
    }
}

/// Prepares every commit of `hashes` on `workers` threads, at most `window`
/// products alive at once, and folds each product on the calling thread in
/// window order. `compute` must be a pure per-commit function (it reads the
/// repository but shares no mutable state), so the fold sees the same
/// sequence for any worker count and window.
///
/// `None` when a repository does not open, a commit's preparation returns
/// `None`, or `fold` returns `None` (the `.ok()?` contract of every history
/// walk); a panicking commit panics the caller with its own payload.
pub(crate) fn prepare_in_order<T: Send>(
    repo_path: &str,
    hashes: &[Hash],
    workers: usize,
    window: usize,
    compute: impl Fn(&cf_gitlib::Repository, Hash) -> Option<T> + Sync,
    mut fold: impl FnMut(usize, T) -> Option<()>,
) -> Option<()> {
    if hashes.is_empty() {
        return Some(());
    }
    let window = window.max(1);
    // More workers than window positions would only wait for room.
    let workers = workers.clamp(1, hashes.len().min(window));
    let pool = Pool::new(hashes, window, workers);
    let (pool, compute) = (&pool, &compute);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|me| s.spawn(move || work(pool, me, repo_path, compute)))
            .collect();
        let result = fold_in_order(pool, &mut fold);
        if result.is_none() {
            pool.fail();
        }
        for handle in handles {
            if let Err(panic) = handle.join() {
                std::panic::resume_unwind(panic);
            }
        }
        result
    })
}

/// Worker `me`: open the repository once, then prepare claimed commits until
/// none are left.
fn work<T>(
    pool: &Pool<'_, T>,
    me: usize,
    repo_path: &str,
    compute: &(impl Fn(&cf_gitlib::Repository, Hash) -> Option<T> + Sync),
) {
    let _unwind = UnwindGuard(pool);
    let Ok(repo) = cf_gitlib::Repository::open(repo_path) else {
        pool.fail();
        return;
    };
    while let Some(pos) = pool.claim(me) {
        pool.deliver(pos, compute(&repo, pool.hashes[pos]));
    }
}

/// The fold side: every position in order, each as soon as it is prepared.
fn fold_in_order<T>(
    pool: &Pool<'_, T>,
    fold: &mut impl FnMut(usize, T) -> Option<()>,
) -> Option<()> {
    for pos in 0..pool.hashes.len() {
        let product = pool.take(pos)?;
        fold(pos, product)?;
        pool.advance(pos);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use cf_gitlib::testutil::{init_repo, CommitBuilder, TestRepo};

    use super::*;

    /// 2021-01-01T00:00:00Z.
    const T0: i64 = 1_609_459_200;
    /// Commits in the fixture history.
    const COMMITS: usize = 6;
    /// Worker threads every test runs with, independent of the host's CPUs.
    const WORKERS: usize = 2;
    /// How long a deliberately stalled commit waits before giving up.
    const STALL_LIMIT: Duration = Duration::from_secs(10);
    /// Polling period of a stalled commit.
    const POLL: Duration = Duration::from_millis(1);

    /// A linear history of [`COMMITS`] commits, oldest first.
    fn linear_history() -> (TestRepo, Vec<Hash>) {
        let test = init_repo().expect("init");
        let hashes = (0..COMMITS)
            .map(|i| {
                CommitBuilder::new(&test, &format!("c{i}"), T0 + i as i64)
                    .write("f.txt", format!("v{i}\n").as_bytes())
                    .commit()
                    .expect("commit")
            })
            .collect();
        (test, hashes)
    }

    fn path(test: &TestRepo) -> &str {
        test.path().to_str().expect("utf-8 path")
    }

    /// Waits until `ready` holds; `false` once [`STALL_LIMIT`] passes.
    fn wait_for(ready: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + STALL_LIMIT;
        while !ready() {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(POLL);
        }
        true
    }

    /// The `(position, product)` pairs in the order the fold saw them.
    fn folded<T: Send>(
        test: &TestRepo,
        hashes: &[Hash],
        window: usize,
        compute: impl Fn(&cf_gitlib::Repository, Hash) -> Option<T> + Sync,
    ) -> Option<Vec<(usize, T)>> {
        let mut seen = Vec::new();
        prepare_in_order(
            path(test),
            hashes,
            WORKERS,
            window,
            compute,
            |pos, product| {
                seen.push((pos, product));
                Some(())
            },
        )?;
        Some(seen)
    }

    /// Products reach the fold in window order even when later commits
    /// finish first.
    #[test]
    fn products_reach_the_fold_in_window_order() {
        let (test, hashes) = linear_history();
        let seen = folded(&test, &hashes, 3, |_, hash| {
            let pos = hashes.iter().position(|h| *h == hash)?;
            std::thread::sleep(POLL * (COMMITS - pos) as u32);
            Some(hash)
        });
        let expected: Vec<(usize, Hash)> = hashes.iter().copied().enumerate().collect();
        assert_eq!(seen, Some(expected));
    }

    /// A slow commit never holds the commits queued behind it hostage: an
    /// idle worker takes the next unclaimed commit. The first commit finishes
    /// only once every other commit is prepared, which a static split (the
    /// first worker owning the first half of the window) can never satisfy.
    #[test]
    fn a_slow_commit_does_not_stall_the_commits_queued_behind_it() {
        let (test, hashes) = linear_history();
        let prepared = AtomicUsize::new(0);
        let seen = folded(&test, &hashes, COMMITS, |_, hash| {
            if hash == hashes[0] && !wait_for(|| prepared.load(Ordering::SeqCst) == COMMITS - 1) {
                return None;
            }
            prepared.fetch_add(1, Ordering::SeqCst);
            Some(hash)
        });
        assert_eq!(seen.map(|s| s.len()), Some(COMMITS));
    }

    /// The window bounds the products in flight; it is not a barrier. Once
    /// the oldest commit is folded, an idle worker starts the commit one
    /// window ahead while a slow commit is still being prepared: with a
    /// window of two, commit 1 finishes only after commit 2 is prepared,
    /// which a chunk {2, 3} waiting for chunk {0, 1} can never satisfy.
    #[test]
    fn a_slow_commit_does_not_stall_the_next_window() {
        let (test, hashes) = linear_history();
        let third = AtomicBool::new(false);
        let seen = folded(&test, &hashes, 2, |_, hash| {
            if hash == hashes[1] && !wait_for(|| third.load(Ordering::SeqCst)) {
                return None;
            }
            if hash == hashes[2] {
                third.store(true, Ordering::SeqCst);
            }
            Some(hash)
        });
        assert_eq!(seen.map(|s| s.len()), Some(COMMITS));
    }

    /// A commit whose preparation fails fails the walk.
    #[test]
    fn a_failed_commit_fails_the_walk() {
        let (test, hashes) = linear_history();
        let seen = folded(&test, &hashes, 2, |_, hash| {
            (hash != hashes[2]).then_some(hash)
        });
        assert!(seen.is_none());
    }

    /// A fold that fails stops the walk, and no worker runs ahead of the
    /// window: with a window of one, only the two folded commits were ever
    /// prepared.
    #[test]
    fn a_failed_fold_stops_the_walk() {
        let (test, hashes) = linear_history();
        let prepared = AtomicUsize::new(0);
        let mut folds = 0;
        let result = prepare_in_order(
            path(&test),
            &hashes,
            WORKERS,
            1,
            |_, hash| {
                prepared.fetch_add(1, Ordering::SeqCst);
                Some(hash)
            },
            |pos, _| {
                folds += 1;
                (pos != 1).then_some(())
            },
        );
        assert!(result.is_none());
        assert_eq!(folds, 2);
        assert_eq!(prepared.load(Ordering::SeqCst), 2);
    }

    /// A repository no worker can open fails the walk instead of hanging it.
    #[test]
    fn an_unopenable_repository_fails_the_walk() {
        let (test, hashes) = linear_history();
        let missing = test.path().join("missing");
        let result = prepare_in_order(
            missing.to_str().expect("utf-8 path"),
            &hashes,
            WORKERS,
            2,
            |_, hash| Some(hash),
            |_, _| Some(()),
        );
        assert!(result.is_none());
    }

    /// A panicking commit panics the walk with its own message instead of
    /// hanging the fold that waits for it.
    #[test]
    #[should_panic(expected = "commit 1 exploded")]
    fn a_panicking_commit_panics_the_walk() {
        let (test, hashes) = linear_history();
        let _ = folded(&test, &hashes, 2, |_, hash| {
            assert!(hash != hashes[1], "commit 1 exploded");
            Some(hash)
        });
    }

    /// A claim is about a quarter of each worker's share of the window, at
    /// least one commit and at most [`MAX_BLOCK`].
    #[test]
    fn a_claim_is_a_quarter_of_each_workers_share_of_the_window() {
        assert_eq!(block_size(3000, 24), 31);
        assert_eq!(block_size(COMMITS, WORKERS), 1);
        assert_eq!(block_size(1, 1), 1);
        assert_eq!(block_size(usize::MAX, 24), MAX_BLOCK);
        assert_eq!(block_size(1_000_000, 0), MAX_BLOCK);
    }

    /// Commits of the window this long.
    const LONG_WINDOW: usize = 64;

    /// Without slow commits a worker prepares runs of consecutive commits
    /// (its libgit2 object cache then still holds the previous commit's
    /// trees): the preparing thread changes far less often than a per-commit
    /// hand-off between two equally fast workers would make it.
    #[test]
    fn a_worker_prepares_runs_of_consecutive_commits() {
        let test = init_repo().expect("init");
        let hashes: Vec<Hash> = (0..LONG_WINDOW)
            .map(|i| {
                CommitBuilder::new(&test, &format!("c{i}"), T0 + i as i64)
                    .write("f.txt", format!("v{i}\n").as_bytes())
                    .commit()
                    .expect("commit")
            })
            .collect();
        let seen = folded(&test, &hashes, LONG_WINDOW, |_, _| {
            std::thread::sleep(POLL);
            Some(std::thread::current().id())
        })
        .expect("walk");
        let switches = seen.windows(2).filter(|w| w[0].1 != w[1].1).count();
        assert!(
            switches <= LONG_WINDOW / 4,
            "{switches} thread switches over {LONG_WINDOW} commits"
        );
    }

    /// An empty window folds nothing and succeeds.
    #[test]
    fn an_empty_window_folds_nothing() {
        let (test, _) = linear_history();
        let seen = folded(&test, &[], 2, |_, hash| Some(hash));
        assert_eq!(seen, Some(Vec::new()));
    }
}
