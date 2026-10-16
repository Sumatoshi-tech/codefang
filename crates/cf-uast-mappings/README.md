# cf-uast-mappings

The **mapping system of record**: one Rust module per language, each defining the
tree-sitter → UAST mapping table as a `static LanguageMapping` via the
[`uast_language!`](../cf-uast-mapping/src/macros.rs) macro. The loader
(`cf-uast`) feeds the lowering from this registry — no DSL text is parsed on the
analysis path.

Provenance: the 68 modules were transpiled mechanically from the Go-era
`.uastmap` DSL corpus. That corpus and the Go reference are retired, so the
tables are **hand-maintained** and the UAST shape is free to be correct rather
than byte-identical to the old output. Every deliberate shape change is pinned
by a test (see below) and documented in `site/architecture/uast.md` under
"Shape contract" — read that section before editing a rule, because the
analyzers depend on it.

## Adding a language

1. Vendor the tree-sitter grammar and wire it: C sources under
   `cf-uast/vendor/tree-sitter-<lang>-<ver>/`, a `build.rs` `GRAMMARS` entry,
   an `ffi` extern and `match` arm in `cf-uast/src/languages.rs`.
2. Add `src/<lang>.rs` with a `uast_language!` table (see the syntax below) and
   a `pub static <LANG>: LanguageMapping`.
3. Register it: one `mod <lang>;` line and one `("<lang>", &<lang>::<LANG>)`
   entry in `ALL` in `src/lib.rs` — keep `ALL` sorted by stem (a test asserts
   it).
4. Remove the language from the conformance test's `UNWIRED` list if you wired
   its grammar (the test fails until you do), then run
   `cargo test -p cf-uast-mappings` — the conformance test validates every
   pattern against the linked grammar.
5. Add the language to the operator/token table in `tests/shape_contract.rs`
   (`OPERATOR_ROWS`): a snippet with `a + b`, a logical AND, a unary minus, a
   plain and a compound assignment. The lowering supplies `props.operator` and
   identifier/literal tokens for free in most grammars; the row proves it.

## Rule syntax

The macro's full grammar lives in the
[`macros` module docs](../cf-uast-mapping/src/macros.rs) with a **doctested**
example — that doctest is the compile-checked reference. Summary (keys in
canonical order, all but `type` optional):

```text
pub static T: LanguageMapping = uast_language! {
    name: "t",
    extensions: [".t"],
    files: ["Tfile"],                       // exact-filename matches
    rules: {
        assignment_statement => {           // pattern defaults to "(assignment_statement)"
            type: Assignment,               // UastType variant — typo = compile error
            token: self,                    // or child("identifier") / capture("name")
            roles: [Assignment],            // Role variants
            children: ["expression_list"],
        },
        function_definition ("(function_definition declarator: (function_declarator declarator: (_) @name))") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            props: { "name": "@name" },     // @capture | descendant:<kind> | literal
        },
        type_spec_struct ("(type_spec)") => {   // several rules per node kind:
            type: Struct,                       // first whose `when` passes wins,
            roles: [Struct, Declaration],       // an unconditional rule is the catch-all
            when: ["type.type == \"struct_type\""],
        },
        arrow_function_in_declarator ("(arrow_function)") => {
            type: Synthetic,
            when: ["parent.type == \"variable_declarator\""],   // `parent` steps up
        },
    }
};
```

Prop values resolve as `@capture` (a query capture, falling back to a field or
descendant of that name), `descendant:<kind>` (the first descendant's text), or
a literal string. Condition paths navigate fields / child kinds with `.`, a
terminal `.type` compares the node kind, and a leading `parent` segment steps
to the enclosing tree-sitter node.

What the lowering adds on top of any table (so a rule need not spell it out):
`props.operator` on operator nodes, source-text tokens on `Identifier` /
`Literal`, leaf `Comment`s with line-exact spans, and the `Else` role on an
`If`'s `alternative` child. See `cf-uast/src/lowering.rs` ("Shape contract").

## Validation model

- **Compile time:** `type`/`roles` are closed-enum variants; a typo is a
  compile error. Malformed invocations fail with diagnostics pinned by the
  `trybuild` suite in `cf-uast-mapping`.
- **Grammar conformance:** S-expression **patterns** are string literals;
  `tests/grammar_conformance.rs` compiles every pattern as a
  `tree_sitter::Query` against each wired grammar (which also proves the node
  kind exists). Inherited dead rules (anonymous-token / hidden-rule /
  version-drift kinds that have never matched) are acknowledged in its
  allow-list; a dead rule that starts compiling fails the test until its
  entry is removed.
- **Shape contract:** `tests/shape_contract.rs` parses snippets through the
  real parser and asserts the lowered shape per language plus the
  cross-language operator/token table.
- **Schema vocabulary:** `tests/schema_vocabulary.rs` asserts every type and
  role any table (or the lowering) emits is a member of the embedded JSON
  schema's enums, so `uast validate` accepts `uast parse` output.

## Escape hatch

The macro is sugar over plain `MappingRule` literals — any rule (or a whole
module) may be written as `const` data directly. Six languages (python, rust,
latex, markdown, markdown_inline, rust_with_rstml) contain a rule named `_`,
which `$name:ident` cannot match, so those modules are written in
plain-literal form. `UastType::Other(..)`/`Role::Other(..)` exist for
out-of-vocabulary values; the tables are `Other`-free, and any hand-written
use must also appear in the JSON schema's enums (`tests/schema_vocabulary.rs`
rejects anything else) and justify itself in review.
