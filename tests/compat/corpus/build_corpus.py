#!/usr/bin/env python3
"""
Content-addressed CORPUS builder  (SPEC: specs/go-compat-testing/SPEC.md §3.3, roadmap 3).

MINES real inputs the test author never hand-curated:

  * SOURCE FILES spanning MULTIPLE tree-sitter languages codefang supports
    (NOT just Go) -- one real file per language, taken from real local repos.
  * REAL GIT REPOS of differing size BEYOND kubernetes (hercules: Go ~1k commits,
    ioq3: C ~3.8k commits). kubernetes remains available as the large repo.

Each corpus entry is stored CONTENT-ADDRESSED (sha256 of the bytes) under
corpus/files/<sha>.<ext>, and every entry's PROVENANCE (origin path, repo, sha,
size, language, oracle-measured node count) is recorded in corpus/manifest.json.

This builder does NOT compare anything and does NOT re-derive expected output.
It only MINES + records. The oracle (oracle.py) is the sole source of truth and
is invoked here ONLY to (a) prove each mined file actually parses on the LIVE Go
binary (so a dead cell is never silently admitted) and (b) record provenance.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys

ROOT = "/home/dmitriy/sources/codefang"
SRC = "/home/dmitriy/sources"
GO_UAST = os.path.join(ROOT, "build/bin/uast")
HERE = os.path.dirname(os.path.abspath(__file__))
FILES_DIR = os.path.join(HERE, "files")
MANIFEST = os.path.join(HERE, "manifest.json")

PINNED_ENV = {"TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
              "SOURCE_DATE_EPOCH": "315532800"}

# (language-id, extension, repo-root) -- repo roots are real local checkouts.
# The language-id is the codefang/uast language name; the oracle confirms parse.
LANG_SOURCES = [
    ("go",         "go",   f"{SRC}/hercules"),
    ("python",     "py",   f"{SRC}/nanoGPT"),
    ("c",          "c",    f"{SRC}/ioq3"),
    ("c-header",   "h",    f"{SRC}/ioq3"),
    ("rust",       "rs",   f"{SRC}/agent-client-protocol"),
    ("typescript", "ts",   f"{SRC}/onlook"),
    ("tsx",        "tsx",  f"{SRC}/onlook"),
    ("javascript", "js",   f"{SRC}/onlook"),
    ("json",       "json", f"{SRC}/agent-client-protocol"),
    ("yaml",       "yml",  f"{SRC}/hercules"),
    ("cpp",        "cpp",  f"{SRC}/llama.cpp"),
    ("shell",      "sh",   f"{SRC}/ioq3"),
]

# Real git repos for the history/repo-level matrix cells. Differing size/language
# BEYOND kubernetes (which stays as the large repo). hercules is the codefang
# lineage repo (Go); ioq3 is a large C repo with very different history shape.
REPOS = [
    {"id": "hercules", "path": f"{SRC}/hercules",
     "note": "Go, medium history (~1k commits) -- beyond kubernetes"},
    {"id": "ioq3", "path": f"{SRC}/ioq3",
     "note": "C, larger history (~3.8k commits), different language -- beyond kubernetes"},
    {"id": "kubernetes", "path": f"{SRC}/kubernetes",
     "note": "Go, very large history -- the reference large repo"},
]

EXCLUDE = ("node_modules/", "/build/", "/dist/", "/vendor/", "/.git/",
           "CMakeFiles", "/testdata/", ".min.")


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def oracle_parse_nodes(path):
    """Invoke the LIVE Go uast binary; return (bytes_len, node_count) or (0,0)."""
    env = dict(os.environ); env.update(PINNED_ENV)
    try:
        p = subprocess.run([GO_UAST, "parse", "--format", "json", path],
                           env=env, stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, timeout=120)
    except Exception:
        return 0, 0
    out = p.stdout
    return len(out), out.count(b'"type"')


def pick_source(ext, base):
    """First real source file 500..25000 bytes under base, excluding artifacts."""
    cands = []
    for dirpath, _, names in os.walk(base):
        if any(x in dirpath + "/" for x in EXCLUDE):
            continue
        for n in names:
            if not n.endswith("." + ext):
                continue
            fp = os.path.join(dirpath, n)
            if any(x in fp for x in EXCLUDE):
                continue
            try:
                sz = os.path.getsize(fp)
            except OSError:
                continue
            if 500 <= sz <= 25000:
                cands.append(fp)
    cands.sort()  # deterministic selection
    return cands[0] if cands else None


def git_commit_count(path):
    try:
        p = subprocess.run(["git", "-C", path, "rev-list", "--count", "HEAD"],
                           stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                           timeout=60)
        return int(p.stdout.strip() or 0)
    except Exception:
        return 0


def main():
    os.makedirs(FILES_DIR, exist_ok=True)
    manifest = {"files": [], "repos": [], "languages_supported_probed": []}

    print("== mining per-language source files (oracle-verified parse) ==")
    seen = set()
    for lang, ext, base in LANG_SOURCES:
        if not os.path.isdir(base):
            print(f"  SKIP {lang}: repo missing {base}")
            continue
        src = pick_source(ext, base)
        if not src:
            print(f"  SKIP {lang}: no {ext} file in size window under {base}")
            continue
        sha = sha256_file(src)
        blen, nodes = oracle_parse_nodes(src)
        if blen == 0 or nodes < 2:
            # the LIVE oracle produced no/trivial UAST: do NOT admit a dead cell.
            print(f"  DROP {lang}: oracle parse trivial ({blen}B nodes={nodes}) {src}")
            continue
        dst = os.path.join(FILES_DIR, f"{sha}.{ext}")
        if sha not in seen:
            shutil.copy2(src, dst)
            seen.add(sha)
        entry = {
            "id": f"file/{lang}",
            "language": lang,
            "ext": ext,
            "sha256": sha,
            "stored": os.path.relpath(dst, HERE),
            "size_bytes": os.path.getsize(src),
            "origin": src,
            "oracle_uast_bytes": blen,
            "oracle_uast_nodes": nodes,
        }
        manifest["files"].append(entry)
        manifest["languages_supported_probed"].append(lang)
        print(f"  OK   {lang:11s} {sha[:12]} {nodes:5d} nodes  {src}")

    print("\n== recording real git repos (provenance, beyond kubernetes) ==")
    for r in REPOS:
        if not os.path.isdir(os.path.join(r["path"], ".git")):
            print(f"  SKIP repo {r['id']}: not a git repo at {r['path']}")
            continue
        head = subprocess.run(["git", "-C", r["path"], "rev-parse", "HEAD"],
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                              timeout=30).stdout.decode().strip()
        n = git_commit_count(r["path"])
        rec = {"id": r["id"], "path": r["path"], "head_commit": head,
               "commit_count": n, "note": r["note"]}
        manifest["repos"].append(rec)
        print(f"  OK   repo {r['id']:12s} commits={n:7d} head={head[:12]} {r['note']}")

    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=2)
    print(f"\nwrote {MANIFEST}: {len(manifest['files'])} files, "
          f"{len(manifest['repos'])} repos")
    # at least one Go-stable language AND multiple languages required
    if len(manifest["files"]) < 5:
        print("ERROR: fewer than 5 languages mined", file=sys.stderr)
        sys.exit(1)
    if len(manifest["repos"]) < 3:
        print("ERROR: fewer than 3 repos recorded", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
