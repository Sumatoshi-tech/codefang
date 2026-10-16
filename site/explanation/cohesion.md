# Understanding cohesion metrics

This page explains the mental model behind the cohesion analyzer: what
counts as a variable, what LCOM-HS measures, how the variable-sharing ratio
captures communicational cohesion, how scores are combined across files, and
how to read them. For the output schema, see the
[Cohesion reference](../analyzers/cohesion.md).

---

## What it measures

The cohesion analyzer computes **LCOM-HS (Henderson-Sellers)** per file and a
**variable sharing ratio** per function to identify files whose functions are
poorly related to each other — a strong signal for refactoring.

Both metrics are built on one relation: *which function uses which
variable*. Everything below follows from how that relation is extracted.

### What counts as a variable

A function *uses* a variable when its body contains an identifier that reads
or writes it. The analyzer works lexically on the UAST — no symbol
resolution — with these rules:

| Counted | Not counted |
|---|---|
| Local variables (`x := 1`, `let m = …`, `const total = …`) — by the declared **name**, never the whole statement | Type names (`int`, `string`, `Config`, `i32`, `Self`, `number`) |
| Parameters (`cfg`, `items`, `n`) | The function's own name and its return-type annotation |
| Fields and attributes (`s.a` counts `a`; `self.x` counts `x`; `this.b` counts `b`) | Callees: `helper()`, `string(v)`, the `Println` in `fmt.Println(x)`, the `Validate` in `cfg.Validate()` |
| The object of a member access (`cfg` in `cfg.Name`, `fmt` in `fmt.Println`) | The receiver: `self`, `this`, and a Go receiver such as `s` in `func (s *S) M()` |
| Variables of a closure, attributed to the enclosing function | Literals (`1`, `"x"`, `nil`, `true`) |

Each function's variables form a **set**: using `x` three times is one use of
`x`.

!!! note "Lexical means lexical"
    `fmt` in `fmt.Println(x)` is indistinguishable from a variable without
    symbol resolution, so it counts. Two different variables that share a
    name across functions count as the same variable. Both are documented
    limitations, not bugs.

### LCOM-HS (lack of cohesion of methods — Henderson-Sellers)

LCOM-HS is the industry-standard cohesion metric used by NDepend, JArchitect,
and CppDepend.

**Formula (per file):**

```text
LCOM = 1 - sum(mA) / (m × a)
```

Where:

- `m` = number of functions in the file **that use at least one variable**
- `a` = number of distinct variables across those functions
- `mA` = for each variable, the number of functions that use it
- `sum(mA)` = the number of (function, variable) usage pairs

**Range:** 0.0 (perfect cohesion: every function uses every variable) to
1.0 (no cohesion: no two functions share a variable).

Functions that use no variables are not part of the usage matrix (there is
nothing they could share), so trivial helpers never penalize a file. When
fewer than two functions use a variable — a single-function file, or a file
of parameterless helpers — there is nothing to compare, and `LCOM = 0`.

**Worked example** — `f` uses `{x, y}`, `g` uses `{x}`:

```text
m = 2, a = 2 ({x, y}), sum(mA) = 2 (x) + 1 (y) = 3
LCOM = 1 - 3 / (2 × 2) = 0.25
```

### Cohesion score

A convenience inversion of LCOM-HS: `cohesion_score = 1.0 - LCOM`. Higher is
better. It drives the section score (`6/10` means `0.6`) and the summary
message:

| Cohesion score | Summary |
|---|---|
| ≥ 0.7 | Excellent overall cohesion across all analyzed code |
| ≥ 0.4 | Good overall cohesion with room for improvement |
| ≥ 0.3 | Fair overall cohesion - consider refactoring some functions |
| < 0.3 | Poor overall cohesion - significant refactoring recommended |

### Function-level cohesion (variable sharing ratio)

For each function, the analyzer computes what fraction of its variables is
shared with **at least one other function in the same file**. This measures
**communicational cohesion** (Yourdon-Constantine level 5).

**Formula:**

```text
function_cohesion = shared_vars / unique_vars
```

Where:

- `unique_vars` = the function's distinct variables
- `shared_vars` = those of them that at least one *other* function of the
  file also uses (a repeat inside the same function is not sharing)
