//! `cf-goyaml` — YAML emitter for the codefang Rust rewrite.
//!
//! Port target documented in specs/rust-rewrite/DESIGN.md §1. A fully
//! byte-faithful `gopkg.in/yaml.v3` emitter is ROADMAP Step 4 and is not yet
//! implemented; the [`marshal`] function below produces plausible block-style
//! YAML (2-space indent, byte-sorted map keys) so that crates such as
//! `cf-analyze` link and produce machine-readable output.
#![allow(dead_code)]

use cf_gojson::{GoValue, ftoa};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-goyaml";

/// Serializes a [`GoValue`] to block-style YAML bytes.
///
/// This is an intentionally minimal emitter (ROADMAP Step 4 covers the
/// byte-faithful `gopkg.in/yaml.v3` port). It produces:
///
/// * 2-space block indentation,
/// * map keys in the encoder's byte-sorted order (via
///   [`GoValue::encode_order`] semantics through [`cf_gojson`]),
/// * `key: value` for scalar map values and nested blocks for compound values,
/// * `- ` list items,
/// * `{}` / `[]` for empty maps/sequences.
///
/// It never panics and is safe to call on any `GoValue`.
pub fn marshal(value: &GoValue) -> Vec<u8> {
    let mut out = String::new();
    emit_document(value, &mut out);
    out.into_bytes()
}

fn emit_document(value: &GoValue, out: &mut String) {
    match value {
        GoValue::Map(m) if m.len() > 0 => emit_mapping(value, 0, out),
        GoValue::Array(items) if !items.is_empty() => emit_sequence(items, 0, out),
        other => {
            out.push_str(&scalar(other));
            out.push('\n');
        }
    }
}

/// Emits the entries of a mapping at `indent` columns.
fn emit_mapping(value: &GoValue, indent: usize, out: &mut String) {
    let GoValue::Map(m) = value else { return };
    let pad = " ".repeat(indent);
    for (k, v) in m.encode_order() {
        out.push_str(&pad);
        out.push_str(&yaml_key(k));
        out.push(':');
        match v {
            GoValue::Map(inner) if inner.len() > 0 => {
                out.push('\n');
                emit_mapping(v, indent + 2, out);
            }
            GoValue::Array(items) if !items.is_empty() => {
                out.push('\n');
                emit_sequence(items, indent, out);
            }
            GoValue::Map(_) => {
                out.push_str(" {}\n");
            }
            GoValue::Array(_) => {
                out.push_str(" []\n");
            }
            scalar_value => {
                out.push(' ');
                out.push_str(&scalar(scalar_value));
                out.push('\n');
            }
        }
    }
}

/// Emits a sequence at `indent` columns. List dashes align with the parent key
/// indentation, matching `gopkg.in/yaml.v3`'s default block style.
fn emit_sequence(items: &[GoValue], indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for item in items {
        match item {
            GoValue::Map(inner) if inner.len() > 0 => {
                // First key on the dash line, remaining keys block-indented.
                let mut block = String::new();
                emit_mapping(item, indent + 2, &mut block);
                push_sequence_block(&pad, &block, out);
            }
            GoValue::Array(nested) if !nested.is_empty() => {
                let mut block = String::new();
                emit_sequence(nested, indent + 2, &mut block);
                push_sequence_block(&pad, &block, out);
            }
            GoValue::Map(_) => {
                out.push_str(&pad);
                out.push_str("- {}\n");
            }
            GoValue::Array(_) => {
                out.push_str(&pad);
                out.push_str("- []\n");
            }
            scalar_value => {
                out.push_str(&pad);
                out.push_str("- ");
                out.push_str(&scalar(scalar_value));
                out.push('\n');
            }
        }
    }
}

