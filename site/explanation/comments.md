# Understanding documentation coverage

This page explains the mental model behind the comments analyzer: what
documentation coverage means, how a comment is attached to a declaration, and
how the score is built. For the output schema, see the
[Comments reference](../analyzers/comments.md).

---

## What it measures

The comments analyzer looks at two things in every parsed file:

- **Documentation coverage** — the share of declarations (functions, methods,
  classes, interfaces, structs) that carry a doc comment.
- **Comment placement** — where each comment sits: directly above a
  declaration, inside a body, or attached to nothing.

Both are summed over the whole analyzed tree: coverage is
*documented declarations / all declarations* and the good-comment ratio is
*good comments / all comments*. A file without declarations (a Markdown
README, a file of constants) adds nothing to either denominator, so it cannot
dilute the result.

### Which comments document a declaration

A comment block documents a declaration when it **ends on the line directly
above the declaration**. There may be no blank line in between. Attribute and
decorator lines (`#[derive(...)]`, `@dataclass`, annotations) belong to the
declaration, so a doc comment above them still counts.

=== "Go"

    ```go
    // Add adds.            ← above Add: documents it
    func Add(a, b int) int {
        // inline comment   ← inline (inside Add)
        return a + b
    }

    // Mul multiplies.      ← unassociated: a blank line separates it
                            
    func Mul(a, b int) int { return a * b }
    ```

=== "Rust"

    ```rust
    /// Documents foo.      ← above foo
    #[inline]               ← attribute lines belong to the declaration
    pub fn foo() -> i32 { 1 }

    //! Crate docs          ← unassociated unless directly above an item
    ```

=== "Python"

    ```python
    # documents K           ← above K, through the decorator line
    @dataclass
    class K:
        """Class docstring."""   ← docstring: documents K

        # documents m       ← above m (inside K's span, but directly above m)
        def m(self): ...
    ```

=== "Java"

    ```java
    /** Javadoc for A. */   ← above A
    public class A {
        /** Javadoc for foo. */  ← above foo
        public int foo() {
            return 1; // trailing   ← inline (inside foo)
        }
    }
    ```

Adjacent comment lines form one block, so a multi-line `//` comment counts as
several comments that share one placement. Comment text is not analyzed:
`// do the thing` documents a function as well as a paragraph does.

### Placement categories

| Placement | Meaning | Counts as |
|---|---|---|
| `above` | Ends on the line directly above a declaration (attribute lines included) | good — documents the declaration |
| `inline` | Lies inside a declaration's span; attributed to the innermost one | bad |
| `unassociated` | Everything else: file headers, license blocks, comments after the last declaration, comments separated from a declaration by a blank line | bad |

### Score and status

The section score is the good-comment ratio, shown as `N/10`, and the status
is its threshold label:

| Good ratio | Status |
|---|---|
| `>= 0.8` | Excellent comment quality and placement |
| `>= 0.6` | Good comment quality with room for improvement |
| `>= 0.4` | Fair comment quality - consider improving placement |
| below | Poor comment quality - significant improvement needed |

A tree with neither comments nor declarations reports `No comments found`.
Coverage is reported alongside the score (`Doc Coverage`) and drives the
Documented / Undocumented distribution and the issue list; it is not folded
into the score.

---

## Use cases

- **Documentation enforcement**: require a minimum `Doc Coverage` in CI.
- **Onboarding assessment**: measure how well-documented a codebase is before
  new team members join.
- **API surface quality**: the issue list names every undocumented
  declaration with its file; `--per-file` breaks it down per file.
- **Placement hygiene**: a low good ratio with high coverage means most
  comments live inside bodies or float between declarations.

---

## Limitations

- **Comment quality**: presence and placement are measured, not content.
- **Docstrings**: a Python docstring (the string that is the first statement
  of a `def` / `class` body) is a `Comment` with the `Doc` role in the UAST
  and documents its declaration (placement `docstring`). A string literal
  that appears later in a body is an `inline` comment of that body, and a
  module docstring is `unassociated`.
- **Declaration names**: a declaration's name is its UAST `name` property or
  its name identifier; a declaration the mapping leaves nameless — an
  anonymous function expression, for instance — is reported as `unknown`
  (a nameless class / struct / interface node is not counted at all).
  Same-named declarations are tracked separately, so two `String` methods on
  different receivers are documented independently.
- **Unparsable files**: a file with a supported extension that the parser
  cannot handle (Markdown) is listed as an empty file with zero counts.
- **Auto-generated docs**: comments generated by tools (protobuf stubs) count
  like hand-written ones.
- **Attribute roles**: attribute / decorator lines bridge the gap only when
  the language mapping gives them the `Attribute` role (Rust attributes and
  Python decorators do).

---

## See also

- [Comments reference](../analyzers/comments.md) — output schema and
  definitions.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.
