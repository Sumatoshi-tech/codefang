//! Test support for the per-analyzer fold tests
//! (`specs/frds/FRD-bounded-memory-history.md`): one repository exercising
//! every chunk edge, and a driver that folds an analyzer's REAL pre-pass
//! products through [`walk_chunks`] in chunks of every size, checking each
//! finished value against the one-pass value and the live products against
//! the chunk size.

use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cf_gitlib::testutil::{init_repo, CommitBuilder, TestRepo};
use cf_gitlib::Hash;

use super::{walk_chunks, ChunkPlan, CommitStamp, HistoryRun, StampSummary};

/// Base instant of the fixture: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits (one default tick each).
const DAY: i64 = 86_400;
/// The fixture developers.
const ALICE: (&str, &str) = ("alice", "alice@example.com");
const BOB: (&str, &str) = ("bob", "bob@example.com");
/// Alice under her full name (an email alias).
const ALICE_SMITH: (&str, &str) = ("Alice Smith", "alice@example.com");
/// Alice under another email (a name alias).
const ALICE_ELSEWHERE: (&str, &str) = ("alice", "alice@elsewhere.org");
/// The fixture's `--max-changes-per-commit` cap: its oversized commit
/// changes one file more than this.
const FIXTURE_CAP: &str = "3";
/// Files the oversized commit writes (one above [`FIXTURE_CAP`]).
const OVERSIZED_FILES: usize = 4;
/// The window position of the oversized commit.
pub(crate) const OVERSIZED_POS: usize = 6;
/// The chunk sizes every fold runs with (the window length is added per
/// run): one commit per chunk; two; three, so the merge and the oversized
/// commit open chunks; seven, so the oversized commit closes the first one.
const CHUNK_SIZES: [usize; 4] = [1, 2, 3, 7];
/// A blob the binary detector rejects (NUL bytes after a PNG signature).
const PNG_V1: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x01";
/// The binary blob, changed.
const PNG_V2: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x02";

/// `n` numbered lines.
fn lines(n: usize) -> Vec<u8> {
    (1..=n)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes()
}

/// A Go source body with `n` one-line functions.
fn go_source(n: usize) -> Vec<u8> {
    let mut s = String::from("package main\n\n");
    for i in 0..n {
        s.push_str(&format!("func f{i}() int {{ return {i} }}\n"));
    }
    s.into_bytes()
}

/// A ten-commit window with every edge a chunk boundary can split:
///
/// | pos | commit | author | edit |
/// |-----|--------|--------|------|
/// | 0 | c1 | alice | `a.go`, `b.txt`, binary `logo.png` |
/// | 1 | side | bob | side line off c1: `c.txt`, `b.txt` |
/// | 2 | c3 | Alice Smith (email alias) | `a.go` |
/// | 3 | merge | alice | merges side |
/// | 4 | rename | bob | `a.go` → `src/a.go` |
/// | 5 | empty | bob | no change |
/// | 6 | oversized | bob | four new files (above the cap) |
/// | 7 | delete | alice (name alias) | deletes `b.txt` |
/// | 8 | binary | alice | `logo.png`, `src/a.go` |
/// | 9 | tail | bob | `c.txt`, `d0.txt` |
pub(crate) fn chunk_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    let at = |n: i64| T0 + n * DAY;
    let c1 = CommitBuilder::new(&test, "c1", at(0))
        .author(ALICE.0, ALICE.1)
        .write("a.go", &go_source(2))
        .write("b.txt", &lines(3))
        .write("logo.png", PNG_V1)
        .commit()
        .expect("c1");
    let side = CommitBuilder::new(&test, "side", at(1))
        .author(BOB.0, BOB.1)
        .parent(c1)
        .detached()
        .write("c.txt", &lines(2))
        .write("b.txt", &lines(4))
        .commit()
        .expect("side");
    CommitBuilder::new(&test, "c3", at(2))
        .author(ALICE_SMITH.0, ALICE_SMITH.1)
        .write("a.go", &go_source(3))
        .commit()
        .expect("c3");
    CommitBuilder::new(&test, "merge", at(3))
        .author(ALICE.0, ALICE.1)
        .merge_parent(side)
        .write("c.txt", &lines(2))
        .write("b.txt", &lines(4))
        .commit()
        .expect("merge");
    CommitBuilder::new(&test, "rename", at(4))
        .author(BOB.0, BOB.1)
        .rename("a.go", "src/a.go")
        .commit()
        .expect("rename");
    CommitBuilder::new(&test, "empty", at(5))
        .author(BOB.0, BOB.1)
        .commit()
        .expect("empty");
    let mut oversized = CommitBuilder::new(&test, "oversized", at(6)).author(BOB.0, BOB.1);
    for i in 0..OVERSIZED_FILES {
        oversized = oversized.write(&format!("d{i}.txt"), &lines(i + 1));
    }
    oversized.commit().expect("oversized");
    CommitBuilder::new(&test, "delete", at(7))
        .author(ALICE_ELSEWHERE.0, ALICE_ELSEWHERE.1)
        .delete("b.txt")
        .commit()
        .expect("delete");
    CommitBuilder::new(&test, "binary", at(8))
        .author(ALICE.0, ALICE.1)
        .write("logo.png", PNG_V2)
        .write("src/a.go", &go_source(4))
        .commit()
        .expect("binary");
    CommitBuilder::new(&test, "tail", at(9))
        .author(BOB.0, BOB.1)
        .write("c.txt", &lines(5))
        .write("d0.txt", &lines(2))
        .commit()
        .expect("tail");
    test
}

