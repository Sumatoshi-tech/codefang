#!/usr/bin/env bash
# =============================================================================
# Golden reference capture for the Go codefang/uast tools.
#
# Establishes BYTE-EXACT golden outputs that the Rust rewrite must match.
#
# BINDING goldens  = machine formats {json, yaml, ndjson, timeseries,
#                    timeseries+ndjson, bin} that were VERIFIED STABLE
#                    (byte-identical across two runs). The Rust port MUST
#                    reproduce these byte-for-byte.
# NON-BINDING      = (a) human formats {text, plot, html, compact} -> cosmetic.
#                    (b) machine formats that are INTRINSICALLY NONDETERMINISTIC
#                        in the Go tool itself (Go map-iteration ordering in
#                        per-file / per-pair sections). For these, sorted content
#                        is identical across runs but byte order is not, so they
#                        cannot be byte-golden. The Rust harness must compare them
#                        after CANONICALIZATION (sort the per-file/per-pair arrays),
#                        OR the Rust port may emit them deterministically (an
#                        improvement). Marked nonBinding=true, stable=false.
#
# Determinism strategy (the Rust harness MUST replicate ALL of this):
#   * env: TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800
#   * `set -f` (noglob) so the literal '*' analyzer selector reaches the binary
#     instead of being expanded by the shell against the cwd.
#   * Only STDOUT is captured. STDERR carries timestamped progress logs and is
#     discarded.
#   * Cross-run state disabled: --checkpoint=false --resume=false --no-cache and
#     the checkpoint dir is wiped, so every capture is self-contained.
#   * Work is bounded:
#       - STATIC analyzers run on a FIXED SUBDIRECTORY of kubernetes (10 files,
#         see SUBDIR below) via `-p`. This is the "fast deterministic subset".
#         A full-repo static capture is impractical (100k+ files, GBs of output)
#         AND no more deterministic (same map-ordering issue), so the subset is
#         the canonical static golden.
#       - HISTORY analyzers run on the FULL kubernetes repo bounded by --head
#         (HEAD commit only) and/or streaming --limit N --workers 1.
#   * burndown `generated_at` is a FIXED constant in the Go source, not wall
#     clock, so its timestamps are reproducible regardless of SOURCE_DATE_EPOCH.
#   * uast parse/analyze/query run on a FIXED working-tree file; the file's git
#     blob hash + sha256 are recorded. uast output embeds the ABSOLUTE file path,
#     so the harness must invoke with the same absolute path.
#
# Each BINDING candidate is captured twice and its sha256 compared; only captures
# with stable=true are treated as BINDING. Unstable machine captures are
# downgraded to nonBinding automatically (with note).
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
# Fast deterministic STATIC subset: a small, fixed subdirectory (10 source files).
SUBDIR=$KREPO/staging/src/k8s.io/apimachinery/pkg/util/sets
GOLDEN=/home/dmitriy/sources/codefang/rust/tests/golden
RUNDIR="$GOLDEN/run"
STATICDIR="$GOLDEN/static"
UASTDIR="$GOLDEN/uast"
rm -rf "$RUNDIR" "$STATICDIR" "$UASTDIR"
mkdir -p "$RUNDIR" "$STATICDIR" "$UASTDIR"

git config --global --add safe.directory '*' >/dev/null 2>&1 || true
HEAD=$(git -C "$KREPO" rev-parse HEAD)