- Functions with no variables score 1.0 (trivial, no penalty)
- Every function of a file with fewer than two variable-using functions
  scores 1.0 (nothing to compare against)

**Range:** 0.0 (completely isolated function) to 1.0 (all variables shared).

| Function cohesion | Assessment |
|---|---|
| ≥ 0.6 | Excellent — function shares most variables with the module |
| ≥ 0.4 | Good — reasonable sharing with room for improvement |
| ≥ 0.3 | Fair — consider refactoring |
| < 0.3 | Poor — function is isolated from the module |

Functions below **Excellent** are listed as issues, worst first. The
distribution counts every function in exactly one of the four buckets, so it
always sums to **Total Functions**.

In the worked example above, `f` shares `x` but not `y` (`0.5`, Good) and `g`
shares its only variable (`1.0`, Excellent).

### Across files

A variable is a per-file notion, so LCOM is never recomputed over a whole
tree. Instead:

- **LCOM Score** = the function-weighted mean of the per-file LCOM values (a
  file with five functions weighs five times a file with one);
- **Cohesion Score** = `1 - LCOM Score`;
- **Avg Cohesion** = the mean per-function cohesion over every function;
- **Total Functions** = every function found. Same-named functions are kept
  (methods are reported as `Type.name`, e.g. `A.String` and `B.String`).

Files without functions — struct-only files, empty files, Markdown — carry
**no cohesion data**: they are excluded from every denominator. A tree in
which no file has a function reports `No cohesion data available` with score
`0/10` (and exit code 0). Use `--per-file` to see each file's own LCOM.

---

## Per-language notes

- **Go** — `type_identifier` nodes (`int`, `Config`) are typed and excluded;
  receivers are excluded and name the method (`S.M1`); closures (`func()
  {…}` literals) belong to the enclosing function.
- **Python** — `self` is excluded; methods are qualified by their class
  (`C.m1`); decorators are outside the function. Type annotations are plain
  identifiers in Python's UAST and therefore count.
- **JavaScript / TypeScript** — `this` is excluded; class methods are
  qualified (`A.m1`); TypeScript type annotations are excluded.
- **Java** — `this` is excluded; methods, including constructors, are
  qualified by their class (`A.m1`, `A.A`); primitive types are excluded.
- **Rust** — `self` is excluded; `impl` blocks do not qualify method names
  (`m1`, not `S.m1`). Identifier nodes in the Rust mapping carry no token;
  their names are recovered from the enclosing function's source span.
- Any language supported by the UAST works; results are best where variable
  declarations and function definitions are explicit.

---

## Use cases

- **Refactoring targets**: Find functions with low sharing ratio that do unrelated work and should be split.
- **Architecture reviews**: Validate that files follow the Single Responsibility Principle by checking LCOM-HS scores.
- **Code quality tracking**: Monitor cohesion trends over time to catch degradation.
- **Code review**: Use per-function cohesion scores to identify newly added functions that lack cohesion with existing code.

---

## Methodology references

- **LCOM-HS**: Henderson-Sellers, B. (1996). *Object-Oriented Metrics: Measures of Complexity*. Prentice Hall. The Henderson-Sellers variant normalizes LCOM to [0, 1] and is the variant used by NDepend, JArchitect, and CppDepend.
- **Variable Sharing Ratio**: Measures communicational cohesion per the Yourdon-Constantine classification. A function that shares all its variables with other functions in the module exhibits high communicational cohesion.

---

## Limitations

- **Lexical names**: The analyzer uses identifier spellings. Different variables with the same name across functions count as shared; package qualifiers (`fmt`) count as variables.
- **Constructors and initializers**: A constructor that initializes every field (`S { a: 1, b: 2 }`, `this.a = …`) uses every field and ties the other methods together, raising the file's cohesion.
- **Accessor methods**: Simple getters/setters may inflate cohesion since each touches only one shared field.
- **Single-function files**: Files with only one function always receive perfect cohesion (LCOM = 0.0, cohesion = 1.0) since there are no other functions to compare against.
- **Trivial functions**: Functions with no variables receive a cohesion score of 1.0 to avoid penalizing simple utility functions.

---

## See also

- [Cohesion reference](../analyzers/cohesion.md) — output schema and thresholds.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.
