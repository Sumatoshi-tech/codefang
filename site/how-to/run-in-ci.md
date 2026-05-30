# How to run Codefang in CI

<!-- Template: Good Docs Project how-to (CC-BY 4.0) — https://github.com/thegooddocsproject/templates -->

## Goal

Add Codefang to a continuous-integration pipeline so every push and pull request
runs code analysis automatically, and optionally fail the build when analysis
detects issues.

## Prerequisites

- A repository hosted on GitHub (this guide uses the published GitHub Action).
- Permission to add workflow files under `.github/workflows/`.
- For history analyzers, the checkout step must fetch full history
  (`fetch-depth: 0`); a shallow clone only sees the most recent commits.

## Steps

1. Create `.github/workflows/codefang.yml` and add the action with the
   analyzers you want. Static analysis works on a shallow clone:

   ```yaml
   name: Code Quality
   on: [push, pull_request]

   jobs:
     analyze:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Run Codefang
           id: codefang
           uses: Sumatoshi-tech/codefang@main
           with:
             analyzers: "static/*"
             format: "json"
   ```

2. Turn the run into a quality gate by setting `fail-on-error: "true"`. The step
   fails the workflow when analysis detects issues:

   ```yaml
         - name: Run Codefang
           uses: Sumatoshi-tech/codefang@main
           with:
             analyzers: "static/complexity,static/comments"
             fail-on-error: "true"
   ```

3. For history analyzers, fetch the full Git history in the checkout step,
   otherwise the analysis only sees the shallow clone:

   ```yaml
         - uses: actions/checkout@v4
           with:
             fetch-depth: 0

         - name: Run Codefang History
           uses: Sumatoshi-tech/codefang@main
           with:
             analyzers: "history/burndown,history/devs"
             format: "json"
   ```

4. Read the action outputs in a later step. `pass` is `true` when analysis
   completed without errors, and `report` holds the full report content:

   ```yaml
         - name: Check results
           run: |
             echo "Pass: ${{ steps.codefang.outputs.pass }}"
             echo "${{ steps.codefang.outputs.report }}"
   ```

5. If a large repository runs out of memory in CI, constrain it with a memory
   budget by running the binary directly inside the job, or in Docker:

   ```bash
   docker run --rm -v "$(pwd):/workspace:ro" \
     codefang run -a 'history/*' --memory-budget 2GiB --format json --silent /workspace
   ```

## Result

Open the Actions tab after a push. The Codefang job runs, prints the report, and
— when `fail-on-error` is set — marks the check red if analysis detected issues.
A green check with a populated `report` output confirms the pipeline works.

## See also

- [Docker & GitHub Actions reference](../integrations/docker-and-actions.md)
- [How to analyze a monorepo](analyze-a-monorepo.md)
- [Output formats](../guide/output-formats.md)
