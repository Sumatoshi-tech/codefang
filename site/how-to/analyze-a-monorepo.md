# How to analyze a monorepo

<!-- Template: Good Docs Project how-to (CC-BY 4.0) — https://github.com/thegooddocsproject/templates -->

## Goal

Run Codefang against a single large repository that holds many languages and
services, and get a clean per-service report without drowning in vendored or
generated noise.

## Prerequisites

- `codefang` and `uast` installed. See [Installation](../getting-started/installation.md).
- A monorepo checked out locally, with full Git history (history analyzers need
  every commit, not a shallow clone).
- `jq` if you want to post-process the JSON output.

## Steps

1. Run static analysis across the whole tree first. By default Codefang already
   excludes vendored dependencies (`vendor/`, `node_modules/`, `third_party/`,
   `testdata/`) and generated files, so the report covers your own code only:

   ```bash
   codefang run -a 'static/*' --format json --silent .
   ```

2. Narrow a noisy polyglot repo to the languages you care about. The
   `--languages` filter applies to both the static and history phases and skips
   non-matching files before they are parsed:

   ```bash
   codefang run -a 'static/*' --languages go,typescript,python --format json .
   ```

3. Scope a run to a single service directory instead of the whole repo by
   passing its path (or `--path`):

   ```bash
   codefang run -a 'static/complexity,static/cohesion' --format json ./services/api
   ```

4. Add extra exclusion prefixes for tooling directories that Linguist's
   heuristics do not catch, such as a Python virtualenv or a Rust target dir:

   ```bash
   codefang run -a '*' --extra-excluded-prefixes '.venv/,target/,build/' --format json .
   ```

5. Bound the history window on a large monorepo — there is no memory or
   worker knob; a history run costs what its commit window costs. Limit the
   range with `--since` or `--limit`, skip merge side branches with
   `--first-parent`, and drop mass-change commits (vendoring dumps,
   generated-code regenerations) with `--max-changes-per-commit`:

   ```bash
   codefang run -a 'history/*' --limit 5000 --first-parent --format json --silent .
   ```

6. If you need only a period of the history, select it with `--since`:

   ```bash
   codefang run -a 'history/devs' --since 2025-01-01 --format json .
   ```

## Result

You have a JSON report scoped to your own source code (vendored and generated
files excluded) and, when you narrow by `--languages` or path, scoped to the
service you care about. Confirm the scope by inspecting the report — for
example, count the files Codefang actually analyzed:

```bash
codefang run -a 'static/complexity' --languages go --format json . | jq '.["static/complexity"] | length'
```

## See also

- [How to run Codefang in CI](run-in-ci.md)
- [Large-scale repository scanning](../operations/large-scale-scanning.md)
- [CLI reference](../guide/cli-reference.md)
