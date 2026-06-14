# cf-uast-mappings

The **mapping system of record**: one Rust module per language, each defining the
tree-sitter → UAST mapping table as a `static LanguageMapping` via the
[`uast_language!`](../cf-uast-mapping/src/macros.rs) macro. The loader
(`cf-uast`) feeds the lowering from this registry — no DSL text is parsed on the
analysis path.

Provenance: the 68 modules were transpiled mechanically from the Go-era
`.uastmap` DSL corpus and are **equality-gated** against the DSL parser
(`tests/equality_gate.rs` asserts exact `Vec<Rule>` + `LanguageInfo` equality per
language), which is what makes the cutover byte-identical. The vendored
`.uastmap` files in `cf-uast-uastmaps` remain as a frozen snapshot serving the
dev server's text endpoints and the gate; an intentional mapping change here
must update that snapshot or retire the gate — the gate fails on any divergence,
so drift cannot be silent.

## Adding a language

1. Vendor the tree-sitter grammar and wire it, exactly as before this crate
   existed: C sources under `cf-uast/vendor/tree-sitter-<lang>-<ver>/`, a
   `build.rs` `GRAMMARS` entry, an `ffi` extern and `match` arm in
   `cf-uast/src/languages.rs`.
2. Add `src/<lang>.rs` with a `uast_language!` table (see the syntax below) and
   a `pub static <LANG>: LanguageMapping`.
3. Register it: one `mod <lang>;` line and one `("<lang>", &<lang>::<LANG>)`
   entry in `ALL` in `src/lib.rs` — keep `ALL` sorted by stem (a test asserts
   it).
4. Remove the language from the conformance test's `UNWIRED` list if you wired
   its grammar (the test fails until you do), then run
   `cargo test -p cf-uast-mappings` — the conformance test validates every
   pattern against the linked grammar.

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
        qualified_type ("(qualified_type package: (package_identifier) @pkg)") => {
            type: Synthetic,
            token: capture("pkg"),
            props: { "kind": "qualified" },
        },
    }
};
```

`extends:`/`when:` (rule inheritance and conditions) are supported but unused by
the entire corpus.

## Validation model

- **Compile time:** `type`/`roles` are closed-enum variants (62/61 values from
  the corpus); a typo is a compile error. Malformed invocations fail with
  diagnostics pinned by the `trybuild` suite in `cf-uast-mapping`.
- **CI time, not compile time:** S-expression **patterns** are string literals;
  `tests/grammar_conformance.rs` compiles every pattern as a
  `tree_sitter::Query` against each wired grammar (which also proves the node
  kind exists). 679 inherited dead rules (anonymous-token / hidden-rule /
  version-drift kinds that have never matched, in Go either) are acknowledged in
  its allow-list; a dead rule that starts compiling fails the test until its
  entry is removed.
- **Equality gate:** `tests/equality_gate.rs` pins every table against the DSL
  snapshot.

## Escape hatch

The macro is sugar over plain `MappingRule` literals — any rule (or a whole
module) may be written as `const` data directly. This escape hatch is
load-bearing in the generated corpus: six languages (python, rust, latex,
markdown, markdown_inline, rust_with_rstml) contain a rule named `_`, which
`$name:ident` cannot match, so those modules are emitted in plain-literal form.
`UastType::Other(..)`/`Role::Other(..)` exist for out-of-vocabulary values;
generated code is `Other`-free (asserted by a test), and any hand-written use is
expected to justify itself in review.
