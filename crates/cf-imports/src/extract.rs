//! Language-aware import extraction over the UAST.
//!
//! [`extract_imports`] walks a parsed file and returns the modules, packages
//! or files its import statements name — deduplicated per file in first-seen
//! order. The walk is **consuming**: once a node matches the language's
//! import rule, its subtree is never visited again, so specifier lists,
//! aliases and nested import-role nodes can never count as imports.
//!
//! What counts as one import, per language:
//!
//! | Language | Statement | Import(s) |
//! |---|---|---|
//! | Go | `import str "strings"`, `. "math"`, `_ "embed"` | the quoted path (`strings`, `math`, `embed`) |
//! | Python | `import a, b as c` / `from M import x` / `from . import y` | `a`, `b` / `M` / `.y` |
//! | JavaScript, TypeScript | `import X from 'm'`, `import 'm'`, `import x = require("m")`, `require('m')`, `import('m')` | the module specifier `m` |
//! | Rust | `use a::{b, c::{self, d}};`, `pub use x as y;`, `extern crate z;` | `a::b`, `a::c`, `a::c::d`; `x`; `z` |
//! | Java | `import java.util.*;`, `import static java.lang.Math.max;` | `java.util.*`, `java.lang.Math.max` |
//! | C, C++ | `#include <stdio.h>`, `#include "local.h"` | `stdio.h` (package), `local.h` (local file) |
//! | C# | `using Alias = System.IO;`, `using static System.Math;` | `System.IO`, `System.Math` |
//! | PHP | `use A\B\{C, D};`, `use X\Y as Z;`, `require_once "f.php";` | `A\B\C`, `A\B\D`; `X\Y`; `f.php` (local file) |
//! | Ruby | `require "json"`, `require_relative "lib/x"` | `json`; `lib/x` (local file) |
//! | Kotlin | `import a.b.c as d`, `import a.b.*` | `a.b.c`, `a.b.*` (recovered from the source span) |
//! | other | an `Import` node | the generic statement rule ([`crate::statement::generic_targets`]) |
//!
//! Re-exports (`export … from`), dynamic calls whose argument is not a string
//! literal, and Kotlin files parsed without their source bytes contribute
//! nothing.

use std::collections::HashSet;

use cf_uast_node::Node;

use crate::language::Language;
use crate::statement;

/// UAST type of an import statement node.
const TYPE_IMPORT: &str = "Import";
/// UAST type of a string literal node.
const TYPE_LITERAL: &str = "Literal";
/// UAST type of an identifier node.
const TYPE_IDENTIFIER: &str = "Identifier";
/// UAST type of a synthetic (grammar-specific) node.
const TYPE_SYNTHETIC: &str = "Synthetic";
/// UAST type of a call expression node.
const TYPE_CALL: &str = "Call";
/// The CommonJS loader function.
const JS_REQUIRE: &str = "require";
/// The token the dynamic-import keyword node carries.
const JS_IMPORT_KEYWORD: &str = "import";
/// Ruby's library loader.
const RUBY_REQUIRE: &str = "require";
/// Ruby's file-relative loader.
const RUBY_REQUIRE_RELATIVE: &str = "require_relative";
/// Ruby's unconditional file loader.
const RUBY_LOAD: &str = "load";
/// Opening brace of a PHP group `use`.
const PHP_GROUP_OPEN: char = '{';
/// PHP namespace separator.
const PHP_SEP: char = '\\';
/// Java / Kotlin on-demand (wildcard) import suffix.
const WILDCARD_SUFFIX: &str = ".*";

/// One import extracted from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The module, package or file path the statement names.
    pub path: String,
    /// `true` when the statement names a file relative to the importing file
    /// (C `#include "x.h"`, PHP `require "x.php"`, Ruby `require_relative`),
    /// which classifies as `relative` regardless of the path's spelling.
    pub local_file: bool,
}

impl Import {
    /// A module/package import.
    #[must_use]
    pub fn package(path: impl Into<String>) -> Self {
        Import {
            path: path.into(),
            local_file: false,
        }
    }

    /// A file-relative include.
    #[must_use]
    pub fn local(path: impl Into<String>) -> Self {
        Import {
            path: path.into(),
            local_file: true,
        }
    }
}

/// Extracts the imports of one parsed file.
///
/// * `language` is the parser's language name for the file (`go`, `python`,
///   …); unknown names use the generic rule.
/// * `source` is the file's bytes, used only where the tree carries no
///   tokens (Kotlin); pass an empty slice when unavailable.
///
/// The result is deduplicated by path, first occurrence first.
#[must_use]
pub fn extract_imports(root: &Node, language: &str, source: &[u8]) -> Vec<Import> {
    let lang = Language::from_name(language);
    let mut sink = Sink::default();
    walk(root, lang, source, &mut sink);
    sink.imports
}

