//! [`Node::to_map`] — the byte-identity-critical conversion of a node to its map
//! representation.
//!
//! The report-format contract requires map keys to serialize in raw-UTF-8 byte
//! order (pinned by `tests/compat`). To guarantee that, `to_map` returns a
//! [`cf_gojson::GoValue`] whose objects are built with
//! [`cf_gojson::GoMap::from_map`] (sort-on-encode). The full key set a node can
//! emit is `children, id, pos, props, roles, token, type`, which in byte order
//! becomes exactly that sequence — see DESIGN.md §2.2.

use crate::node::{Node, Positions};
use cf_gojson::{GoMap, GoValue};

impl Node {
    /// Converts this node (and its subtree) into a [`GoValue`] whose
    /// serialization is part of the report-format contract (pinned by
    /// `tests/compat`).
    ///
    /// Map-origin objects are used throughout so keys byte-sort on encode. The
    /// field-population rules are frozen:
    /// - `type` is always present (even if empty).
    /// - `id` present only when non-empty, lowercase-hex-encoded.
    /// - `token` present only when non-empty.
    /// - `props` present only when non-empty (its own keys also byte-sort).
    /// - `roles` always present (an array, empty when there are no roles).
    /// - `pos` always present (zeros when the node has no position).
    /// - `children` present only when the node has children.
    ///
    /// There is no nil-node case (a `&Node` is never null); callers that model
    /// an absent node with `Option` should branch before calling.
    #[must_use]
    pub fn to_map(&self) -> GoValue {
        let mut entries: Vec<(String, GoValue)> = Vec::with_capacity(7);

        // `type` is unconditional.
        entries.push(("type".to_string(), GoValue::Str(self.node_type.clone())));

        // `id`: only when non-empty; lowercase hex of the raw ID bytes.
        if !self.id.is_empty() {
            use std::fmt::Write;
            let mut hex = String::with_capacity(self.id.len() * 2);
            for b in &self.id {
                let _ = write!(hex, "{b:02x}");
            }
            entries.push(("id".to_string(), GoValue::Str(hex)));
        }

        // `token`: only when non-empty.
        if !self.token.is_empty() {
            entries.push(("token".to_string(), GoValue::Str(self.token.clone())));
        }

        // `props`: only when non-empty. It is map-origin, so its keys byte-sort.
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

        // `roles`: always present as a (possibly empty) string array.
        let roles: Vec<GoValue> = self.roles.iter().map(|r| GoValue::Str(r.clone())).collect();
        entries.push(("roles".to_string(), GoValue::Array(roles)));

        // `pos` is always present.
        entries.push(("pos".to_string(), position_map(self.pos.as_ref())));

        // `children`: only when present.
        if !self.children.is_empty() {
            let children: Vec<GoValue> = self.children.iter().map(Self::to_map).collect();
            entries.push(("children".to_string(), GoValue::Array(children)));
        }

        GoValue::Object(GoMap::from_map(entries))
    }
}

/// Builds the position sub-map: an absent position yields all-zero fields;
/// otherwise the six `Positions` fields. Either way the six keys
/// (`end_col,end_line,end_offset,start_col,start_line,start_offset` in byte
/// order) are always present.
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
    use crate::node::Builder;
    use cf_gojson::Encoder;

    #[test]
    fn to_map_basic_fields() {
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
        // token, type; `id` is lowercase-hex-encoded; the nested position map's
        // keys are byte-sorted too. Using exact bytes (rather than a substring
        // scan) avoids false matches from the child node's own `pos`.
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
