//! Per-language extraction over the trees the production parser produces.
//!
//! Each test parses a small source with `cf_uast::Parser` (the same facade
//! the CLI uses) and asserts the exact import list, so every rule in
//! `cf_imports::extract` is pinned to a real tree shape rather than a
//! hand-built one.

use cf_imports::{extract_imports, Import};
use cf_uast::Parser;

/// Parses `src` as `file` and extracts its imports with the parser's own
/// language name and the source bytes.
fn imports(file: &str, src: &str) -> Vec<Import> {
    let parser = Parser::new();
    let root = parser
        .parse(file, src.as_bytes())
        .unwrap_or_else(|e| panic!("parse {file}: {e}"));
    extract_imports(&root, &parser.get_language(file), src.as_bytes())
}

/// The import paths of `src`, in extraction order.
fn paths(file: &str, src: &str) -> Vec<String> {
    imports(file, src).into_iter().map(|i| i.path).collect()
}

/// The `(path, local_file)` pairs of `src`.
fn flagged(file: &str, src: &str) -> Vec<(String, bool)> {
    imports(file, src)
        .into_iter()
        .map(|i| (i.path, i.local_file))
        .collect()
}

#[test]
fn go_specs_yield_the_quoted_path_without_alias_dot_or_blank() {
    let src = "package main\n\nimport \"fmt\"\n\nimport (\n\t\"os\"\n\tstr \"strings\"\n\t. \"math\"\n\t_ \"embed\"\n\t\"fmt\"\n\t\"github.com/example/lib/v2\"\n\t\"github.com/example/lib/v2\"\n\t\"../relative/pkg\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args, str.ToUpper(\"x\"), Pi, lib.X)\n}\n";
    assert_eq!(
        paths("a.go", src),
        [
            "fmt",
            "os",
            "strings",
            "math",
            "embed",
            "github.com/example/lib/v2",
            "../relative/pkg",
        ]
    );
}

#[test]
fn python_statements_yield_each_module_once_without_aliases() {
    let src = "import os, sys\nimport os.path\nfrom typing import List, Dict\nfrom x import y as z\nfrom . import sibling\nfrom ..pkg.sub import thing\nimport os\nimport numpy as np\n";
    assert_eq!(
        paths("a.py", src),
        [
            "os",
            "sys",
            "os.path",
            "typing",
            "x",
            ".sibling",
            "..pkg.sub",
            "numpy"
        ]
    );
}

#[test]
fn javascript_counts_modules_not_specifiers_and_sees_require_and_dynamic_import() {
    let src = "const fs = require('fs');\nconst path = require(\"path\");\nimport React from 'react';\nimport { useState, useEffect } from 'react';\nimport * as ns from 'ns-lib';\nimport './side-effect.css';\nimport def, { named } from './local/mod';\nasync function f() { const m = await import('dyn-mod'); return m; }\n";
    assert_eq!(
        paths("a.js", src),
        [
            "fs",
            "path",
            "react",
            "ns-lib",
            "./side-effect.css",
            "./local/mod",
            "dyn-mod"
        ]
    );
}

#[test]
fn typescript_type_imports_and_import_equals_require() {
    let src = "import type { T } from \"./types\";\nimport fs = require(\"fs\");\nimport { readFile } from \"node:fs/promises\";\nimport * as path from \"path\";\nimport express from \"express\";\nexport { x } from \"./reexport\";\n";
    assert_eq!(
        paths("a.ts", src),
        ["./types", "fs", "node:fs/promises", "path", "express"]
    );
}

#[test]
fn rust_use_trees_expand_to_leaf_paths() {
    let src = "use std::collections::{HashMap, HashSet};\nuse serde::Serialize;\nuse crate::foo::bar;\nuse super::baz as qux;\nuse std::io::*;\nextern crate alloc;\npub(crate) use std::io::{self, Read};\n\nfn main() {}\n";
    assert_eq!(
        paths("a.rs", src),
        [
            "std::collections::HashMap",
            "std::collections::HashSet",
            "serde::Serialize",
            "crate::foo::bar",
            "super::baz",
            "std::io::*",
            "alloc",
            "std::io",
            "std::io::Read",
        ]
    );
}

#[test]
fn java_imports_keep_wildcards_and_static_members() {
    let src = "package a;\n\nimport java.util.List;\nimport java.util.*;\nimport static java.lang.Math.max;\nimport static java.lang.Math.*;\nimport java.util.List;\n\npublic class A {}\n";
    assert_eq!(
        paths("A.java", src),
        [
            "java.util.List",
            "java.util.*",
            "java.lang.Math.max",
            "java.lang.Math.*"
        ]
    );
}

#[test]
fn c_includes_distinguish_system_and_local_headers() {
    let src = "#include <stdio.h>\n#include <stdlib.h>\n#include \"local.h\"\n#include \"../up/other.h\"\nint main(void) { return 0; }\n";
    assert_eq!(
        flagged("a.c", src),
        [
            ("stdio.h".to_string(), false),
            ("stdlib.h".to_string(), false),
            ("local.h".to_string(), true),
            ("../up/other.h".to_string(), true),
        ]
    );
}