/// First-seen-order, path-deduplicated collector.
#[derive(Default)]
struct Sink {
    imports: Vec<Import>,
    seen: HashSet<String>,
}

impl Sink {
    fn push(&mut self, import: Import) {
        if !import.path.is_empty() && self.seen.insert(import.path.clone()) {
            self.imports.push(import);
        }
    }
}

/// Pre-order walk that stops descending below a matched import node.
fn walk(node: &Node, lang: Language, source: &[u8], sink: &mut Sink) {
    match match_node(node, lang, source) {
        Some(found) => found.into_iter().for_each(|i| sink.push(i)),
        None => node
            .children
            .iter()
            .for_each(|c| walk(c, lang, source, sink)),
    }
}

/// Applies the language's rule: `Some(imports)` consumes the subtree (even
/// when empty), `None` keeps walking the children.
fn match_node(node: &Node, lang: Language, source: &[u8]) -> Option<Vec<Import>> {
    match lang {
        Language::Go => go_rule(node),
        Language::Python => python_rule(node),
        Language::JavaScript => javascript_rule(node),
        Language::Rust => rust_rule(node),
        Language::Java => java_rule(node),
        Language::C | Language::Cpp => c_rule(node),
        Language::CSharp => csharp_rule(node),
        Language::Php => php_rule(node),
        Language::Ruby => ruby_rule(node),
        Language::Kotlin => kotlin_rule(node, source),
        Language::Other => generic_rule(node),
    }
}

// --- per-language rules ---

/// Go: an import *spec* carries its path as the last quoted string of its
/// token (`str "strings"` -> `strings`); the statement-level node has no
/// token and is walked through.
fn go_rule(node: &Node) -> Option<Vec<Import>> {
    let token = tokened_import(node)?;
    Some(packages(statement::last_quoted(token).map(str::to_string)))
}

/// Python: the import node's token is the statement text.
fn python_rule(node: &Node) -> Option<Vec<Import>> {
    let token = tokened_import(node)?;
    Some(packages(statement::python_targets(token)))
}

/// JavaScript / TypeScript: an ES import's specifier is its literal child
/// (never the specifier list); `require("m")` and `import("m")` are calls
/// whose first argument literal is the module.
fn javascript_rule(node: &Node) -> Option<Vec<Import>> {
    if let Some(token) = tokened_import(node) {
        let specifier = first_descendant_token(node, TYPE_LITERAL)
            .map(statement::unquote)
            .or_else(|| statement::last_quoted(token));
        return Some(packages(specifier.map(str::to_string)));
    }
    if node.node_type != TYPE_CALL {
        return None;
    }
    let callee = node.children.first()?;
    let is_loader = (callee.node_type == TYPE_IDENTIFIER && callee.token == JS_REQUIRE)
        || (is_import(callee) && callee.token == JS_IMPORT_KEYWORD);
    if !is_loader {
        return None;
    }
    let module = call_argument_literal(node)?;
    Some(vec![Import::package(module)])
}

/// Rust: the import node's token is the `use` / `extern crate` item.
fn rust_rule(node: &Node) -> Option<Vec<Import>> {
    let token = tokened_import(node)?;
    Some(packages(statement::rust_targets(token)))
}

/// Java: the identifier child names the type/package; a trailing synthetic
/// child is the on-demand `*`.
fn java_rule(node: &Node) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    let Some(path) = child_tokens(node, TYPE_IDENTIFIER).next() else {
        return Some(Vec::new());
    };
    let wildcard = node.children.iter().any(|c| c.node_type == TYPE_SYNTHETIC);
    let path = if wildcard {
        format!("{path}{WILDCARD_SUFFIX}")
    } else {
        path.to_string()
    };
    Some(vec![Import::package(path)])
}

/// C / C++: `<x>` is an identifier child (a system header), `"x"` a literal
/// child (a file next to the source).
fn c_rule(node: &Node) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    if let Some(system) = child_tokens(node, TYPE_IDENTIFIER).next() {
        return Some(vec![Import::package(statement::strip_angle(system))]);
    }
    Some(packages_or_local(
        child_tokens(node, TYPE_LITERAL)
            .next()
            .map(statement::unquote),
        true,
    ))
}

