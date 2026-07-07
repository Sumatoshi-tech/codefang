#!/usr/bin/env python3
"""
CLI SURFACE EXTRACTOR  (SPEC: specs/go-compat-testing/SPEC.md, Scope #1, Roadmap #2)

Recursively invokes `--help` on a LIVE binary (root + every subcommand) under the
pinned env and parses the help text into a NORMALIZED, format-agnostic surface
model:

    {
      "<cmd path>": {
        "subcommands": [name, ...],          # child command names
        "flags": { "--long": {short, takes_value, default}, ... },
        "positionals": [ "name", ... ],      # positional arg names (best-effort)
        "help_rc": int,                      # exit code of `<cmd> --help`
        "help_stream": "stdout"|"stderr",    # where help text was written
      }, ...
    }

WHY A MODEL, NOT RAW BYTES: Go uses cobra and Rust uses clap; the two render help
prose DIFFERENTLY by construction (section headers, wrapping, the literal "Print
help" line). A byte diff of help text would FAIL on cosmetic rendering and tell us
nothing about whether the SURFACE (the set of flags, their short names, defaults,
the positional args, the subcommand tree) is the same. The conformance contract is
therefore the structured surface; the comparator (cli_surface.py) diffs THIS model.

THE ORACLE IS THE LIVE GO BINARY. This program only ever EXECUTES the binaries; it
never re-derives what a flag/subcommand "should" be. Run Go to learn the truth.

This parser understands BOTH cobra help and clap help so the SAME model can be
extracted from either side and compared apples-to-apples.
"""

import json
import os
import re
import subprocess
import sys

GO_DIR = "/home/dmitriy/sources/codefang/build/bin"
RU_DIR = "/home/dmitriy/sources/codefang/target/release"

PINNED_ENV = {
    "TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
    "SOURCE_DATE_EPOCH": "315532800",
}