/// Splices a pre-rendered block under a `- ` dash, putting the first line on the
/// dash line and aligning the rest.
fn push_sequence_block(pad: &str, block: &str, out: &mut String) {
    let mut lines = block.lines();
    if let Some(first) = lines.next() {
        out.push_str(pad);
        out.push_str("- ");
        // `first` already carries (indent+2) spaces of padding; strip the two
        // that the dash replaces.
        out.push_str(first.trim_start());
        out.push('\n');
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// Renders a scalar `GoValue` to its inline YAML representation.
fn scalar(value: &GoValue) -> String {
    match value {
        GoValue::Null => "null".to_string(),
        GoValue::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        GoValue::Int(i) => i.to_string(),
        GoValue::Uint(u) => u.to_string(),
        GoValue::Float(f) => ftoa::format_float_g(*f),
        GoValue::Str(s) => yaml_string(s),
        // Compound values are handled by the block emitters; reaching here means
        // an empty container used in scalar position.
        GoValue::Array(_) => "[]".to_string(),
        GoValue::Map(_) => "{}".to_string(),
    }
}

/// Quotes a mapping key when plain-scalar rules would be ambiguous.
fn yaml_key(key: &str) -> String {
    yaml_string(key)
}

/// Renders a string as a YAML scalar, quoting when needed.
fn yaml_string(s: &str) -> String {
    if needs_quoting(s) {
        let mut q = String::with_capacity(s.len() + 2);
        q.push('"');
        for ch in s.chars() {
            match ch {
                '"' => q.push_str("\\\""),
                '\\' => q.push_str("\\\\"),
                '\n' => q.push_str("\\n"),
                '\t' => q.push_str("\\t"),
                '\r' => q.push_str("\\r"),
                c => q.push(c),
            }
        }
        q.push('"');
        q
    } else {
        s.to_string()
    }
}

/// Decides whether a plain YAML scalar would be misread and thus needs quoting.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Reserved plain-scalar words / type-ambiguous forms.
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "null" | "~" | "true" | "false" | "yes" | "no" | "on" | "off"
    ) {
        return true;
    }
    // Numbers would be re-typed if left bare.
    if s.parse::<f64>().is_ok() || s.parse::<i64>().is_ok() {
        return true;
    }
    let first = s.chars().next().unwrap();
    if matches!(
        first,
        '!' | '&' | '*' | '-' | '?' | '{' | '}' | '[' | ']' | ','
            | '#' | '|' | '>' | '@' | '`' | '"' | '\'' | '%' | ' '
    ) {
        return true;
    }
    if s.ends_with(' ') {
        return true;
    }
    s.chars().any(|c| {
        matches!(c, ':' | '#' | '\n' | '\t' | '\r')
            || c.is_control()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, GoValue};

    fn s(value: &GoValue) -> String {
        String::from_utf8(marshal(value)).unwrap()
    }

    #[test]
    fn scalar_document() {
        assert_eq!(s(&GoValue::Int(42)), "42\n");
        assert_eq!(s(&GoValue::Str("hi".into())), "hi\n");
        assert_eq!(s(&GoValue::Bool(true)), "true\n");
    }

    #[test]
    fn mapping_keys_are_byte_sorted() {
        let m = GoMap::from_map(vec![
            ("b".into(), GoValue::Int(2)),
            ("a".into(), GoValue::Int(1)),
        ]);
        assert_eq!(s(&GoValue::Map(m)), "a: 1\nb: 2\n");
    }

    #[test]
    fn nested_mapping_indents_two_spaces() {
        let inner = GoMap::from_map(vec![("x".into(), GoValue::Int(1))]);
        let outer = GoMap::from_map(vec![("k".into(), GoValue::Map(inner))]);
        assert_eq!(s(&GoValue::Map(outer)), "k:\n  x: 1\n");
    }

    #[test]
    fn sequence_of_scalars() {
        let arr = GoValue::Array(vec![GoValue::Int(1), GoValue::Int(2)]);
        let m = GoMap::from_map(vec![("items".into(), arr)]);
        assert_eq!(s(&GoValue::Map(m)), "items:\n- 1\n- 2\n");
    }

    #[test]
    fn ambiguous_strings_quoted() {
        assert_eq!(s(&GoValue::Str("true".into())), "\"true\"\n");
        assert_eq!(s(&GoValue::Str("123".into())), "\"123\"\n");
        assert_eq!(s(&GoValue::Str("".into())), "\"\"\n");
    }
}