/// C#: `using [Alias =] [static] Target;` — the last identifier child is the
/// target.
fn csharp_rule(node: &Node) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    Some(packages(
        child_tokens(node, TYPE_IDENTIFIER)
            .last()
            .map(str::to_string),
    ))
}

/// PHP: `require`/`include` carry a literal; group `use A\{B, C}` expands
/// under its prefix; plain and aliased `use` nest one import node per
/// target.
fn php_rule(node: &Node) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    if let Some(file) = first_descendant_token(node, TYPE_LITERAL) {
        return Some(vec![Import::local(statement::unquote(file))]);
    }
    if let Some(group) = node
        .children
        .iter()
        .find(|c| is_import(c) && c.token.starts_with(PHP_GROUP_OPEN))
    {
        let prefix = child_tokens(node, TYPE_IDENTIFIER).next().unwrap_or("");
        return Some(packages(php_group_members(group, prefix)));
    }
    let nested: Vec<String> = node
        .children
        .iter()
        .filter(|c| is_import(c))
        .filter_map(|c| first_descendant_token(c, TYPE_IDENTIFIER))
        .map(str::to_string)
        .collect();
    if !nested.is_empty() {
        return Some(packages(nested));
    }
    Some(packages(
        first_descendant_token(node, TYPE_IDENTIFIER).map(str::to_string),
    ))
}

/// The members of a PHP group `use`, each joined to `prefix`.
fn php_group_members(group: &Node, prefix: &str) -> Vec<String> {
    group
        .children
        .iter()
        .filter(|c| is_import(c))
        .filter_map(|c| first_descendant_token(c, TYPE_IDENTIFIER))
        .map(|name| {
            if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}{PHP_SEP}{name}")
            }
        })
        .collect()
}

/// Ruby: `require "m"` / `require_relative "f"` / `load "f"` calls.
fn ruby_rule(node: &Node) -> Option<Vec<Import>> {
    if node.node_type != TYPE_CALL {
        return None;
    }
    let callee = node.children.first()?;
    if callee.node_type != TYPE_IDENTIFIER {
        return None;
    }
    let local = match callee.token.as_str() {
        RUBY_REQUIRE => false,
        RUBY_REQUIRE_RELATIVE | RUBY_LOAD => true,
        _ => return None,
    };
    let target = call_argument_literal(node)?;
    Some(packages_or_local(Some(target), local))
}

/// Kotlin: the import nodes carry no tokens, so the statement text is read
/// back from the node's source span (`import a.b.c as d` -> `a.b.c`).
fn kotlin_rule(node: &Node, source: &[u8]) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    Some(packages(
        span_text(node, source).and_then(statement::dotted_target),
    ))
}

/// Generic rule for languages without a dedicated one: statement text when
/// the node has a token; otherwise walk through to tokened nested imports,
/// else take the first literal / identifier child.
fn generic_rule(node: &Node) -> Option<Vec<Import>> {
    if !is_import(node) {
        return None;
    }
    if !node.token.is_empty() {
        return Some(packages(statement::generic_targets(&node.token)));
    }
    if has_tokened_import_below(node) {
        return None;
    }
    if let Some(literal) = first_descendant_token(node, TYPE_LITERAL) {
        return Some(vec![Import::package(statement::unquote(literal))]);
    }
    Some(packages(
        child_tokens(node, TYPE_IDENTIFIER)
            .next()
            .map(|t| statement::strip_angle(t).to_string()),
    ))
}

// --- node helpers ---

/// True for an import statement node.
fn is_import(node: &Node) -> bool {
    node.node_type == TYPE_IMPORT
}

/// The token of an import node that carries one.
fn tokened_import(node: &Node) -> Option<&str> {
    (is_import(node) && !node.token.is_empty()).then_some(node.token.as_str())
}

/// Tokens of the direct children of type `kind` that carry one.
fn child_tokens<'a>(node: &'a Node, kind: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    node.children
        .iter()
        .filter(move |c| c.node_type == kind && !c.token.is_empty())
        .map(|c| c.token.as_str())
}

/// The first descendant (pre-order, excluding `node`) of type `kind` that
/// carries a token.
fn first_descendant_token<'a>(node: &'a Node, kind: &str) -> Option<&'a str> {
    for child in &node.children {
        if child.node_type == kind && !child.token.is_empty() {
            return Some(&child.token);
        }
        if let Some(found) = first_descendant_token(child, kind) {
            return Some(found);
        }
    }
    None
}