def run_help(exe, argv):
    """Run `exe <argv> --help` under pinned env. Return (rc, stdout, stderr)."""
    env = dict(os.environ)
    env.update(PINNED_ENV)
    p = subprocess.run([exe] + list(argv) + ["--help"], env=env,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       timeout=60)
    return (p.returncode,
            p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


# --------------------------------------------------------------------------- #
# Subcommand discovery: works for cobra ("Available Commands:") and clap
# ("Commands:"). We take command names only (first token of each indented line),
# dropping the synthetic "help"/"completion" rows handled by the comparator.
# --------------------------------------------------------------------------- #
def parse_subcommands(text):
    subs = []
    in_block = False
    for line in text.splitlines():
        if re.match(r"^(Available Commands|Commands):\s*$", line):
            in_block = True
            continue
        if in_block:
            if not line.strip():
                in_block = False
                continue
            if not line.startswith((" ", "\t")):
                in_block = False
                continue
            m = re.match(r"^\s{2,}(\S+)", line)
            if m:
                subs.append(m.group(1))
    return subs


# --------------------------------------------------------------------------- #
# Flag parsing. Cobra flag lines look like:
#     -a, --analyzers strings    help text (default ...)
#         --head                 help text
#   -h, --help                   help for run
# Clap flag lines look like (long form, with --help):
#   -a, --analyzers <analyzers>
#           help text
#   --format <format>
#           help text
#           [default: json]
#       --checkpoint [<checkpoint>]
#           [default: true]
#           [possible values: true, false]
# We extract per long flag: short letter (if any), whether it takes a value,
# and the default value if discoverable.
# --------------------------------------------------------------------------- #
_FLAG_RE = re.compile(
    r"""^\s*
        (?:-(?P<short>[A-Za-z]),\s+)?      # optional short
        --(?P<long>[A-Za-z0-9][A-Za-z0-9-]*)   # long name
        (?P<rest>.*)$
    """, re.VERBOSE)

# A REAL cobra default is a typed token: a quoted string, a number, a bool, or a
# bracketed list — e.g. (default "json"), (default 24), (default true),
# (default [all]). It is NOT free prose like "(default is $HOME/.uast.yaml)" which
# cobra emits inside a help SENTENCE. We therefore require the captured token to be
# a quoted string / number / bool / bracketed list, anchored to end-of-line.
_DEFAULT_COBRA = re.compile(
    r"""\(default\s+(
        "[^"]*"            # "json"
        | '[^']*'          # 'json'
        | \[[^\]]*\]       # [all]
        | -?\d+(?:\.\d+)?  # 24 / -1 / 1.5
        | true | false
    )\)\s*$""", re.VERBOSE)
_DEFAULT_CLAP = re.compile(r"\[default:\s*(.*?)\]")


def _norm_default(raw):
    if raw is None:
        return None
    raw = raw.strip()
    # clap quotes empty string default as "" — normalize to empty
    if raw == '""' or raw == "''":
        return ""
    # cobra arrays: [all] ; clap: all — strip surrounding brackets/quotes
    if raw.startswith("[") and raw.endswith("]"):
        raw = raw[1:-1]
    if (raw.startswith('"') and raw.endswith('"')) or \
       (raw.startswith("'") and raw.endswith("'")):
        raw = raw[1:-1]
    return raw


def parse_flags(text):
    """
    Returns {long: {"short": str|None, "takes_value": bool, "default": str|None}}.
    Handles cobra (single-line) and clap (flag line + indented continuation that
    may carry [default: ...]).
    """
    flags = {}
    lines = text.splitlines()
    # find the Flags/Options block(s); cobra: "Flags:" and "Global Flags:";
    # clap: "Options:". We scan ALL such blocks.
    block_starts = []
    for i, line in enumerate(lines):
        if re.match(r"^(Flags|Global Flags|Options):\s*$", line):
            block_starts.append(i)
    if not block_starts:
        return flags

    for bi in block_starts:
        j = bi + 1
        while j < len(lines):
            line = lines[j]
            if not line.strip():
                # blank line: end of block UNLESS next non-blank is still indented
                # (clap separates entries by blank lines). Peek.
                k = j + 1
                while k < len(lines) and not lines[k].strip():
                    k += 1
                if k >= len(lines) or not lines[k].startswith((" ", "\t")):
                    break
                if re.match(r"^[A-Za-z].*:\s*$", lines[k]):
                    break
                j = k
                continue
            # a new section header (e.g. another "Global Flags:") ends this block
            if re.match(r"^[A-Za-z][A-Za-z ]*:\s*$", line) and \
               not line.startswith((" ", "\t")):
                break
            m = _FLAG_RE.match(line)
            if not m:
                j += 1
                continue
            long = m.group("long")
            short = m.group("short")
            rest = m.group("rest")
            # detect value-taking: cobra has a type token (string/int/strings/...)
            # right after the long name; clap has <...> or [<...>] after it.
            takes_value = False
            if re.match(r"\s+(string|int|strings|float|float64|uint|duration|bytes)\b",
                        rest):
                takes_value = True
            if re.match(r"\s*\[?<", rest):
                takes_value = True
            # default: search this line and any indented clap continuation lines
            default = None
            md = _DEFAULT_COBRA.search(rest)
            if md:
                default = _norm_default(md.group(1))
            # clap continuation lines (indented further than the flag line)
            k = j + 1
            while k < len(lines):
                cont = lines[k]
                if not cont.strip():
                    break
                if _FLAG_RE.match(cont) and not cont.startswith("          "):
                    break
                if re.match(r"^[A-Za-z][A-Za-z ]*:\s*$", cont):
                    break
                mdc = _DEFAULT_CLAP.search(cont)
                if mdc:
                    default = _norm_default(mdc.group(1))
                if re.search(r"<", cont) and not takes_value:
                    pass
                k += 1
            flags[long] = {"short": short, "takes_value": takes_value,
                           "default": default}
            j += 1
    return flags


# --------------------------------------------------------------------------- #
# Positional args: from the Usage: line. cobra: "uast parse [files...] [flags]";
# clap: "Usage: codefang render [OPTIONS] <store-dir>". We extract the bracketed/
# angle tokens that are not OPTIONS/flags/command, normalizing the NAME only.
# --------------------------------------------------------------------------- #
def parse_positionals(text, path):
    """
    Extract positional args from the Usage line, normalized to a SHAPE descriptor
    per positional: {"variadic": bool, "required": bool}. We deliberately discard
    the NAME (cobra "path" vs clap "path_arg" vs "store-dir" are the same arg under
    different framework conventions) and keep only the comparable shape.

    `path` is the KNOWN command-path (binary-relative), e.g. ["diff"] or [] for
    root. Because the walker already knows the exact command path, we strip exactly
    (1 + len(path)) leading bare tokens (binary + each subcommand word) from the
    usage line — removing the cobra ambiguity where a bare subcommand word
    ("uast version [flags]") could otherwise be mistaken for a positional.

    cobra usage:  "uast diff file1 file2 [flags]"      bare-word positionals
                  "uast parse [files...] [flags]"       bracketed variadic
                  "codefang render <store-dir> [flags]"
                  "uast version [flags]"                NO positionals
    clap usage:   "Usage: codefang render [OPTIONS] <store-dir>"
                  "Usage: uast parse [files...] [flags]"
    """
    usage = None
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() == "Usage:":  # cobra block form: usage is on next non-blank
            for k in range(i + 1, len(lines)):
                if lines[k].strip():
                    usage = lines[k].strip()
                    break
            break
        m = re.match(r"^Usage:\s+(.*)$", line)  # clap inline form
        if m:
            usage = m.group(1).strip()
            break
    if not usage:
        return []

    placeholders = {"[flags]", "[OPTIONS]", "[command]", "[COMMAND]"}
    toks = usage.split()
    # strip leading command-path tokens: binary + every known subcommand word.
    strip_n = 1 + len(path)
    toks = toks[strip_n:]
    # whatever remains, minus placeholders, is positionals
    pos = []
    for t in toks:
        tt = t.strip()
        if tt in placeholders:
            continue
        variadic = "..." in tt
        required = tt.startswith("<")
        core = re.sub(r"[\[\]<>.]", "", tt)
        if not core:
            continue
        pos.append({"variadic": variadic, "required": required})
    return pos


def detect_help_stream(rc, out, err):
    """Where did --help write? Both frameworks write success help to stdout."""
    if out.strip():
        return "stdout"
    if err.strip():
        return "stderr"
    return "none"


# --------------------------------------------------------------------------- #
# Recursive walk of the command tree.
# --------------------------------------------------------------------------- #
def walk(exe, path, surface, seen):
    key = " ".join(path) if path else "(root)"
    if key in seen:
        return
    seen.add(key)
    rc, out, err = run_help(exe, path)
    text = out if out.strip() else err
    subs = parse_subcommands(text)
    surface[key] = {
        "subcommands": sorted(subs),
        "flags": parse_flags(text),
        "positionals": parse_positionals(text, path),
        "help_rc": rc,
        "help_stream": detect_help_stream(rc, out, err),
    }
    for s in subs:
        # don't recurse into the synthetic cobra "help" command (clap has none)
        if s in ("help",):
            continue
        walk(exe, path + [s], surface, seen)


def extract(exe):
    surface = {}
    walk(exe, [], surface, set())
    return surface


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("go", "rust"):
        print("usage: extract_surface.py {go|rust} {codefang|uast} [--out FILE]",
              file=sys.stderr)
        sys.exit(2)
    side, binname = sys.argv[1], sys.argv[2]
    base = GO_DIR if side == "go" else RU_DIR
    exe = os.path.join(base, binname)
    surface = extract(exe)
    out_path = None
    if "--out" in sys.argv:
        out_path = sys.argv[sys.argv.index("--out") + 1]
    blob = json.dumps({"side": side, "bin": binname, "surface": surface},
                      indent=2, sort_keys=True)
    if out_path:
        with open(out_path, "w") as f:
            f.write(blob)
    else:
        print(blob)


if __name__ == "__main__":
    main()
