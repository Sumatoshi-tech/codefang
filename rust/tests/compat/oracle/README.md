# Differential Oracle (Go ↔ Rust compatibility)

Implements **layer 3 / roadmap item 1** of `specs/go-compat-testing/SPEC.md`:
the differential oracle + Go-measured canonicalizer. The live Go binary is the
source of truth; this oracle never re-derives expected output.

## Files

| File | Role |
|------|------|
| `oracle.py` | The oracle. Runs Go N≥3× and Rust 2× under the pinned env, MEASURES the stable/variant field classification with stored evidence, and emits a per-invocation verdict PASS/FAIL/SIM. |
| `parity_gate.sh` | The generalization of `rust/tests/antisim/parity_gate.sh` onto the oracle. Strictly stronger: nothing is hand-declared, everything is measured. |
| `selftest/self_test.py` | The self-proof. Plants every defect class and asserts the oracle reports the correct non-PASS verdict. A green that cannot catch a planted bug is worthless. |

## What the oracle does per invocation

```
set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>
```

1. **Run Go N≥3×** (the oracle). Classify every JSON leaf path as
   - `GO-STABLE`  — identical value in every Go run
   - `GO-VARIANT` — differs across Go runs (or present/absent inconsistently)

   For every variant path the **distinct differing Go values are stored as
   EVIDENCE** in the manifest. Canonicalization is *measured, never declared.*

2. **Run Rust 2×.** If the two identical-arg Rust runs differ, Rust is
   nondeterministic → **FAIL** (a faithful port is deterministic).

3. **Compare Rust vs Go:**
   - `GO-STABLE` fields → **byte/value-exact** against the one stable Go value.
     A dropped or blanked stable field cannot pass.
   - `GO-VARIANT` **lists** → sorted (multiset) on both sides, then compared.
   - `GO-VARIANT` **numeric scalars** → compared inside Go's **own observed
     `[min,max]` envelope** (e.g. summation-order float wobble), *not* blanked.
     A Rust value outside Go's measured spread still diverges.
   - If Go's *canonicalized* runs still disagree (the member SET itself is
     Go-nondeterministic, e.g. `history/shotness`), byte/canonical parity is
     measurably impossible; the oracle falls back to the **structural realcheck**
     (non-empty, grows with `--limit`, Rust deterministic) and records the
     Go canonical evidence hashes.

4. **Verdict:** `PASS` (rc 0) / `FAIL` (rc 1) / `SIM` (rc 3) + the field
   classification + evidence (with `--manifest <path>`).

## The forbidden cheat, and how it is detected

The historic failure was **blanking a Go-STABLE field** to hide a buggy Rust
value. The oracle accepts an optional `--normalize <path>` request and, if any
requested path is measured `GO-STABLE`, returns **FAIL: TAMPER** — fail-closed.
You cannot neutralize a field the oracle measured stable.

## Run it

```bash
# one invocation, with a stored manifest
python3 oracle.py --n-go 3 --manifest /tmp/m.json -- uast parse --format json FILE.go

# the generalized parity gate over the full probe set
bash parity_gate.sh            # all probes
bash parity_gate.sh history    # filter by substring
NGO=5 bash parity_gate.sh      # more Go runs per probe

# the self-proof (must be green for the oracle to be trusted)
python3 selftest/self_test.py
```

## Self-proof (`selftest/self_test.py`)

Plants each defect by scripting the binary outputs (the binaries *are* the
oracle, so faking their output is the honest way to plant a defect without
corrupting the real port) and asserts the verdict:

| Planted defect | Expected |
|----------------|----------|
| Rust wrong on a Go-stable field | FAIL |
| Rust drops a Go-stable field | FAIL |
| Rust nondeterministic (2 runs differ) | FAIL |
| `--normalize` targets a Go-STABLE field (the cheat) | FAIL / TAMPER |
| Variant-list content bug (wrong member after sort) | FAIL |
| Hardcoded constant on content-nondeterministic Go | FAIL |
| Empty Rust stub | FAIL |
| Everything agrees (negative control) | PASS |
| Order-only Go nondeterminism, Rust canonically correct | PASS |
| Honest structural growth with `--limit` | PASS |

## Real bugs this oracle has already surfaced

Both were **GREEN under the old hand-classified `parity_gate.sh`** because that
gate declared the analyzer "content-nondeterministic" and only ran a structural
`realprobe` (non-empty / grows / deterministic). The oracle MEASURES that Go is
actually stable on these fields and demands parity:

- `history/file-history`: Go-stable integer aggregates (`total_commits=308`,
  `total_files=124` on kubernetes `--limit 50`) are wrong in Rust (`316`, `123`).
- `history/couples`: Go-stable-after-envelope `avg_coupling_strength`
  (`0.91406…`) is wrong in Rust (`0.91354…`), a divergence ~5000× larger than
  Go's own ULP wobble.
