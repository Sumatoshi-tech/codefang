//! Compiles the vendored tree-sitter grammar C sources into `cf-uast`.
//!
//! The grammar sources under `vendor/tree-sitter-<lang>-<ver>/` are copied
//! verbatim from `go-sitter-forest` at the **exact** grammar revision the
//! reference build links (see each vendored `grammar.json`). Because
//! tree-sitter node kinds and spans flow into machine report bytes (DESIGN
//! §5), using the identical grammar revision is what keeps `uast parse
//! --format json` byte-identical (pinned by tests/compat).
//!
//! Each grammar exports a `tree_sitter_<lang>()` C entry point (some ship an
//! external `scanner.c` as well). All sources compile into a single static
//! archive that the FFI bindings in `languages.rs` call.

use std::path::Path;

/// One vendored grammar: the directory under `vendor/` and its `parser.c` plus
/// any `scanner.c`. Listed explicitly so adding a language is a one-line edit.
struct Grammar {
    /// Directory name under `vendor/`.
    dir: &'static str,
    /// Source files to compile, relative to the grammar directory.
    sources: &'static [&'static str],
}

/// The vendored grammars compiled into this crate; new go-sitter-forest
/// grammars are added here as they are vendored.
const GRAMMARS: &[Grammar] = &[
    Grammar {
        dir: "tree-sitter-go-1.9.4",
        sources: &["parser.c"],
    },
    // HTML (go-sitter-forest html@v1.9.1): has an external scanner, so both
    // parser.c and scanner.c must be compiled. Vendored at the exact pinned
    // revision so the tree-sitter parse (and thus the UAST identifier
    // extraction the typos analyzer depends on) is byte-faithful.
    Grammar {
        dir: "tree-sitter-html-1.9.1",
        sources: &["parser.c", "scanner.c"],
    },
    // The compat-corpus grammars, each pinned to the exact go-sitter-forest
    // revision the reference build records (so node kinds/spans flow into
    // machine output byte-identically). `#include "..."` is a quote-include
    // that resolves to the source file's own directory first, so each grammar's
    // own parser.h/array.h/alloc.h/scanner.h are used even though all grammar
    // dirs share the same `-I` set.
    Grammar {
        dir: "tree-sitter-python-1.9.10",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-c-1.9.4",
        sources: &["parser.c"],
    },
    Grammar {
        dir: "tree-sitter-rust-1.9.13",
        sources: &["parser.c", "scanner.c"],
    },
    // typescript & tsx ship an external scanner whose scanner.c includes the
    // sibling scanner.h (quote-include, resolved per-grammar dir). Their external
    // scanner symbols are uniquely prefixed (tree_sitter_typescript_* vs
    // tree_sitter_tsx_*), so both link into one archive without clashing.
    Grammar {
        dir: "tree-sitter-typescript-1.9.4",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-tsx-1.9.2",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-javascript-1.9.2",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-json-1.9.1",
        sources: &["parser.c"],
    },
    // yaml's scanner.c #includes schema.core.c via the YAML_SCHEMA macro
    // (default `core`), so the schema is compiled transitively — do NOT list
    // schema.*.c as separate sources.
    Grammar {
        dir: "tree-sitter-yaml-1.9.6",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-cpp-1.9.5",
        sources: &["parser.c", "scanner.c"],
    },
    // bash is the grammar behind the corpus "shell" (.sh) language.
    Grammar {
        dir: "tree-sitter-bash-1.9.6",
        sources: &["parser.c", "scanner.c"],
    },
    // proto (go-sitter-forest proto@v1.9.1) and java (java@v1.9.5): both are
    // single-file grammars (parser.c, no external scanner) exporting
    // tree_sitter_proto()/tree_sitter_java(). Vendored at the exact pinned
    // revisions so node kinds/spans flow into machine output byte-identically
    // (e.g. comment/struct/interface nodes the comments analyzer counts in
    // .proto/.java sources).
    Grammar {
        dir: "tree-sitter-proto-1.9.1",
        sources: &["parser.c"],
    },
    Grammar {
        dir: "tree-sitter-java-1.9.5",
        sources: &["parser.c"],
    },
    // markdown_inline (go-sitter-forest markdown_inline@v1.9.3): the grammar
    // the loader dispatches `.md`/`.markdown` to (its UAST root is
    // `Synthetic`). Ships an external scanner (parser.c + scanner.c) exporting
    // tree_sitter_markdown_inline(). Vendored at the exact pinned revision so
    // the parse — and the per-file report COUNT the complexity aggregator
    // divides by — matches the reference reports.
    Grammar {
        dir: "tree-sitter-markdown_inline-1.9.3",
        sources: &["parser.c", "scanner.c"],
    },
    // cmake (go-sitter-forest cmake@v1.9.5): the grammar the loader
    // dispatches `.cmake`/`CMakeLists.txt` to. Ships an external scanner
    // (parser.c + scanner.c) exporting tree_sitter_cmake(). Vendored at the
    // exact pinned revision so the parse — and the per-file report COUNT that
    // the static aggregators (complexity/comments/halstead) divide by —
    // matches the reference reports on CMake-bearing repos (e.g. ioq3).
    Grammar {
        dir: "tree-sitter-cmake-1.9.5",
        sources: &["parser.c", "scanner.c"],
    },
    // Non-code corpus grammars (zero functions, but each is one parsed file
    // in the static aggregators' report-count divisor, so omitting them
    // shifts averaged metrics like complexity's "Cognitive Total"). Vendored
    // at the exact pinned go-sitter-forest revisions.
    Grammar {
        dir: "tree-sitter-xml-1.9.5",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-toml-1.9.2",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-perl-1.9.9",
        sources: &["parser.c", "scanner.c"],
    },
    Grammar {
        dir: "tree-sitter-gitignore-1.9.0",
        sources: &["parser.c"],
    },
    Grammar {
        dir: "tree-sitter-gitattributes-1.9.1",
        sources: &["parser.c"],
    },
    // PowerShell (go-sitter-forest powershell@v1.9.5): .ps1/.psm1. Has functions,
    // so omitting it under-counts the static aggregators' function totals on repos
    // with PowerShell (e.g. kubernetes cluster/gce/windows: 115 functions).
    Grammar {
        dir: "tree-sitter-powershell-1.9.5",
        sources: &["parser.c", "scanner.c"],
    },
];

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is always set by cargo");
    let vendor = Path::new(&manifest_dir).join("vendor");

    let mut build = cc::Build::new();
    build.warnings(false).flag_if_supported("-std=c11");

    let mut any = false;
    for grammar in GRAMMARS {
        let dir = vendor.join(grammar.dir);
        // The grammar directory is its own include root (parser.c does
        // `#include "parser.h"`, scanner.c includes the tree-sitter headers it
        // ships alongside).
        build.include(&dir);
        for src in grammar.sources {
            let path = dir.join(src);
            println!("cargo:rerun-if-changed={}", path.display());
            build.file(&path);
            any = true;
        }
        println!("cargo:rerun-if-changed={}", dir.join("parser.h").display());
    }

    if any {
        build.compile("cf_uast_grammars");
    }
}
