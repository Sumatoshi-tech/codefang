# Analyzers

Codefang ships two families of analyzers that extract quantitative insights from your codebase.

**Static analyzers** operate on source code via UAST (Universal Abstract Syntax Tree) and evaluate structural properties of the code as it exists right now. **History analyzers** walk Git commit history and track how the codebase evolves over time.

---

## Static Analyzers

Static analyzers parse source files into a UAST and compute metrics in a single pass. They require no repository history and can run on individual files or entire directory trees.

| Analyzer | ID | Description |
|---|---|---|
| [Complexity](complexity.md) | `complexity` | Cyclomatic complexity, cognitive complexity, nesting depth |
| [Cohesion](cohesion.md) | `cohesion` | LCOM4 class cohesion, method-field usage graphs |
| [Halstead](halstead.md) | `halstead` | Program length, vocabulary, volume, difficulty, effort |
| [Comments](comments.md) | `comments` | Documentation coverage, comment placement quality |
| [Imports](imports.md) | `imports` | Import and dependency analysis |

### Running Static Analyzers

Pipe a UAST through `codefang analyze`:

```bash
# Single file, single analyzer
uast parse main.go | codefang analyze -a complexity

# Single file, all static analyzers
uast parse main.go | codefang analyze

# Entire directory tree
codefang analyze -a complexity ./src/
```

---

## History Analyzers

History analyzers iterate over Git commits (oldest to newest) and accumulate statistics about how files, developers, and code structure change over time. They require a Git repository.

| Analyzer | ID | Description |
|---|---|---|
| [Burndown](burndown.md) | `history/burndown` | Code survival over time, line ownership tracking |
| [Developers](developers.md) | `history/devs` | Developer activity, language breakdown, bus factor |
| [Couples](couples.md) | `history/couples` | File coupling, co-change patterns |
| [File History](file-history.md) | `history/file-history` | Per-file lifecycle and modification tracking |
| [Sentiment](sentiment.md) | `history/sentiment` | Comment sentiment analysis over time |
| [Shotness](shotness.md) | `history/shotness` | Structural hotspots (function-level change tracking) |
| [Typos](typos.md) | `history/typos` | Typo detection dataset builder |
| [Anomaly](anomaly.md) | `history/anomaly` | Z-score temporal anomaly detection |

### Running History Analyzers

Use `codefang run` with the `-a` flag:

```bash
# Single analyzer
codefang run -a history/burndown .

# Multiple analyzers
codefang run -a history/devs -a history/couples .

# All history analyzers
codefang run -a 'history/*' .
```

---

## Glob Patterns

You can select analyzers using glob patterns with the `-a` flag:

```bash
# All static analyzers
codefang analyze -a 'static/*' ./src/

# All history analyzers
codefang run -a 'history/*' .

# All analyzers (both static and history)
codefang run -a '*' .
```

---

## Output Formats

All analyzers support multiple output formats via the `-f` flag:

=== "JSON"

    ```bash
    codefang run -a history/devs -f json .
    ```

=== "YAML"

    ```bash
    codefang run -a history/devs -f yaml .
    ```

=== "Plot"

    ```bash
    codefang run -a history/burndown -f plot .
    ```

=== "Binary"

    ```bash
    codefang run -a history/burndown -f binary .
    ```

!!! tip "Combining analyzers"
    When running multiple history analyzers together, they share the same commit walk, making combined runs significantly faster than running each analyzer separately.
