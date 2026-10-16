# Changelog

All notable changes to the Codefang project are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/).

---

## [Unreleased] — History core: one shared walk, deterministic output

`specs/bugs/BUG-history-core-pipeline.md`. Every `history/*` analyzer now
runs through ONE shared core (`crates/cf-commands/src/handlers/history/mod.rs`):
one commit window, one identity detector, one tick assigner, one change
filter, one chronological reduce.

### Fixed

- **Every history analyzer drops a commit above `--max-changes-per-commit`,
  alone or co-selected** (`specs/frds/FRD-bounded-memory-history.md`). The
  direct walks of `history/typos`, `history/sentiment`, `history/anomaly`
  and `history/imports` never applied the gate while the shared UAST walk
  did, so `-a history/typos` and `-a history/typos,history/quality` reported
  different typos, comments, imports and author ids for the same window.
  Their ndjson records now carry the commit's window position, so a merged
  ndjson stream stays in commit order across a dropped commit
  (`bins/codefang/tests/chunked_history_e2e.rs`).
- **`history/shotness` co-selected with another UAST analyzer honors
  `--first-parent`, `--shotness-dsl-struct` and `--shotness-dsl-name`.** The
  shared multi-analyzer walk used the default DSL and skipped first-parent
  merges, so the co-selected report differed from the single-analyzer one
  (`bins/codefang/tests/shotness_e2e.rs`).
- **Mixed static+history selections render every advertised format.** A
  `-a static/…,history/…` run with `--format timeseries` (or
  `timeseries --ndjson`) emitted `{"analyzers": [], "commits": []}`: the mixed
  path converted the final reports, which carry no per-commit data. It now
  emits the history time series and names the omitted static analyzers once
  on stderr. `--format text` prints the static sections followed by the
  history text and `--format compact` every analyzer's compact line, instead
  of being rejected (`bins/codefang/tests/mixed_selection_e2e.rs`).
- **`history/shotness` ndjson records keep their commit position** when
  `--max-changes-per-commit` drops a commit, so a merged ndjson stream that
  selects shotness before another analyzer stays in commit order
  (`chunked_history_e2e.rs::merged_ndjson_keeps_window_order_across_a_dropped_commit`).
- **`history_core_e2e.rs::output_does_not_depend_on_the_host_cpu_count` no
  longer flakes**: it compared two multi-analyzer documents whose
  `metadata.analyzed_at` came from the wall clock; its runs now pin
  `CODEFANG_NOW`.
- **`history/devs` ndjson records keep their commit position** when
  `--max-changes-per-commit` drops a commit: devs numbered its records by the
  kept-commit index while every other analyzer used the window position, so
  a multi-analyzer ndjson stream came out of commit order (`[1, 2, 1, 3, 2,
  3]`).
