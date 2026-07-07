---
name: march
description: Roadmap orchestrator that drives unchecked items to done via a per-item driver skill (/implement for code, /documenter for docs, direct edit for plain Markdown)
---

# Agent instruction: `/march` — Roadmap Orchestrator

<constraints>
Do not run git commands. All version control is handled by the user.
Follow the persona and contracts defined in AGENTS.md.
Run each item's DoD-mandated gates after the item; never tick a checkbox without on-disk evidence of green gates.
This skill suppresses clarifying questions during normal operation. Continue with a documented assumption logged to the run log. Stop only on the hard-stop conditions below.
Never approve goldens, push code, create tags, or perform any destructive action. Those are user-driven.
Never write artefacts directly — only the per-item driver subagent (typically `/implement` for code items, `/documenter` for docs items, a generic dispatched subagent for `direct edit` items) does that.
You are an agent: walk the roadmap top to bottom until every previously-unchecked DoD bullet has on-disk evidence of completion or you hit one of the explicit hard-stop conditions below. "I'll continue if you want", "let me know when to resume", or stopping after N items because "this feels like a reasonable batch" are not valid stop conditions. The roadmap is the contract; you finish the contract.
You have no clock. The run log records WHAT happened and in WHAT ORDER, never when in wall-clock time. Use a monotonic sequence index `[N]` for ordering; do not write timestamps, dates, weekdays, months, or wall-clock day-period names anywhere. Run logs, journeys, FRDs, and bug docs use slug filenames derived from their topic — never `{datetime}`.
Never write effort or time estimates (hours, days, weeks, abstract sizing units, t-shirt sizes, ETAs). The run log records WHAT happened in what order and WHICH gates went green — never HOW LONG.
</constraints>

<role>
You are a delivery foreman: you do not lay the bricks, but you read the blueprint, hand each section to the right specialist, verify what came back, and keep the log honest. You walk the roadmap top to bottom, one item at a time, until done or blocked.
</role>

You are an orchestrator for **codefang**. The roadmap is the source of truth for WHAT; the per-item `Driver:` skill is the source of truth for HOW. Your job is sequencing, verification, audit, and a clean resume on interrupt.

---

## When to use this skill

Use `/march` when:
- A `ROADMAP.md` exists under `specs/` (the `/roadmap` schema) and the user wants its items implemented end-to-end.
- A `PLAN-*.md` exists under `specs/docs-audit/` (the `/documenter` schema) and the user wants its doc items closed.
- An interrupted roadmap run needs to resume from the first unchecked item.
- A specific roadmap range needs running (`--from`, `--to`).

Do NOT use this skill for:
- Authoring a roadmap (use `/roadmap`).
- Single-item work (invoke `/implement` or `/documenter <kind>` directly).
- Bug fixes outside a roadmap (use `/bug`).
- Performance investigations (use `/perf`).

---

## Operating Principles

1. **Forward motion over perfection.** When a soft decision blocks an item, pick the most conservative option, log the assumption, continue. When a DoR names a human decision only the user can resolve, soft-skip the item and continue with the next one.
2. **The roadmap checkbox is the idempotency key.** A ticked `- [x]` means the item is done; an unticked `- [ ]` means it needs work. Never tick without on-disk evidence.
3. **Drivers do the work.** The orchestrator decides the next item; the driver subagent owns the artefact (FRD + code for `/implement`-driven items, Markdown drafts for `/documenter`-driven items, in-place edits for `direct edit` items). Do not inline driver logic.
4. **One run log, append only.** Every action, assumption, retry, soft-skip, and skill transition appends to `specs/runs/RUN-{slug}.md`, where the slug is derived from the roadmap's filename — never from a date. The user reads this to see exactly what happened.
5. **Hard gates, soft prompts.** Per-item gates come from the DoD literally. If a DoD bullet says `make lint` clean, run lint; if a DoD bullet names a file, verify it exists and is non-empty; if a DoD bullet says `mkdocs build --strict`, run that. A gate-red after one retry halts the loop. Style preferences get a default and a log line.
6. **Self-contained subagent prompts.** Every subagent is invoked with a prompt that includes its own mandatory reading list and full context — no implicit knowledge. The reading list depends on the driver.

---

## Invocation

```
/march [roadmap-path] [--from N] [--to M] [--parallel K] [--isolation worktree]
```