/// The parsed `run` matches for `codefang run -a <id>
/// --max-changes-per-commit 3 <extra> <repo>`.
pub(crate) fn run_matches(test: &TestRepo, id: &str, extra: &[&str]) -> clap::ArgMatches {
    let path = test.path().to_str().expect("utf-8 path");
    let argv: Vec<&str> = [
        "codefang",
        "run",
        "-a",
        id,
        "--max-changes-per-commit",
        FIXTURE_CAP,
    ]
    .into_iter()
    .chain(extra.iter().copied())
    .chain([path])
    .collect();
    crate::build_codefang_command()
        .try_get_matches_from(argv)
        .expect("run args parse")
        .subcommand_matches("run")
        .expect("run")
        .clone()
}

/// The opened run of [`run_matches`].
pub(crate) fn open_run(test: &TestRepo, id: &str, extra: &[&str]) -> HistoryRun {
    HistoryRun::open(&run_matches(test, id, extra)).expect("open run")
}

/// One live product's slot in a counter: a fold test pairs every product
/// with a ticket, so it observes how many products the walk holds at once.
struct Ticket(Arc<AtomicUsize>);

impl Ticket {
    fn new(live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(live))
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Folds `run` in chunks of `size` through an analyzer's REAL pre-pass
/// (`prepare`), drop gate (`oversized`) and fold (`consume` per kept commit
/// into `fold`), asserting that no more than one chunk of products is ever
/// alive and that every product is dropped; returns the folded state and
/// the identity summary for the analyzer's `finish`.
fn fold_in_chunks<P: Send, F>(
    run: &HistoryRun,
    size: usize,
    prepare: &(impl Fn(&cf_gitlib::Repository, Hash) -> Option<P> + Sync),
    oversized: &impl Fn(&P) -> bool,
    mut fold: F,
    consume: &impl Fn(&mut F, &CommitStamp, P),
) -> (F, StampSummary) {
    let live = Arc::new(AtomicUsize::new(0));
    let summary = walk_chunks(
        run,
        ChunkPlan::sized(size),
        |repo, hash| prepare(repo, hash).map(|p| (Ticket::new(&live), p)),
        |(_, product)| oversized(product),
        |stamp, (ticket, product)| {
            let now = live.load(Ordering::SeqCst);
            assert!(now <= size, "{now} live products with chunk size {size}");
            consume(&mut fold, stamp, product);
            drop(ticket);
        },
    )
    .expect("walk");
    assert_eq!(live.load(Ordering::SeqCst), 0, "every product dropped");
    (fold, summary)
}

/// Folding `run` in chunks of every [`CHUNK_SIZES`] size and of the window
/// length finishes to the one-pass value, holding at most one chunk of
/// products at a time; returns the one-pass value.
pub(crate) fn assert_chunk_invariant<P: Send, F, R: PartialEq + Debug>(
    run: &HistoryRun,
    prepare: impl Fn(&cf_gitlib::Repository, Hash) -> Option<P> + Sync,
    oversized: impl Fn(&P) -> bool,
    new: impl Fn() -> F,
    consume: impl Fn(&mut F, &CommitStamp, P),
    finish: impl Fn(F, StampSummary) -> R,
) -> R {
    let folded = |size: usize| {
        let (fold, summary) = fold_in_chunks(run, size, &prepare, &oversized, new(), &consume);
        finish(fold, summary)
    };
    let whole = folded(ChunkPlan::whole_window().size);
    for size in CHUNK_SIZES.into_iter().chain([run.hashes.len()]) {
        assert_eq!(folded(size), whole, "chunk size {size}");
    }
    whole
}
