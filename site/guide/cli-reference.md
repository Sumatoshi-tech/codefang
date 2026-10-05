# CLI reference

Codefang ships two binaries: **`codefang`**, the analyzer engine, and **`uast`**, the
Universal Abstract Syntax Tree parser. This page is the exhaustive reference for
every command, sub-command, and flag. Every flag listed by `codefang run --help`
is documented here and is read by the pipeline; a flag that is accepted but
has no effect is a bug.

---

## `codefang`

The main analysis binary. It runs static and history analyzers against source
code and Git repositories.

#### Global flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--verbose` | `-v` | `bool` | `false` | Print run details on stderr: `progress: config file: <path>` (or `none`) and `progress: analyzers: <resolved ids>` |
| `--quiet` (alias `--silent`) | `-q` | `bool` | `false` | Suppress the `progress:` lines on stderr; warnings and errors still print. `-v` and `-q` together are a usage error. |
| `--profile` | | `bool` | `false` | Memory watchdog: logs `MEM t=<s> RSS=<MiB> threads=<n>` to stderr at start and every 2 s, writes `/proc/self/maps` to `$TMPDIR/maps_baseline.txt` at start and `maps_spike_<MiB>MiB.txt` on the first RSS spike above 4096 MiB (Linux) |

Global flags are accepted before or after the subcommand
(`codefang -q run …` ≡ `codefang run -q …`).

### `codefang run`

The primary entry point. Runs selected static and/or history analyzers on a
codebase or repository.

```bash
codefang run [path] [flags]
```

If `path` is omitted the current directory (or the value of `--path`) is used.

#### Analyzer selection

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--analyzers` | `-a` | `[]string` | all | Analyzer IDs or glob patterns. Comma-separated. Omitted ⇒ every analyzer. |
| `--list-analyzers` | | `bool` | `false` | Print every analyzer ID (one per line) and exit |

Analyzer IDs follow a `<category>/<name>` convention. Globs are supported:

```bash
# Single analyzer
codefang run -a static/complexity .

# All static analyzers
codefang run -a 'static/*' .

# All history analyzers
codefang run -a 'history/*' .

# Multiple specific analyzers
codefang run -a static/complexity,history/burndown,history/devs .

# Everything
codefang run -a '*' .
```

??? info "Available Analyzer IDs"

    **Static analyzers:**
    `static/clones`, `static/cohesion`, `static/comments`, `static/complexity`,
    `static/composition`, `static/halstead`, `static/imports`

    **History analyzers:**
    `history/anomaly`, `history/burndown`, `history/couples`,
    `history/devs`, `history/file-history`, `history/imports`,
    `history/quality`, `history/sentiment`, `history/shotness`,
    `history/typos`

#### Output flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--format` | | `string` | `json` | Output format: `json`, `yaml`, `bin`, `text`, `compact`, `ndjson`, `timeseries`, `plot` |
| `--ndjson` | | `bool` | `false` | With `--format timeseries`: emit one JSON line per commit instead of the merged document. With any other format the run fails with `Error: invalid --ndjson: requires --format timeseries, got --format <f>`. |
| `--output` | `-o` | `string` | `""` | Write the report to this file (created or truncated) instead of stdout. With `--format plot` it is the output directory for the HTML pages, and required. |
| `--per-file` | `-F` | `bool` | `false` | Add one `files` entry per analyzed file to every static section. **`--format json` only**: with any other format the flag is ignored and the run prints `--per-file applies to --format json only; ignored for <format>` on stderr (once). |
| `--no-color` | | `bool` | `false` | Disable ANSI color in `text` / `compact` output (same as `NO_COLOR=1`) |

Which formats a selection accepts:

| Selection | Accepted formats |
|-----------|------------------|
| static only | `json`, `yaml`, `bin`, `text`, `compact`, `plot` |
| history only | `json`, `yaml`, `bin`, `text`, `compact`, `ndjson`, `timeseries` (+ `--ndjson`), `plot` |
| mixed static + history | `json`, `yaml`, `bin`, `ndjson`, `timeseries`, `plot` |

