---
title: Observability
description: What a codefang run reports on stderr — progress lines, warnings, named errors, exit codes — and the --profile memory watchdog.
---

# Observability

A `codefang run` keeps its two output channels separate: the **report** goes
to stdout (or to the file named by `-o`), and every **diagnostic** goes to
stderr. A fleet orchestrator can therefore pipe stdout straight into storage
and collect stderr as the run log.

The Rust binary exports no telemetry: there is no OpenTelemetry exporter, no
metrics endpoint and no trace sampling flag. Everything a run says about
itself is in the stderr lines and the exit code described below.

---

## Stderr lines

| Line | When | Suppressed by `-q` |
|------|------|--------------------|
| `progress: resolved analyzers: static=<n> history=<n> output_format=<f>` | at start | yes |
| `progress: config file: <path>` / `progress: config file: none` | at start, with `-v` | yes |
| `progress: analyzers: <id>,<id>,…` | at start, with `-v` | yes |
| `progress: skipped <n> unreadable/oversized/unparseable files` | at the end, when files were skipped | yes |
| `progress: run finished in <ms>ms` / `in <s>s` | at the end | yes |
| `warning: skipped <path>: <reason>` | once per skipped file | no |
| `warning: ignoring unsupported config keys: <key>, …` | once, when the config file carries keys codefang does not read | no |
| `--per-file applies to --format json only; ignored for <format>` | once, `--per-file` with another format | no |
| `Error: <cause>` | on failure | no |

`-q` / `--quiet` (alias `--silent`) removes the `progress:` lines;
`-v` / `--verbose` adds the config-file and resolved-analyzer lines.

```bash
# Report to a file, progress on the terminal
codefang run -a 'history/*' --limit 500 -o report.json .

# Only warnings and errors on stderr
codefang run -a 'static/*' -q --format json .

# Show which config file and which analyzers the run resolved
codefang run -a 'history/*' --limit 50 -v .
```

---

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | The report was written |
| `1` | A user error (`Error: <cause>` names it: path not found, not a git repository, unsupported format, invalid flag value, unreadable `--config`, …) or a command-line usage error |
| `130` | Interrupted by SIGINT / SIGTERM — no report is written (`Error: interrupted`); a second signal terminates immediately |
| `141` | The downstream pipe closed (`codefang run … \| head`) |

The full list of error messages is in the
[CLI reference](../guide/cli-reference.md#errors).

---

## Memory watchdog (`--profile`)

`--profile` (a global flag, valid with every command) starts a background
watchdog that writes one line to stderr at start and every 2 seconds:

```text
MEM t=0 RSS=<MiB> threads=<n>
MEM t=2 RSS=<MiB> threads=<n>
```

It also copies `/proc/self/maps` to `maps_baseline.txt` in the temp directory
(`$TMPDIR`, else `/tmp`) at start and, the first time resident memory exceeds
4096 MiB, to `maps_spike_<rss>MiB.txt` there, logging the smaps rollup of each spike
(`SPIKE #<n>: RSS=<MiB> MiB smaps: …`, at most five). The watchdog reads
Linux `/proc`; elsewhere it logs zeros. It never changes the report.

```bash
codefang --profile run -a 'history/*' --limit 500 .
```

For a whole-run peak, `/usr/bin/time -v` reports the maximum resident set
size; measured on the hercules repository, `-a 'history/*' --limit 500`
peaks at 752 MB in 4.7 s wall time.