RUN_DETERMINISM="--checkpoint=false --resume=false --no-cache"
rm -rf "$HOME/.codefang/checkpoints"/* 2>/dev/null || true

# Fixed source file for uast goldens (deterministic by working-tree content).
UFILE="$SUBDIR/byte.go"
UFILE_BLOB=$(git -C "$KREPO" hash-object "$UFILE" 2>/dev/null || echo "")
UFILE_SHA=$(sha256sum "$UFILE" | cut -d' ' -f1)

ENV_JSON='{"TZ":"UTC","NO_COLOR":"1","LANG":"C","LC_ALL":"C","SOURCE_DATE_EPOCH":"315532800"}'

RECORDS="$GOLDEN/.records.ndjson"
: > "$RECORDS"

json_escape() { printf '%s' "$1" | python3 -c 'import json,sys; sys.stdout.write(json.dumps(sys.stdin.read()))'; }

# capture <id> <outfile> <machine 0|1> <forceNonBind 0|1> <note> -- <argv...>
# Globbing is disabled (set -f) so a literal '*' in argv reaches the program.
# A machine capture that fails the double-run stability check is auto-downgraded
# to nonBinding=true (stable=false).
capture() {
  local id="$1" out="$2" machine="$3" forcenb="$4" note="$5"; shift 5
  [ "$1" = "--" ] && shift
  local argv=("$@")

  set -f
  "${argv[@]}" > "$out" 2>/dev/null
  local rc=$?
  set +f

  local bytes sha stable nonbind
  bytes=$(wc -c < "$out")
  sha=$(sha256sum "$out" | cut -d' ' -f1)

  stable="n/a"
  nonbind="$forcenb"
  if [ "$machine" = "1" ]; then
    local tmp; tmp=$(mktemp)
    set -f
    "${argv[@]}" > "$tmp" 2>/dev/null
    set +f
    local sha2; sha2=$(sha256sum "$tmp" | cut -d' ' -f1)
    if [ "$sha" = "$sha2" ]; then stable="true"; else stable="false"; nonbind=1; fi
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
  printf 'CAP %-34s rc=%s bytes=%-9s stable=%-5s nb=%s %s\n' \
    "$id" "$rc" "$bytes" "$stable" "$nonbind" "${sha:0:12}"
}

MACHINE_FMTS="json yaml bin timeseries"
ALL_ANALYZERS_HEAD="static/complexity,static/composition,static/comments,static/cohesion,static/halstead,static/imports,static/clones,history/anomaly,history/devs,history/couples,history/shotness"

# ===========================================================================
# 1. STATIC analyzers on the fast deterministic subset (each, all machine fmts)
# ===========================================================================
STATIC="static/clones static/cohesion static/comments static/complexity static/composition static/halstead static/imports"
echo "=== STATIC analyzers on subset ($SUBDIR), all machine + human formats ==="
for a in $STATIC; do
  base=$(printf '%s' "$a" | tr '/' '_')
  for f in json yaml bin; do
    capture "static.$base.$f" "$STATICDIR/$base.$f" 1 0 \
      "$a, format $f, subset (--head, single worker)" -- \
      "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format "$f" --head --workers 1 --static-workers 1 -p "$SUBDIR"
  done
  # compact + text are human-readable progress output (nonbinding).
  for f in compact text; do
    capture "static.$base.$f" "$STATICDIR/$base.$f" 0 1 \
      "$a, human format $f (cosmetic)" -- \
      "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format "$f" --head --workers 1 --static-workers 1 -p "$SUBDIR"
  done
  # per-file json variant (exposes nondeterminism explicitly; canonicalize to compare)
  capture "static.$base.perfile.json" "$STATICDIR/$base.perfile.json" 1 0 \
    "$a per-file json (per-file array ordering is Go-map-nondeterministic; canonicalize)" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --head --workers 1 --static-workers 1 --per-file -p "$SUBDIR"
done

# ===========================================================================
# 2. HISTORY analyzers on full repo, bounded.
# ===========================================================================
LIMIT=5
STREAM_LIMIT=10

# 2a. burndown: the keystone history analyzer; capture EVERY machine format.
echo "=== history/burndown: all machine formats (head + streaming) ==="
for f in json yaml bin timeseries; do
  capture "run.burndown.$f" "$RUNDIR/burndown.$f" 1 0 \
    "history/burndown $f, --head --limit $LIMIT" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format "$f" --head --limit "$LIMIT" "$KREPO"
done
# ndjson + timeseries+ndjson are STREAMING (empty in --head mode): --limit N --workers 1.
capture "run.burndown.ndjson" "$RUNDIR/burndown.ndjson" 1 0 \
  "history/burndown ndjson; streaming --limit $LIMIT --workers 1" -- \
  "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format ndjson --limit "$LIMIT" --workers 1 "$KREPO"
capture "run.burndown.timeseries_ndjson" "$RUNDIR/burndown.timeseries.ndjson" 1 0 \
  "history/burndown timeseries+ndjson (--format timeseries --ndjson); streaming --limit $LIMIT --workers 1" -- \
  "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format timeseries --ndjson --limit "$LIMIT" --workers 1 "$KREPO"
# human formats (nonbinding)
for f in text compact; do
  capture "run.burndown.$f" "$RUNDIR/burndown.$f" 0 1 \
    "history/burndown human $f (cosmetic)" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers history/burndown --format "$f" --head --limit "$LIMIT" "$KREPO"
done

# 2b. Other history analyzers, json (head-meaningful ones) + json (streaming ones).
echo "=== history analyzers, json (head-mode) ==="
for a in history/anomaly history/couples history/devs history/shotness; do
  base=$(printf '%s' "$a" | tr '/' '_')
  capture "run.$base.json" "$RUNDIR/$base.json" 1 0 \
    "$a json, --head --limit $LIMIT --workers 1" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --head --limit "$LIMIT" --workers 1 "$KREPO"
done
echo "=== history analyzers, json (streaming-mode) ==="
for a in history/imports history/quality history/sentiment history/typos history/file-history; do
  base=$(printf '%s' "$a" | tr '/' '_')
  capture "run.$base.json" "$RUNDIR/$base.json" 1 0 \
    "$a json; streaming --limit $STREAM_LIMIT --workers 1" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers "$a" --format json --limit "$STREAM_LIMIT" --workers 1 "$KREPO"
done

# 2c. devs: also yaml + bin + timeseries (representative multi-format history analyzer).
echo "=== history/devs: extra machine formats ==="
for f in yaml bin; do
  capture "run.history_devs.$f" "$RUNDIR/history_devs.$f" 1 0 \
    "history/devs $f, --head --limit $LIMIT --workers 1" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers history/devs --format "$f" --head --limit "$LIMIT" --workers 1 "$KREPO"
done

# ===========================================================================
# 3. ALL analyzers '*' on the fast subset (machine fmts) — combined report.
#    Full-repo '*' produces 250MB+ outputs and is dominated by nondeterministic
#    static sections; the subset is the practical combined golden.
# ===========================================================================
echo "=== ALL static '*' on subset, machine formats ==="
for f in json yaml bin; do
  capture "all_static.$f" "$RUNDIR/all_static.$f" 1 0 \
    "all static analyzers (static/*) $f on subset" -- \
    "$CODEFANG" run $RUN_DETERMINISM --analyzers 'static/*' --format "$f" --head --workers 1 --static-workers 1 -p "$SUBDIR"
done

# ===========================================================================
# 4. uast parse / analyze / query on the fixed file.
#    Verified supported formats:
#      parse:   json, compact, tree, none   (NOT yaml)
#      analyze: json, text, html            (NOT yaml)
#      query:   json, compact, count        (NOT yaml/text)
# ===========================================================================
echo "=== uast parse ($UFILE) ==="
capture "uast.parse.json" "$UASTDIR/parse.json" 1 0 \
  "uast parse json (blob $UFILE_BLOB)" -- \
  "$UAST" parse --format json "$UFILE"
capture "uast.parse.compact" "$UASTDIR/parse.compact" 1 0 \
  "uast parse compact (blob $UFILE_BLOB)" -- \
  "$UAST" parse --format compact "$UFILE"
capture "uast.parse.tree" "$UASTDIR/parse.tree" 0 1 \
  "uast parse tree (human tree view; cosmetic)" -- \
  "$UAST" parse --format tree "$UFILE"

echo "=== uast analyze ($UFILE) ==="
capture "uast.analyze.json" "$UASTDIR/analyze.json" 1 0 \
  "uast analyze json (blob $UFILE_BLOB)" -- \
  "$UAST" analyze --format json "$UFILE"
capture "uast.analyze.text" "$UASTDIR/analyze.text" 0 1 \
  "uast analyze text (human; cosmetic)" -- \
  "$UAST" analyze --format text "$UFILE"

echo "=== uast query ($UFILE) ==="
capture "uast.query.json" "$UASTDIR/query.json" 1 0 \
  "uast query 'filter(.roles has Function)' json (blob $UFILE_BLOB)" -- \
  "$UAST" query 'filter(.roles has "Function")' --format json "$UFILE"
capture "uast.query.compact" "$UASTDIR/query.compact" 1 0 \
  "uast query 'filter(.roles has Function)' compact (blob $UFILE_BLOB)" -- \
  "$UAST" query 'filter(.roles has "Function")' --format compact "$UFILE"
capture "uast.query.count" "$UASTDIR/query.count" 1 0 \
  "uast query 'reduce(count)' count (blob $UFILE_BLOB)" -- \
  "$UAST" query 'reduce(count)' --format count "$UFILE"

# ---- Assemble MANIFEST.json ----------------------------------------------------
python3 - "$RECORDS" "$GOLDEN/MANIFEST.json" "$HEAD" "$UFILE" "$UFILE_BLOB" "$UFILE_SHA" "$CODEFANG" "$UAST" "$KREPO" "$SUBDIR" <<'PY'
import json, sys, subprocess, datetime
recs, manifest_path, head, ufile, ublob, usha, cf, ua, krepo, subdir = sys.argv[1:11]
caps = [json.loads(l) for l in open(recs) if l.strip()]

def sh(c):
    try: return subprocess.check_output(c, shell=True, stderr=subprocess.DEVNULL).decode().strip()
    except Exception: return ""

binding = [c for c in caps if c["machine"] and not c["nonBinding"]]
nonbind = [c for c in caps if c["nonBinding"]]

manifest = {
  "description": "Byte-exact golden reference outputs for the Go codefang + uast tools, "
                 "for validating the Rust rewrite. Captures with machine=true AND "
                 "nonBinding=false are BINDING: the Rust port must reproduce them byte-for-byte. "
                 "nonBinding=true captures are either human formats (cosmetic) or machine formats "
                 "that are intrinsically nondeterministic in the Go tool (Go map-iteration order "
                 "in per-file/per-pair sections) and must be compared after canonicalization.",
  "generated_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "binaries": {
    "codefang": cf, "codefangSha256": sh(f"sha256sum {cf} | cut -d' ' -f1"),
    "uast": ua,     "uastSha256":     sh(f"sha256sum {ua} | cut -d' ' -f1"),
    "goVersion": sh("go version"),
    "buildVia": "make build (CGO_ENABLED=1, static libgit2 from third_party/libgit2/install)",
    "codefangVersionNote": "the `version` subcommand embeds the build timestamp via -ldflags and is "
                           "NOT deterministic; it is not a report and is not captured.",
  },
  "env": {"TZ":"UTC","NO_COLOR":"1","LANG":"C","LC_ALL":"C","SOURCE_DATE_EPOCH":"315532800"},
  "envNote": "Set exactly these vars AND use `set -f` (noglob) before each command so a literal "
             "'*'/'static/*' analyzer selector reaches the binary. Only STDOUT is captured; STDERR "
             "(timestamped progress logs) is discarded. Cross-run state is disabled via "
             "--checkpoint=false --resume=false --no-cache and wiping ~/.codefang/checkpoints.",
  "inputs": {
    "runRepo": krepo,
    "runHeadPinned": head,
    "staticSubset": subdir,
    "staticSubsetNote": "STATIC analyzers are nondeterministic and produce huge output on the full repo; "
                        "they are captured on this fixed 10-file subdirectory (passed via -p) bounded by "
                        "--head, single-worker. This is the canonical static golden ('fast deterministic "
                        "subset'). HISTORY analyzers run on the full repo bounded by --head/--limit.",
    "limitHeadSingle": 5,
    "limitStream": 10,
    "streamNote": "ndjson and timeseries+ndjson are STREAMING formats: one JSON line per commit, emitted "
                  "during the streaming pipeline and EMPTY in --head mode. They are captured with "
                  "--limit N --workers 1 (no --head), which is deterministic for a fixed HEAD.",
    "uastFile": ufile,
    "uastFileGitBlob": ublob,
    "uastFileSha256": usha,
    "uastFileNote": "uast parse/analyze/query operate on this file's working-tree bytes and embed its "
                    "ABSOLUTE path in the output; the Rust harness must invoke with the identical path.",
  },
  "formats": {
    "machineBinding": ["json","yaml","ndjson","timeseries","timeseries+ndjson","bin"],
    "humanNonBinding": ["text","plot","html","compact"],
    "runFormats": "codefang run --format: json yaml plot bin timeseries ndjson text compact (default json). "
                  "timeseries+ndjson = `--format timeseries --ndjson`.",
    "uastParseFormats": "uast parse --format: json compact tree none (default json). yaml NOT supported.",
    "uastAnalyzeFormats": "uast analyze --format: text json html (default text). yaml NOT supported.",
    "uastQueryFormats": "uast query --format: json compact count (default json). yaml/text NOT supported.",
    "compactNote": "run --format compact is a human progress-bar format (empty for combined '*' selections "
                   "and for history analyzers); treated as nonBinding.",
    "binNote": "bin is binary; outPath holds raw bytes and sha256 is the stable comparison hash. "
               "bin is STABLE for history analyzers but UNSTABLE for static analyzers (map ordering).",
    "plotHtmlNote": "run --format plot/html render multi-page HTML to a DIRECTORY (-o), not stdout; their "
                    "stdout golden would be empty. Not captured (nonBinding by nature)."
  },
  "nondeterminismNote": "The Go tool emits per-file (static) and per-pair (couples/shotness) sections in "
                        "Go map-iteration order, which is randomized per process. Sorted content is "
                        "identical across runs; only byte order differs. Such captures are recorded with "
                        "stable=false, nonBinding=true. The Rust harness must canonicalize (sort those "
                        "arrays by file_path / key) before comparing, OR the Rust port may emit them in a "
                        "deterministic (sorted) order, which is a correctness improvement over Go.",
  "stabilityNote": "Every machine capture ran twice with identical env/argv (set -f). stable=true means "
                   "both runs produced an identical sha256; only those are BINDING.",
  "counts": {"total": len(caps), "binding": len(binding), "nonBinding": len(nonbind)},
  "captures": caps,
}
json.dump(manifest, open(manifest_path,"w"), indent=2)
open(manifest_path,"a").write("\n")
print(f"MANIFEST: {len(caps)} captures, {len(binding)} binding, {len(nonbind)} nonBinding")
PY

echo "=== DONE ==="
