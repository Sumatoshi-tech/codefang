//! Build [`cf_textutil::GoValue`] report values from a UAST [`Node`].
//!
//! Per DESIGN rule (1) all MACHINE-format report bytes are produced through the
//! shared Go-byte-compatible serialization path ([`cf_textutil::write_json`],
//! which wraps the `cf-gojson` encoder) — never raw `serde_json`.
//!
//! The `uast` binary's report values originate from a parsed [`Node`]. The
//! canonical serialization shape is Go's `Node.ToMap` (`pkg/uast/pkg/node`),
//! reproduced byte-for-byte by [`cf_uast_node::Node::to_map`]. That method
//! returns a [`cf_uast_node::GoValue`], whose private `GoMap` entries are not
//! externally iterable, so rather than convert that opaque value we re-derive the
//! identical map-origin shape here directly from the [`Node`] fields, emitting a
//! [`cf_textutil::GoValue`]. [`cf_textutil::GoValue::object`] byte-sorts its keys
//! exactly as Go's `encoding/json` sorts `map[string]any` (and as
//! `cf_uast_node`'s map-origin `GoMap` does on encode), so the resulting JSON is
//! byte-identical to Go's `ToMap` output.
//!
//! The key population rules mirror Go's `ToMap` / `Positions` `omitempty`
//! semantics exactly (see `pkg/uast/pkg/node/node.go` and
//! `cf-uast-node/src/tomap.rs`):
//! - `type` is always present (even when empty);
//! - `token` only when non-empty;
//! - `roles` only when non-empty (array of strings);
//! - `pos` only when the node has a position; each of the six `uint` fields is
//!   emitted only when non-zero;
//! - `props` only when non-empty (nested map-origin object);
//! - `children` only when non-empty (array of recursively-mapped nodes);
//! - `id` only when non-empty, hex-encoded (`fmt.Sprintf("%x", id)`).

use cf_textutil::{GoMap, GoValue};
use cf_uast_node::Node;

/// Converts a [`Node`] (and its subtree) into the byte-identical `Node.ToMap`
/// [`GoValue`] consumed by [`cf_textutil::write_json`].
pub fn node_to_value(node: &Node) -> GoValue {
    let mut entries: Vec<(String, GoValue)> = Vec::new();

    // type — always present.
    entries.push(("type".to_string(), GoValue::Str(node.node_type.clone())));

    // token — omitempty.
    if !node.token.is_empty() {
        entries.push(("token".to_string(), GoValue::Str(node.token.clone())));
    }

    // roles — omitempty.
    if !node.roles.is_empty() {
        let roles = node.roles.iter().map(|r| GoValue::Str(r.clone())).collect();
        entries.push(("roles".to_string(), GoValue::Array(roles)));
    }

    // pos — only when present; the six uint fields are each omitempty.
    if let Some(pos) = node.pos {
        let mut pos_entries: Vec<(String, GoValue)> = Vec::new();
        if pos.start_line != 0 {
            pos_entries.push(("start_line".to_string(), GoValue::Uint(pos.start_line)));
        }
        if pos.start_col != 0 {
            pos_entries.push(("start_col".to_string(), GoValue::Uint(pos.start_col)));
        }
        if pos.start_offset != 0 {
            pos_entries.push(("start_offset".to_string(), GoValue::Uint(pos.start_offset)));
        }
        if pos.end_line != 0 {
            pos_entries.push(("end_line".to_string(), GoValue::Uint(pos.end_line)));
        }
        if pos.end_col != 0 {
            pos_entries.push(("end_col".to_string(), GoValue::Uint(pos.end_col)));
        }
        if pos.end_offset != 0 {
            pos_entries.push(("end_offset".to_string(), GoValue::Uint(pos.end_offset)));
        }
        entries.push(("pos".to_string(), GoValue::object(GoMap::from_map(pos_entries))));
    }

    // props — omitempty (nested map-origin object).
    if !node.props.is_empty() {
        let prop_entries: Vec<(String, GoValue)> = node
            .props
            .iter()
            .map(|(k, v)| (k.clone(), GoValue::Str(v.clone())))
            .collect();
        entries.push(("props".to_string(), GoValue::object(GoMap::from_map(prop_entries))));
    }

    // children — omitempty.
    if !node.children.is_empty() {
        let kids = node.children.iter().map(node_to_value).collect();
        entries.push(("children".to_string(), GoValue::Array(kids)));
    }

    // id — omitempty, hex-encoded.
    if !node.id.is_empty() {
        entries.push(("id".to_string(), GoValue::Str(hex_encode(&node.id))));
    }

    GoValue::object(GoMap::from_map(entries))
}

/// Hex-encodes raw bytes lowercase, reproducing Go's `fmt.Sprintf("%x", b)`.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Node;

    #[test]
    fn type_always_present_token_omitempty() {
        let n = Node::with_token("Identifier", "x");
        let v = node_to_value(&n);
        let bytes = cf_textutil::marshal_json(&v, false).unwrap();
        assert_eq!(bytes, br#"{"token":"x","type":"Identifier"}"#.to_vec().into_iter().chain(*b"\n").collect::<Vec<u8>>());
    }

    #[test]
    fn empty_token_omitted_type_still_present() {
        let n = Node::with_token("File", "");
        let v = node_to_value(&n);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        assert_eq!(s, "{\"type\":\"File\"}\n");
    }

    #[test]
    fn keys_byte_sorted_matches_go_tomap_order() {
        // A node exercising several keys; map-origin byte order is
        // children < id < pos < props < roles < token < type.
        let mut n = Node::with_token("Function", "foo");
        n.roles = vec!["Declaration".into()];
        n.id = vec![0xab, 0xcd];
        n.add_child(Node::with_token("Identifier", "x"));
        let v = node_to_value(&n);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        // children, id, roles, token, type (no pos/props here).
        assert_eq!(
            s,
            "{\"children\":[{\"token\":\"x\",\"type\":\"Identifier\"}],\
\"id\":\"abcd\",\"roles\":[\"Declaration\"],\"token\":\"foo\",\"type\":\"Function\"}\n"
        );
    }

    #[test]
    fn hex_encode_matches_go() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
