//! [`Node::to_map`] — the byte-identity-critical conversion of a node to its map
//! representation. Ported from `node.go`'s `ToMap` / `buildBaseMap` /
//! `buildPositionMap` / `buildChildrenMap`.
//!
//! In Go, `ToMap` returns `map[string]any`, which `encoding/json` serializes
//! with keys sorted by raw UTF-8 byte order. To reproduce that byte-for-byte the
//! Rust port returns a [`cf_gojson::GoValue`] whose objects are built with
//! [`cf_gojson::GoMap::from_map`] (sort-on-encode). The full key set a node can
//! emit is `children, id, pos, props, roles, token, type`, which in byte order
//! becomes exactly that sequence — see DESIGN.md §2.2.

use crate::gojson::{GoMap, GoValue};
use crate::node::{Node, Positions};

impl Node {
    /// Converts this node (and its subtree) into a [`GoValue`] that serializes
    /// byte-identically to Go's `Node.ToMap()` piped through `encoding/json`.
    ///
    /// Map-origin objects are used throughout so keys byte-sort on encode. The
    /// field-population rules match Go exactly:
    /// - `type` is always present (even if empty).
    /// - `id` present only when non-empty, hex-encoded (`fmt.Sprintf("%x", id)`).
    /// - `token` present only when non-empty.
    /// - `props` present only when non-empty (its own keys also byte-sort).
    /// - `roles` always present (an array, empty when there are no roles).
    /// - `pos` always present (zeros when the node has no position).
    /// - `children` present only when the node has children.
    ///
    /// Returns [`GoValue::Null`] for the conceptual nil-node case is *not*
    /// applicable in Rust (a `&Node` is never nil); callers that need Go's
    /// `ToMap()`-on-nil → `nil` behavior should branch on the `Option` before
    /// calling, mirroring `if n == nil { return nil }`.
    pub fn to_map(&self) -> GoValue {
        let mut entries: Vec<(String, GoValue)> = Vec::with_capacity(7);

        // buildBaseMap: type is unconditional.
        entries.push(("type".to_string(), GoValue::Str(self.node_type.clone())));

        // addIDToMap: only when non-empty; hex of the raw ID bytes.
        if !self.id.is_empty() {
            let mut hex = String::with_capacity(self.id.len() * 2);
            for b in &self.id {
                hex.push_str(&format!("{:02x}", b));
            }
            entries.push(("id".to_string(), GoValue::Str(hex)));
        }

        // addTokenToMap: only when non-empty.
        if !self.token.is_empty() {
            entries.push(("token".to_string(), GoValue::Str(self.token.clone())));
        }

        // addPropsToMap: only when non-empty. props is itself a Go map → sorted.
        if !self.props.is_empty() {
            let prop_entries: Vec<(String, GoValue)> = self
                .props
                .iter()
                .map(|(k, v)| (k.clone(), GoValue::Str(v.clone())))
                .collect();
            entries.push((
                "props".to_string(),
                GoValue::Object(GoMap::from_map(prop_entries)),
            ));
        }

        // addRolesToMap: always present as a (possibly empty) string array.
        let roles: Vec<GoValue> =
            self.roles.iter().map(|r| GoValue::Str(r.clone())).collect();
        entries.push(("roles".to_string(), GoValue::Array(roles)));

        // ToMap: pos is always present.
        entries.push(("pos".to_string(), position_map(self.pos.as_ref())));

        // ToMap: children only when present.
        if !self.children.is_empty() {
            let children: Vec<GoValue> = self.children.iter().map(|c| c.to_map()).collect();
            entries.push(("children".to_string(), GoValue::Array(children)));
        }

        GoValue::Object(GoMap::from_map(entries))
    }
}

