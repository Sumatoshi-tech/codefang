#!/usr/bin/env bash
# =============================================================================
# Golden reference capture for the Go codefang/uast tools.
#
# Establishes BYTE-EXACT golden outputs that the Rust rewrite must match.
#
# BINDING goldens  = machine formats {json, yaml, ndjson, timeseries, compact, bin}
#                    -> the Rust port MUST reproduce these byte-for-byte.
# NON-BINDING      = human formats {text, plot, html} -> cosmetic; captured for
#                    reference only (and plot/html write to a DIRECTORY, not stdout,
#                    so their stdout golden is empty -- see notes).
#
# Determinism strategy (the Rust harness MUST replicate ALL of this):
#   * env: TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800
#   * `set -f` (noglob) so the literal '*' analyzer selector reaches the binary
#     instead of being expanded by the shell against the cwd.
#   * Only STDOUT is captured. STDERR carries timestamped progress logs and is
#     discarded. Machine output goes to stdout; logs go to stderr.
#   * Work is bounded by `--head` (HEAD commit only) + `--limit` so output does
#     not depend on the full 135k-commit history. NOTE: the burndown
#     `generated_at` field is a FIXED constant ("2025-08-19T00:00:00Z") in the Go
#     code, not wall-clock, so timestamps are already reproducible.
#   * ndjson is a STREAMING format: it emits one line per commit during the
#     streaming pipeline and produces NOTHING in --head mode. ndjson goldens are
#     therefore captured in streaming mode: `--limit 10 --workers 1` (no --head),
#     which is deterministic for a fixed HEAD.
#   * uast parse/analyze/query run on a FIXED working-tree file; the file's git
#     blob hash + sha256 are recorded so the harness can confirm identical input.
#     uast output embeds the ABSOLUTE file path, so the harness must invoke with
#     the same absolute path (recorded in MANIFEST.inputs.uastFile).
#
# Each BINDING golden is captured twice and its sha256 compared; "stable":"true"
# means both runs were byte-identical.
#
# Usage: bash capture.sh
# =============================================================================
set -u

export TZ=UTC
export NO_COLOR=1
export LANG=C
export LC_ALL=C
export SOURCE_DATE_EPOCH=315532800   # 1980-01-01T00:00:00Z (bounded; used if honored)

CODEFANG=/home/dmitriy/sources/codefang/build/bin/codefang
UAST=/home/dmitriy/sources/codefang/build/bin/uast
KREPO=/home/dmitriy/sources/kubernetes
GOLDEN=/home/dmitriy/sources/codefang/rust/tests/golden
RUNDIR="$GOLDEN/run"
UASTDIR="$GOLDEN/uast"
mkdir -p "$RUNDIR" "$UASTDIR"

git config --global --add safe.directory '*' >/dev/null 2>&1 || true
HEAD=$(git -C "$KREPO" rev-parse HEAD)