#[test]
fn csharp_using_directives_take_the_target_not_the_alias() {
    let src = "using System;\nusing System.Collections.Generic;\nusing Alias = System.IO;\nusing static System.Math;\nusing MyLib.Utils;\nnamespace X { class A {} }\n";
    assert_eq!(
        paths("A.cs", src),
        [
            "System",
            "System.Collections.Generic",
            "System.IO",
            "System.Math",
            "MyLib.Utils",
        ]
    );
}

#[test]
fn php_use_groups_aliases_and_file_includes() {
    let src = "<?php\nnamespace App;\nuse App\\Models\\User;\nuse App\\Http\\{A, B};\nuse Vendor\\Pkg\\Thing as T;\nrequire_once \"x.php\";\ninclude \"y.php\";\nrequire 'z.php';\n";
    assert_eq!(
        flagged("a.php", src),
        [
            ("App\\Models\\User".to_string(), false),
            ("App\\Http\\A".to_string(), false),
            ("App\\Http\\B".to_string(), false),
            ("Vendor\\Pkg\\Thing".to_string(), false),
            ("x.php".to_string(), true),
            ("y.php".to_string(), true),
            ("z.php".to_string(), true),
        ]
    );
}

#[test]
fn ruby_require_calls() {
    let src = "require \"json\"\nrequire 'set'\nrequire_relative \"lib/x\"\nrequire \"nokogiri\"\nputs \"hello\"\n";
    assert_eq!(
        flagged("a.rb", src),
        [
            ("json".to_string(), false),
            ("set".to_string(), false),
            ("lib/x".to_string(), true),
            ("nokogiri".to_string(), false),
        ]
    );
}

#[test]
fn kotlin_imports_are_recovered_from_the_source_span() {
    let src = "package a\n\nimport java.util.List\nimport kotlin.collections.Map\nimport com.example.lib.Thing as T\nimport kotlinx.coroutines.*\n\nfun main() {}\n";
    assert_eq!(
        paths("A.kt", src),
        [
            "java.util.List",
            "kotlin.collections.Map",
            "com.example.lib.Thing",
            "kotlinx.coroutines.*",
        ]
    );
}

#[test]
fn kotlin_without_source_bytes_yields_no_imports() {
    let src = "import java.util.List\n";
    let parser = Parser::new();
    let root = parser.parse("A.kt", src.as_bytes()).expect("parse");
    assert!(extract_imports(&root, "kotlin", &[]).is_empty());
}

#[test]
fn unknown_language_name_uses_the_generic_rule_on_real_trees() {
    let parser = Parser::new();
    let go = "package main\n\nimport (\n\tstr \"strings\"\n\t. \"math\"\n)\n";
    let root = parser.parse("a.go", go.as_bytes()).expect("parse go");
    let got: Vec<String> = extract_imports(&root, "", go.as_bytes())
        .into_iter()
        .map(|i| i.path)
        .collect();
    assert_eq!(got, ["strings", "math"]);

    let py = "import os, sys\nfrom . import sibling\n";
    let root = parser.parse("a.py", py.as_bytes()).expect("parse py");
    let got: Vec<String> = extract_imports(&root, "", py.as_bytes())
        .into_iter()
        .map(|i| i.path)
        .collect();
    assert_eq!(got, ["os", "sys", ".sibling"]);

    let js = "import React from 'react';\nimport './x.css';\n";
    let root = parser.parse("a.js", js.as_bytes()).expect("parse js");
    let got: Vec<String> = extract_imports(&root, "", js.as_bytes())
        .into_iter()
        .map(|i| i.path)
        .collect();
    assert_eq!(got, ["react", "./x.css"]);
}

#[test]
fn a_file_without_imports_yields_nothing() {
    assert!(paths(
        "types.go",
        "package main\n\ntype Config struct {\n\tName string\n}\n"
    )
    .is_empty());
    assert!(paths("README.md", "# Title\n\nimport nothing here\n").is_empty());
}

/// The parser's language names for the fixtures are the names the
/// extractor and the stdlib tables key on.
#[test]
fn parser_language_names_match_the_language_table() {
    let parser = Parser::new();
    let cases = [
        ("a.go", "go"),
        ("a.py", "python"),
        ("a.js", "javascript"),
        ("a.ts", "typescript"),
        ("a.tsx", "tsx"),
        ("a.rs", "rust"),
        ("A.java", "java"),
        ("a.c", "c"),
        ("a.cpp", "cpp"),
        ("A.cs", "c_sharp"),
        ("a.php", "php"),
        ("a.rb", "ruby"),
        ("A.kt", "kotlin"),
    ];
    for (file, want) in cases {
        assert_eq!(parser.get_language(file), want, "{file}");
    }
}