/// Builds the position sub-map. Mirrors Go's `buildPositionMap`: a nil position
/// yields all-zero fields; otherwise the six `Positions` fields. Either way the
/// six keys (`end_col,end_line,end_offset,start_col,start_line,start_offset` in
/// byte order) are always present.
fn position_map(pos: Option<&Positions>) -> GoValue {
    let p = pos.copied().unwrap_or_default();
    let entries = vec![
        ("start_line".to_string(), GoValue::Uint(p.start_line)),
        ("start_col".to_string(), GoValue::Uint(p.start_col)),
        ("start_offset".to_string(), GoValue::Uint(p.start_offset)),
        ("end_line".to_string(), GoValue::Uint(p.end_line)),
        ("end_col".to_string(), GoValue::Uint(p.end_col)),
        ("end_offset".to_string(), GoValue::Uint(p.end_offset)),
    ];
    GoValue::Object(GoMap::from_map(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gojson::Encoder;
    use crate::node::Builder;

    #[test]
    fn to_map_basic_fields() {
        // Mirrors Go TestNode_ToMap_Basic.
        let n = Builder::new()
            .with_type("Function")
            .with_token("foo")
            .with_roles(vec!["Declaration".into(), "Name".into()])
            .with_position(Some(Positions {
                start_line: 1,
                start_col: 2,
                start_offset: 3,
                end_line: 4,
                end_col: 5,
                end_offset: 6,
            }))
            .build();
        let json = Encoder::marshal().encode_to_string(&n.to_map());
        // Keys must be byte-sorted: pos,roles,token,type (no id/props/children).
        assert_eq!(
            json,
            r#"{"pos":{"end_col":5,"end_line":4,"end_offset":6,"start_col":2,"start_line":1,"start_offset":3},"roles":["Declaration","Name"],"token":"foo","type":"Function"}"#
        );
    }

    #[test]
    fn to_map_no_children_omits_children_key() {
        // Mirrors Go TestNode_ToMap_NoChildren.
        let n = Node::with_token("Identifier", "x");
        let json = Encoder::marshal().encode_to_string(&n.to_map());
        assert!(!json.contains("\"children\""));
    }

    #[test]
    fn to_map_nil_position_emits_zeros() {
        let n = Node::with_token("Identifier", "x");
        let json = Encoder::marshal().encode_to_string(&n.to_map());
        assert!(json.contains(r#""pos":{"end_col":0,"end_line":0,"end_offset":0,"start_col":0,"start_line":0,"start_offset":0}"#));
    }

    #[test]
    fn to_map_key_order_full_set() {
        // A node with every field exercises the full byte-sorted key order:
        // children, id, pos, props, roles, token, type.
        let mut props = std::collections::HashMap::new();
        props.insert("lang".to_string(), "go".to_string());
        let mut n = Builder::new()
            .with_type("Function")
            .with_token("foo")
            .with_roles(vec!["Name".into()])
            .with_props(props)
            .build();
        n.id = vec![0xde, 0xad];
        n.add_child(Node::with_token("Identifier", "x"));
        let json = Encoder::marshal().encode_to_string(&n.to_map());

        // Assert the exact byte-sorted output (the byte-identity goal). Top-level
        // keys emit in raw-UTF-8 byte order: children, id, pos, props, roles,
        // token, type; `id` is hex-encoded (`fmt.Sprintf("%x", id)`); the nested
        // position map's keys are byte-sorted too. Using exact bytes (rather than
        // a substring scan) avoids false matches from the child node's own `pos`.
        assert_eq!(
            json,
            r#"{"children":[{"pos":{"end_col":0,"end_line":0,"end_offset":0,"start_col":0,"start_line":0,"start_offset":0},"roles":[],"token":"x","type":"Identifier"}],"id":"dead","pos":{"end_col":0,"end_line":0,"end_offset":0,"start_col":0,"start_line":0,"start_offset":0},"props":{"lang":"go"},"roles":["Name"],"token":"foo","type":"Function"}"#
        );
    }

    #[test]
    fn to_map_props_keys_sorted() {
        let mut props = std::collections::HashMap::new();
        props.insert("b".to_string(), "2".to_string());
        props.insert("a".to_string(), "1".to_string());
        let n = Builder::new().with_type("X").with_props(props).build();
        let json = Encoder::marshal().encode_to_string(&n.to_map());
        assert!(json.contains(r#""props":{"a":"1","b":"2"}"#));
    }
}