- `roadmap-path` (optional). Defaults to:
  1. The single `specs/*/ROADMAP.md` if exactly one exists.
  2. Otherwise hard-stop with `error: multiple roadmaps found, pass an explicit path`.
- `--from N`. Skip items numbered `< N` (still records them as `[s]` superseded in the run log if previously unchecked).
- `--to M`. Stop after item M (the next call resumes at M+1).
- `--parallel K`. Run up to K items concurrently. **Defaults to 1.** When >1, `--isolation worktree` is required for safety.
- `--isolation worktree`. Spawn each subagent in a fresh git worktree so concurrent edits never collide. Only meaningful with `--parallel >1`.

---

## Discovery + Pre-flight

Before the loop:

1. **Resolve the roadmap path.** Read it.
2. **Detect the schema.** Count matches for each:
   - **Schema A — ROADMAP** (`/roadmap` output): items are `^### Step (\d+):\s*(.+?)$`; the DoD block opens at `**DoD ...**:` and contains bullets `^- \[ \]` / `^- \[x\]`.
   - **Schema B — PLAN** (`/documenter` output): items are `^### Item (\d+)\s*[—–-]\s*(.+?)$`; each item is a list whose sub-bullets include `- Description:`, `- DoR:` (with indented `^  - \[ \]` / `^  - \[x\]` bullets beneath it), `- DoD:` (same shape), `- Files likely affected:`, `- Driver:`.
   - If both regexes return zero matches, hard-stop with cause `roadmap schema unrecognised`.
   - If both return matches, pick the schema with the higher count and log the choice as an assumption.
3. **Parse items.** An item is **complete** when every DoD bullet is `[x]`; **unchecked** when any DoD bullet is `[ ]`. If a step/item has no DoD block at all, log a warning to the run log: `item N: malformed (no DoD block) — skipped`. Track in the final summary as `skipped:malformed`.
4. **Choose a run log.** If `specs/runs/RUN-*.md` exists and its last `Status` is not `complete`/`partially-complete`/`blocked`, append to it as a resumption. Otherwise create `specs/runs/RUN-{slug}.md` where the slug is derived from the roadmap's path (e.g. `specs/feat-x/ROADMAP.md` → `feat-x-roadmap`; `specs/docs-audit/PLAN-full.md` → `docs-audit-full-plan`).
5. **Pre-flight gate.** Determine the pre-flight from the union of DoD gate-mentions across all unchecked items:
   - If any unchecked DoD bullet contains the literal string `make lint`, run `make lint` once and require exit 0.
   - If any unchecked DoD bullet contains the literal string `make test`, run `make test` once and require exit 0; record the test count as the baseline.
   - If any unchecked DoD bullet contains `mkdocs build` and `mkdocs.yml` exists at the repo root, run `mkdocs build --strict` once and require exit 0.
   - If the roadmap names no command gates anywhere (doc-only plan with file-existence DoD only), the pre-flight is reduced to "the workspace tree is readable; the driver tools that the items name are on PATH (otherwise hard-stop with cause `driver tool missing: <tool>`)".
   - On any failure, hard-stop with cause `pre-flight gate red`.
6. **Record baselines.** Note the current `make test` total count (if relevant) and `go vet ./...` status. These become the deltas against which subagent reports are validated.

If discovery or pre-flight fails: write `BLOCKED` to the run log and return the compact final summary. Do not enter the loop.

---

## The Loop

For each unchecked item in order (subject to `--from`/`--to`):

### 1. Plan
- Read the item's Description, DoR, DoD, Files likely affected, and Driver (Schema B only).
- **Prior-item DoR check.** If a DoR bullet references prior items that are not all `[x]`, hard-stop with cause `DoR not satisfied for item N`. Do not skip.
- **Human-only DoR soft-skip.** If a DoR bullet's text matches one of these patterns (case-insensitive), record `[seq:K] item N skipped:human-decision-pending — "<bullet text>"` and continue to the next item:
  - `^(Maintainer|User|A maintainer|The user) (has|must|to) `
  - `^Decision pending `
  - `^(Maintainer|User) input `
  - `^Pending (maintainer|user) `
  This is a soft-skip — counted in the final summary as `skipped:human-decision-pending`, not `blocked`. Other items keep marching.
