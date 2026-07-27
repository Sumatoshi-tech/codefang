#!/usr/bin/env python3
"""Summarize per-language function detection from oss-coverage sweep output.

Reads the per-repo --per-file JSON reports produced by run.sh, buckets files
by language (extension -> language from pkg/uast/uastmaps headers), and prints
files analyzed, files with detected functions, detection rate, and total
functions per language. Data/markup languages legitimately sit at 0%.
"""

import collections
import glob
import json
import os
import re
import sys

REPO_ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))


def load_extension_map():
    """Extension -> language, read from the cf-uast-mappings crate (the
    mapping system of record for the Rust binary)."""
    ext2lang = {}
    for path in glob.glob(os.path.join(REPO_ROOT, "crates/cf-uast-mappings/src/*.rs")):
        with open(path, encoding="utf-8") as fh:
            head = fh.read(4096)
        name = re.search(r'name:\s*"([^"]+)"', head)
        exts = re.search(r'extensions:\s*\[([^\]]*)\]', head)
        if not name:
            continue
        for ext in re.findall(r'"(\.[^"]+)"', exts.group(1) if exts else ""):
            ext2lang.setdefault(ext.lower(), name.group(1))
    return ext2lang


def main(out_dir):
    ext2lang = load_extension_map()
    stats = collections.defaultdict(lambda: {"files": 0, "files_with": 0, "funcs": 0, "zeros": []})

    for path in sorted(glob.glob(os.path.join(out_dir, "*.json"))):
        repo = os.path.basename(path)[:-5]
        try:
            with open(path, encoding="utf-8") as fh:
                report = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            print(f"WARN: unreadable report {repo}: {exc}", file=sys.stderr)
            continue

        for section in report.get("sections", []):
            if section.get("title") != "COMPLEXITY":
                continue
            for entry in section.get("files", []):
                metrics = {m["label"]: m["value"] for m in entry.get("metrics", [])}
                funcs = int(metrics.get("Total Functions", 0))
                lang = ext2lang.get(os.path.splitext(entry["file_path"])[1].lower())
                if not lang:
                    continue
                st = stats[lang]
                st["files"] += 1
                st["funcs"] += funcs
                if funcs > 0:
                    st["files_with"] += 1
                elif len(st["zeros"]) < 3:
                    st["zeros"].append(f"{repo}:{entry['file_path']}")

    print(f"{'language':<16}{'files':>6}{'w/funcs':>9}{'rate%':>7}{'funcs':>8}  zero-func examples")
    for lang in sorted(stats, key=lambda l: stats[l]["files_with"] / stats[l]["files"]):
        st = stats[lang]
        rate = 100 * st["files_with"] / st["files"]
        examples = "; ".join(st["zeros"][:2])
        print(f"{lang:<16}{st['files']:>6}{st['files_with']:>9}{rate:>7.0f}{st['funcs']:>8}  {examples[:70]}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")
