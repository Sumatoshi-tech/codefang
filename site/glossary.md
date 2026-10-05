---
title: Glossary
description: Definitions of the project-specific terms Codefang introduces, for human readers and as a spell-check dictionary seed.
---

<!-- Template: Good Docs Project glossary (https://www.thegooddocsproject.dev/template/glossary), CC-BY 4.0. -->

# Glossary

This glossary defines the terms Codefang introduces or uses in a specific way.
Terms are listed alphabetically.

## Abstract syntax tree (AST)

A tree representation of source code that captures its structure rather than its
raw text. Codefang analyzes the AST so it can reason about meaning, not just
characters.

## Analyzer

A pluggable unit of analysis. Each analyzer carries an ID (for example
`static/complexity` or `history/burndown`) and produces metrics. Static analyzers
consume parsed code; history analyzers consume commit history.

## Bare repository

A Git repository without a working tree. Codefang's history mode opens both
normal and bare repositories via libgit2.

## Blob

The raw content of a file at a specific Git object hash. History analysis reads
blobs to compute diffs and parse code.

## Burndown

A history analysis that tracks code age and survival: how many lines written in
a given period are still present in the codebase over time. The result is often
shown as a burndown chart.

## Change filter

The per-run rule that decides which changed files a history analyzer sees:
`--languages`, the vendored and generated path policy (`--include-vendored`,
`--include-generated`, `--extra-excluded-prefixes`), the generated-content
marker in the first 512 bytes, and `--max-changes-per-commit`.

## Chronological reduce

The single-threaded, oldest-first fold of every commit's product into an
analyzer's report. Doing it in one fixed order is what makes history output
byte-identical across runs and machines.

## Cohesion

A static metric measuring how strongly the functions of a file belong together,
computed as LCOM over the variables each function reads or writes. Codefang
reports `1 − LCOM` as the cohesion score.

## Commit window

The commits a history run analyzes: the newest `--limit` commits, or every
commit authored since `--since`, or just `HEAD` with `--head`, delivered to the
analyzers oldest-first.

## Couples

A history analysis that detects co-change coupling: files that tend to change
together in the same commits. Useful for risk-aware refactoring.

## Halstead

A family of static software-science metrics (volume, difficulty, effort, and
related measures) derived from the operators and operands in code.

## History mode

The analysis mode that walks a Git repository's commit history and runs history
analyzers. Contrast with static mode, which reads source files from disk.

## Identity detection

The step (`IdentityDetector`) that maps commit authors to canonical developer
ids, so one developer with several names or emails counts as one person. By
default a known email or name unifies the two; `--exact-signatures` and
`--people-dict` change the rule.

## Per-commit product

The order-independent work for one commit (tree diff with rename detection, line
diffs, blob reads, UAST parses) computed in parallel before the chronological
reduce. Its result depends only on that commit.

## Sentiment

A history analysis that scores the sentiment of code comments over time.

## Shotness

A history analysis that measures function-level change frequency — which
functions are touched most often (structural hotness).

## Static mode

The analysis mode that reads source files from disk, parses them into UASTs, and
runs static analyzers. It requires no Git history.

## Tick

A discrete time index assigned to commits so analyses can build time series.
The first commit's committer time, floored to the tick size (`--tick-size`,
default 24 hours), is tick 0; ticks never go backwards.

## Typos

A history analysis that detects likely identifier typos in diffs.

## UAST (Universal Abstract Syntax Tree)

A language-neutral abstract syntax tree. Codefang parses 60+ languages into a
single UAST shape via Tree-sitter, so one analyzer (for example complexity) works
across every supported language.