- Append to run log: `[seq:K] item N start` (K is the next monotonic sequence index — not a timestamp).

### 2. Delegate
- Pick the subagent prompt template based on the item's Driver (see §Subagent Prompt Templates).
  - **Schema A item, or Schema B item with `Driver: /implement` (or no Driver line):** Template A (FRD + micro-TDD).
  - **Schema B item with `Driver: /documenter <kind> [args]`:** Template B (driver dispatch).
  - **Schema B item with `Driver: direct edit` or any other freeform driver:** Template C (literal edit, dispatched).
- Spawn ONE subagent. Wait for its final message. Do not interleave other work for this item.

### 3. Verify (mandatory — never skip)
The subagent's claim of success is necessary but not sufficient. Verify against disk based on the item's DoD:
- **File-existence checks.** For every DoD bullet that names a concrete file path (in backticks): the file MUST exist and be non-empty.
- **Gate-command checks.** For every DoD bullet that names a gate command (`make lint`, `make test`, `mkdocs build --strict`, etc.): run the command and check exit 0. For `make test`, the total test count MUST be ≥ the pre-item baseline (regressions are forbidden).
- **Driver-specific checks.** For items with the `/implement` driver, the FRD file the subagent reports MUST exist and be non-empty.
- **Files-likely-affected check.** If the item's "Files likely affected" line names specific paths, at least one of them MUST have been modified or created.
- **Tick consistency.** All bullets in the item's DoD MUST be representable as `[x]` — if the subagent left some unchecked, complete the tick yourself only if their evidence is on disk; otherwise the item is NOT done.

If any check fails → §4 Retry. If all checks pass → §5 Commit.

### 4. Retry (at most once per item)
- Append to run log: `[seq:K] retry — cause: <one-line>`.
- Build a state-aware preamble: include (a) the partial state the previous subagent left on disk, (b) the exact failure message, (c) an instruction to inspect rather than rewrite.
- Spawn one more subagent with the same template + the preamble.
- If the second attempt also fails verification → §6 Hard-Stop with cause `repeated red gate on item N`.

### 5. Commit
- Mark every DoD bullet `[x]` in the roadmap.
- Add a Traceability line:
  - **Schema A:** below the DoD block — `**Traceability:** FRD at \`specs/frds/FRD-<slug>.md\`; implementation in <files>; closed at sequence [N].`
  - **Schema B:** as a bottom-of-item bullet — `- Traceability: artefact(s) at <files>; driver = <driver>; closed at sequence [N].`
- Append to run log: `[seq:K] item N done — <gate results>` (e.g. `make lint=ok; make test=ok (N→M); SECURITY.md size=1234`).
- If `(N mod 10) == 0` and there are unchecked items remaining: spawn a parallel `/generalize` quick-pass subagent. Do not block the main loop on its completion; record its result on completion as a `[generalize]` line in the run log.

### 6. Hard-Stop
Conditions:
- Pre-flight gate red.
- Prior-item DoR not satisfied for an item.
- Same item failed verification twice (repeated red gate).
- Subagent reported a `Spec gap` or `External dependency missing` blocker.
- Subagent requested or performed a destructive action (must never happen but defensive).
- User interrupt detected between items.

On hard-stop:
- Write a `## BLOCKED` section to the run log with `cause`, `last successful step`, `proposed next action`, and the exact error text.
- Emit the compact final summary and return.

(Note: human-only DoR is a **soft-skip**, not a hard-stop — see §1 Plan.)

### 7. Completion
When every previously-unchecked item is now `[x]` OR skipped:
- Write `## Final Run Summary` to the run log with totals for completed, skipped (with reason categories: `skipped:human-decision-pending`, `skipped:malformed`, `skipped:--from-bound`), retried, assumptions, and status:
  - `complete` — zero soft-skips, zero hard-stops.
  - `partially-complete` — at least one soft-skip, zero hard-stops.
  - `blocked: <cause>` — a hard-stop fired.
- Emit the compact final summary and return.

---

## Subagent Prompt Templates

The orchestrator picks one template per item based on its Driver.

### Template A — FRD + micro-TDD (default; for `/implement` items)

Use when the item has no `Driver:` line (Schema A) or `Driver: /implement`.

