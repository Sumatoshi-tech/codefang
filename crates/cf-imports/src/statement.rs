//! Statement-text parsing shared by the per-language extraction rules.
//!
//! Several grammars lower an import to a single node whose token is the whole
//! statement (`import os, sys`, `import React from 'react';`,
//! `use std::io::{self, Read};`). The functions here turn such text into the
//! module/package paths it names; they are pure and language-tagged so each
//! extraction rule picks exactly the form its language uses.

/// Keyword that opens a Python `from M import …` statement.
const KW_FROM: &str = "from ";
/// Keyword that opens an `import …` statement.
const KW_IMPORT: &str = "import ";
/// Keyword that opens a Rust `use …;` item.
const KW_USE: &str = "use ";
/// Rust `extern crate x;` prefix.
const KW_EXTERN_CRATE: &str = "extern crate ";
/// Rust visibility prefix (`pub`, `pub(crate)`, `pub(in path)`).
const KW_PUB: &str = "pub";
/// Java `import static …` modifier.
const KW_STATIC: &str = "static ";
/// Alias clause separator (` as x`).
const KW_AS: &str = " as ";
/// Rust path separator.
const RUST_SEP: &str = "::";

/// Strips one layer of matching single or double quotes.
#[must_use]
pub fn unquote(s: &str) -> &str {
    let s = s.trim();
    let stripped = s
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')));
    stripped.unwrap_or(s)
}

/// Returns the content of the last quoted (`"…"` or `'…'`) string in `text`.
///
/// ```
/// use cf_imports::statement::last_quoted;
///
/// assert_eq!(last_quoted(r#"str "strings""#), Some("strings"));
/// assert_eq!(last_quoted("import React from 'react';"), Some("react"));
/// assert_eq!(last_quoted("import os"), None);
/// ```
#[must_use]
pub fn last_quoted(text: &str) -> Option<&str> {
    let close = text.rfind(['"', '\''])?;
    let quote = text.as_bytes()[close] as char;
    let open = text[..close].rfind(quote)?;
    Some(&text[open + 1..close])
}

/// Strips the `<` `>` of an angle-bracket include target.
#[must_use]
pub fn strip_angle(s: &str) -> &str {
    s.trim()
        .strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or_else(|| s.trim())
}

/// The target before any ` as alias` clause, trimmed.
fn strip_alias(item: &str) -> &str {
    item.split(KW_AS).next().unwrap_or(item).trim()
}

/// Normalizes a multi-line statement: parentheses, line continuations and
/// newlines become spaces so comma lists split cleanly.
fn flatten(stmt: &str) -> String {
    stmt.replace(['(', ')', '\\', '\n', '\r'], " ")
}

/// Python `import a, b as c` / `from M import x, y`:
///
/// * `import a, b as c` -> `[a, b]`;
/// * `from M import x` -> `[M]`;
/// * `from . import x, y` (purely relative `M`) -> `[.x, .y]`, so the
///   relative target keeps its module name.
///
/// ```
/// use cf_imports::statement::python_targets;
///
/// assert_eq!(python_targets("import os, sys"), vec!["os", "sys"]);
/// assert_eq!(python_targets("from typing import List, Dict"), vec!["typing"]);
/// assert_eq!(python_targets("from . import sibling"), vec![".sibling"]);
/// assert_eq!(python_targets("from ..pkg.sub import thing"), vec!["..pkg.sub"]);
/// assert_eq!(python_targets("import numpy as np"), vec!["numpy"]);
/// ```
#[must_use]
pub fn python_targets(stmt: &str) -> Vec<String> {
    let flat = flatten(stmt);
    let flat = flat.trim();
    if let Some(rest) = flat.strip_prefix(KW_FROM) {
        return python_from_targets(rest);
    }
    match flat.strip_prefix(KW_IMPORT) {
        Some(list) => comma_targets(list),
        None => Vec::new(),
    }
}

/// `M import x, y` (after `from `): `M` itself, or `M` joined with each name
/// when `M` is purely relative (`.`, `..`).
fn python_from_targets(rest: &str) -> Vec<String> {
    let (module, names) = match rest.split_once(KW_IMPORT) {
        Some((m, n)) => (m.trim(), n),
        None => (rest.trim(), ""),
    };
    if module.is_empty() {
        return Vec::new();
    }
    if !module.chars().all(|c| c == '.') {
        return vec![module.to_string()];
    }
    comma_targets(names)
        .into_iter()
        .map(|name| format!("{module}{name}"))
        .collect()
}

