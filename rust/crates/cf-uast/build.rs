//! Compiles the vendored tree-sitter grammar C sources into `cf-uast`.
//!
//! The grammar sources under `vendor/tree-sitter-<lang>-<ver>/` are copied
//! verbatim from `go-sitter-forest` at the **exact** grammar revision the Go
//! `codefang`/`uast` build links (see each vendored `grammar.json`). Because
//! tree-sitter node kinds and spans flow into machine report bytes (DESIGN §5),
//! using the identical grammar revision is what makes `uast parse --format json`
//! byte-identical to the Go golden.
//!
//! Each grammar is a self-contained `parser.c` (the Go grammar has no external
//! scanner) exporting a `tree_sitter_<lang>()` C entry point. We compile them
//! into a single static archive that the FFI bindings in `languages.rs` call.

use std::path::Path;

/// One vendored grammar: the directory under `vendor/` and its `parser.c` plus
/// any `scanner.c`. Listed explicitly so adding a language is a one-line edit.
struct Grammar {
    /// Directory name under `vendor/`.
    dir: &'static str,
    /// Source files to compile, relative to the grammar directory.
    sources: &'static [&'static str],
}

/// The vendored grammars compiled into this crate. Start with GO only; the
/// remaining go-sitter-forest grammars are added here as they are vendored.
const GRAMMARS: &[Grammar] = &[
    Grammar {
        dir: "tree-sitter-go-1.9.4",
        sources: &["parser.c"],
    },
    // HTML (go-sitter-forest html@v1.9.1): has an external scanner, so both
    // parser.c and scanner.c must be compiled. Vendored at the exact revision
    // the Go build links so the tree-sitter parse (and thus the UAST identifier
    // extraction the typos analyzer depends on) is byte-faithful.
    Grammar {
        dir: "tree-sitter-html-1.9.1",
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