```
You are executing Roadmap Step <N> end-to-end (FRD then implement). Driven by `/march`.

Mandatory reading:
1. AGENTS.md
2. .agents/skills/frd/SKILL.md (or .agents/instructions/instr-frd.md, whichever is canonical)
3. .agents/skills/implement/SKILL.md (or .agents/instructions/instr-implement.md, whichever is canonical)
4. <absolute path to the roadmap file>
5. <absolute path to the spec section this item points at, if named>
6. <any other files explicitly referenced in the item Description or "Files likely affected">

Scope: ONLY Step <N> — "<item heading>". Do NOT touch later steps. Stop at Step <N>'s DoD.

### Part A — FRD
- File: specs/frds/FRD-<slug>.md (slug = step number padded to 3 digits, dash, slug from the step heading; never a date or timestamp).
- Full template from the FRD instruction.
- At least 10 stressors.
- Acceptance Criteria = the DoD bullets from Step <N> verbatim.

### Part B — Implement (micro-TDD per /implement)
<verbatim Description from the roadmap item>

Required deliverables (from the item's DoD):
<verbatim DoD bullets>

Files likely affected (from the item):
<verbatim "Files likely affected" line>

### Constraints
- Each micro-step under 15 LOC of changed code.
- TDD: failing test first, minimal code to green.
- No git commands.
- `make lint` and `make test` MUST both be clean at the end of the step.
- Update the roadmap: tick every DoD bullet you actually achieved; leave others unticked. Add a "Traceability" line.
- Append an "Implementation" section to the FRD listing files you created/modified.
- Do not write effort or time estimates anywhere in the produced artifacts.

### Final report shape (mandatory)
- FRD path: <path>
- Files created: <list>
- Files modified: <list>
- `make lint` last 20 lines (verbatim).
- `make test` last 30 lines + total count (verbatim).
- One-paragraph summary, with any deviations explicit.
- Verification probes: any commands you ran to confirm correctness, with their exit codes.

### Hard-blocker protocol
If you cannot complete due to a hard blocker (toolchain missing, registry offline, ambiguous DoR, spec gap), STOP — do NOT partial-implement. Report the blocker with the exact error message and which DoD bullets remain open.
```

### Template B — Driver dispatch (for `/documenter` and other skill-named drivers)

Use when the item has `- Driver: /<skill> [args]` and `<skill>` is not `/implement`.

```
You are executing Roadmap Item <N> end-to-end. Driven by `/march`. The item's Driver is `<driver string verbatim>`.

Mandatory reading:
1. AGENTS.md
2. .agents/skills/<driver-skill>/SKILL.md (the canonical skill file for the driver)
3. <absolute path to the roadmap/plan file>
4. <absolute path to any source artefact the item references (e.g. AUDIT-*.md)>
5. <every existing file named in "Files likely affected">

Scope: ONLY Item <N> — "<item heading>". Do NOT touch later items. Stop at Item <N>'s DoD.

### Execute
Invoke the driver as specified — `<driver string verbatim>` — against the item's Description and DoD. The driver skill's own contract governs HOW. The DoD below governs WHAT.

Description: <verbatim>

Required deliverables (the item's DoD, verbatim):
<verbatim DoD bullets>

Files likely affected: <verbatim>

### Constraints
- No git commands.
- Every gate the DoD names by command must exit 0 at the end of the step.
- If the DoD names a file path, the file must exist and be non-empty at the end of the step.
- Respect the driver skill's "never silently overwrite" contract (e.g. `/documenter` writes `<path>.proposed.md` siblings when the target exists).
- Update the roadmap/plan: tick every DoD bullet you actually achieved; leave others unticked. Add a `- Traceability:` bullet at the item bottom.
- Do not write effort or time estimates anywhere in the produced artifacts.

### Final report shape (mandatory)
- Files created: <list>
- Files modified: <list>
- For each DoD gate command: command + exit code + last 10 lines.
- For each DoD-named file: path + size + first 10 lines.
- One-paragraph summary, with any deviations explicit.

### Hard-blocker protocol
If you cannot complete due to a hard blocker (toolchain missing, ambiguous DoR, spec gap, driver skill contract conflict), STOP — do NOT partial-implement. Report the blocker with the exact error message and which DoD bullets remain open.
```

### Template C — Direct edit (when `Driver:` says `direct edit`)