/// Splits a comma list of targets, dropping aliases and empties.
fn comma_targets(list: &str) -> Vec<String> {
    list.split(',')
        .map(strip_alias)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Dotted-path import statement (`import a.b.c [as d]`, Kotlin / Java /
/// Swift style): the dotted path, with Java's `static` modifier and any alias
/// removed.
///
/// ```
/// use cf_imports::statement::dotted_target;
///
/// assert_eq!(dotted_target("import kotlinx.coroutines.*"), Some("kotlinx.coroutines.*".into()));
/// assert_eq!(dotted_target("import com.example.Thing as T"), Some("com.example.Thing".into()));
/// assert_eq!(dotted_target("import static java.lang.Math.max;"), Some("java.lang.Math.max".into()));
/// ```
#[must_use]
pub fn dotted_target(stmt: &str) -> Option<String> {
    let rest = stmt.trim().trim_end_matches(';').trim();
    let rest = rest.strip_prefix(KW_IMPORT)?.trim();
    let rest = rest.strip_prefix(KW_STATIC).unwrap_or(rest);
    let target = strip_alias(rest);
    (!target.is_empty()).then(|| target.to_string())
}

/// Rust `use` / `extern crate` items, expanded to leaf paths:
///
/// * visibility (`pub`, `pub(crate)`, …) and the trailing `;` are dropped;
/// * ` as alias` clauses are dropped;
/// * brace groups expand recursively, `self` names the group prefix;
/// * glob imports keep their `*`.
///
/// ```
/// use cf_imports::statement::rust_targets;
///
/// assert_eq!(
///     rust_targets("use std::collections::{HashMap, HashSet};"),
///     vec!["std::collections::HashMap", "std::collections::HashSet"]
/// );
/// assert_eq!(rust_targets("pub(crate) use std::io::{self, Read};"), vec!["std::io", "std::io::Read"]);
/// assert_eq!(rust_targets("use super::baz as qux;"), vec!["super::baz"]);
/// assert_eq!(rust_targets("use std::io::*;"), vec!["std::io::*"]);
/// assert_eq!(rust_targets("extern crate alloc;"), vec!["alloc"]);
/// ```
#[must_use]
pub fn rust_targets(stmt: &str) -> Vec<String> {
    let item = strip_visibility(stmt.trim().trim_end_matches(';').trim());
    if let Some(krate) = item.strip_prefix(KW_EXTERN_CRATE) {
        return vec![strip_alias(krate).to_string()];
    }
    let Some(tree) = item.strip_prefix(KW_USE) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    expand_use_tree("", tree.trim().trim_start_matches(RUST_SEP), &mut out);
    out
}

/// Drops a leading `pub`, `pub(crate)`, `pub(super)` or `pub(in path)`.
fn strip_visibility(item: &str) -> &str {
    let Some(rest) = item.strip_prefix(KW_PUB) else {
        return item;
    };
    let rest = match rest.strip_prefix('(') {
        Some(scoped) => scoped.split_once(')').map_or("", |(_, after)| after),
        None => rest,
    };
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        item
    }
}

/// Recursively expands one use-tree under `prefix` into `out`.
fn expand_use_tree(prefix: &str, tree: &str, out: &mut Vec<String>) {
    let tree = tree.trim();
    if let Some(inner) = tree.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        for item in split_top_level(inner) {
            expand_use_tree(prefix, item, out);
        }
        return;
    }
    if let Some(idx) = tree.find("::{") {
        let head = join_rust(prefix, &tree[..idx]);
        expand_use_tree(&head, &tree[idx + RUST_SEP.len()..], out);
        return;
    }
    let leaf = strip_alias(tree);
    if leaf.is_empty() {
        return;
    }
    if leaf == "self" {
        if !prefix.is_empty() {
            out.push(prefix.to_string());
        }
        return;
    }
    out.push(join_rust(prefix, leaf));
}

/// Joins a Rust path prefix and segment with `::`.
fn join_rust(prefix: &str, segment: &str) -> String {
    let segment = segment.trim();
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}{RUST_SEP}{segment}")
    }
}