/// True when some descendant is an import node with a token.
fn has_tokened_import_below(node: &Node) -> bool {
    node.children
        .iter()
        .any(|c| (is_import(c) && !c.token.is_empty()) || has_tokened_import_below(c))
}

/// The first string literal among a call's arguments (every child after the
/// callee), unquoted.
fn call_argument_literal(call: &Node) -> Option<&str> {
    call.children
        .iter()
        .skip(1)
        .find_map(|arg| {
            if arg.node_type == TYPE_LITERAL && !arg.token.is_empty() {
                Some(arg.token.as_str())
            } else {
                first_descendant_token(arg, TYPE_LITERAL)
            }
        })
        .map(statement::unquote)
}

/// The source text under the node's byte span, when positions and source
/// are available and consistent.
fn span_text<'a>(node: &Node, source: &'a [u8]) -> Option<&'a str> {
    let pos = node.pos.as_ref()?;
    let start = usize::try_from(pos.start_offset).ok()?;
    let end = usize::try_from(pos.end_offset).ok()?;
    if start >= end || end > source.len() {
        return None;
    }
    std::str::from_utf8(&source[start..end]).ok()
}

/// Wraps package paths.
fn packages<I: IntoIterator<Item = String>>(paths: I) -> Vec<Import> {
    paths.into_iter().map(Import::package).collect()
}

/// Wraps an optional path as a package or a local file.
fn packages_or_local(path: Option<&str>, local: bool) -> Vec<Import> {
    path.map(|p| {
        if local {
            Import::local(p)
        } else {
            Import::package(p)
        }
    })
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: &str, token: &str, children: Vec<Node>) -> Node {
        Node {
            node_type: kind.to_string(),
            token: token.to_string(),
            children,
            ..Node::default()
        }
    }

    fn paths(imports: &[Import]) -> Vec<&str> {
        imports.iter().map(|i| i.path.as_str()).collect()
    }

    #[test]
    fn dedup_keeps_first_seen_order() {
        let root = node(
            "File",
            "",
            vec![
                node(TYPE_IMPORT, "import b", vec![]),
                node(TYPE_IMPORT, "import a", vec![]),
                node(TYPE_IMPORT, "import b", vec![]),
            ],
        );
        assert_eq!(paths(&extract_imports(&root, "python", &[])), ["b", "a"]);
    }

    #[test]
    fn nested_import_nodes_below_a_match_never_count() {
        let root = node(
            "File",
            "",
            vec![node(
                TYPE_IMPORT,
                "import React from 'react';",
                vec![
                    node(
                        TYPE_SYNTHETIC,
                        "React",
                        vec![node(TYPE_IDENTIFIER, "React", vec![])],
                    ),
                    node(TYPE_LITERAL, "'react'", vec![]),
                ],
            )],
        );
        assert_eq!(paths(&extract_imports(&root, "javascript", &[])), ["react"]);
    }

    #[test]
    fn kotlin_without_source_or_positions_yields_nothing() {
        let root = node(
            "File",
            "",
            vec![node(
                TYPE_IMPORT,
                "",
                vec![node(TYPE_IDENTIFIER, "", vec![])],
            )],
        );
        assert!(extract_imports(&root, "kotlin", b"import a.b").is_empty());
    }

    #[test]
    fn span_text_rejects_inconsistent_offsets() {
        let mut n = node(TYPE_IMPORT, "", vec![]);
        n.pos = Some(cf_uast_node::Positions {
            start_offset: 4,
            end_offset: 40,
            ..cf_uast_node::Positions::default()
        });
        assert_eq!(span_text(&n, b"import a.b"), None);
        n.pos.as_mut().expect("pos").end_offset = 10;
        assert_eq!(span_text(&n, b"import a.b"), Some("rt a.b"));
    }

    #[test]
    fn unknown_language_generic_rule_handles_token_and_child_shapes() {
        let root = node(
            "File",
            "",
            vec![
                node(TYPE_IMPORT, "from x import y", vec![]),
                node(
                    TYPE_IMPORT,
                    "",
                    vec![node(TYPE_IDENTIFIER, "<stdio.h>", vec![])],
                ),
                node(
                    TYPE_IMPORT,
                    "",
                    vec![node(TYPE_LITERAL, "\"local.h\"", vec![])],
                ),
                node(TYPE_IMPORT, "", vec![node(TYPE_IMPORT, "\"fmt\"", vec![])]),
                node(TYPE_IMPORT, "", vec![]),
            ],
        );
        assert_eq!(
            paths(&extract_imports(&root, "", &[])),
            ["x", "stdio.h", "local.h", "fmt"]
        );
    }
}