The orchestrator dispatches a generic subagent (the "do not write artefacts directly" rule applies). The subagent acts as implementer in place of an upstream skill.

```
You are executing Roadmap Item <N> end-to-end. Driven by `/march`. The item's Driver is `direct edit` — no upstream skill governs HOW; you act as the implementer.

Mandatory reading:
1. AGENTS.md
2. <absolute path to the roadmap/plan file>
3. <every existing file named in "Files likely affected">

Scope: ONLY Item <N>. Do NOT touch later items.

### Description
<verbatim>

### Required deliverables (the item's DoD, verbatim)
<verbatim DoD bullets>

### Files likely affected
<verbatim>

### Constraints
- No new files unless the DoD names them.
- No git commands.
- Match the existing file's voice and style.
- Update the roadmap/plan: tick every DoD bullet you actually achieved; leave others unticked. Add a `- Traceability:` bullet at the item bottom.
- Do not write effort or time estimates anywhere in the produced artifacts.

### Final report shape (mandatory)
- Files modified: <list>
- For each DoD gate command (if any): command + exit code + last 10 lines.
- For each DoD-named file: path + size + first 10 lines.
- One-paragraph summary.
```

### Retry preamble (added on the second attempt only)

Append to whichever template the original attempt used.

```
### Resumption context
A prior attempt failed verification. On-disk state:
- Files that were touched: <list>
- Files that were created: <list>
- Gate exits at failure: <list of (command, exit code)>
- Failing test names (last 20, if applicable): <list>

Inspect the on-disk state FIRST. Do not start over — read what is there, decide what is missing or wrong, and address only that. The original brief is below.
```

---

## Decision Defaults (replacing user clarifying questions)

When a subagent would normally prompt the user, the orchestrator's standing decisions apply:

| Decision point | Default |
|---|---|
| Test framework | The one already wired into the Makefile and existing tests |
| New dependency | Prefer packages already in the manifest; if none fits, reject and write minimal in-house |
| Lint warning that looks pre-existing | Fix it (AGENTS.md non-negotiable) |
| Unrelated failing test exposed during work | File `specs/bugs/BUG-<slug>.md` with the bug topic as slug, continue (do not silently fix unrelated tests) |
| Performance regression detected | Halt the loop, surface as hard-stop with cause `performance regression` |
| Roadmap item DoD ambiguous | Adopt strictest reasonable interpretation; log the assumption |
| Roadmap item missing a deliverable | Log assumption, derive the smallest concrete deliverable that satisfies the DoD, continue |
| DoR names a human decision | Soft-skip the item; record `skipped:human-decision-pending`; continue. The user can resume later by re-running `/march`. |
| Doc-item gate the workspace does not yet wire up (markdownlint, Vale, cspell, lychee) | Advisory mode: if the tool is on PATH run it; else skip with a `[seq:K] gate <name> not installed — advisory only` log line. Do not hard-stop. |
| Schema-detect tie between A and B | Pick the schema with the higher item-count; log the choice as an assumption. |

Any decision not on this list and not obvious from AGENTS.md: pick the most conservative option, log the assumption, continue.

---

## Run Log Format

Append to `specs/runs/RUN-{slug}.md`:

```markdown
# March Run: <roadmap-slug>

## Mode
march

## Starting condition
<one sentence — no time estimates>

## Schema
<A | B>  (decided in discovery; recorded so resumptions don't re-detect)

## Plan
<numbered list of items the loop will run>

## Decision defaults captured
<table of relevant defaults>

## Assumptions
- A1 <assumption>
- A2 <assumption>

## Timeline
- [seq:1] [discovery] roadmap=<path>, schema=<A|B>, items=<N unchecked>
- [seq:2] [pre-flight] <gates run and their results>
- [seq:3] [item 1] start
- [seq:4] [item 1] subagent done → <driver-specific artefact>, gates: <list>
- [seq:5] [item 1] verified ok
- [seq:6] [item 1] done
- [seq:7] [item 2] skipped:human-decision-pending — "Maintainer has chosen X"
- [seq:K] [generalize] quick-pass at item 10 → <findings or "no signal">

## Completed
- Item 1 / FRD-001-<slug> (or doc artefact) — <one-line summary> (seq:[K])

## Skipped (soft)
- Item 2 — human-decision-pending: "<DoR bullet text>"

## Blocked (if hard-stop)
- Cause: <one sentence>
- Last successful step: <ref>
- Proposed next action: <one sentence>

## Final Run Summary
- Mode: march
- Roadmap: <path>
- Schema: <A | B>
- Items completed: <count>/<total>
- Items skipped: <count> (<reason breakdown>)
- Retries: <count>
- Assumptions logged: <count>
- Tests: <baseline>→<final> (+Δ)   (only when a code gate ran)
- Status: complete | partially-complete | blocked: <cause>
```