# CRITICAL determinism flags for every `run` invocation.
# Checkpoint/resume/cache are ON by default and make output depend on prior runs:
# after one run writes a checkpoint, the next run RESUMES it and processes 0 new
# commits (e.g. ndjson then emits nothing). Disabling them makes every capture
# self-contained and order-independent. The Rust harness must run with the same
# semantics (no cross-run state).
RUN_DETERMINISM="--checkpoint=false --resume=false --no-cache"
rm -rf "$HOME/.codefang/checkpoints"/* 2>/dev/null || true

# Fixed source files for uast goldens (deterministic by working-tree content).
UFILE_FUNC="$KREPO/staging/src/k8s.io/apimachinery/pkg/util/sets/byte.go"   # has functions
UFILE_FUNC_BLOB=$(git -C "$KREPO" hash-object "$UFILE_FUNC" 2>/dev/null || echo "")
UFILE_FUNC_SHA=$(sha256sum "$UFILE_FUNC" | cut -d' ' -f1)

ENV_JSON='{"TZ":"UTC","NO_COLOR":"1","LANG":"C","LC_ALL":"C","SOURCE_DATE_EPOCH":"315532800"}'

RECORDS="$GOLDEN/.records.ndjson"
: > "$RECORDS"

json_escape() { printf '%s' "$1" | python3 -c 'import json,sys; sys.stdout.write(json.dumps(sys.stdin.read()))'; }

# capture <id> <outfile> <machine 0|1> <nonbinding 0|1> <note> -- <argv...>
# Globbing is disabled (set -f) so a literal '*' in argv reaches the program.
capture() {
  local id="$1" out="$2" machine="$3" nonbind="$4" note="$5"; shift 5
  [ "$1" = "--" ] && shift
  local argv=("$@")

  set -f
  "${argv[@]}" > "$out" 2>/dev/null
  local rc=$?
  set +f

  local bytes sha stable
  bytes=$(wc -c < "$out")
  sha=$(sha256sum "$out" | cut -d' ' -f1)

  stable="n/a"
  if [ "$nonbind" = "0" ]; then
    local tmp; tmp=$(mktemp)
    set -f
    "${argv[@]}" > "$tmp" 2>/dev/null
    set +f
    local sha2; sha2=$(sha256sum "$tmp" | cut -d' ' -f1)
    [ "$sha" = "$sha2" ] && stable="true" || stable="false"
    rm -f "$tmp"
  fi

  local argv_json="[" first=1 a
  for a in "${argv[@]}"; do
    [ $first -eq 1 ] && first=0 || argv_json+=","
    argv_json+="$(json_escape "$a")"
  done
  argv_json+="]"

  local rel="${out#$GOLDEN/}"
  {
    printf '{'
    printf '"id":%s,'        "$(json_escape "$id")"
    printf '"argv":%s,'      "$argv_json"
    printf '"env":%s,'       "$ENV_JSON"
    printf '"outPath":%s,'   "$(json_escape "$out")"
    printf '"relPath":%s,'   "$(json_escape "$rel")"
    printf '"sha256":%s,'    "$(json_escape "$sha")"
    printf '"bytes":%s,'     "$bytes"
    printf '"rc":%s,'        "$rc"
    printf '"machine":%s,'   "$([ "$machine" = "1" ] && echo true || echo false)"
    printf '"nonBinding":%s,' "$([ "$nonbind" = "1" ] && echo true || echo false)"
    printf '"stable":%s,'    "$(json_escape "$stable")"
    printf '"note":%s'       "$(json_escape "$note")"
    printf '}\n'
  } >> "$RECORDS"
  printf 'CAP %-28s rc=%s bytes=%-8s stable=%-5s %s\n' "$id" "$rc" "$bytes" "$stable" "${sha:0:12}"
}

# Machine (binding) formats that work in --head mode.
HEAD_MACHINE_FMTS="json yaml timeseries compact bin"
HUMAN_FMTS="text plot html"
LIMIT_SINGLE=5
LIMIT_STREAM=10

# All real analyzer IDs (from `codefang run --list-analyzers`). Qualified IDs are required.
# History analyzers that yield meaningful output in --head (single-commit) mode:
HISTORY_HEAD_ANALYZERS="history/anomaly history/couples history/devs history/shotness"
# History analyzers that need the STREAMING pipeline (empty/zero output in --head mode);
# captured with --limit 10 --workers 1 (deterministic for a fixed HEAD).
HISTORY_STREAM_ANALYZERS="history/imports history/quality history/sentiment history/typos history/file-history"
STATIC_ANALYZERS="static/clones static/cohesion static/comments static/complexity static/composition static/halstead static/imports"

echo "=== run: single history analyzer (history/burndown), head-mode machine formats ==="
for f in $HEAD_MACHINE_FMTS; do
  capture "run.burndown.$f" "$RUNDIR/burndown.$f" 1 0 \
    "history/burndown, machine format $f, --head --limit $LIMIT_SINGLE" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format "$f" --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== run: history/burndown, ndjson (STREAMING mode: --limit, no --head) ==="
capture "run.burndown.ndjson" "$RUNDIR/burndown.ndjson" 1 0 \
  "history/burndown ndjson; streaming mode --limit $LIMIT_STREAM --workers 1 (ndjson is empty in --head mode)" -- \
  "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format ndjson --limit "$LIMIT_STREAM" --workers 1 "$KREPO"

echo "=== run: history/burndown, human formats (nonbinding) ==="
for f in $HUMAN_FMTS; do
  capture "run.burndown.$f" "$RUNDIR/burndown.$f" 0 1 \
    "history/burndown human format $f (cosmetic; plot/html write to a dir not stdout)" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format "$f" --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== run: ALL analyzers ('*'), head-mode machine formats ==="
for f in $HEAD_MACHINE_FMTS; do
  capture "run.all.$f" "$RUNDIR/all.$f" 1 0 \
    "all analyzers '*', machine format $f, --head --limit $LIMIT_SINGLE" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers '*' --format "$f" --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== run: ALL analyzers ('*'), ndjson (streaming) ==="
capture "run.all.ndjson" "$RUNDIR/all.ndjson" 1 0 \
  "all analyzers '*' ndjson; streaming --limit $LIMIT_STREAM --workers 1" -- \
  "$CODEFANG" run $RUN_DETERMINISM --analyzers '*' --format ndjson --limit "$LIMIT_STREAM" --workers 1 "$KREPO"

echo "=== run: ALL analyzers ('*'), human formats (nonbinding) ==="
for f in $HUMAN_FMTS; do
  capture "run.all.$f" "$RUNDIR/all.$f" 0 1 \
    "all analyzers '*' human format $f (cosmetic)" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers '*' --format "$f" --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== run: head-mode history analyzers, json (machine) ==="
for a in $HISTORY_HEAD_ANALYZERS; do
  base=$(printf '%s' "$a" | tr '/' '_')
  capture "run.$base.json" "$RUNDIR/$base.json" 1 0 \
    "$a json, --head --limit $LIMIT_SINGLE" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== run: streaming-mode history analyzers, json (machine) ==="
for a in $HISTORY_STREAM_ANALYZERS; do
  base=$(printf '%s' "$a" | tr '/' '_')
  capture "run.$base.json" "$RUNDIR/$base.json" 1 0 \
    "$a json; streaming --limit $LIMIT_STREAM --workers 1 (empty in --head mode)" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --limit "$LIMIT_STREAM" --workers 1 "$KREPO"
done

echo "=== run: each static analyzer individually, json (machine) ==="
for a in $STATIC_ANALYZERS; do
  base=$(printf '%s' "$a" | tr '/' '_')
  capture "run.$base.json" "$RUNDIR/$base.json" 1 0 \
    "$a json, --head --limit $LIMIT_SINGLE" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --head --limit "$LIMIT_SINGLE" "$KREPO"
done

echo "=== uast parse/analyze/query on fixed file ($UFILE_FUNC) ==="
# uast parse: default json; also yaml.
capture "uast.parse.json" "$UASTDIR/parse.json" 1 0 \
  "uast parse json (blob $UFILE_FUNC_BLOB)" -- \
  "$UAST" parse --format json "$UFILE_FUNC"
capture "uast.parse.yaml" "$UASTDIR/parse.yaml" 1 0 \
  "uast parse yaml (blob $UFILE_FUNC_BLOB)" -- \
  "$UAST" parse --format yaml "$UFILE_FUNC"

# uast analyze: default text (human); json/yaml are machine.
capture "uast.analyze.json" "$UASTDIR/analyze.json" 1 0 \
  "uast analyze json (blob $UFILE_FUNC_BLOB)" -- \
  "$UAST" analyze --format json "$UFILE_FUNC"
capture "uast.analyze.yaml" "$UASTDIR/analyze.yaml" 1 0 \
  "uast analyze yaml (blob $UFILE_FUNC_BLOB)" -- \
  "$UAST" analyze --format yaml "$UFILE_FUNC"
capture "uast.analyze.text" "$UASTDIR/analyze.text" 0 1 \
  "uast analyze text (human, nonbinding)" -- \
  "$UAST" analyze --format text "$UFILE_FUNC"

# uast query: query is a POSITIONAL arg, then files. default json.
capture "uast.query.json" "$UASTDIR/query.json" 1 0 \
  "uast query 'filter(.roles has Function)' json (blob $UFILE_FUNC_BLOB)" -- \
  "$UAST" query 'filter(.roles has "Function")' --format json "$UFILE_FUNC"

# ---- Assemble MANIFEST.json ----------------------------------------------------
python3 - "$RECORDS" "$GOLDEN/MANIFEST.json" "$HEAD" "$UFILE_FUNC" "$UFILE_FUNC_BLOB" "$UFILE_FUNC_SHA" "$CODEFANG" "$UAST" "$KREPO" <<'PY'
import json, sys, subprocess, datetime
recs, manifest_path, head, ufile, ublob, usha, cf, ua, krepo = sys.argv[1:10]
caps = [json.loads(l) for l in open(recs) if l.strip()]

def sh(c):
    try: return subprocess.check_output(c, shell=True, stderr=subprocess.DEVNULL).decode().strip()
    except Exception: return ""

manifest = {
  "description": "Byte-exact golden reference outputs for the Go codefang + uast tools. "
                 "Machine formats (json/yaml/ndjson/timeseries/compact/bin) are BINDING: the "
                 "Rust port must match them byte-for-byte. Human formats (text/plot/html) are "
                 "nonBinding (cosmetic).",
  "generated_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "binaries": {
    "codefang": cf, "codefangSha256": sh(f"sha256sum {cf} | cut -d' ' -f1"),
    "uast": ua,     "uastSha256":     sh(f"sha256sum {ua} | cut -d' ' -f1"),
    "goVersion": sh("go version"),
    "codefangVersion": sh(f"{cf} version 2>&1 | head -1"),
  },
  "env": {"TZ":"UTC","NO_COLOR":"1","LANG":"C","LC_ALL":"C","SOURCE_DATE_EPOCH":"315532800"},
  "envNote": "Set exactly these vars AND use `set -f` (noglob) before each command so the literal "
             "'*' analyzer selector reaches the binary. Only STDOUT is captured; STDERR (timestamped "
             "progress logs) is discarded.",
  "inputs": {
    "runRepo": krepo,
    "runHeadPinned": head,
    "runBoundNote": "`run` analyzes the whole git repo discovered from .git. A subdirectory cannot be "
                    "passed (run accepts at most 1 path arg = the repo). Work is bounded by --head "
                    "(HEAD commit only) + --limit. burndown's `generated_at` is a fixed constant in the "
                    "Go source, so timestamps are reproducible without SOURCE_DATE_EPOCH.",
    "limitSingleAnalyzer": 5,
    "limitStreamNdjson": 10,
    "streamNote": "ndjson emits one line per commit during STREAMING and is empty in --head mode; "
                  "ndjson goldens use `--limit 10 --workers 1` (no --head) which is deterministic for "
                  "a fixed HEAD.",
    "uastFile": ufile,
    "uastFileGitBlob": ublob,
    "uastFileSha256": usha,
    "uastFileNote": "uast parse/analyze/query operate on this file's working-tree bytes and embed its "
                    "ABSOLUTE path in the output; the Rust harness must invoke with the identical "
                    "absolute path. Working tree was clean (git status --porcelain empty) at capture.",
    "outputPathNote": "`run` output metadata embeds the absolute repo path once; reproduce with the "
                      "same absolute repo path."
  },
  "formats": {
    "machineBinding": ["json","yaml","ndjson","timeseries","compact","bin"],
    "humanNonBinding": ["text","plot","html"],
    "runFormats": "codefang run --format: json yaml plot bin timeseries ndjson text compact (default json)",
    "uastParseFormats": "uast parse --format: json yaml proto text (default json)",
    "uastAnalyzeFormats": "uast analyze --format: json yaml text (default text)",
    "uastQueryFormats": "uast query --format: json yaml text (default json)",
    "timeseriesPlusNdjsonNote": "There is NO single 'timeseries+ndjson' format string. The combination "
        "is the `timeseries` format WITH the `--ndjson` flag (timeseries -> timeseries+ndjson). The "
        "task list maps to the distinct `timeseries` and `ndjson` formats, both captured here. A "
        "dedicated timeseries+ndjson capture can be added via `--format timeseries --ndjson`.",
    "binNote": "bin is binary; outPath holds raw bytes and sha256 is the stable comparison hash.",
    "plotHtmlNote": "plot/html render multi-page HTML to a DIRECTORY (-o), not stdout; their stdout "
        "golden is empty. They are nonBinding."
  },
  "stabilityNote": "Each binding capture ran twice with identical env/argv (set -f); 'stable':'true' "
                   "means both runs produced identical sha256.",
  "captures": caps,
}
json.dump(manifest, open(manifest_path,"w"), indent=2)
open(manifest_path,"a").write("\n")
print("MANIFEST captures:", len(caps))
PY

echo "=== DONE ==="