`text` is supported by the six history analyzers with a text renderer
(sentiment, shotness, burndown, couples, devs, file-history); selecting any
other history analyzer with `--format text` fails with
`serialization error for <Name>: unsupported format: text`.

```bash
# Human-readable table
codefang run -a 'static/*' --format text .

# One summary line per history analyzer
codefang run -a 'history/*' --format compact .

# Interactive HTML charts
codefang run -a 'history/*' --format plot --output ./report .

# The report in a file instead of stdout
codefang run -a static/complexity --format json -o complexity.json .

# Unified time-series JSON
codefang run -a 'history/devs,history/sentiment' --format timeseries .
```

!!! note "Several analyzers, one document"
    A `json` or `yaml` run that selects more than one analyzer — a mixed
    static+history selection, or several history analyzers — always emits
    **one** document:

    ```json
    {
      "version": "codefang.run.v1",
      "metadata": { "...": "run provenance" },
      "analyzers": [
        { "id": "history/devs", "mode": "history", "report": { "...": "the devs report" } },
        { "id": "history/burndown", "mode": "history", "report": { "...": "the burndown report" } }
      ]
    }
    ```

    A single analyzer emits its own report as the document root. Several
    **static** analyzers in `json`/`text` merge their sections into one report
    with an `overall_score`.

#### Path flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--path` | `-p` | `string` | `.` | Folder or repository path to analyze (the positional `path` wins when both are given) |

#### Git history flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--limit` | `int` | `0` | Analyze at most N commits — the N **newest** (`0` = no limit). Negative values are rejected. |
| `--since` | `string` | `""` | Only analyze commits **authored** at/after this time or revision |
| `--first-parent` | `bool` | `false` | Follow only the first parent of merge commits |
| `--head` | `bool` | `false` | Analyze only the HEAD commit (ignores `--limit` and `--since`) |
| `--tick-size` | `int` | `24` | Width of one tick in hours (must be `> 0`). Burndown bands, devs activity, anomaly windows and `commits_by_tick` are bucketed by tick. |
| `--max-changes-per-commit` | `int` | `0` | Drop commits whose raw tree diff exceeds this many changes (`0` = default 10000; negative values are rejected). Dropped commits are invisible to every analyzer; burndown re-inserts an affected file whole the next time it is touched. Raise this on monorepos with legitimate large commits. |
| `--chunk-size` | `int` | planned from the budget | Commits per history chunk: worker threads prepare the per-commit work (diffs, line stats, parses) of at most this many commits ahead of the walk, which folds each commit into its report in commit order and frees it (must be `>= 1`). Overrides the chunk size `--memory-budget` plans; the walk then runs one worker per core. The output is byte-identical for every chunk size. |
| `--memory-budget` | `size` | half the RAM, at most 2 GiB | Memory a history run plans against, e.g. `512MB`, `2GB`, `1.5GiB` (minimum `256MiB`). The history walks running at once share it; each walk sizes its chunk and its worker count from its share and its measured per-worker and per-commit cost, and libgit2 gets a fixed part of it (25 % for its object cache, 7.5 % for mapped pack files). Free heap pages go back to the OS after every chunk. `-v` prints each walk's plan. The output never depends on the budget. |

These flags bound the **history** phase only — static analyzers read the
working tree, so none of them affects a static run.

The window is selected **newest-first** (`--limit 10` = the ten most recent
commits, like `git log -n 10`) and then handed to the analyzers in
chronological oldest-first order, because ticks, burndown line survival and
author identity all depend on that order. `--limit` and `--since` compose:
`--since` picks the window, `--limit` caps its newest end. `--head` is the
single-commit window `[HEAD]` — the same window `--limit 1` selects, so the two
produce byte-identical reports for every analyzer and format.

The `--since` flag accepts these forms, in precedence order:

- **Duration**: `24h`, `168h` (one week), `1h30m` — units are `h`/`m`/`s`
  only. There is **no** `d` or `w` unit and `m` means **minutes**, so a week
  is `168h`, not `7d`
- **Date**: `2025-01-01` (midnight UTC)
- **RFC 3339**: `2025-01-01T00:00:00Z`
- **Git revision**: `v1.2.0`, `HEAD~100`, or a bare SHA — that commit's author
  time becomes the cutoff

The cutoff compares the commit's **author** time, and every commit authored
at/after it is kept regardless of its position in the committer-time walk
(a commit authored after the cutoff but committed before it is still
included; one authored before the cutoff but committed after it is not).

```bash
# Everything authored in the last 168h (one week)
codefang run -a history/devs --since 168h .

# Since a specific date
codefang run -a history/burndown --since 2025-01-01 .

# Since a release tag
codefang run -a history/couples --since v1.2.0 .

# Only the latest 500 commits
codefang run -a history/couples --limit 500 .

# Hourly ticks
codefang run -a history/devs --tick-size 1 --format timeseries .
```

!!! note "Empty windows and bad values"
    A `--since` cutoff newer than every commit analyzes nothing: the run
    succeeds (exit 0) and every history analyzer emits its empty report. A
    cutoff at or before the root commit filters nothing. A value that is
    neither a time form nor a resolvable revision fails immediately with
    `Error: cannot resolve --since "<value>"`, empty stdout, exit 1.

!!! note "Burndown always walks first-parent"
    The burndown analyzer tracks line survival along the main line of
    development, so **its own** walk always follows the first parent of merge
    commits, whatever `--first-parent` says. This never leaks into the other
    analyzers: selecting burndown next to devs, couples or file-history leaves
    their walks exactly as the user's `--first-parent` flag configures them.

!!! note "Renames"
    The tree diff detects renames (libgit2 `find_similar`, the `git diff -M`
    default of 50 % similarity). A pure `git mv` is one change whose old and
    new paths differ and whose blob is unchanged, so devs reports 0 added /
    0 removed lines for it and burndown, couples, file-history and shotness
    keep the file's identity across the move.

!!! note "Deterministic output"
    A history report is a pure function of the repository and the flags. It
    does not depend on the host's CPU count, the wall clock, or the number of
    threads the walk happens to use.