- **Report metadata and both binaries name the same version.** Report
  `codefang_version` read `dev` and `uast version` printed `dev (commit:
  none, …)` while `codefang version` printed the crate version: cf-version
  kept the retired reference's frozen defaults and only the `version`
  subcommand patched over them. cf-version now defaults to the crate version
  and finds the commit itself (`CF_COMMIT`, `GIT_COMMIT`, `GIT_HASH`, else the
  checkout's `HEAD`); cf-commands' duplicate build script is gone.
- Docs: the glossary, the site index and the contributing guide no longer
  describe the retired Go pipeline (chunks, hibernation, checkpoints,
  OpenTelemetry); `uast query` examples use the real syntax (`uast query
  '<expr>' <file>`, no `-e`); the MCP how-to says no launcher ships yet.
- **`history/shotness` tracks every declaration, not only the top-level
  ones** (`specs/bugs/BUG-history-shotness-nested-entities.md`). Class
  methods (Java, C#, TypeScript, JavaScript, Python) and nested functions
  were invisible because the default structural query `filter(...)` only
  looked at the file node's direct children. The UAST DSL now implements
  `rfilter(expr)` / `rmap(expr)` (recursive pre-order filter / map; they
  were identity no-ops) and the `props` field (`.props.name`), and the
  shotness default is `rfilter(.roles has "Function")`. `uast query`
  gains the same stages.
- **Shotness entities carry a qualified name**: `Class.method`,
  `Outer.Inner.method`, `outer.inner`, and the receiver type for
  `func (a *A) String()` (`A.String`), so same-named methods on two
  receivers are two entities instead of one.
- **Shotness renames and deletions reach the report.** The report is now
  built from the ONE live entity state (a replay of each commit's
  touched / ended / moved effects): a `git mv` moves every entity of the
  file under the new path with its counts and couplings (6 entities for 3
  before), a removed declaration or deleted file ends its entity (it is
  dropped with its couplings), and merge commits are no longer counted
  against the first parent (`--first-parent` keeps them as ordinary
  commits).
- **`--shotness-dsl-struct` and `--shotness-dsl-name` are honored** and
  validated before the walk (an invalid expression is a named flag error);
  they were accepted and ignored.
- **The shotness walk runs its per-commit work in parallel** through the
  shared pre-pass (the reduce stays sequential and chronological), and a
  recursive DSL query no longer allocates per visited node: hercules
  `--limit 300`, debug build, 43.1 s → 10.7 s wall with identical totals.
- cf-shotness: the unused report store (`store.rs`, `ReportWriter`),
  the per-tick aggregator and `CommitSummary` are deleted.
- **`history/burndown` no longer panics on a history reaching tick 16383**
  (`previousTime cannot be TreeMergeMark`, exit 101 on any 45-year daily or
  2-year hourly history); the merge-mark sentinel is gone and `(person,
  tick)` values are packed with a tick field sized from the window
  (`specs/bugs/BUG-history-burndown-survival.md`).
- **Burndown debits a merge's deletions at the merge's tick** (they were
  filed at tick 0, erasing the lines from every earlier sample, lowering
  `total_peak_lines` and reporting `lines_removed: 0` for merges).
- **`--granularity`, `--sampling`, `--burndown-files`, `--burndown-people`
  are applied.** The two sizes shape the sampling grid; the two switches
  fill `file_survival` (per-file survival, ownership with both flags),
  `developer_survival` and `interactions` (lines of an author deleted or
  replaced by a modifier), `tracked_files` / `tracked_developers`, the
  ndjson `FileDeltas` / `PeopleDeltas` / `MatrixDeltas`, and the text
  report's "Top Developers" section.
- **`--format timeseries` stamps every burndown commit with its own tick**
  (every commit carried the last tick) and **lists the devs commits**
  (`"commits": []` for a devs-only run).
- **The burndown text and plot age bands read newest-to-oldest** (band 0,
  the oldest code, was labelled one month old).
- **`analysis_period_days` is the tick span** (`last_tick · tick_size_hours
  / 24`; a 310-day history reported 300, a 10-day one 0).
- **A windowed burndown run (`--limit`, `--since`, `--head`) seeds the
  tracked state from the base tree**, so a commit's `LinesAdded` /
  `LinesRemoved` are the lines it changed (the first touch of a
  pre-existing file counted the whole file) and burndown agrees with devs
  on the same commit; the base lines form band 0 of the global curve.
- **Burndown never leaks or double-counts lines:** a deletion always debits
  the tracked lines, a re-insertion debits first, a rename moves the treap.
- **Burndown applies `--max-changes-per-commit`** like devs, so both see
  the same commit set.
- **`history/devs` counts a 0-byte blob as an empty text side**
  (`empty.txt` 0 → 2 lines was a `0/0/0` entry) and **honors
  `--empty-commits`** (`specs/bugs/BUG-history-devs-stats.md`).
- **Bus factor reports `secondary_dev_id` whenever a secondary developer
  exists** (it was omitted for developer id 0).
- **Sentiment lengths count characters, not bytes.** `--min-comment-len`
  and the per-comment weight inside a tick compared UTF-8 byte lengths, so a
  14-character Russian comment passed a 20-character minimum and weighed
  about twice an English comment of the same length. Both now count
  characters (Unicode scalar values).
- **`history/couples` follows renames, counts every change once and keeps
  oversized commits in ownership** (`specs/bugs/BUG-history-couples-coupling.md`).
  A `git mv` carries the old path's co-change row, column, change count and
  owners to the new path (one row, chained renames included); a merge
  contributes only the files no earlier commit of the window changed and
  credits its author with those only; the changeset cap is the documented
  **1000** files (was 200) and a commit above it still owns every file it
  touched, keeps its real hash in ndjson and lists its `AuthorFiles`; the
  report lists every file at the window head that was coupled OR owned;
  `--format timeseries` carries each commit's tick (was `0`) and
  `files_touched` is the number of files the author touched;
  `avg_coupling_strength` is one mean of the pair strengths (the 101-pass
  shuffled-sum median is gone). Dead code deleted: `cf-couples`
  `report_section.rs`, `store.rs`, the dense-report / raw-JSON helpers,
  `filter_top_devs`, `sort_ownership_by_risk`, the `FileSpill` trait and the
  unused `TickData` / `CommitSummary` / `config` / `descriptor` items.
- **`history/file-history` composition counts distinct files by content-aware
  category, with tick bounds** (`specs/bugs/BUG-history-file-history-lifecycle.md`).
  Every changed file is classified over its path AND the first 8 KiB of its
  blob (binary and generated content are recognised); `composition` is the
  distinct files changed in the window per category (a one-file repository
  reports `source 1 (100%)`, not the number of edits) and `composition_ts`
  the distinct files changed per tick, each entry with its `start_time` /
  `end_time`; a rename retires the old path and the latest classification
  wins. Dead code deleted: `cf-file-history` `framework.rs`, the placeholder
  `Classifier` / `Enry` / `GeneratedDetector` (the category set lives in
  `category.rs`), `to_compact_json*` and the unused descriptor constants.
- **Selecting `history/burndown` no longer forces `--first-parent` on every
  co-selected analyzer.** Burndown walks its own first-parent window; devs,
  couples, file-history and the rest honor only the user's `--first-parent`
  (`codefang run .` reported 2 developers where the standalone devs run
  reported 12).
- **Reports no longer depend on the host CPU count.** The per-worker
  seen-files Bloom filters (couples), the worker-strided consume order
  (file-history, typos dedup) and the strided ndjson line order are gone:
  every reduce runs in chronological order over ONE exact set, and the
  "files at HEAD" filter uses the newest commit of the window. The output
  is a pure function of the repository and the flags (e2e: byte-identical
  under `taskset -c 0-1`).
- **Renames are detected** (`git_diff_find_similar`, renames on): a pure
  `git mv` is one Modify with `from.name != to.name` and the same blob, so
  devs reports 0 added / 0 removed instead of 100/100 and burndown,
  couples, file-history and shotness keep the file's identity.
- **`--since` keeps every commit authored at/after the cutoff.** The walk is
  ordered by committer time, so the author-time cutoff is now a skip filter
  over the whole walk instead of a stop condition that dropped younger
  commits sorted below an older one.
- **`--head` is the single-commit window `[HEAD]` in one place, for every
  analyzer and format.** Burndown ndjson/timeseries honor it; devs
  `--head --format json` reports the HEAD commit (it returned
  `total_commits: 0` for a non-merge HEAD — the quickstart example printed
  an empty report). `--head` ≡ `--limit 1` byte for byte; the closed-form
  `--head` special cases (devs, burndown, anomaly) are deleted.
- **`--tick-size` is applied** (validated `> 0`): one shared
  `HistoryTicks` replaces the eight hard-coded 24-hour copies, and
  `tick_size_hours` in the timeseries document reflects the flag
  (`--tick-size 1` on the ticks fixture gives `[0, 1, 11, 31, 73]`).
- **`--languages ""` means no filter in the history phase** (as in the
  static phase; one shared token parser) instead of excluding everything.
  History language detection now reads the blob content (`.h`, `.m`,
  `.tsx`, extensionless scripts resolve), and devs/anomaly use the same
  `cf-langpath` detector as the filter — the second extension table in
  cf-analyzers-plumbing is deleted.
- **The history change filter honors `--include-vendored`,
  `--include-generated` and `--extra-excluded-prefixes`** and applies the
  generated-file marker (`DO NOT EDIT`, `Code generated`, … in the first
  512 bytes) to the blob header, as the static phase does.
  `StaticFilter::skips_with_header` is the static-walk counterpart.
- **Identity:** a hit on one token (email or name) now registers the other,
  so `alice <a@x>` / `Alice Smith <a@x>` / `Alice Smith <a@y>` is one
  person in every analyzer; burndown uses the shared `IdentityDetector`
  instead of its own copy; `--exact-signatures`, `--people-dict` (an
  unreadable file is an error) and `--anonymize` (`Developer-A`, `-B`, …)
  are applied. One `AUTHOR_MISSING` constant (`cf-identity`) everywhere.
- **No 32-file spill gate.** Commits touching more than 32 files were
  contributing zero files to quality / sentiment / imports / typos /
  shotness; the gate is gone, and devs honors `--max-changes-per-commit`
  instead of a hard-coded cap.
- **Numeric flags are validated:** `--limit=-5`, `--tick-size 0`,
  `--max-changes-per-commit=-1` are rejected with
  `Error: invalid --<flag>: …`.
- **Every user error names its cause** (`Error: path not found: <p>`,
  `cannot open repository: <p>`, `unsupported format: <f>`, `format <f> is
  not supported for <analyzer>`, `no report produced for <analyzer>
  --format <f>`); the `command dispatch is blocked` placeholder is deleted.
  `--no-color` is applied (it sets `NO_COLOR` for the text/compact
  renderers).
- **`history/sentiment` scores only the comments a commit introduced or
  changed** (`specs/bugs/BUG-history-sentiment-scoring.md`): the per-commit
  product is the multiset difference between the filtered comment sets of
  the file's new and old version, so a code-only commit to a commented file
  contributes nothing. A tick without such comments carries **no score** —
  it is absent from the time series, the trend and the low-sentiment
  periods (it was scored `0.0`, "negative", HIGH risk); `total_commits`
  still counts every commit. Javadoc / Doxygen blocks (` * ` continuation
  lines) are scored; trend endpoints are clamped to `[0, 1]` and
  `change_percent` follows the clamped values; `--min-comment-len` is
  applied (`0` disables the length filter, a negative value is
  `Error: invalid --min-comment-len: …`); the SE-domain terms match whole
  words from a reviewed list (`deadline` no longer hits `dead`, `skill` no
  longer hits `kill`, `master` is gone) and the aggregate average is the
  plain mean.
- **`history/quality` samples are the static analyzers' numbers**
  (`specs/bugs/BUG-history-quality-ticks.md`): each parsed file is measured
  by `cf_quality::analyze_file` with the file bytes
  (`cf_complexity::node::Node::from_uast` + `AnalysisOptions { source }`,
  `cf_halstead::analyze_with_source`), so a one-file repository reports what
  `static/complexity`, `static/halstead`, `static/comments` and
  `static/cohesion` report for that file. A failed parse contributes
  nothing (it counted as an analyzed file with cohesion `1.0`), a
  function-free file contributes no cohesion sample, and a tick without
  analyzed files is absent from the time series and the aggregate — the
  aggregate means no longer fall below the aggregate minimums
  (`cohesion_mean_mean 0.667` next to `min_cohesion 1`).
- **The generated-content marker is applied in the static phase**
  (`specs/bugs/BUG-static-walk-generated-marker.md`). A file whose first
  512 bytes carry `Code generated … DO NOT EDIT` (or any other generated
  marker) was listed by every static analyzer although `--help` promised
  its exclusion; every `static/*` analyzer now runs on ONE shared walk
  (`cf_commands::handlers::walk_source_files`) that reads each file once
  and applies the marker to the bytes it read, so the file is skipped by
  default and re-admitted by `--include-generated`.
- **One static walk instead of seven.** The per-handler copies drifted:
  composition counted symlinked directories as files, imports had no
  64 MiB read cap, `--extra-excluded-prefixes` was matched against the
  absolute path (so `gen/` never matched under an absolute or `./` root),
  and a single-file root was stamped `.` by some analyzers and by its name
  by others. The shared walk skips `.git`, cache-tagged and symlinked
  directories (no loops), applies the policy to the root-relative path,
  caps every read at 64 MiB, and stamps a file root with the file's name
  everywhere. `static/composition` keeps its own semantics on the same
  walk (`walk_all_files`): every regular file is classified, header only.
- **Skipped files are reported once.** An unreadable or oversized file
  prints `warning: skipped <path>: <reason>` on stderr once per run and
  the end-of-run `progress: skipped N …` line counts distinct paths — a
  `-a 'static/*'` run no longer reports one unreadable file as 5–7.
- **`--per-file` says when it is ignored.** The flag is a `--format json`
  enrichment; with `yaml`, `bin`, `text`, `compact` or `plot` the run now
  prints `--per-file applies to --format json only; ignored for <format>`
  on stderr (once) instead of rendering silently without it.
- **`-a 'static/*'` equals the single runs** for every analyzer and format
  (json sections, yaml, bin), and the merged `overall_score` averages only
  the scored sections (info-only sections such as imports / composition no
  longer pull it toward `-1`) — pinned by `crates/cf-commands/tests/flag_parity.rs`
  and `bins/codefang/tests/static_walk_e2e.rs`.
- The complexity and halstead plot pages classify through
  `cf_complexity::thresholds` / `cf_halstead::thresholds` instead of their
  own copies of the band constants, so a function at a band bound is
  colored as the report labels it.

- **`history/typos` reports real typo fixes**
  (`specs/bugs/BUG-history-typos-false-positives.md`). A typo is ONE
  identifier a commit changed into a DIFFERENT, Levenshtein-close
  identifier on the same line: the identifiers the two lines share cancel
  (`v := recieve()` → `v := receive()` is reported), the pair itself must be
  within `--typos-max-distance` (`foo` → `completelyDifferent` is not), and
  a literal change next to an unchanged identifier, an empty or
  whitespace-bearing token (Markdown headings) or a self-identical pair is
  never a typo. `typo_list` lists each `wrong → correct` pair once at its
  chronologically first fix (the same bytes under any CPU count),
  `patterns` counts every occurrence (a pair fixed on two lines has
  `frequency: 2`), `file_typos` / `aggregate` count occurrences,
  `unique_patterns` counts distinct pairs; every list is `[]` when empty;
  `line` is 1-based; `--format bin` carries the json value instead of `{}`.
  The rules live in `cf-typos` (`candidate_line_pairs`, `identifier_pair`);
  its duplicate JSON encoder (`compat::GoValue`) and Levenshtein module are
  deleted.
- **`history/anomaly` counts distinct files and scores a real baseline**
  (`specs/bugs/BUG-history-anomaly-files.md`). Per tick `files_changed` is
  the number of DISTINCT files touched and `files` names them (a delete
  names the deleted file instead of `""`); the zero-variance sentinel
  `±100` needs a window of at least two observations, so the second tick of
  a history is no longer flagged by construction; `--anomaly-window` is
  applied (`>= 2`, default 20; a shorter history scores against every
  earlier tick) through the validated `cf_anomaly::config::Config` and
  rejected with a named error at preflight; `--anomaly-threshold` (`> 0`,
  default 2.0) is registered and validated the same way (it was documented
  but rejected by the argument parser); `anomalies` is always a list (`[]`, never `null`); anomalies sort by
  severity then tick.
- **`history/imports --format json|yaml|bin` is the documented history
  report** (`specs/bugs/BUG-history-imports-report.md`): `tick_size`
  (seconds), `author_index` and `imports`: developer → language → import →
  tick → count, with the parser's language names (`go`, `python`, …)
  instead of the constant `uast` and the extractor run with the file bytes
  (Kotlin imports are found). The constant zero static-shaped report, the
  generic `extract_imports_from_uast` shim and the unused
  `cf_imports::store` / `HistoryAnalyzer` are deleted.
- **Every history analyzer renders every format `--help` advertises**
  (`specs/bugs/BUG-history-text-format.md`). `--format text` exited 1 for
  `history/quality`, `history/typos`, `history/anomaly` and
  `history/imports` after writing the version header and the section name
  to stdout (`serialization error for <Name>: unsupported format: text`);
  each now prints a human summary — quality: the aggregate complexity,
  Halstead, comment and cohesion figures and the most complex ticks; typos:
  the fix counts, every `wrong → correct` pair at its first fix
  (`file:line`, commit), the recurring pairs and the files; anomaly: the
  detection settings, the per-tick baselines and the anomalous ticks by
  severity; imports: the counts, the most used imports and the top
  importers. `--format timeseries --ndjson` exited 1 for `history/couples`,
  `history/shotness`, `history/typos` and `history/anomaly`
  (`format timeseries+ndjson is not supported`); it is now the merged
  timeseries document's `commits` array, one line per commit, for every
  analyzer (byte-identical to the former output of the six analyzers that
  had it). Every whole-selection history format (`text`, `compact`,
  `ndjson`, `timeseries`) renders every selected analyzer BEFORE writing:
  an analyzer without a report is `Error: no report produced for <id>
  --format <f>` with nothing on stdout.
- **The merged `--format timeseries` keeps every analyzer's commits.** The
  commit order came from the first analyzer (by flag) that listed commits,
  so `-a history/imports,history/typos` dropped every typo-fixing commit
  that changed no import; the order is now the longest walk's, and a commit
  only a shorter walk lists is placed by its tick. `history/typos`
  contributes `{"fixes", "files"}` per typo-fixing commit (it contributed
  nothing). Each contribution is indexed by hash once (a linear search per
  commit made the merge quadratic in the commit count).
- **`history/couples` and `history/file-history` apply
  `--max-changes-per-commit`** like devs, burndown and the UAST-based
  analyzers: a commit whose raw tree diff exceeds the cap is dropped before
  identities and ticks are assigned, so every history analyzer sees the
  same commit set. Couples ndjson records carry the commit's window
  position, like the other analyzers, so a dropped commit cannot reorder
  the merged ndjson.
- **`history/imports --format compact` is a summary**
  (`developers= languages= distinct_imports= usages=`) instead of
  `tick_size=86400`.

- **The configuration file is loaded** (`specs/bugs/BUG-cli-inert-surface.md`).
  `cf-config` was never linked into the binary, so `--config` and the
  documented `.codefang.yaml` discovery did nothing. `codefang run` now
  reads `--config <file>` (unreadable → `Error: cannot read --config <p>: …`),
  else `./.codefang.yaml`, else `$HOME/.codefang.yaml`, plus the `CODEFANG_*`
  variables, and installs every supported key as the default of its flag:
  explicit flag > environment > file > compiled default. The fifteen keys
  (`analyzers`, `pipeline.max_changes_per_commit`, `history.tick_size`,
  `history.burndown.{granularity,sampling,track_files,track_people}`,
  `history.devs.{consider_empty_commits,anonymize}`,
  `history.sentiment.min_comment_length`,
  `history.shotness.{dsl_struct,dsl_name}`, `history.typos.max_distance`,
  `history.anomaly.{threshold,window_size}`) are validated with the flag's
  rule (`Error: invalid config <p>: <key> must be …, got <v>`); a key nothing
  reads loads with ONE `warning: ignoring unsupported config keys: …` line.
- **`-q/--quiet` and `-v/--verbose` do what their help says**: `-q` (now
  also spelled `--silent`) suppresses the `progress:` lines, warnings and
  errors still print; `-v` adds `progress: config file: …` and
  `progress: analyzers: …`; both together are a usage error.
- **`-o/--output <file>` writes the report to the file** for every format
  but `plot` (it was read only as the plot directory and ignored otherwise);
  `Error: cannot write --output <p>: …` when it cannot be created.
- **`--ndjson` without `--format timeseries` is an error**
  (`invalid --ndjson: requires --format timeseries, got --format <f>`); it
  was accepted and ignored.
- **A non-positive `--granularity`, `--sampling` or `--typos-max-distance`
  is rejected** (`Error: invalid --granularity: must be > 0, got 0`) instead
  of silently meaning the default.
- **A mixed static+history `--format text` fails at preflight** with
  `format text is not supported for a mixed static+history selection`
  (it ran every analyzer, then failed with `no report produced`).
- **`codefang version` names the build**: the crate version and the git
  commit (`CF_COMMIT` / `GIT_COMMIT` / `GIT_HASH` at build time, else the
  checkout's `HEAD` read by `crates/cf-commands/build.rs`, else `unknown`)
  instead of `dev (commit: none, …)`; no wall-clock time is embedded.
- **`codefang completion <shell> --no-descriptions` drops the help texts**
  (it was accepted and ignored).
- **`--profile` logs a `MEM` sample at start** and writes its
  `/proc/self/maps` snapshots to the temp directory instead of a hard-coded
  `/tmp`; its help no longer promises heap dumps.
- **`uast query -i <file>` queries the file** (it printed nothing without
  positional files) and `uast query -t <file>` explores that file;
  `uast parse -o` / `uast query -o` keep every input's output (only the last
  survived); `-` reads stdin for `parse` and `query`;
  `uast mapping --show-treesitter --language <lang>` prints the raw tree
  (it always failed with `unsupported language`); `uast completion` covers
  every subcommand, `validate` and `mapping` included.

### Changed

- `cf_persist::GobCodec` encodes straight into its writer instead of
  building the whole encoding in memory first (same bytes).

- **`history/anomaly` time-series entries carry the raw values once**, under
  `metrics`: the duplicated top-level `language_diversity` and
  `author_count` keys are gone. `Record.files` is `[]` instead of `null`
  when empty.
- **`history/typos` `typo_list` entries are 1-based `line`s** (they were
  0-based indices), and `patterns` is `[]` (not `null`) when no pair
  repeats.
- **Several history analyzers in `--format json`/`yaml` emit ONE document**
  (`{"version": "codefang.run.v1", "metadata": …, "analyzers": [{"id",
  "mode", "report"}]}` — the same shape as a mixed static+history run)
  instead of concatenated documents that `json.load` rejected.
- **`--format compact` works for history selections**: one line per
  analyzer with its aggregate figures, rendered by the shared compact
  renderer.
- `--format ndjson` over several history analyzers merges every analyzer's
  records by commit position (selection order within a commit).
- `crates/cf-commands/src/handlers/history.rs` (4583 lines) is split into
  `handlers/history/{mod, anomaly, devs, file_history, imports, quality,
  sentiment, typos}.rs`; public paths are re-exported unchanged.
- `cf-analyzers-plumbing` now holds only `HistoryTicks` and
  `IdentityDetector` (+ `Signature`, `anonymize_names`).
- `cf_gitlib::testutil` gained `CommitBuilder` (authors, skewed committer
  dates, side lines, merges, deletes, pure renames) plus
  `commit_files_as`, `delete_files`, `rename_file`.
- **History `--format text` sections and `--format compact` lines are
  titled by the analyzer id** (`history/anomaly:` instead of
  `TemporalAnomaly:`, `history/imports` instead of `ImportsPerDeveloper`);
  compact titles are padded to one width and fractional figures rounded to
  two decimals (`anomaly_rate=33.33`, not `33.33333333333333`).
- `--format timeseries --ndjson` over several history analyzers emits ONE
  line per commit with every analyzer merged into it (it concatenated each
  analyzer's lines).
- **Rankings are stable on an explicit total key.** The couples text's
  ownership risk (fewest contributors, then path), the devs / shotness /
  file-history plot treemaps and bars and the comments / halstead /
  cohesion plot charts (value, then name) sort with a stable `sort_by`;
  equal values list in name order instead of the Go pdqsort tie
  permutation `handlers/go_sort.rs` replicated.
- `site/architecture/streaming-pipeline.md` is rewritten as **History
  pipeline** (nav title too): the commit window, rename detection, the
  change filter and caps, the parallel per-commit pass, identity and ticks,
  the chronological reduce and the serialization — the Go buffering,
  hibernation, checkpointing and the spans / metrics it described do not
  exist in the Rust binary (the memory budget and chunking it now describes
  are the ones added below).
  `architecture/overview.md` and `architecture/uast.md` drop the same
  Go-era claims (coordinator, plumbing analyzers, OpenTelemetry, config
  layering, go-enry, `sync.Pool`, interning, arenas) and list which
  workspace crates the binaries do not link.

- `--max-changes-per-commit` help: dropped commits are invisible to every
  analyzer; burndown re-inserts an affected file whole the next time it is
  touched (the old text said the cap could desync burndown).
- The `--shotness-dsl-struct` default is `rfilter(.roles has "Function")`
  (`rfilter` walks the whole tree; `filter` sees only the root's children).
- `cf-config` is the flag-default layer: every field is optional, the
  compiled defaults live only in the flag registrations.
- `uast parse --format` help lists only `json, compact, none`;
  `uast validate --schema` defaults to the embedded schema (shown empty).
- The doc-examples harness runs every documented command with an empty
  `HOME` and working directory (no `.codefang.yaml` leaks in, `-o` files
  never land in the source tree) and no longer appends the removed
  checkpoint / cache flags.

### Removed

- The retired Go differential gate and its oracle data: `build/bin`,
  `tests/compat`, `tests/antisim`, `tests/golden`, `tests/golden-harness`,
  `crates/cf-gojson/oracle`, `crates/cf-gojson/tests/oracle_data`,
  `crates/cf-goyaml/tests/oracle.rs`,
  `crates/cf-langpath/tests/content_heuristics_diff.rs`, the compat
  workflows, `.deadcode-whitelist` and `.goreleaser.yml`. Crate docs now
  point at the report e2e suites in `bins/codefang/tests` as what pins the
  report bytes.

- The per-analyzer `timeseries --ndjson` / `ndjson` registry arms and their
  six wrapper functions (`*_timeseries_ndjson`, `burndown_record_ndjson`):
  every history selection reaches those formats through the whole-selection
  renderers, so the arms were unreachable.
- Accepted-and-ignored flags: `--workers`, `--static-workers`, `--input`,
  `--input-format`, `--gogc`, `--ballast-size`, `--cpuprofile`,
  `--heapprofile`, `--buffer-size`, `--commit-batch-size`,
  `--blob-cache-size`, `--diff-cache-size`, `--blob-arena-size`,
  `--diagnostics-addr`, `--keep-store`, `--tmp-dir`,
  `--whitelist`, `--fail-on-missing-submodules`, `--blob-cache-goroutines`,
  `--no-diff-cleanup`, `--no-diff-whitespace`, `--diff-timeout`,
  `--diff-goroutines`, `--uast-changes-goroutines`,
  `--burndown-hibernation-threshold`, `--burndown-hibernation-disk`,
  `--burndown-hibernation-dir`, `--burndown-debug`, `--burndown-goroutines`.
- The dead plumbing providers `TreeDiff`, `FileDiff`, `BlobCache`,
  `UASTChanges`, `LanguagesDetection`, `TicksSinceStart` (and their
  `Analyzer` trait / clock scaffolding) in cf-analyzers-plumbing.
- Dead quality / sentiment code: `cf_quality::analyzer::{ComponentSet,
  ScalarReport, accumulate_file, consume_commit, fold_commits}`,
  `cf_quality::{aggregate_commits_to_ticks, parse_report_data,
  extract_commit_time_series, TickData}`, the cf-quality store scaffolding,
  `cf_sentiment::analyzer::Config` and its gap / config-key constants, the
  cf-sentiment store record kinds, `modal_f32_mean`, and
  `cf_cohesion::AnalyzeError` with the never-failing
  `analyze_with_find_depth`.
- The seven private static folder walks and their `make_relative_path` /
  `parent_dir` copies; the no-op `ensure_sections_files_key`; the
  `go_pdqsort` delegate (the complexity plot ranks with `sort_by` on the
  raw report's total order); the unused `stacker` dependency of
  cf-commands.
- `handlers/go_sort.rs` (the Go pdqsort replica); the always-empty
  `CouplesPlotData::dev_names` and the `dev_names` parameter of the couples
  plot sections (no developer heatmap is rendered);
  `history_formats::{PartialFailure, HistoryFormatResult, leaf_name,
  leaf_supports_text, leaf_report_exists}`; the history plot phase's copies
  of the output-directory and `report.json` writers (it shares
  `plot::{create_output_dir, write_report_json_atomic, page_renderer}`).
- The Go-era `pipeline:` configuration keys (`workers` / GOMAXPROCS, GC,
  memory, cache and goroutine tuning) from `site/guide/configuration.md`:
  nothing in the Rust binary reads them.

- Accepted-and-ignored `codefang run` flags: `--checkpoint`,
  `--checkpoint-dir`, `--resume`, `--clear-checkpoint`, `--cache-dir`,
  `--no-cache`, `--debug-trace`, and the hidden `--skip-blacklist` /
  `--blacklisted-prefixes` (use `--extra-excluded-prefixes`). No checkpoint
  store, incremental cache or trace exporter is linked into the binary.
- The `codefang render` subcommand (it only printed "not available"; use
  `codefang run --format plot -o <dir>`).
- Inert `uast` flags: the global `--config`, `-v/--verbose`, `-q/--quiet`;
  `parse --progress/-p`, `parse --workers/-w`; `validate --color/--no-color`;
  `mapping --mapping` / `--coverage` (the mapping rules were never loaded).
- Every `cf-config` key without a live consumer: the `pipeline:` tuning tree
  (all but `max_changes_per_commit`), `checkpoint:`, burndown hibernation /
  debug / goroutines, and the couples / devs / file-history / imports /
  sentiment / clones thresholds; `cf_config::defaults`, `Facts` /
  `apply_to_facts`, `load_from_yaml_and_env`, `config_file_path`.
- Dead plumbing: `cf-commands` `observability.rs` and its `runtime` / `mcp`
  features (nothing enabled them), `deprecated_flag_message`,
  `build_render_command`, the binary's empty `mcp` / `profile` features and
  the `--profile` pprof stub.

### Added

- **`--shotness-keep-ended`** (config key `history.shotness.keep_ended`):
  `history/shotness` keeps entities whose declaration or file was removed,
  marked `"ended": true` with their counts and couplings, adds
  `aggregate.ended_nodes`, and lets an entity declared again continue its
  history. Hotspots, `hot_nodes` and the plot pages still show only live
  entities; the text report marks ended rows. Off by default, so the report
  is unchanged; on 31,923 kubernetes commits the flag grows the shotness
  report from 256 MB to 859 MB (`bins/codefang/tests/shotness_e2e.rs`).

- **`codefang mcp`: the Model Context Protocol server ships.** It serves
  `codefang_analyze`, `uast_parse` and `codefang_history` over stdio
  (newline-delimited JSON-RPC); every tool result is the report
  `codefang run` renders for the same input (`cf_commands::render_run`),
  and the tools take Linguist language names (`python`, `JavaScript`) as
  well as extensions. The server keeps no results between calls
  (`bins/codefang/tests/mcp_e2e.rs`; `site/integrations/mcp.md`).
- **`history/burndown` prepares its per-change work on the worker pool**
  (`specs/frds/FRD-parallel-burndown.md`): blob line counts, the binary
  check, the change filter and the line diffs run in parallel; only the
  per-file treap updates stay sequential, with the sequential rule for a
  diverged file. The raw-count pre-pass hands the tree diffs of commits with
  256 or more changes (vendoring, mass renames) to the main pass instead of
  repeating their rename detection. On 31,923 kubernetes commits, measured
  back to back: 59.4 s before, 25.9 s after, byte-identical output.
- **The finished UAST results wait on disk while the other history walks
  run** (`specs/frds/FRD-history-spill.md`). When a run selects a non-UAST
  history analyzer next to the UAST ones, each UAST analyzer's results are
  written to an unlinked temporary file in `TMPDIR` (bincode, through
  `cf_persist::GobCodec`) and read back only when that analyzer renders;
  `-v` prints each spill's size. On 31,923 kubernetes commits with
  `--memory-budget 4GB`, `history/*` peaks at 3.80 GiB instead of 4.75 GiB
  (121 MB spilled), output unchanged. On a `tmpfs` `/tmp` the spill leaves
  the process but not RAM; point `TMPDIR` at a disk.

- **Bounded-memory history walks: `--memory-budget`, `--chunk-size` and the
  chunked core** (`specs/frds/FRD-bounded-memory-history.md`). Every history
  walk prepares its per-commit work on a worker pool at most one chunk ahead
  of an in-order fold and frees each product once folded; identity and tick
  assignment run one commit at a time (`Stamper`), so the output is
  byte-identical for every chunk size, budget and CPU count. The workers
  live for the whole walk and claim short runs of consecutive commits from a
  shared cursor (their libgit2 caches keep the shared trees), taking over
  half of another worker's unstarted run when idle, so a slow commit no
  longer stalls its slice and chunk boundaries cost no barrier
  (`history/anomaly` on 31,923 kubernetes commits: 43 s → 32 s;
  `history/sentiment` on 7,973: 26.1 s → 16.1 s).
  `--memory-budget <size>` (default: half the RAM, at most 2 GiB; also
  `pipeline.memory_budget` in the config file) plans each walk's chunk size
  and worker count from its share of the budget and its measured cost per
  worker and per product, and gives libgit2 a fixed part of it: a quarter
  for its object cache (its 256 MiB default made the parallel walks evict
  each other's trees) and 7.5 % for mapped pack files (8 GiB by default). `--chunk-size <n>` (also `pipeline.chunk_size`) fixes the chunk
  instead. `-v` prints each walk's plan. Invalid values fail before the walk
  with `Error: invalid --memory-budget: …` / `Error: invalid --chunk-size: …`
  (`bins/codefang/tests/chunked_history_e2e.rs`,
  `bins/codefang/tests/config_e2e.rs`).
- **Free heap memory goes back to the OS between chunks**
  (`cf_meminfo::release_free_heap`). glibc kept every worker's arena at its
  largest transient working set: a forced trim dropped a running
  `history/sentiment` from 1777 MB to 679 MB.
- **A combined report is rendered without copying it.** The multi-analyzer
  document is encoded from the decoded reports in place
  (`cf_analyze::conversion::render_converted_output`) instead of from a deep
  copy, and the encoded bytes are returned instead of copied into a second
  buffer; a 31,923-commit kubernetes `history/*` report is 538 MB of json.
- **The UAST analyzers parse one change at a time.** `history/quality`,
  `sentiment`, `typos`, `imports` and `shotness` run as one memoized pass,
  alone or co-selected; each change's two sides are parsed once, shared by
  every selected analyzer and dropped before the next change, instead of
  holding every parse of a commit. Peak RSS on 7,973 kubernetes commits:
  `history/sentiment` 2.5 GB → 0.97 GB, the five co-selected 2.9 GB →
  1.7 GB, output unchanged.
- `bins/codefang/tests/common/mod.rs`: every CLI e2e suite builds its
  commands through one helper that points `HOME` at the test scratch
  directory and drops inherited `CODEFANG_*` configuration variables, so a
  developer's `~/.codefang.yaml` or environment can no longer change test
  results (verified by running all suites under a hostile `HOME` config).
- `bins/codefang/tests/history_formats_e2e.rs`: every history analyzer in
  every format `--help` advertises (json, yaml, bin, timeseries,
  timeseries --ndjson, ndjson, text, compact, plot) exits 0 with a
  non-empty, well-formed result; the matrix is checked against the
  `--help` format list; `-a 'history/*'` in text (one section per analyzer)
  and compact (one line per analyzer); the merged timeseries --ndjson
  equals the document's `commits`; a failing text run writes nothing.
- `bins/codefang/tests/history_core_e2e.rs`: 13 CLI-level scenarios over
  real git fixtures (first-parent isolation, CPU-count independence, pure
  rename, skewed `--since`, `--head` per format, one-document JSON, named
  errors, `--tick-size 1`, `--languages ""`, the generated marker, alias
  identities / `--exact-signatures` / `--anonymize` / `--people-dict`,
  a 40-file commit under quality, history `--format compact`).
- `bins/codefang/tests/quality_e2e.rs` and `sentiment_e2e.rs`: CLI-level
  scenarios over real git fixtures (per-tick complexity 2 → 4 with a gap
  tick, aggregate `min <= mean`, a 40-file commit, static == quality per-file
  numbers for Go and Python; ordered positive / negative scores in `[0, 1]`,
  unscored comment-free ticks, Javadoc, unchanged comments not re-counted,
  clamped trend endpoints, `--min-comment-len`, determinism, json / yaml
  agreement).
- `cf_quality::analyze_file` / `FileSample` / `TickQuality::push`;
  `cf_sentiment::new_or_changed`; VADER rule tests against the published
  example scores (negation, `but`, idioms, `least`, `!` / `?` emphasis,
  ALLCAPS, emoji).
- Unit tests: rename detection and the `--since` skip filter (cf-gitlib);
  register-on-hit, exact signatures, people dict, anonymization, ticks
  (cf-analyzers-plumbing); the shared preflight, the generated-marker
  filter, `--extra-excluded-prefixes`, the window with `--head`
  (cf-commands).
- `bins/codefang/tests/static_walk_e2e.rs`: the generated marker under
  every static analyzer (composition's exception included), `static/*`
  parity with the single runs in json / yaml / bin, the `--per-file`
  stderr note, the once-per-run unreadable-file warning, determinism.
- Walker unit tests in `crates/cf-commands/src/handlers.rs` (file root,
  lexical nesting, `.git` / cache / symlinked directories, symlink loop,
  oversized and unreadable files, the generated marker, vendored paths,
  root-relative `--extra-excluded-prefixes`, `--languages` globs, the
  all-files walk) and the seven-analyzer parity matrix in
  `crates/cf-commands/tests/flag_parity.rs`.
- `bins/codefang/tests/config_e2e.rs` (11 scenarios: file / env / flag
  precedence, discovery, errors, unsupported keys, `-v`),
  `bins/codefang/tests/cli_surface_e2e.rs` (16 scenarios over every wired
  flag and the removed surface), `bins/uast/tests/cli_surface_e2e.rs`
  (10 scenarios), and the cf-config / cf-commands unit tests listed in
  `specs/bugs/BUG-cli-inert-surface.md`.

---

## [Unreleased] — `--limit` selects the newest commits; `--since` works

### Changed

- **`--limit N` now analyzes the N NEWEST commits** (the `git log -n`
  convention) instead of the N oldest. Commits are still delivered to the
  analyzers oldest-first, so tick, burndown and identity semantics are
  unchanged — only the selected window moves. This is a deliberate
  divergence from the retired Go reference, which selected the oldest
  window (measured: `--limit 3` on kubernetes emitted `2c4b3a562ce3`, its
  first commits). `specs/frds/FRD-newest-commit-window.md`.
- **`--since` with a cutoff in the middle of the history now works.** It
  previously planned a non-zero commit count, loaded zero (the oldest-first
  loader stopped at the first too-old commit) and aborted with
  `expected N commits, got 0: EOF`, empty stdout and exit 1. A cutoff newer
  than every commit still yields the empty report with exit 0.
- **`--since` now accepts a git revision** (`v1.2.0`, `HEAD~100`, a SHA):
  that commit's author time becomes the cutoff. Duration forms now honor the
  `CODEFANG_NOW` test clock, and the CLI's duplicate time parser was deleted in
  favour of the single `cf-gitlib` one.
- **A bad `--since` value says so**: `Error: cannot resolve --since "<value>"`
  instead of the generic `command dispatch is blocked` placeholder (same exit
  code, still no stdout).

### Removed

- **The Go oracle is gone.** The frozen Go reference binaries
  (`build/bin/{codefang,uast}`), the differential parity harness
  (`tests/compat/`, `tests/antisim/`), the Go-captured golden reports and their
  `golden-harness` crate (`tests/golden/`, `tests/golden-harness/`), the
  always-skipped yaml.v3 / enry differential tests, and the Go-era release and
  deadcode configs (`.goreleaser.yml`, `.deadcode-whitelist`) were removed. The
  Rust binaries are the product; machine-format output is pinned by the
  workspace's own CLI-level end-to-end tests, and analyzer behaviour is free to
  be corrected where the retired reference was wrong.

### Fixed

- **`static/complexity` counts what the docs say** (`specs/bugs/BUG-static-complexity-metrics.md`).
  Cyclomatic now sees every `&&`/`||` in JavaScript and Java and Python's
  `and`/`or` (operators are read from the node's `operator` property, a bare
  operator token, or the file source between the operands — the file source
  is passed into the analysis instead of the function token), no longer
  counts Go's three-clause `for` twice, Rust's `_ =>` arm, or Go `func(T) U`
  parameter types as functions named `(T)`. Cognitive complexity follows the
  SonarSource rules: braced `else { if … }` is nested, boolean sequences
  count in every expression (returns, assignments, loop conditions, call
  arguments), Go `if init; cond {}` no longer counts its then-block as an
  `else`, `try` adds nothing and `catch` is the nesting node, a function's
  own arrow/function-expression body is level 0, wrapped `else if` chains
  (JS/Rust/C) are flat, Python's `else:` is counted, and labelled jumps
  (`break label`, `continue label`, `goto`) add one.
- C functions were reported twice and named by their whole source text (the
  declarator is a body-less `Function` child); declarations without a body
  (C prototypes, Go interface methods and function types, Rust trait
  signatures, Java abstract/interface methods) are never reported, named
  nested functions are their own units, and anonymous literals belong to the
  enclosing function.
- `--format yaml|bin` overflowed the stack on deeply nested sources (20k
  nested `if`s, exit 134): one stack-guarded UAST bridge
  (`cf_complexity::node::Node::from_uast`) now serves every format, and the
  metric walks grow their stack on demand.
- `yaml`/`bin` `aggregate.cognitive_complexity` and `nesting_depth` were
  always `0`, and the JSON/text `Cognitive Total` was a truncated per-file
  average: aggregates are now the real cognitive sum and the max nesting
  depth, with a function-weighted average; `Max Nesting` joins the key
  metrics.
- `lines_of_code` was the raw line span; it is now the non-blank,
  non-comment-only lines of the function (nested named functions excluded),
  so `complexity_density` is meaningful. Nesting depth counts a `try`/`catch`
  statement as one level. An oversized `--format bin` payload is an error on
  stderr instead of a panic.
- Function ordering is a total order (cyclomatic, cognitive, nesting
  descending; name, file, line ascending) applied with `sort_by` in every
  format — the complexity-specific pdqsort port, the yaml handler's second
  `computed_metrics`, the three UAST bridges, the four
  `build_complexity_message` copies (two "Good" wordings) and the unused node
  constants are gone; every threshold lives in `cf_complexity::thresholds`.
- `--format text` prints the ranked function rows ("Top Issues") the docs
  showed; the complexity docs show the real `json` and `yaml` shapes, the
  thresholds, every counting rule, and the fixture-based validation story
  (the retired gocyclo/gocognit claim is gone). `--per-file` remains a
  `json`-only enrichment and is documented as such.
- **`static/clones` finds near-miss clones and labels them by the taxonomy**
  (`specs/bugs/BUG-static-clones-classification.md`). The LSH candidate stage
  moved from 16×8 to 32×4 bands (recall at the 0.5 threshold 0.06 → 0.87)
  and every candidate pair is now verified by the exact shingle Jaccard,
  which is what `similarity` reports. Type-1 = identical structure and
  tokens, Type-2 = identical structure with renamed identifiers/literals,
  Type-3 = modified structure at Jaccard ≥ 0.5 — a renamed copy no longer
  reads `Type-1`, a copy with an added statement is `Type-3` (0.64), and
  comment-only differences stay Type-1. Closures (UAST `Lambda`) are
  candidates (`Handle.func#2`, `func#N`), never paired with their enclosing
  function. The status message is keyed off `clone_ratio` (cloned functions
  / total functions) with the same bands as the score, so `0/10` no longer
  sits next to `Low duplication`. Pairs are ordered by similarity then name.
  `--per-file` lists each file's cloned functions and pairs instead of an
  empty `files: []`. The engine is linear in candidate pairs with no per-pair
  allocation: 4000 identical functions take 2.5 s / 27 MB (measured: 51.5 s
  / 1.71 GB before; 2000 functions 6.6 s / 448 MB → 0.7 s / 21 MB).
- **`static/composition` counts vendored and generated files by default and
  has an `other` category** (`specs/bugs/BUG-static-composition-per-file.md`).
  The walk no longer applies the vendor/generated path policy to the one
  analyzer whose job is to report those shares (only `--languages` narrows
  it); files are classified against their root-relative path, so `docs/`
  rules match; a file enry cannot recognize is `other` instead of `source`;
  prose extensions (`.md`, `.rst`, `.txt`, …) are documentation anywhere.
  `--per-file` entries carry the file's category and language
  (`Classified as vendor`, `Vendor Files 1`, `Language Go`) instead of the
  empty-report placeholder. New reference page `site/analyzers/composition.md`.
- `site/analyzers/clones.md` / `site/explanation/clones.md` now state the
  real contract: the sections JSON envelope (not the YAML metrics shape), the
  bounded `clone_ratio` definition, the 20-node candidate minimum, the 32×4
  LSH parameters and the exact-Jaccard `similarity`.
- `cf-clones` and `cf-composition` own all of their math again: the unused
  per-file `Analyzer`s, the `_func_signatures` report round-trip, the
  duplicated pair/distribution extraction and the hand-copied section
  builders in `cf-commands` are gone; `cf-alg-lsh` is keyed by integer ids
  with a reusable query scratch (`query_after`) and documents its S-curve
  (`candidate_probability`); `cf-alg-minhash` gained `Signature::add_hash`.
- **`static/halstead` now counts Halstead's tokens** (`specs/bugs/BUG-static-halstead-operators.md`).
  Operators were recorded under UAST type names (`"BinaryOp": 2`), by
  substring containment on statement text (`return c * 2` → `*`), or as the
  whole call text; declaration statements were operands (`"c := a + b": 1`);
  token-less literals and identifiers (Go literals, Rust, Java) were dropped,
  so `func same() int { return 1 }` had volume 0. The `cf-halstead` crate now
  owns one documented classification table (`classify.rs`): leaf
  identifiers/literals are operands by text, `props["operator"]` wins when a
  mapping exposes it, calls/index/slice are `()`/`[]`/`[:]`, and every other
  node contributes the symbol runs and keywords of its source *residue* (its
  span minus its children), recovered from the file bytes, the node's verbatim
  token, or the enclosing function's token. Signatures, parameters and
  comments are skipped; closures belong to their enclosing function.
  `hal.go` (`c := a + b; return c * 2`) reports n1 = 4, n2 = 4, N1 = 4,
  N2 = 5, V = 27, D = 2.5, E = 67.5 in every format.
- **`static/halstead` aggregates are the union, not a per-file average.**
  "Total Operators (N1)" reported the mean of per-file totals; yaml/bin
  hard-coded `distinct_operators: 0 … length: 0`; `health_score` divided an
  average file volume by the function count; `message` was the modal label of
  12 unrelated keys. The aggregate (and each `--per-file` entry) is now the
  union of the functions' operator/operand multisets with per-function
  statistics (`mean_*`/`max_*` volume, difficulty, effort; `high_effort_*`),
  and the status, health score and section score follow the documented
  high-effort share bands (`cf_halstead::thresholds`).
- **`static/halstead` thresholds are defined once** and agree across json,
  yaml, bin, text and plot: volume bands `<100 / <1000 / <5000 / ≥5000`
  (lower-inclusive), `high_effort_functions` filtered by effort ≥ 10000
  (was: volume ≥ 1000), `volume_assessment` "🟡 Medium" reachable. Function
  lists use one total order (effort desc, volume desc, name, file) via
  `sort_by`; the pdqsort and `math.Log2` ports and the duplicated tables and
  formulas in `cf-commands` are deleted.
- **`static/halstead` on an empty folder** (or one with only unsupported
  files) returns the empty report with `No functions found` and exit 0
  instead of `Error: command dispatch is blocked…` / exit 1.
- **Python docstrings document their declarations.** With the UAST now
  lowering a docstring to a `Comment` carrying the `Doc` role,
  `static/comments` places the first statement of a `def` / `class` body as
  `docstring` (good, documents the declaration); a string literal later in a
  body stays `inline` and a module docstring is `unassociated`.
- **UAST shapes the analyzers can trust** (`specs/bugs/BUG-uast-mapping-shapes.md`;
  contract in `site/architecture/uast.md` → "Shape contract"):
  - C/C++: one `Function` per definition, `name`/`token` = the declarator's
    identifier (`Foo::bar` in C++); declarators are structural, prototypes
    are not functions (ioq3 `code/qcommon` counted 2580 "functions" for
    ~1300). Ternaries carry `operator: "?:"`.
  - Go: `func(T) U` parameter types are `TypeAnnotation`s, a three-clause
    `for` is one `Loop`, literals carry tokens, `type X struct{}` /
    `interface{}` / other are `Struct` / `Interface` / `TypeDeclaration`
    with `name`; `x++` / `x--` are `UnaryOp`s; `if` carries `Condition`.
  - Rust: identifiers and literals carry tokens; `///` / `//!` / `/** */`
    comments are single leaf `Comment`s with role `Doc` whose span is the
    comment text only (they were nested three deep and spilled onto the
    next line).
  - Java: `case` labels are `Case`s, one `Catch` per clause (the caught
    type is a `TypeAnnotation`), ternaries are `If` + `operator`, bodies are
    `Block`s (one `Class` per class), literals carry tokens; the false
    declarations `class_literal`, `method_reference`,
    `explicit_constructor_invocation`, `super_interfaces`, `superclass`,
    `scoped_type_identifier` (was `If`) are retyped.
  - Python: `and` / `or` / `not` / comparisons are `BinaryOp` / `UnaryOp`
    with `operator`; `elif` is an `If` child with `Else`, `else` a `Block`
    with `Else, Body`; docstrings are leaf `Comment`s with role `Doc`.
  - JavaScript/TypeScript/TSX: else-if chains sit directly under their `If`
    (no `Synthetic` wrapper); `const f = () => {}` and class-field functions
    are one named `Function` / `Method` (no nested `Lambda`); loop `kind`
    props are real props (also Rust, Kotlin).
  - Every language: `props.operator` on `BinaryOp` / `UnaryOp` /
    `Assignment` / ternary nodes, source-text tokens on every `Identifier` /
    `Literal`, leaf comments, and the `Else` role on an `If`'s alternative —
    implemented once in the lowering and verified for Go, Python,
    JavaScript, TypeScript, Rust, Java, C, C++, C#, Ruby, PHP, Kotlin and
    Swift. Prop values are `@capture`, `descendant:<kind>` or a literal
    (literals were silently dropped before); conditions accept a `parent`
    path segment.
  - The embedded JSON schema's `Role` / `NodeType` enums now include every
    value the tables (and the lowering's `Else`) emit, so `uast validate`
    accepts `uast parse` output; a test keeps them in sync.
- **`static/cohesion` measures what the docs say it measures**
  (`specs/bugs/BUG-static-cohesion-variables.md`). A variable is now a lexical
  identifier the function reads or writes — type names, the function's own
  name, callees, receivers (`self`/`this`/Go receivers) and whole declaration
  tokens no longer count, and "shared" means used by at least two functions
  (a repeat inside one function is not sharing). Token-less identifiers
  (Rust) resolve from their source span, so Rust files report real
  variables instead of a uniform 1.0. Functions without variables and
  single-function files score 1.0 as documented. Cross-file scores are
  function-weighted; files without functions are "no data" and no longer
  pull a tree of struct-only files to "Excellent"; an empty directory
  reports "No cohesion data available" with exit 0. Every function is kept
  (same-named methods were silently dropped, so the distribution did not sum
  to Total Functions); methods are named `Type.method`. The issues list holds
  only non-Excellent functions (worst first), metrics render with two
  decimals, the `Excellent (>=0.6)` label matches the code, and
  `text`/`compact` now render the distribution and issues. `line_count` is
  the function's line span and `variable_count` counts distinct variables.
  The crate's unused encoders, Bloom-filter "shared" sets and the duplicated
  aggregation in the handler were removed.
- **`static/imports` reports import targets, not statements**
  (`specs/bugs/BUG-static-imports-extraction.md`). Go alias/dot/blank specs
  came back as `str "strings`, `. "math`, `_ "embed`; JavaScript specifier
  lists (`React`, `{ useState, useEffect }`, `* as ns`) were counted as
  modules while `require('fs')` and `import('m')` were missed; Python
  `import os, sys` reported `os,` and lost `sys`, and `as` aliases counted;
  Rust `use` items were reported verbatim. One language-aware extractor
  (`cf_imports::extract_imports`, used by the static and history paths)
  now yields `strings`, `math`, `embed`, `react`, `fs`, `os`, `sys`,
  `std::collections::HashMap`, … with per-language rules for Go, Python,
  JavaScript/TypeScript, Rust, Java, C/C++, C#, PHP, Ruby and Kotlin.
- **`static/imports` stdlib classification is per language.** The single
  32-name table marked Go `errors`/`log`/`strconv`/`testing` and Python
  `datetime`/`subprocess` as `external`, skewing `external_ratio`. Each
  import is now classified by the importing file's language (Go
  first-element rule, CPython `sys.stdlib_module_names`, Node
  `builtinModules` incl. `node:`, Rust toolchain crates, `java.`/`javax.`/
  `kotlin.`, C# `System`, C/C++ standard headers, Ruby default libraries);
  Rust `crate::`/`super::`/`self::` and local-file includes classify as
  `relative`.
- **`static/imports` on an empty directory, a directory without supported
  files, or a single file** aborted with
  `Error: command dispatch is blocked on cf-commands (tier 8)` and exit 1.
  Empty roots now yield the empty report (`No import data available`,
  Unique Imports 0, Total Files 0) with exit 0, and a file root analyzes
  that one file.
- `static/imports --per-file` entries said `Total Files` `0`; each entry
  stands for one file and now says `1`. Aggregate issues are ordered by
  importing-file count, then name.
- `site/analyzers/imports.md` showed a `{"imports":{"files":[…]}}` JSON
  shape that no format emits and a "dependency graph" the analyzer never
  produced; the page now documents the real json/yaml shapes and the
  per-language extraction rules.
- `site/guide/data-analytics.md` told users to run `--since 6m` for "the last
  6 months"; `m` is minutes, so that meant 6 minutes. Docs now state the
  supported units (`h`/`m`/`s`) and give `4380h`.
- `cf_gitlib::testutil` documented a `testutil` feature that did not exist, so
  no other crate could build a deterministic fixture repo. The feature exists.
- `make lint` and `make deadcode` are implemented — `AGENTS.md` mandated both
  gates, but neither target existed (and `cargo clippy` was failing outright on
  deny-by-default lints in `cf-reportutil`, `cf-gojson` and `cf-goyaml`). Both
  are clean now.
- Removed the never-read `InitState.rule_index` field in `cf-uast` (it pinned a
  HashMap of rule names per parser) and cleared the workspace clippy backlog.
- **`static/comments` counts what its definitions say**
  (`specs/bugs/BUG-static-comments-coverage.md`):
  - a file without comments keeps its functions (they were reported as
    zero functions, zero issues);
  - documentation is keyed by declaration identity, not name, so two
    `String` methods are documented independently; names come from the
    `name` property or the name identifier, never from a node's source
    text (C functions were "named" by their whole body, Go structs by
    `struct{}`); nameless class / struct / interface nodes are skipped;
  - a comment documents a declaration only when it ends on the line
    directly above it (attribute / decorator lines belong to the
    declaration); a blank line in between no longer counts;
  - comments inside a body are `inline` for that declaration instead of
    "above" the next one; nested comment pieces (Rust `///`) count once and
    document their item;
  - `Doc Coverage` and `Good Ratio` are Σ/Σ over every analyzed file, never
    a per-file mean diluted by READMEs and comment-free files; the status
    and score follow the weighted good ratio;
  - `--format yaml` / `bin` rows carry the real name, type, line, length,
    placement, target and status, and `undocumented_functions` lists only
    the undocumented declarations with a length-based risk level;
  - `--per-file` from a relative cwd stamps `a.go`, not `../../a.go`; a
    single-file root stamps the file name;
  - the CFB1 length cap is an error path instead of a panic; the unused
    `cf_comments::Aggregator` was replaced by the one the handler uses and
    `CommentConfig::max_comment_length` was removed;
  - `site/analyzers/comments.md` and `site/explanation/comments.md`
    describe the implemented schema, placement categories, gap rule,
    thresholds and limitations (Python docstrings are not comments).

### Added

- `crates/cf-halstead/tests/real_source.rs`: Halstead numbers asserted from
  real Go, Python, JavaScript, Rust, Java and C source parsed with
  `cf_uast::Parser`, against hand-computed n1/n2/N1/N2/V/D/E.
- `bins/codefang/tests/halstead_e2e.rs`: the real binary over a two-file
  fixture — exact per-function operator/operand maps (`--format plot`),
  union aggregate and `--per-file` sums, json/yaml/text agreement, empty
  folder exit 0, determinism.
- `crates/cf-commands/src/handlers.rs`: `history_window_tests` — window
  contract (newest-N, empty/partial/full `--since`, `--limit` inside a window,
  revision spec, injected clock, unresolvable spec).
- `bins/codefang/tests/history_window.rs`: CLI-level e2e driving the real
  binary over a real git repo, including the `--head` ≡ `--limit 1` invariant.
- `crates/cf-complexity/tests/fixtures.rs` + `tests/fixtures/`: hand-computed
  cyclomatic/cognitive/nesting/LOC expectations parsed from real Go, Python,
  JavaScript, Rust, Java and C sources (else-if chains, nested else, boolean
  sequences, if-with-init, try/catch, lambdas, switch/match arms, recursion,
  labelled jumps, nested functions, declarations); the Java `switch`/ternary
  expectation is recorded `#[ignore]`d pending the UAST mapping contract.
- `bins/codefang/tests/complexity_e2e.rs`: the real binary over the same
  sources — exact per-function numbers in `json`/`bin`/`yaml`/`text`,
  `--per-file` sums, every format on 20k nesting levels, empty directory,
  tie ordering, determinism.

---

## [Unreleased] — Repo hygiene & race fix

### Fixed

- **Race in `internal/framework.PipelineSampler`**:
  `t1Captured` was a plain `bool` concurrently read by the sampler
  goroutine (`sample`) and written by the caller (`CaptureT1`),
  causing intermittent `DATA RACE` under `go test -race`. Converted
  to `sync/atomic.Bool` with `CompareAndSwap` — at most one t1 heap
  profile is captured regardless of which goroutine observes the
  trigger first. Removed the unused `t0Captured` field. Full
  `go test -race ./...` now green.

### Chore

- **Removed `// FRD: specs/frds/FRD-...md` comments from all `.go`
  files.** `specs/` is gitignored, so those references broke for
  anyone cloning the repo. Traceability now lives in FRDs and
  PR descriptions instead of source code.

---

## [Unreleased] — Cross-phase defaults: vendor & generated excluded

**Breaking change.** Default analysis output across both phases
now **excludes vendor and generated files** — matching the
convention of every mature multi-language analyser (eslint skips
`node_modules/`, rubocop skips `vendor/`, pylint skips `.venv/`,
scalafix skips `target/`, phpcs skips `vendor/`, GitHub Linguist
excludes vendor/generated from its language breakdown). Users who
want the pre-2026-04 behaviour back pass `--include-vendored
--include-generated` in their invocation.

### Flags (cross-phase)

- `--include-vendored` (bool, default `false`) — re-include paths
  detected as vendored by enry / Linguist. Covers `vendor/`,
  `node_modules/`, `third_party/`, `testdata/`, `dist/`,
  minified bundles, and more. Cross-language by construction.
- `--include-generated` (bool, default `false`) — re-include
  auto-generated files. Covers `*.pb.go`, `zz_generated_*.go`,
  `*_pb2.py`, `*.min.js`, and content-header markers
  (`DO NOT EDIT`, `Code generated`, `@generated`, …).
- `--extra-excluded-prefixes` (strings, default `[]`) — additional
  UNIX path prefixes to exclude, for ecosystems enry doesn't know
  about (e.g. `.venv/`, `target/`, `.gradle/`).

All three flags apply identically to both `-a 'static/*'` and `-a
'history/*'` runs — one flag set, one meaning.

### Deprecated

- `--skip-blacklist` — now a no-op (the new default already excludes
  vendor and generated). Cobra deprecation warning fires when the
  flag is passed.
- `--blacklisted-prefixes` — migrate to `--extra-excluded-prefixes`
  (identical semantics). Cobra deprecation warning fires when the
  flag is passed.

Both will be removed in the next minor release.

### Architecture

New package `internal/analyzers/plumbing/pathpolicy` exposing a pure
`Exclude(path, content, opts) bool` backed by enry.IsVendor +
`pkg/pathfilter`'s content-aware generated-file detection. Both
phases call the same helper — single source of truth, no
phase-specific drift.

### Measured impact (cross-language fixture, `-a static/complexity`)

| Invocation                                        | Total Functions |
| ------------------------------------------------- | --------------: |
| *(defaults)*                                      | 1               |
| `--include-vendored`                              | 4               |
| `--include-vendored --include-generated`          | 5               |

---

## [Unreleased] — Cross-phase consistency for `--languages`

**Motivation**: After the history-side push-down, `--languages` meant
different things depending on `-a 'history/*'` vs `-a 'static/*'`. Static
analysis silently ignored the flag — every UAST-supported file was parsed
and fed to every requested static analyzer regardless of the user's
preference. This release makes the flag cross-phase: one flag, one
meaning, both phases narrowed.

### Changes

- **`StaticService.LanguageGlobs`** — new field on the static service,
  populated from `--languages` via the existing
  `internal/analyzers/plumbing/langpath` single source of truth. Empty
  disables the filter (default behavior unchanged).
- **Path-based walker hooks** — both `StaticService.streamFiles` (UAST
  walker) and `StaticService.rawFilePhase` visit-check the basename
  against the glob set via `matchesLanguageGlobs` before sending the
  path downstream. Filtered files never reach the UAST parser or any
  analyzer.
- **Runtime wiring** — `runStaticAnalyzers` and `runStaticPlotAnalyzers`
  build the globs via a shared `applyStaticLanguageFilter` helper.
  Unknown language tokens fail fast on static-only runs with the same
  error shape as the history side.
- **Executor signatures** — `staticExecutor` and `staticPlotExecutor`
  gain a `languages []string` parameter; test stubs updated
  mechanically.

### Non-goals

- No content-aware post-pass on the static side (the UAST parser's
  own language router is the final authority for matched files; a
  second pass would duplicate work).
- No changes to the history side.

---

## [Unreleased] — Performance: `--languages` filter push-down into libgit2

**Motivation**: The `--languages` flag used to be applied *after* libgit2 had
already produced a full tree diff. Every delta crossed the cgo boundary, was
materialised in Go, and only then dropped by the analyzer if its detected
language wasn't in the allow-list. On polyglot repositories with a narrow
filter, libgit2 was doing 4× the tree-diff work it needed to.

### Changes

- **New package `internal/analyzers/plumbing/langpath`** — pure Go
  `Globs(langs []string) (globs []string, wantsAll bool, err error)` backed
  by enry's generated Linguist dataset (`data.ExtensionsByLanguage` +
  `data.LanguagesByFilename`). Single source of truth; 100 % test coverage.
- **New C ABI `cf_tree_diff_v2`** in `pkg/gitlib/clib/{codefang_git.h,diff_ops.c}`
  accepts a pathspec array which it forwards to libgit2's
  `git_diff_options.pathspec`. The old `cf_tree_diff` is retired in favour of
  `cf_tree_diff_v2` via `CGOBridge.TreeDiffWithPathspec`.
- **`TreeDiffRequest.Pathspec` + `BlobPipeline.TreeDiffPathspec` +
  `CoordinatorConfig.TreeDiffPathspec`** thread the pathspec from the
  analyzer through the pipeline to every worker call.
- **`TreeDiffAnalyzer.Pathspec` + `applyLanguageConfig`** resolve aliases via
  `enry.GetLanguageByAlias` (so `--languages golang` / `js` / `ts` now work,
  not just canonical Linguist names) and pre-compute the pathspec at
  `Configure` time.
- **Fail-fast on unknown languages**: `--languages notalang` now returns
  `failed to configure TreeDiff: tree-diff pathspec: unknown language: "notalang"`
  instead of silently producing an empty report.

### Measured impact

On a 500-commit × 200-file × 4-language synthetic fixture with
`--languages go`:

| Metric                      | Before  | After   | Δ      |
| --------------------------- | ------: | ------: | -----: |
| Wall time                   | 0.44 s  | 0.29 s  | −34 %  |
| Max RSS                     | 74 MB   | 66 MB   | −11 %  |
| `cgocall` cumulative CPU    | 800 ms  | 510 ms  | −36 %  |
| Unique functions in profile | 286     | 209     | −27 %  |
| JSON report                 |    —    |    —    | byte-identical |

Regression guard (no `--languages` filter): wall time 0.51 s → 0.49 s,
within noise.

### Non-goals (for this changeset)

- No new user flags.
- The Go-side `shouldIncludeChange` language filter remains as the precise
  post-pass (pathspec is deliberately over-inclusive for
  content-disambiguated extensions such as `.h`, `.pl`, `.m`, `.r`).

---

## [Unreleased] — Analytics Readiness & DWH Suitability

**Motivation**: A comprehensive data analyst review of Codefang's JSON output revealed that while the data was analytically rich (17 analyzers, 1M+ function-level rows, time-series, coupling data), it was structurally hostile to analytics tooling and DWH loading. Function records had bare names with no file paths, time-series ticks had no calendar dates, developer identities used pipe-delimited strings, and nested maps blocked efficient columnar ingestion. This release systematically fixes every identified blocker, raising the data quality score from **2.1/5 to 4.6/5**.

### Architecture: Pipeline Stage Refactor

#### `RawFileAnalyzer` and `FormattableAnalyzer` interfaces

Replaced the `FileContentAnalyzer` + `WalksAllFiles` marker interface pattern with a proper pipeline stage architecture.

**Before**: Analyzers that needed raw file access (not UAST) had to implement `StaticAnalyzer` with a no-op `Analyze(*node.Node)`, plus two marker interfaces discovered at runtime via type assertions.

**After**: Two clean interface hierarchies — `StaticAnalyzer` for UAST-based analysis and `RawFileAnalyzer` for raw file analysis — both embed a shared `FormattableAnalyzer` base. `StaticService` holds separate slices. `AnalyzeFolder` uses `pipeline.RunPhases` with explicit `rawFilePhase` and `uastPhase` stages.

**Why it matters for BI**: The pipeline refactor enabled `StampSourceFile` to receive `rootPath` and convert all file paths to relative — a prerequisite for portable DWH data. It also enabled `StampLanguage` to inject detected language into every function record.

**Files changed**:
- `internal/analyzers/analyze/analyzer.go` — new `FormattableAnalyzer`, `RawFileAnalyzer` interfaces; `StaticAnalyzer` refactored to embed `FormattableAnalyzer`
- `internal/analyzers/analyze/static.go` — `StaticService` gains `UASTAnalyzers` + `RawFileAnalyzers` slices; `AnalyzeFolder` uses `pipeline.RunPhases`
- `internal/analyzers/composition/analyzer.go` — implements `RawFileAnalyzer` directly (removed no-op `Analyze`, `NeedsAllFiles`)
- `internal/analyzers/analyze/registry.go` — `NewRegistry` accepts three slices
- `cmd/codefang/commands/run.go` — split `defaultStaticAnalyzers` into `defaultUASTAnalyzers` + `defaultRawFileAnalyzers`
- `internal/analyzers/analyze/perfile.go` — `PerFileEnricher` uses `[]FormattableAnalyzer`
- `internal/analyzers/common/renderer/json.go` — `EnrichWithPerFileData` uses `[]FormattableAnalyzer`

---

### Static Analyzers: New Fields on Every Function Record

#### `source_file` — File path on every function record

**Motivation**: 152,000+ function records in the JSON output had bare names like `"ForKind"` with no indication of which file they belonged to. This made it impossible to join function metrics to file-level data, build file heatmaps, or drill down from "bad function" to "where in the repo."

**Root cause**: The `_source_file` stamping mechanism existed and worked through aggregation, but `FormatReportBinary` called `ComputeAllMetrics` which parsed `[]map[string]any` items into typed structs. Those structs had no `SourceFile` field, silently dropping the value during struct conversion.

**Fix**: Added `SourceFile string` to all input `FunctionData` and output data structs (`FunctionComplexityData`, `FunctionHalsteadData`, `FunctionCohesionData`, all comment data structs, `HighRiskFunctionData`, `HighEffortFunctionData`, `LowCohesionFunctionData`, `UndocumentedFunctionData`). Populated from `_source_file` map key during `parseFunctionData` → `Compute()`. Updated `StampSourceFile` to accept `rootPath` and convert to relative via `MakeRelativePath`.

**JSON output key**: `"source_file"` (relative path, e.g., `"pkg/kubelet/kubelet.go"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

#### `language` — Programming language on every function record

**Motivation**: Analysts had to infer language from file extension at query time. The parser already knows the language.

**Fix**: Added `LanguageKey` constant, `StampLanguage()` function, and `Language` field to `TypedCollection` struct. Language is stamped in `analyzeFilesParallel` via `parser.GetLanguage(filePath)` and propagated through `TypedCollection` → `DetailedDataCollector.buildItems()` → `stampCollectionMetadata()` to reach the output structs.

**JSON output key**: `"language"` (e.g., `"go"`, `"bash"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

#### `directory` — Parent directory on every function record

**Motivation**: Directory-level aggregation (e.g., "which package has worst complexity") requires parsing file paths at query time, which is expensive in columnar DWH.

**Fix**: Added `DirectoryKey` constant and `Directory` field to `TypedCollection`. Stamped as `filepath.Dir(relativePath)` inside `StampSourceFile`. Propagated via `stampCollectionMetadata()` alongside language.

**JSON output key**: `"directory"` (e.g., `"pkg/kubelet"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

---

### History Analyzers: Tick Timestamps

#### `start_time` / `end_time` on every time-series tick

**Motivation**: All 6 history time-series analyzers emitted `tick: <int>` with no calendar date. Every time-series chart had an unlabeled X-axis. The `TICK` struct already carried `StartTime`/`EndTime` internally but didn't export them.

**Fix**: Created `TickBounds` type and `BuildTickBounds(ticks []TICK)` helper. Each analyzer's `ticksToReport` adds `tick_bounds` to the Report. Each `ParseReportData` reads it. Each time-series output struct gains `StartTime`/`EndTime` string fields (RFC 3339). For quality and devs analyzers, added timestamp tracking to their tick accumulators (`tickAccumulator.startTime/endTime`, `TickDevData.startTime/endTime`) with min/max tracking in `extractTC` and population in `buildTick`.

**JSON output keys**: `"start_time"`, `"end_time"` (RFC 3339, e.g., `"2024-01-15T10:30:00Z"`)

**Analyzers affected**: `history/sentiment`, `history/anomaly`, `history/quality`, `history/devs` (activity + churn), `history/file-history` (composition_ts)

---

### Developer Identity Normalization

#### Split pipe-delimited names into `name` + `email`

**Motivation**: Developer identity used `"daniel smith|dbsmith@google.com"` pipe-delimited strings from `ReversedPeopleDict`. This blocked clean dimension table creation in DWH systems.

**Fix**: Created `SplitIdentity(s string) (name, email string)` in `internal/identity/split.go`. Handles pipe-delimited, exact `"name <email>"`, and plain name formats. Updated `devName()` → `devNameAndEmail()` and `getDevName()` → `getDevNameAndEmail()`.

**Fields added**:
- `DeveloperData`: `email` field
- `BusFactorData`: `primary_dev_email`, `secondary_dev_email`
- `DeveloperCouplingData`: `developer1_email`, `developer2_email`

**Analyzers affected**: `history/devs`, `history/couples`

---

### Output Structure: Flattened Arrays

#### `developers[].languages` — map → array

**Motivation**: `map[string]LineStats` with variable language-name keys cannot be UNNEST'd in columnar DWH without custom ETL.

**Fix**: Changed `DeveloperData.Languages` from `map[string]pkgplumbing.LineStats` to `[]LanguageStatsEntry`. Internal accumulation uses unexported `langMap`, converted to sorted array via `finalizeLanguages()`. Empty language strings replaced with `"Other"`.

**Before**: `{"Go": {"added": 100, "removed": 5, "changed": 3}}`
**After**: `[{"language": "Go", "added": 100, "removed": 5, "changed": 3}]`

#### `activity[].by_developer` — map → array

**Motivation**: `map[int]int` (dev_id → commit_count) serializes to JSON with string keys, blocking typed ingestion.

**Fix**: Changed to `[]DeveloperCommits` with `{dev_id, commits}` fields. Sorted by dev_id for deterministic output.

**Before**: `{"2": 5, "3": 3}`
**After**: `[{"dev_id": 2, "commits": 5}, {"dev_id": 3, "commits": 3}]`

#### `file_contributors[].contributors` — map → array

**Motivation**: `map[int]LineStats` blocked DWH UNNEST.

**Fix**: Changed to `[]ContributorEntry` with `{dev_id, added, removed, changed}` fields. Sorted by dev_id.

**Before**: `{"2": {"added": 42, "removed": 5, "changed": 3}}`
**After**: `[{"dev_id": 2, "added": 42, "removed": 5, "changed": 3}]`

---

### Output Envelope

#### Top-level `metadata` section

**Motivation**: A DWH ingesting reports from multiple repos could not distinguish them. No repo name, analysis timestamp, or version.

**Fix**: Added `AnalysisMetadata` struct with `repo_path`, `repo_name` (from `filepath.Base`), `analyzed_at` (RFC 3339), `codefang_version` (from build ldflags). Injected after `DecodeCombinedBinaryReports` in the combined render path.

```json
{
  "version": "codefang.run.v1",
  "metadata": {
    "repo_path": "/home/user/sources/kubernetes",
    "repo_name": "kubernetes",
    "analyzed_at": "2026-04-07T23:33:00Z",
    "codefang_version": "dev"
  },
  "analyzers": [...]
}
```

#### Per-analyzer `schema` manifest

**Motivation**: DWH consumers need to know field types, grain, and cardinality for automated ETL generation.

**Fix**: Added `FieldMeta` struct with `{type, grain, description}` and static `analyzerSchemas` registry covering all 17 analyzers. Each `AnalyzerResult` in the output includes a `schema` field.

```json
{
  "id": "static/complexity",
  "schema": {
    "function_complexity": {
      "type": "list",
      "grain": "function",
      "description": "Per-function cyclomatic and cognitive complexity"
    }
  },
  "report": {...}
}
```

#### NDJSON output format

**Motivation**: The monolithic JSON (467MB for kubernetes) must be fully parsed to extract any single analyzer. NDJSON enables streaming ingestion into ClickHouse.

**Fix**: Added `FormatNDJSON` case to `WriteConvertedOutput`. One JSON line per analyzer result, with optional metadata line prepended.

```bash
codefang run --format ndjson /repo > output.ndjson
```

---

### Clone Analysis

#### `clone_type_distribution` from full population

**Motivation**: Clone pairs are capped at 1,000 in the output, but the distribution metrics (Type-1/2/3 breakdown) were computed from the capped sample, skewing percentages for large codebases with 22M+ total pairs.

**Fix**: Added `typeDistribution cloneTypeCounts` to `clonePairResult`. `matchCandidates` increments per-type counters for ALL valid pairs before the cap check. Both aggregator and per-file paths emit `clone_type_distribution` in the report. `ReportSection.Distribution()` reads from the full-population distribution.

**Before**: Distribution from 1,000 capped pairs
**After**: Distribution from 22,381,694 total pairs: `{"Type-1": 12366266, "Type-2": 3307147, "Type-3": 6708281}`

#### Relative paths in clone pairs

Clone pair `func_a` / `func_b` paths changed from absolute (`/home/user/sources/repo/file.go::funcName`) to relative (`cmd/controller/app.go::newController`). Enabled by the `StampSourceFile` rootPath change.

---

### New Files Created

| File | Purpose |
|------|---------|
| `internal/analyzers/analyze/tick_bounds.go` | `TickBounds` type + `BuildTickBounds` helper |
| `internal/analyzers/analyze/metadata.go` | `AnalysisMetadata` struct + `NewAnalysisMetadata` constructor |
| `internal/analyzers/analyze/schema_registry.go` | Static schema registry for all 17 analyzers |
| `internal/identity/split.go` | `SplitIdentity(s string) (name, email string)` |

---

### Empty Analyzer Root Causes (Documented)

Investigation of 4 analyzers that returned empty data on kubernetes (1000 commits):

| Analyzer | Root Cause | Resolution |
|----------|-----------|------------|
| `burndown.developer_survival` | Disabled by default (`Burndown.TrackPeople: false`) | Enable via config |
| `burndown.file_survival` | Disabled by default (`Burndown.TrackFiles: false`) | Enable via config |
| `history/imports` | Requires UAST-enabled pipeline mode (`NeedsUAST() = true`) | Architectural dependency |
| `history/typos` | Requires UAST-enabled pipeline mode (`NeedsUAST() = true`) | Architectural dependency |
