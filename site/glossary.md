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

## Blob cache

A core plumbing analyzer (`BlobCacheAnalyzer`) that caches blob content so the
pipeline can re-read the same file content without fetching it from Git again.
It wraps an LRU cache.

## Blob pipeline

One of the three parallel stages the Coordinator runs. The blob pipeline loads
blob content from Git for the commits in a chunk.

## Burndown

A history analysis that tracks code age and survival: how many lines written in
a given period are still present in the codebase over time. The result is often
shown as a burndown chart.

## Checkpoint

A serialized snapshot of analyzer state written after each streaming chunk so an
interrupted run can recover instead of restarting from the first commit.

## Chunk

A memory-bounded batch of commits. The streaming pipeline splits a large commit
history into chunks so memory stays within budget.

## Cohesion

A static metric measuring how strongly the members of a class or module belong
together. Codefang reports cohesion using metrics such as LCOM.

## Coordinator

The component that orchestrates a worker pool across the three pipeline stages
(blob, diff, and UAST) for each chunk of commits.

## Couples

A history analysis that detects co-change coupling: files that tend to change
together in the same commits. Useful for risk-aware refactoring.

## Diff pipeline

One of the three parallel stages the Coordinator runs. The diff pipeline computes
per-commit tree and file diffs.

## Halstead

A family of static software-science metrics (volume, difficulty, effort, and
related measures) derived from the operators and operands in code.

## Hibernate/boot cycle

The state-management cycle of the streaming pipeline. Between chunks a
hibernatable analyzer serializes (hibernates) its state to a compact form, then
reboots to resume. This keeps memory bounded across a long history walk.

## History mode

The analysis mode that walks a Git repository's commit history and runs history
analyzers. Contrast with static mode, which reads source files from disk.

## Identity detection

A core plumbing step (`IdentityDetector`) that maps commit authors to canonical
identities so a single developer with multiple names or emails counts as one
person.

## Leaf analyzer

A history analyzer that consumes the output of the core plumbing analyzers — for
example burndown, couples, or devs. Leaf analyzers run after plumbing in the
pipeline.

## Pipeline stage

One of the three parallel work types the Coordinator dispatches per chunk: blob
loading, diff computation, and UAST parsing.

## Plumbing

The shared core analyzers that all history analyzers depend on (tree diff, blob
cache, identity detection, tick assignment, line stats, language detection, UAST
changes). Plumbing runs before any leaf analyzer.

## Runner

The component that coordinates the full history-analysis lifecycle: initialize,
process each chunk, then finalize with aggregators. It supports single-pass and
streaming execution strategies.

## Sentiment

A history analysis that scores the sentiment of code comments over time.

## Shotness

A history analysis that measures function-level change frequency — which
functions are touched most often (structural hotness).

## Static mode

The analysis mode that reads source files from disk, parses them into UASTs, and
runs static analyzers. It requires no Git history.

## Streaming pipeline

The execution strategy that processes a large history in memory-bounded chunks
with hibernate/boot cycles and optional double-buffered pipelining, planned by
the streaming planner.

## Tick

A discrete time index assigned to commits so analyses can build time series. The
`TicksSinceStart` plumbing analyzer assigns a tick to each commit relative to the
start of the history.

## Typos

A history analysis that detects likely identifier typos in diffs.

## UAST (Universal Abstract Syntax Tree)

A language-neutral abstract syntax tree. Codefang parses 60+ languages into a
single UAST shape via Tree-sitter, so one analyzer (for example complexity) works
across every supported language.