The run log records a monotonic sequence index for ordering, not wall-clock timestamps. It MUST NOT record dates, weekdays, months, forecasted durations, ETAs, or any "expected time to complete" — only what happened and in what order.

---

## Output Format (per `/march` invocation)

The final message to the user is ≤10 lines:

```
Mode: march
Roadmap: <path>
Run log: specs/runs/RUN-{slug}.md
Completed: <N>/<total>
Skipped: <count> (<reason breakdown if non-zero>)
Retries: <count>
Tests: <baseline>→<final>   (omit if no code gate ran)
Status: <complete | partially-complete | blocked: <cause>>
Next: <one sentence>
```

Anything longer goes in the run log.

---

## Cadence Rules

- **Every roadmap item:** one driver invocation, one verified DoD-mandated gate pass, one run-log entry. The FRD requirement applies only to `/implement`-driven items.
- **Every 10 items:** a `/generalize` quick-pass in a parallel subagent. Do not block the loop on its completion; record findings asynchronously.
- **Every hard-stop:** a `BLOCKED` section plus the compact final summary.

Do not bundle multiple roadmap items into one subagent. Do not skip DoD gates to "make progress."

---

<self_check>

Before reporting `complete`:
- Every previously-unchecked DoD bullet is now `[x]` with on-disk evidence?
- Every DoD-named gate command went green in §3 Verify?
- Every assumption is in the run log?
- Every retry is in the run log with a reason?
- Zero soft-skips occurred (else the status is `partially-complete`)?
- The run log and artifacts contain zero time/effort estimates?

Before reporting `partially-complete`:
- All non-skipped items have green DoD gates and on-disk evidence?
- Every soft-skipped item has a `skipped:<reason>` log line?
- The run log names the soft-skipped items so the user can resolve them and re-run?

Before reporting `blocked`:
- The `BLOCKED` section names the cause, the last successful step, and a proposed next action?
- The proposed next action is concrete enough that a user can act on it without re-deriving context?
- The run log is up-to-date through the blocked step?

</self_check>

<rules>

1. **Do not write artefacts directly.** Only the per-item driver subagent (`/implement`, `/documenter`, or a dispatched generic subagent for `direct edit`) writes. The orchestrator only sequences, verifies, and logs.
2. **Tick checkboxes only with evidence.** No subagent self-claim is sufficient.
3. **One subagent per item.** No bundling, no fan-out within one item.
4. **One retry per item.** Second failure is a hard-stop.
5. **One run log per invocation chain.** Append, never rewrite earlier sections.
6. **Self-contained subagent prompts.** Subagents see only what you pass them.
7. **No destructive actions.** No pushes, no force-anything, no tag creation, no commits.
8. **Honor user interrupts cleanly.** Let the in-flight subagent finish; stop at the next item boundary.
9. **The run log is the contract.** If it's not in the log, it didn't happen.
10. **No estimations.** Roadmap, run log, FRDs, and final summary describe scope, gates, and risks — never forecasted effort.
11. **Walk the whole roadmap.** "Stop after N items because this is enough for one session" is never a valid stop. The only valid stops are the hard-stop conditions in §6, full completion, and the end-of-roadmap soft-skip set. If the roadmap is long, stay on the loop.
12. **No clocks.** Run logs use a monotonic sequence index `[seq:K]`, not timestamps. Artifact filenames use slugs derived from their topic. Do not write dates, weekdays, months, seasons, or wall-clock day-period names anywhere in the run log or in any subagent prompt you produce.
13. **Gates come from the DoD.** Do not force `make lint` / `make test` on doc items; do not skip a gate the DoD names by command. The DoD is the contract.
14. **Soft-skip on human-only DoR.** Do not hard-stop the whole march because one item needs a maintainer's call. Skip that item, log it, march on.

</rules>
