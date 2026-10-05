# Understanding code burndown

This page explains the mental model behind the burndown analyzer: what code
survival means, how the per-file and per-developer breakdowns are defined,
and how the line-tracking algorithm works. For the flags and the output
schema, see the [Burndown reference](../analyzers/burndown.md).

---

## What it measures

The burndown analyzer follows every line of code through Git history and
answers one question per time band: **of the lines written in this band,
how many are still alive at each later sample?** The result is the classic
burndown chart — stacked areas showing how much code from each era remains.

### Global code survival

A matrix where each row is a sample and each column is an age band. The
value at `[sample][band]` is the number of lines last written in that band
that still exist at the sample. Three figures summarise it:

- **peak lines** — the sum over bands of each band's maximum, i.e. the lines
  ever written that lived until at least one sample boundary;
- **current lines** — the live lines at the last sample;
- **survival rate** — `current / peak`, never above 100 %.

A line that is written and deleted inside one sampling period never reaches
a sample boundary and is invisible to the matrix (it still shows up in the
per-commit `LinesAdded` / `LinesRemoved` of the ndjson and timeseries
formats).

### Per-file survival (`--burndown-files`)

The same matrix per file, reduced to `current_lines`, `peak_lines` and
`survival_rate`. Files are keyed by their path at the end of the window:
renames are followed (a pure `git mv` keeps the file's history), and a file
deleted inside the window stays in the list with zero current lines.
Binary files are never tracked.

### Per-developer survival and interactions (`--burndown-people`)

Every line also remembers who wrote it. This gives:

- **developer survival** — per developer, the lines they wrote that are
  still alive (`current_lines`), the lines they ever wrote that reached a
  sample (`peak_lines`) and the ratio. Every developer the shared identity
  detector knows appears, even with zeros;
- **interactions** — the overwrite matrix: one entry per `(author,
  modifier)` pair counting the lines written by `author` that `modifier`
  deleted or replaced inside the window. `is_self_modify` marks a developer
  rewriting their own code.

### File ownership (both flags)

With both flags on, each file carries `ownership` — live lines per
developer id — and the top owner (most lines; the smaller id on a tie) with
its percentage of the file's owned lines. A deleted file has an empty
ownership map and no top owner.

!!! note "Memory"
    Per-file tracking keeps one sparse history per path ever tracked;
    per-developer tracking keeps one per developer. Both are small compared
    to the line treaps the global curve needs anyway.

---

## How it works

### Algorithm overview

1. **First-parent walk.** Burndown always follows the first parent, whatever
   `--first-parent` says, because line survival is a property of the main
   line of development. A merge commit is consumed as the change set it
   introduces on that line: the lines it deletes are debited at the merge's
   own tick, the lines it brings in are credited to it. The user's
   `--first-parent` flag is never forced on any other analyzer.

2. **Window base.** In a `--limit`, `--since` or `--head` window the tracked
   state is seeded from the window's base tree — the first in-window
   commit's parent — at tick 0 before the first commit is applied. A
   commit's `LinesAdded` / `LinesRemoved` are therefore the lines it really
   changed (the same delta devs reports for that commit), and the base
   lines form band 0 of the global curve as "older than the window" code
   with no author. A window starting at a root commit has no base.

3. **Line tracking.** Each tracked file is an implicit treap of line
   segments. A segment's value packs `(author, tick)`: the tick field is
   exactly as wide as the window's last tick needs and the author takes
   the remaining bits of the 32-bit value (without `--burndown-people` the
   whole value is the tick), so no tick is ever reserved and a 45-year or
   an hourly history runs like any other. Every edit splits and merges the
   treap in `O(log N)`.

4. **Deltas.** Each `File::update` reports `(current, previous, delta)`:
   an insertion `(now, now, +n)`, a deletion `(now, written, −n)` per
   deleted segment. The per-commit reduce turns the reports into the
   commit's sparse global history (`current tick → written tick → delta`),
   the per-file and per-developer histories (keyed by the author of the
   affected lines) and the interaction cells (a negative report whose
   author differs from, or equals, the committing developer). A path that
   is re-inserted — a length mismatch after a dropped commit, a text file
   turning binary and back — is debited first, so no line is ever counted
   twice or leaked.

5. **Dense matrix.** The sparse histories are laid out on the sampling
   grid: band `j` holds the lines written in ticks `[j·granularity,
   (j+1)·granularity)`, sample `i` is the state after the last tick of
   `[i·sampling, (i+1)·sampling)`, and samples with no recorded tick repeat
   the previous row. The grid runs to the newest commit's tick, so the last
   sample is always the end of the window.

6. **Figures.** Peak, current and survival per matrix; ownership from the
   live segments of each file at the end of the walk.

### Key parameters

- **`--granularity`** — ticks per age band. Wider bands, fewer columns.
- **`--sampling`** — ticks per sample. Wider periods, fewer rows; a
  non-positive value means the default (30), and the two are independent.
- **`--tick-size`** — hours per tick (24 by default). All commits inside one
  tick share a band.
- **`--max-changes-per-commit`** — applied exactly as devs applies it: a
  commit whose raw tree diff exceeds the cap is dropped before the ticks
  and identities are assigned, so burndown and devs see the same commit
  set. A dropped commit's edits are never applied; the next commit that
  touches an affected file finds its tracked state out of sync, debits it
  and re-inserts the file whole at that commit's tick.

### Age labels

The text report groups the last sample's bands into `< 1 month`, `1-3
months`, `3-6 months`, `6-12 months` and `> 12 months` by the band's age at
the end of the window: band `i` of `n` is `(n − i) · granularity ·
tick_hours / 24 / 30` months old, so the last band is the youngest. The plot
labels its series the same way (`3mo`, `2y`, …) unless the history spans two
calendar years, in which case the bands are regrouped by year.

---

## Use cases

- **Project health monitoring**: track the overall survival rate. A falling
  rate means the code base is being rewritten faster than it grows.
- **Developer contribution analysis**: `developer_survival` shows whose code
  persists; `interactions` shows who rewrites whose code.
- **Code age visualization**: the burndown chart shows how much ancient code
  remains.
- **Refactoring impact**: run with `--since` on the refactoring window and
  read how much of the base tree (band 0) the window replaced.
- **File-level hot spots**: `file_survival` ranks files by surviving lines;
  a low survival rate with a high peak is a file that keeps being rewritten.

---

## Limitations

- **Sequential processing**: line survival is cumulative state, so commits
  are applied one after another (only the per-commit tree diffs used for
  the cap are computed in parallel).
- **Memory**: every live line of every tracked file is a treap segment.
  Large repositories need memory in proportion to their line count.
- **Binary files excluded**: a binary blob is never tracked; a text file
  turning binary is debited at that commit.
- **Rename tracking**: renames come from Git's similarity detection. A move
  with heavy edits appears as a deletion plus an insertion.
- **Tick resolution**: all commits inside one tick share a band; sub-tick
  ordering inside a band is not represented.
- **Unknown authors**: lines older than the window (the seeded base tree)
  have no author. They count in the global curve and in per-file survival,
  but not in developer survival, interactions or ownership.
- **Identity capacity**: with `--burndown-people` the developer id shares
  the 32-bit segment value with the tick. A window of `T` ticks leaves
  `32 − ⌈log₂(T + 1)⌉` bits for developers — 2²³ for a 500-tick window,
  2¹⁷ for a 45-year daily one. Ids beyond that fold into the "unknown"
  slot.

---

## See also

- [Burndown reference](../analyzers/burndown.md) — flags and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