/// Splits on commas that are not nested inside `{…}`.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Generic statement rule for languages without a dedicated one: tries the
/// known statement forms in order and falls back to the statement text.
///
/// 1. `from M import …` (Python form);
/// 2. `import … from 'x'` / any `import …` carrying a quoted target (ES /
///    TypeScript / Go spec form) -> the last quoted string;
/// 3. `import a, b as c` (list form, also dotted Java/Kotlin/Swift paths);
/// 4. `use …;` / `extern crate …;` (Rust form);
/// 5. any other text carrying a quoted string -> the last quoted string;
/// 6. `<x>` -> `x`;
/// 7. otherwise the statement text without a trailing `;`.
#[must_use]
pub fn generic_targets(stmt: &str) -> Vec<String> {
    let text = stmt.trim();
    if text.starts_with(KW_FROM) {
        return python_targets(text);
    }
    if text.starts_with(KW_IMPORT) {
        return match last_quoted(text) {
            Some(target) => vec![target.to_string()],
            None => python_targets(text.trim_end_matches(';')),
        };
    }
    if text.starts_with(KW_USE) || text.starts_with(KW_EXTERN_CRATE) || text.starts_with(KW_PUB) {
        return rust_targets(text);
    }
    if let Some(target) = last_quoted(text) {
        return vec![target.to_string()];
    }
    let bare = strip_angle(text.trim_end_matches(';'));
    if bare.is_empty() {
        Vec::new()
    } else {
        vec![bare.to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquote_strips_one_matching_layer() {
        assert_eq!(unquote("\"fmt\""), "fmt");
        assert_eq!(unquote("'react'"), "react");
        assert_eq!(unquote("bare"), "bare");
        assert_eq!(unquote("\"mismatched'"), "\"mismatched'");
    }

    #[test]
    fn last_quoted_takes_the_final_string() {
        assert_eq!(last_quoted(". \"math\""), Some("math"));
        assert_eq!(last_quoted("_ \"embed\""), Some("embed"));
        assert_eq!(last_quoted("import fs = require(\"fs\");"), Some("fs"));
        assert_eq!(
            last_quoted("import def, { named } from './local/mod';"),
            Some("./local/mod")
        );
        assert_eq!(last_quoted("\"unterminated"), None);
    }

    #[test]
    fn python_multiline_parenthesised_from_import() {
        assert_eq!(
            python_targets("from pkg.mod import (\n    a,\n    b as c,\n)"),
            vec!["pkg.mod"]
        );
        assert_eq!(python_targets("import a, \\\n    b"), vec!["a", "b"]);
        assert_eq!(python_targets("from . import a, b"), vec![".a", ".b"]);
        assert_eq!(python_targets("from .. import x"), vec!["..x"]);
        assert!(python_targets("not an import").is_empty());
    }

    #[test]
    fn rust_nested_groups_and_visibility() {
        assert_eq!(
            rust_targets("pub use a::{b::{c, d as e}, f};"),
            vec!["a::b::c", "a::b::d", "a::f"]
        );
        assert_eq!(rust_targets("pub(in crate::x) use y::z;"), vec!["y::z"]);
        assert_eq!(rust_targets("use ::std::env;"), vec!["std::env"]);
        assert_eq!(rust_targets("use {a, b};"), vec!["a", "b"]);
        assert_eq!(rust_targets("extern crate serde as s;"), vec!["serde"]);
        assert!(rust_targets("fn main() {}").is_empty());
        // `pub` that is not a visibility prefix of a use item stays untouched.
        assert!(rust_targets("public_fn();").is_empty());
    }

    #[test]
    fn dotted_target_forms() {
        assert_eq!(
            dotted_target("import java.util.List;"),
            Some("java.util.List".into())
        );
        assert_eq!(dotted_target("package a"), None);
        assert_eq!(dotted_target("import "), None);
    }

    #[test]
    fn generic_rule_covers_every_statement_form() {
        assert_eq!(generic_targets("from x import y"), vec!["x"]);
        assert_eq!(generic_targets("import React from 'react';"), vec!["react"]);
        assert_eq!(
            generic_targets("import './styles.css'"),
            vec!["./styles.css"]
        );
        assert_eq!(generic_targets("import os, sys"), vec!["os", "sys"]);
        assert_eq!(
            generic_targets("import java.util.List;"),
            vec!["java.util.List"]
        );
        assert_eq!(generic_targets("use std::fmt;"), vec!["std::fmt"]);
        assert_eq!(generic_targets("pub use a::b;"), vec!["a::b"]);
        assert_eq!(generic_targets("str \"strings\""), vec!["strings"]);
        assert_eq!(generic_targets("require_once \"x.php\""), vec!["x.php"]);
        assert_eq!(generic_targets("<stdio.h>"), vec!["stdio.h"]);
        assert_eq!(
            generic_targets("App\\Models\\User;"),
            vec!["App\\Models\\User"]
        );
        assert!(generic_targets("   ").is_empty());
    }
}
