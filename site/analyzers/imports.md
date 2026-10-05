# Imports analyzer reference

The imports analyzer extracts **import and dependency information** from source
code using UAST parsing. It operates in both static mode (per-file analysis
aggregated over a tree) and history mode (tracking import usage per developer
over time).

For the conceptual model — what it measures and how the history-mode pipeline
works — see [Understanding import and dependency
analysis](../explanation/imports.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Running static analysis

```bash
codefang run -a static/imports --format json ./path/to/repo
codefang run -a static/imports --format json --per-file ./path/to/repo
codefang run -a static/imports --format yaml ./path/to/repo
codefang run -a static/imports --format json ./path/to/single/file.go
```

- The root may be a **directory** (walked recursively, `.git` and
  cache-tagged directories skipped, vendor/generated paths excluded by the
  path policy, `--languages` honoured) or a **single file**.
- Every supported file counts towards `Total Files`, including files with
  no imports.
- A root with no analyzable files produces the empty report (status
  `No import data available`, `Unique Imports` 0, `Total Files` 0) with
  exit code 0.
- Output is deterministic: two runs over the same tree emit identical bytes.

---

## What counts as one import

Each file contributes the set of modules, packages or files its import
statements name — once per file, in first-seen order. Aliases, specifier
lists and keywords never appear as imports. The aggregate value of an import
is the **number of files importing it**.

| Language | Statement | Reported import(s) |
|---|---|---|
| Go | `import "fmt"`, `str "strings"`, `. "math"`, `_ "embed"` | `fmt`, `strings`, `math`, `embed` |
| Python | `import os, sys`, `import numpy as np` | `os`, `sys`, `numpy` |
| Python | `from typing import List, Dict`, `from x import y as z` | `typing`, `x` |
| Python | `from . import sibling`, `from ..pkg.sub import thing` | `.sibling`, `..pkg.sub` |
| JavaScript / TypeScript | `import React from 'react'`, `import { a, b } from 'react'`, `import * as ns from 'ns-lib'`, `import './side.css'` | `react`, `ns-lib`, `./side.css` |
| JavaScript / TypeScript | `require('fs')`, `import('dyn-mod')`, `import fs = require("fs")`, `import type { T } from "./types"` | `fs`, `dyn-mod`, `./types` |
| Rust | `use std::collections::{HashMap, HashSet};`, `pub(crate) use std::io::{self, Read};` | `std::collections::HashMap`, `std::collections::HashSet`, `std::io`, `std::io::Read` |
| Rust | `use super::baz as qux;`, `use std::io::*;`, `extern crate alloc;` | `super::baz`, `std::io::*`, `alloc` |
| Java | `import java.util.List;`, `import java.util.*;`, `import static java.lang.Math.max;` | `java.util.List`, `java.util.*`, `java.lang.Math.max` |
| C / C++ | `#include <stdio.h>`, `#include "local.h"` | `stdio.h`, `local.h` (local file) |
| C# | `using System.IO;`, `using Alias = System.IO;`, `using static System.Math;` | `System.IO`, `System.IO`, `System.Math` |
| PHP | `use App\Models\User;`, `use App\Http\{A, B};`, `use Vendor\Thing as T;` | `App\Models\User`, `App\Http\A`, `App\Http\B`, `Vendor\Thing` |
| PHP | `require_once "x.php";`, `include "y.php";` | `x.php`, `y.php` (local files) |
| Ruby | `require "json"`, `require_relative "lib/x"` | `json`, `lib/x` (local file) |
| Kotlin | `import a.b.Thing as T`, `import kotlinx.coroutines.*` | `a.b.Thing`, `kotlinx.coroutines.*` |
| other languages | an import statement node | the generic statement rule: `from M import …` → `M`; `import … from 'm'` → `m`; `import a, b` → `a`, `b`; `use a::{b, c};` → `a::b`, `a::c`; any quoted target → the quoted string |

Not reported: re-exports (`export { x } from './m'`) and dynamic calls whose
argument is not a string literal (`require(name)`). Kotlin imports are read
back from the source span (the Kotlin grammar lowers imports without
tokens); both the static and the history path hand the extractor the file
bytes.

---

## Classification

The yaml and bin formats classify every unique import by the language of the
first file that imported it:

| Category | Rule |
|---|---|
| `relative` | `./x`, `../x`, `/x`, Python `.x`, Rust `crate::`/`super::`/`self::`, and local-file includes (C `"x.h"`, PHP `require`, Ruby `require_relative`) |
| `stdlib` | Go: first path element has no dot (`errors`, `net/http`) · Python: top-level module in `sys.stdlib_module_names` · JavaScript/TypeScript: `node:` prefix or a Node builtin (`fs`, `fs/promises`) · Rust: `std`, `core`, `alloc`, `proc_macro`, `test` · Java/Kotlin: `java.`, `javax.`, `jdk.`, `kotlin.` · C#: `System` · C/C++: standard and POSIX headers · Ruby: default libraries |
| `external` | everything else |

`external_ratio` is `external_imports / total_imports`.

---

## Configuration options

### Static mode

No configuration options. Uses UAST directly.

### History mode

No analyzer-specific options. The shared history flags apply:
`--tick-size` (the tick width in hours, default 24), the commit window
(`--head`, `--limit`, `--since`, `--first-parent`), the identity flags
(`--people-dict`, `--exact-signatures`, `--anonymize`) and the change
filter (`--languages`, vendor / generated policy). Blobs above 256 KiB are
not parsed.

---

## Example output

For a tree with `a.go` (`fmt`, `os`) and `b.py` (`import os, sys`):

=== "Static (JSON)"

    ```json
    {
      "overall_score_label": "Info",
      "sections": [
        {
          "title": "IMPORTS",
          "score_label": "Info",
          "status": "Found 3 unique imports",
          "metrics": [
            { "label": "Unique Imports", "value": "3" },
            { "label": "Total Files", "value": "2" }
          ],
          "issues": [
            { "name": "os", "location": "", "value": "2", "severity": "info" },
            { "name": "fmt", "location": "", "value": "1", "severity": "info" },
            { "name": "sys", "location": "", "value": "1", "severity": "info" }
          ],
          "score": -1
        }
      ],
      "overall_score": -1
    }
    ```

    The section is info-only (`score` -1). `issues` lists every unique
    import ordered by the number of importing files (descending), then by
    name. With `--per-file`, a `files` array follows `issues` with one entry
    per analyzed file:

    ```json
    {
      "file_path": "b.py",
      "score_label": "Info",
      "status": "Found 2 unique imports",
      "metrics": [
        { "label": "Unique Imports", "value": "2" },
        { "label": "Total Files", "value": "1" }
      ],
      "issues": [
        { "name": "os", "location": "b.py", "value": "1", "severity": "info" },
        { "name": "sys", "location": "b.py", "value": "1", "severity": "info" }
      ],
      "score": -1
    }
    ```

=== "Static (YAML)"

    ```yaml
    import_list:
        - path: fmt
          category: stdlib
          is_external: false
        - path: os
          category: stdlib
          is_external: false
        - path: sys
          category: stdlib
          is_external: false
    categories:
        - category: stdlib
          count: 3
    dependencies: []
    aggregate:
        total_imports: 3
        external_imports: 0
        internal_imports: 3
        unique_packages: 3
        external_ratio: 0
    ```

    `import_list` is sorted by category then path; `categories` by count
    (descending) then name. `dependencies` flags deeply nested relative
    imports (three or more `..`, `MEDIUM`) and long paths (five or more `/`,
    `LOW`). The `bin` format carries the same structure in a CFB1 envelope.

=== "History (JSON)"

    ```bash
    codefang run -a history/imports --format json ./path/to/repo
    ```

    ```json
    {
      "tick_size": 86400,
      "author_index": ["alice|alice@x.io", "bob|bob@x.io"],
      "imports": {
        "alice|alice@x.io": {
          "go": {
            "fmt": { "0": 1, "2": 1 },
            "net/http": { "2": 1 },
            "os": { "0": 1 }
          }
        },
        "bob|bob@x.io": {
          "go": { "encoding/json": { "1": 1 }, "fmt": { "1": 1 } },
          "python": { "os": { "1": 1 }, "sys": { "1": 1 } }
        }
      }
    }
    ```

=== "History (YAML)"

    ```yaml
    codefang (v2):
      version: ...
      hash: ...
    ImportsPerDeveloper:
      tick_size: 86400
      author_index:
        - alice|alice@x.io
        - bob|bob@x.io
      imports:
        alice|alice@x.io:
          go:
            fmt:
              0: 1
              2: 1
            net/http:
              2: 1
            os:
              0: 1
        bob|bob@x.io:
          go:
            encoding/json:
              1: 1
            fmt:
              1: 1
          python:
            os:
              1: 1
            sys:
              1: 1
    ```

    The history report is one value for json, yaml and bin (the CFB1
    envelope over the json bytes):

    | Key | Description |
    |---|---|
    | `tick_size` | The tick width in seconds (`--tick-size` hours × 3600; `86400` by default) |
    | `author_index` | Developer labels by author id, as the identity detector merged them (`--people-dict` names, `--anonymize` pseudonyms) |
    | `imports` | `developer -> language -> import -> tick -> count`: how many times each commit of that developer, in a file of that language, in that tick, imported the path. Every Insert/Modify change's After version counts, so a file rewritten in two commits counts its imports twice. Languages are the parser's names (`go`, `python`, `javascript`, …). Tick keys are integers (decimal strings in json). |

    A history without any supported file is `{"tick_size": 86400,
    "author_index": [...], "imports": {}}`; a developer whose commits
    contain no imports does not appear under `imports`. Two runs over the
    same repository emit identical bytes.

    The per-commit `ndjson` records carry `{"authorID", "entries":
    [{"Lang", "Import"}]}` for every commit with at least one import; the
    `timeseries` document carries `import_count` and the per-language
    counts per commit under `imports-per-dev`.

---

## See also

- [Understanding import and dependency analysis](../explanation/imports.md) — the mental model, history-mode architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static and history analysis.