#### Language filtering

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--languages` | `[]string` | `[all]` | Restrict analysis to the given Linguist languages; comma-separated. `all` (default), an absent flag and an explicit empty value (`--languages ""`) all disable the filter. Applies to **both** history and static phases with one meaning. In the static phase each language resolves to its Linguist filename globs, matched against the file's **basename** (`*.go`, `*.py`, `Dockerfile`, …). |

**History phase** — each changed file is classified with the full enry
cascade over its basename **and** its blob content, so ambiguous extensions
(`.h`, `.m`, `.pl`, `.r`) and extensionless scripts (shebang) resolve
correctly; changes whose language is not in the set are dropped before any
analyzer sees them.

**Static phase** — the filter is applied by the ONE directory walk every
static analyzer shares, before the UAST parser or raw-file analyzers see the
file: the language globs are matched against the basename, so the parser's
own language router remains the final authority for how a matched file is
parsed. Both phases read the same `langpath` tables, so the flag value has
one meaning across `-a 'static/*'`, `-a 'history/*'`, and `-a '*'` runs.

#### The static walk

Every `static/*` analyzer reads the same walk of the path you pass
(a directory or a single file):

- directories are visited in lexical order, depth-first; `.git` and any
  directory carrying a `CACHEDIR.TAG` (Cargo's `target/`) are skipped;
  symlinked directories are never followed (a symlink loop cannot recurse
  and the walk cannot leave the root); symlinks to regular files are read;
- a file is admitted when the UAST parser supports its extension, its
  basename matches `--languages`, and its **root-relative** path passes the
  vendor / generated / `--extra-excluded-prefixes` policy;
- each admitted file is read **once**; a file larger than **64 MiB** or one
  that cannot be read is skipped with one stderr line
  (`warning: skipped <path>: larger than 64 MiB` / `: unreadable`) for the
  whole run, however many analyzers are selected, and counted once in the
  end-of-run `progress: skipped N unreadable/oversized/unparseable files`
  line;
- the generated-content marker (`DO NOT EDIT`, `Code generated`,
  `@generated`, … in the first 512 bytes) is applied to the bytes just read
  unless `--include-generated`;
- a single-file path analyzes that one file and stamps its **name**
  (`a.go`, never `.`) on every per-file entry and issue location.

`static/composition` is the designed exception: it exists to report
vendored / generated / binary shares, so it classifies **every** regular
file the walk reaches (reading only the first 8 KiB of each); only
`--languages` and `--extra-excluded-prefixes` narrow it.

Language names are [Linguist keys](https://github.com/github/linguist/blob/master/lib/linguist/languages.yml)
and common aliases resolve automatically:

```bash
# Canonical names (any case, whitespace is trimmed)
codefang run -a 'history/devs' --languages go,python,typescript .

# Aliases resolve via the Linguist tables
codefang run -a 'history/devs' --languages golang,js,ts .

# Unknown language fails fast instead of silently returning an empty report:
codefang run -a 'history/devs' --languages notalang .
# → Error: history --languages: unknown language: "notalang"
codefang run -a 'static/complexity' --languages notalang .
# → Error: static --languages: unknown language: "notalang"
```

Filename-only languages (e.g. `Dockerfile`, `Makefile`) are also supported:

```bash
codefang run -a 'history/devs' --languages dockerfile .
```

#### Vendor & generated exclusion

By default, Codefang excludes **vendored dependencies** and **auto-generated
files** from analysis. This matches the convention of every major
single-language analyser (`go vet`, `eslint`, `ruff`, `rubocop`, `scalafix`,
`phpcs`, …) — vendor/generated code is noise for a code-quality report.

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--include-vendored` | `bool` | `false` | Re-include vendored dependencies (detected by the Linguist vendor patterns over the **root-relative** path) in analysis. Cross-language: covers `vendor/`, `node_modules/`, `third_party/`, `testdata/`, minified bundles, and more. `static/composition` counts vendored files regardless. |
| `--include-generated` | `bool` | `false` | Re-include auto-generated files in analysis. Covers `*.pb.go`, `zz_generated_*.go`, `*_pb2.py`, `*.min.js` (path heuristics) **and** any file whose first 512 bytes contain a generated-file marker (`DO NOT EDIT`, `Code generated`, `@generated`, …) — the static walk reads every admitted file and applies the marker to it; the history filter applies it to the blob header. `static/composition` counts generated files regardless. |
| `--extra-excluded-prefixes` | `[]string` | `[]` | Additional UNIX path prefixes to exclude on top of the built-in heuristics (e.g. `".venv/,target/,build/"`), matched against the **root-relative** path (`gen/` excludes `<root>/gen/…` whatever the root spelling). Always excludes, even with the include flags set, and narrows `static/composition` too. |

All three flags apply **identically** to both static and history phases: the
history change filter reads the blob header for the generated-file marker
exactly as the static walk reads the bytes of each admitted file.

```bash
# default: your own code only
codefang run -a '*' .

# include vendored deps (node_modules/, vendor/, …)
codefang run -a '*' --include-vendored .

# include everything
codefang run -a '*' --include-vendored --include-generated .

# skip extras the heuristics do not know about (e.g. Python venv, Rust target/)
codefang run -a '*' --extra-excluded-prefixes '.venv/,target/' .
```

!!! warning "Removed spellings"
    The hidden `--skip-blacklist` and `--blacklisted-prefixes` flags were
    accepted and ignored; they are removed and fail as unknown arguments.
    Drop `--skip-blacklist` (the default already excludes) and write
    `--blacklisted-prefixes X,Y` as `--extra-excluded-prefixes X,Y`.

#### Developer identity

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--people-dict` | `path` | `""` | File with one person per line, `|`-separated identity tokens (names and emails, matched case-insensitively). With a dictionary, an author matching no line is reported as `<unmatched>`. An unreadable file is an error. |
| `--exact-signatures` | `bool` | `false` | Match the whole `name <email>` signature instead of merging authors that share a name **or** an email. Yields considerably more identities. |
| `--anonymize` | `bool` | `false` | Replace developer names in every report with `Developer-A`, `Developer-B`, … |

By default an author is identified by **either** the lowercased email or the
lowercased name, and a hit on one token registers the other: `alice <a@x>`,
`Alice Smith <a@x>` and `Alice Smith <a@y>` are one developer. One identity
detector serves every history analyzer, so devs, burndown, couples,
file-history and anomaly agree on author ids.

```
# people.txt
Alice|alice@example.com|alice@other.org
Bob|bob@example.com
```

```bash
codefang run -a history/devs --people-dict people.txt .
codefang run -a history/devs --anonymize --format text .
```

#### Analyzer options

| Flag | Type | Default | Applies to | Description |
|------|------|---------|------------|-------------|
| `--typos-max-distance` | `int` | `4` | `history/typos` | Maximum Levenshtein distance between the two identifiers of a typo-fix pair (also bounds the whole-line prefilter); must be `> 0` |
| `--granularity` | `int` | `30` | `history/burndown` | Ticks per age band of the survival matrix (must be `> 0`; `Error: invalid --granularity: must be > 0, got <v>`) |
| `--sampling` | `int` | `30` | `history/burndown` | Ticks per sample row of the survival matrix (must be `> 0`; independent of `--granularity`) |
| `--burndown-files` | `bool` | `false` | `history/burndown` | Per-file survival (`file_survival`, `tracked_files`, ndjson `FileDeltas`); with `--burndown-people` also per-file ownership |
| `--burndown-people` | `bool` | `false` | `history/burndown` | Per-developer survival and the interaction matrix (`developer_survival`, `interactions`, `tracked_developers`, ndjson `PeopleDeltas` / `MatrixDeltas`, the text "Top Developers" section) |
| `--anomaly-window` | `int` | `20` | `history/anomaly` | Trailing window size in ticks for the rolling mean and standard deviation (must be `>= 2`; a shorter history uses every earlier tick) |
| `--anomaly-threshold` | `float` | `2.0` | `history/anomaly` | Z-score threshold, in standard deviations, above which a tick is flagged (must be `> 0`) |
| `--empty-commits` | `bool` | `false` | `history/devs` | Count commits whose filtered diff is empty (trivial merges, `--allow-empty` commits, commits touching only excluded paths) as commits with zero lines |
| `--shotness-dsl-struct` | `string` | `rfilter(.roles has "Function")` | `history/shotness` | UAST DSL query selecting the tracked declarations anywhere in each file (`filter(...)` would see the top level only); validated before the walk, an invalid expression is a flag error |
| `--shotness-dsl-name` | `string` | `.props.name` | `history/shotness` | UAST DSL query extracting a declaration's own name (applied to selected nodes and their enclosing declarations); validated before the walk |
| `--shotness-keep-ended` | `bool` | `false` | `history/shotness` | Keep entities whose declaration or file was removed, marked `"ended": true` with their counts and couplings (adds `ended_nodes`); hotspots and plot pages still show only live entities. Grows the report on long histories |
| `--min-comment-len` | `int` | `20` | `history/sentiment` | Minimum comment length in characters to analyze; `0` disables the length filter, a negative value is rejected (`Error: invalid --min-comment-len: …`) |

#### Configuration

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--config` | `string` | `""` | Configuration file. Without it `./.codefang.yaml`, then `$HOME/.codefang.yaml` is read when present. An explicit file that cannot be read is an error. |

Every key of the file — and its `CODEFANG_*` environment variable — sets
the **default** of one flag; the precedence is explicit flag > environment
> config file > compiled default. `--config /dev/null` runs with the
compiled defaults only. Keys nothing reads are ignored with one
`warning: ignoring unsupported config keys: …` line. See
[Configuration](configuration.md) for the key → flag table.

```bash
codefang run -a history/anomaly --config ./ci.codefang.yaml .
```

No checkpoint store, incremental cache or trace exporter is linked into the
binary, so the Go-era `--checkpoint`, `--checkpoint-dir`, `--resume`,
`--clear-checkpoint`, `--cache-dir`, `--no-cache` and `--debug-trace` flags
are removed; passing one fails as an unknown argument. A run always analyzes
the whole requested window — bound it with `--limit` / `--since`.

#### Errors

Every user error prints `Error: <message>` to stderr, leaves stdout empty and
exits 1:

| Situation | Message |
|-----------|---------|
| the path does not exist | `path not found: <path>` |
| a history analyzer on a directory that is not a git repository | `cannot open repository: <path>` |
| an unknown `--format` | `unsupported format: <value>` |
| a known format the selection cannot render (e.g. static-only `ndjson`, mixed `compact`) | `format <value> is not supported for <analyzer>` |
| an unknown analyzer id | `unknown analyzer id: <id>` |
| `--limit`, `--tick-size`, `--max-changes-per-commit`, `--chunk-size`, `--granularity`, `--sampling`, `--typos-max-distance`, `--anomaly-window`, `--anomaly-threshold`, `--min-comment-len` out of range | `invalid --<flag>: must be … , got <value>` |
| a `--memory-budget` that is not a size, or below `256MiB` | `invalid --memory-budget: "<value>" is not a size (examples: 512MB, 2GB, 1.5GiB)` / `… is below the minimum 256 MiB` |
| `--ndjson` without `--format timeseries` | `invalid --ndjson: requires --format timeseries, got --format <f>` |
| `--verbose` with `--quiet` | `--verbose and --quiet cannot be used together` |
| an unreadable `--config` file | `cannot read --config <path>: …` |
| a malformed config file or an out-of-range key | `invalid config <path>: <key> must be …, got <value>` (or the parser's message naming the key and line) |
| a malformed `CODEFANG_*` variable | `invalid environment variable <NAME>: …` |
| `-o` cannot be created | `cannot write --output <path>: …` |
| an unresolvable `--since` | `cannot resolve --since "<value>": …` |
| an unknown `--languages` token | `static --languages: unknown language: "<token>"` / `history --languages: …` |
| an unreadable `--people-dict` | `cannot read --people-dict <path>: …` |
| an analyzer produced nothing for an otherwise valid run | `no report produced for <analyzer> --format <format>` |

A history `--format text` selection that includes an analyzer without a text
renderer writes the header and the section name first, then fails with
`serialization error for <Name>: unsupported format: text`.

### `codefang version`

Prints `codefang <version> (commit: <hash>, built: <date>)`:

- `<version>` — the crate version (or `CF_VERSION` / `GIT_VERSION` set at
  build time);
- `<hash>` — the commit the binary was built from: `CF_COMMIT` /
  `GIT_COMMIT` / `GIT_HASH` set at build time, else the `HEAD` of the
  checkout it was built in, else `unknown`;
- `<date>` — `CF_DATE` / `SOURCE_DATE_EPOCH` set at build time, else
  `unknown` (the build never reads the wall clock).

```bash
codefang version
```

The former `codefang render <store-dir>` subcommand only ever printed
"not available" — there is no stored-report format to render from — and is
removed; `codefang run --format plot -o <dir>` writes the HTML pages
directly.

### `codefang completion <shell>`

Generates the shell-completion script for `bash`, `fish`, `powershell` or
`zsh` on stdout, from the live command tree (every flag on this page).
`--no-descriptions` leaves the help texts out of the script.

```bash
codefang completion bash > ~/.local/share/bash-completion/completions/codefang
```

---

### `codefang mcp`

Serves the Model Context Protocol over stdio (newline-delimited JSON-RPC) until
the client closes stdin. It takes no flags and exposes three tools AI agents
discover with `tools/list` and invoke with `tools/call`:

| Tool | Description |
|------|-------------|
| `codefang_analyze` | Static code analysis (complexity, cohesion, halstead, comments, imports) |
| `codefang_history` | Git history analysis (burndown, couples, devs, sentiment, etc.) |
| `uast_parse` | Parse source code into Universal AST |

Each tool result is the report the CLI renders for the same input. See the
[MCP Integration](../integrations/mcp.md) guide for the tool reference.

---

## `uast`

The UAST binary parses source files into a Universal Abstract Syntax Tree and
provides querying, diffing, and exploration utilities. The root command has
no flags of its own; every flag below belongs to its subcommand and is read by
it (`uast <command> --help` lists the same set).

### `uast parse`

Parse one or more source files into UAST format.

```bash
uast parse [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--language` | `-l` | `string` | `""` | Force language detection override |
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `json` | Output format: `json` (pretty), `compact` (one line per input), `none` (parse only) |
| `--all` | | `bool` | `false` | Parse all source files in the codebase recursively |

With no files (and no `--all`), or for a `-` file argument, the source is read
from stdin and parsed as Go unless `-l` names the language. With `-o`, every
input's document is written to the one file, in input order.

```bash
# Parse a single file
uast parse main.go

# Parse all Go files, one compact document per line
uast parse -f compact *.go

# Force language detection
uast parse -l go main.c

# Read from stdin
cat main.go | uast parse -

# Save to file
uast parse -o output.json main.go

# Parse entire codebase
uast parse --all
```

---

### `uast query`

Query UAST nodes using the functional DSL.

```bash
uast query <expression> [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--input` | `-i` | `string` | `""` | Input file (UAST JSON or source code), queried before any positional files |
| `--output` | `-o` | `string` | `""` | Output file (default: stdout); every input's result is written to it, in input order |
| `--format` | `-f` | `string` | `json` | Output format: `json`, `compact`, `count` |
| `--interactive` | `-t` | `bool` | `false` | Interactive query mode over one file (`--input`, else the single positional file, else source read from stdin) |

With neither files nor `--input`, or for a `-` file argument, the query runs
over a UAST JSON tree read from stdin.

```bash
# Find the top-level functions (filter looks at the file's direct children)
uast query "filter(.type == 'Function')" main.go

# Find every function-role node anywhere in the tree, methods included
uast query 'rfilter(.roles has "Function")' main.go

# The name of every declaration in the tree
uast query "rmap(.props.name)" main.go

# Find exported items
uast query "filter(.roles has 'Exported')" *.go

# Count all nodes
uast query "reduce(count)" main.go

# Query from piped UAST JSON
uast parse main.go | uast query "filter(.type == 'Call')" -

# The same query over an --input file
uast query "filter(.type == 'Call')" -i main.go

# Count format
uast query -f count "filter(.type == 'Function')" main.go

# Interactive mode
uast query -t main.go
```

??? info "DSL Syntax Reference"

    A query is a pipeline of stages joined by `|`. `filter` and `map` look
    at their input set only — a `filter(...)` at the start of a query runs
    over the root's **direct children** — while `rfilter` and `rmap` apply
    the same predicate / expression to every node of each input's subtree in
    pre-order (the input itself first).

    | Expression | Description |
    |-----------|-------------|
    | `filter(.type == "Function")` | Direct children of the root whose type is `Function` |
    | `rfilter(.type == "Call")` | Every function call anywhere in the tree |
    | `rfilter(.type == "Identifier")` | Every identifier |
    | `rfilter(.roles has "Function")` | Every function-role node (functions and methods) |
    | `filter(.roles has "Exported")` | Direct children carrying a role |
    | `rfilter(.type == "Class") \| map(.props.name)` | The names of the classes |
    | `rmap(.props.name)` | The `name` property of every node that has one |
    | `map(.children)` | The children of each input node |
    | `reduce(count)` | Count the matching nodes |

    Field access resolves `children`, `token`, `id`, `roles`, `type`,
    `props`, `first`, `last`; any other name reads that property
    (`.props.name` ≡ `.name`). Comparisons: `==`, `!=`, `<`, `<=`, `>`,
    `>=`, `has` (membership in `roles`). Builtins: `contains`,
    `startsWith`, `endsWith`, `has`, `count`, `first`, `last`, `sort`.

---

### `uast diff`

Compare two files at the UAST structural level and report changes.

```bash
uast diff <file1> <file2> [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `unified` | Output format: `unified`, `summary`, `json` |

```bash
# Unified diff output (default)
uast diff old.go new.go

# Summary of change types
uast diff -f summary old.go new.go

# Machine-readable JSON
uast diff -f json old.go new.go

# Save to file
uast diff -o changes.json -f json old.go new.go
```

---

### `uast explore`

Start an interactive session for exploring the UAST structure of a file.

```bash
uast explore [file] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--language` | `-l` | `string` | `""` | Force language detection override |

```bash
uast explore main.go
```

Available interactive commands:

| Command | Description |
|---------|-------------|
| `tree` | Show AST tree structure |
| `stats` | Show node type statistics |
| `find <type>` | Find nodes by type |
| `query <dsl>` | Execute a DSL query |
| `help` | Show available commands |
| `quit` | Exit exploration |

---

### `uast analyze`

Quick analysis shortcut that parses and analyzes source files in one step.

```bash
uast analyze [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `text` | Output format: `text`, `json`, `html` |

```bash
# Analyze a single file
uast analyze main.go

# Analyze all Go files as JSON
uast analyze -f json *.go

# Generate an HTML report
uast analyze -o report.html -f html *.go
```

---

### `uast validate`

Validate a UAST JSON document against the UAST JSON Schema. Exit codes: `0`
valid, `1` the document violates the schema, `2` an error (unreadable input,
malformed JSON, unreadable schema).

```bash
uast validate <file.json|-> [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--schema` | | `string` | `""` | Path to a UAST JSON schema; empty validates against the schema embedded in the binary |

```bash
# Validate a parsed tree against the embedded schema
uast parse -o tree.json main.go
uast validate tree.json
```

---

### `uast mapping`

Grammar helpers for UAST mapping authors: summarize and classify a
tree-sitter `node-types.json`, generate a `.uastmap` DSL skeleton from it, or
dump the raw tree-sitter tree of source files.

```bash
uast mapping [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--node-types` | | `string` | `""` | Path to `node-types.json` (required unless `--show-treesitter`) |
| `--format` | | `string` | `text` | Output format of the node-types summary: `text`, `json` |
| `--generate` | | `bool` | `false` | Print a `.uastmap` DSL skeleton generated from `node-types.json` |
| `--show-treesitter` | | `bool` | `false` | Print the raw tree-sitter tree (named nodes, positions, byte ranges, text) of each input file |
| `--language` | | `string` | `""` | Tree-sitter language name (e.g. `go`): required by `--show-treesitter`, written into the `--generate` header |
| `--extensions` | | `string` | `""` | Comma-separated file extensions written into the `--generate` header |

```bash
# Classify a grammar's node types
uast mapping --node-types node-types.json --format json

# Generate a mapping skeleton
uast mapping --node-types node-types.json --generate --language go --extensions .go

# Dump the tree-sitter tree of a file
uast mapping --show-treesitter --language go main.go
```

---

### `uast completion <shell>`

Generates the completion script for `bash`, `zsh`, `fish` or `powershell` on
stdout, covering every subcommand and flag listed on this page.

### `uast version`

Prints `uast <version> (commit: <hash>, built: <date>)`.
