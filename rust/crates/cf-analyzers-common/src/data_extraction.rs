//! Generic UAST node data extraction (`data_extraction.go`).
//!
//! Extracts names, values, types, roles, positions, and properties from UAST
//! nodes via configurable extractor closures, plus a set of standalone helpers
//! (`extract_entity_name`, `extract_name_from_props`, …) reused across
//! analyzers.

use std::collections::BTreeMap;

use crate::node::Node;
use crate::report::{Item, Value};

/// Extracts a name from a node, returning `(name, found)`.
pub type NameExtractor = Box<dyn Fn(&Node) -> Option<String>>;

/// Extracts a value from a node, returning `(value, found)`.
pub type ValueExtractor = Box<dyn Fn(&Node) -> Option<Value>>;

/// Configuration for a [`DataExtractor`].
///
/// Mirrors `common.ExtractionConfig`. When `default_extractors` is set, the
/// built-in extractors are merged in, with caller-supplied entries overriding
/// defaults of the same key.
#[derive(Default)]
pub struct ExtractionConfig {
    /// Named name-extractors.
    pub name_extractors: BTreeMap<String, NameExtractor>,
    /// Named value-extractors.
    pub value_extractors: BTreeMap<String, ValueExtractor>,
    /// Whether to merge in the built-in default extractors.
    pub default_extractors: bool,
}

/// Generic node data extractor.
///
/// Mirrors `common.DataExtractor`.
pub struct DataExtractor {
    config: ExtractionConfig,
}

impl DataExtractor {
    /// Creates an extractor, merging in defaults when requested.
    ///
    /// Mirrors `common.NewDataExtractor`. Default extractors are added only for
    /// keys the caller did not already provide.
    #[must_use]
    pub fn new(mut config: ExtractionConfig) -> Self {
        if config.default_extractors {
            for (k, v) in default_name_extractors() {
                config.name_extractors.entry(k).or_insert(v);
            }
            for (k, v) in default_value_extractors() {
                config.value_extractors.entry(k).or_insert(v);
            }
        }
        DataExtractor { config }
    }

    /// Extracts a name using the named extractor, or `None` if the extractor
    /// does not exist or yields nothing. Mirrors `DataExtractor.ExtractName`.
    #[must_use]
    pub fn extract_name(&self, n: &Node, extractor_key: &str) -> Option<String> {
        self.config
            .name_extractors
            .get(extractor_key)
            .and_then(|e| e(n))
    }

    /// Extracts a value using the named extractor. Mirrors
    /// `DataExtractor.ExtractValue`.
    #[must_use]
    pub fn extract_value(&self, n: &Node, extractor_key: &str) -> Option<Value> {
        self.config
            .value_extractors
            .get(extractor_key)
            .and_then(|e| e(n))
    }

    /// Extracts a name from node properties. Mirrors
    /// `DataExtractor.ExtractNameFromProps`.
    #[must_use]
    pub fn extract_name_from_props(&self, n: &Node, prop_key: &str) -> Option<String> {
        extract_name_from_props(n, prop_key)
    }

    /// Extracts a name from a node token. Mirrors
    /// `DataExtractor.ExtractNameFromToken`.
    #[must_use]
    pub fn extract_name_from_token(&self, n: &Node) -> Option<String> {
        extract_name_from_token(n)
    }

    /// Extracts a name from node children. Mirrors
    /// `DataExtractor.ExtractNameFromChildren`.
    #[must_use]
    pub fn extract_name_from_children(&self, n: &Node, child_index: usize) -> Option<String> {
        extract_name_from_children(n, child_index)
    }

    /// Extracts the node type. Returns `None` only for an absent node, matching
    /// Go's `nil` check (an empty type string is still "found").
    #[must_use]
    pub fn extract_node_type(&self, n: Option<&Node>) -> Option<String> {
        n.map(|node| node.node_type.clone())
    }

    /// Extracts node roles, or `None` when there are none. Mirrors
    /// `DataExtractor.ExtractNodeRoles`.
    #[must_use]
    pub fn extract_node_roles(&self, n: &Node) -> Option<Vec<String>> {
        if n.roles.is_empty() {
            None
        } else {
            Some(n.roles.clone())
        }
    }

    /// Extracts node position as a map keyed by `start_line`, `end_line`,
    /// `start_col`, `end_col`, `start_offset`, `end_offset`. Mirrors
    /// `DataExtractor.ExtractNodePosition`. Values are unsigned to match Go.
    #[must_use]
    pub fn extract_node_position(&self, target: &Node) -> Option<Item> {
        position_map(target)
    }

    /// Extracts a copy of all node properties, or `None` when there are none.
    /// Mirrors `DataExtractor.ExtractNodeProperties`.
    #[must_use]
    pub fn extract_node_properties(&self, n: &Node) -> Option<BTreeMap<String, String>> {
        if n.props.is_empty() {
            None
        } else {
            Some(n.props.clone())
        }
    }

    /// Extracts the number of children. Mirrors
    /// `DataExtractor.ExtractChildCount` (always "found" for a present node).
    #[must_use]
    pub fn extract_child_count(&self, n: Option<&Node>) -> Option<usize> {
        n.map(|node| node.children.len())
    }
}

/// Extracts a name from a node (function, variable, class, …).
///
/// Tries `props["name"]`, then the token, then the first child's
/// token/properties. Mirrors `common.ExtractEntityName`.
#[must_use]
pub fn extract_entity_name(n: Option<&Node>) -> Option<String> {
    let n = n?;
    if let Some(name) = extract_name_from_props(n, "name") {
        return Some(name);
    }
    if let Some(name) = extract_name_from_token(n) {
        return Some(name);
    }
    extract_name_from_children(n, 0)
}

/// Extracts a named property value. Mirrors `common.ExtractNameFromProps`.
#[must_use]
pub fn extract_name_from_props(n: &Node, prop_key: &str) -> Option<String> {
    n.props.get(prop_key).cloned()
}

/// Extracts a non-empty token. Mirrors `common.ExtractNameFromToken`.
#[must_use]
pub fn extract_name_from_token(n: &Node) -> Option<String> {
    if n.token.is_empty() {
        None
    } else {
        Some(n.token.clone())
    }
}

/// Extracts a name from the child at `child_index`, trying its token then its
/// `name` property. Mirrors `common.ExtractNameFromChildren`.
#[must_use]
pub fn extract_name_from_children(n: &Node, child_index: usize) -> Option<String> {
    let child = n.children.get(child_index)?;
    if let Some(name) = extract_name_from_token(child) {
        return Some(name);
    }
    extract_name_from_props(child, "name")
}

/// Builds the position map shared by the position extractors.
fn position_map(target: &Node) -> Option<Item> {
    let pos = target.pos.as_ref()?;
    let mut m = Item::new();
    m.insert("start_line".into(), Value::Uint(u64::from(pos.start_line)));
    m.insert("end_line".into(), Value::Uint(u64::from(pos.end_line)));
    m.insert("start_col".into(), Value::Uint(u64::from(pos.start_col)));
    m.insert("end_col".into(), Value::Uint(u64::from(pos.end_col)));
    m.insert("start_offset".into(), Value::Uint(u64::from(pos.start_offset)));
    m.insert("end_offset".into(), Value::Uint(u64::from(pos.end_offset)));
    Some(m)
}

/// Returns the built-in name extractors (`token`, `props_name`, `props_id`).
fn default_name_extractors() -> Vec<(String, NameExtractor)> {
    vec![
        (
            "token".into(),
            Box::new(|n: &Node| {
                if n.token.is_empty() {
                    None
                } else {
                    Some(n.token.clone())
                }
            }) as NameExtractor,
        ),
        (
            "props_name".into(),
            Box::new(|n: &Node| n.props.get("name").cloned()) as NameExtractor,
        ),
        (
            "props_id".into(),
            Box::new(|n: &Node| n.props.get("id").cloned()) as NameExtractor,
        ),
    ]
}

/// Returns the built-in value extractors (`type`, `roles`, `position`,
/// `properties`, `child_count`).
fn default_value_extractors() -> Vec<(String, ValueExtractor)> {
    vec![
        (
            "type".into(),
            Box::new(|n: &Node| Some(Value::Str(n.node_type.clone()))) as ValueExtractor,
        ),
        (
            "roles".into(),
            Box::new(|n: &Node| {
                if n.roles.is_empty() {
                    None
                } else {
                    Some(Value::List(
                        n.roles.iter().cloned().map(Value::Str).collect(),
                    ))
                }
            }) as ValueExtractor,
        ),
        (
            "position".into(),
            Box::new(|n: &Node| position_map(n).map(Value::Item)) as ValueExtractor,
        ),
        (
            "properties".into(),
            Box::new(|n: &Node| {
                if n.props.is_empty() {
                    None
                } else {
                    let mut m = Item::new();
                    for (k, v) in &n.props {
                        m.insert(k.clone(), Value::Str(v.clone()));
                    }
                    Some(Value::Item(m))
                }
            }) as ValueExtractor,
        ),
        (
            "child_count".into(),
            Box::new(|n: &Node| Some(Value::Int(n.children.len() as i64))) as ValueExtractor,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Positions;

    fn node() -> Node {
        Node::default()
    }

    #[test]
    fn extract_name_token() {
        let de = DataExtractor::new(ExtractionConfig {
            default_extractors: true,
            ..Default::default()
        });
        let mut n = node();
        n.token = "myFunc".into();
        assert_eq!(de.extract_name(&n, "token").as_deref(), Some("myFunc"));
    }

    #[test]
    fn extract_name_props_name() {
        let de = DataExtractor::new(ExtractionConfig {
            default_extractors: true,
            ..Default::default()
        });
        let mut n = node();
        n.props.insert("name".into(), "myVar".into());
        assert_eq!(de.extract_name(&n, "props_name").as_deref(), Some("myVar"));
    }

    #[test]
    fn extract_name_not_found() {
        let de = DataExtractor::new(ExtractionConfig {
            default_extractors: true,
            ..Default::default()
        });
        assert_eq!(de.extract_name(&node(), "nonexistent"), None);
    }

    #[test]
    fn extract_value_type() {
        let de = DataExtractor::new(ExtractionConfig {
            default_extractors: true,
            ..Default::default()
        });
        let mut n = node();
        n.node_type = "Function".into();
        assert_eq!(
            de.extract_value(&n, "type"),
            Some(Value::Str("Function".into()))
        );
    }

    #[test]
    fn entity_name_priority() {
        let from_props = Node {
            props: [("name".to_string(), "funcName".to_string())].into(),
            ..Default::default()
        };
        assert_eq!(extract_entity_name(Some(&from_props)).as_deref(), Some("funcName"));

        let from_token = Node {
            token: "tokenName".into(),
            ..Default::default()
        };
        assert_eq!(extract_entity_name(Some(&from_token)).as_deref(), Some("tokenName"));

        let from_child = Node {
            children: vec![Node {
                token: "childToken".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(extract_entity_name(Some(&from_child)).as_deref(), Some("childToken"));

        assert_eq!(extract_entity_name(None), None);
    }

    #[test]
    fn name_from_props_helper() {
        let n = Node {
            props: [
                ("name".to_string(), "test".to_string()),
                ("id".to_string(), "123".to_string()),
            ]
            .into(),
            ..Default::default()
        };
        assert_eq!(extract_name_from_props(&n, "name").as_deref(), Some("test"));
        assert_eq!(extract_name_from_props(&n, "id").as_deref(), Some("123"));
    }

    #[test]
    fn name_from_token_helper() {
        let n = Node {
            token: "tokenValue".into(),
            ..Default::default()
        };
        assert_eq!(extract_name_from_token(&n).as_deref(), Some("tokenValue"));
        assert_eq!(extract_name_from_token(&node()), None);
    }

    #[test]
    fn name_from_children_helper() {
        let n = Node {
            children: vec![
                Node {
                    token: "child0".into(),
                    ..Default::default()
                },
                Node {
                    token: "child1".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(extract_name_from_children(&n, 0).as_deref(), Some("child0"));
        assert_eq!(extract_name_from_children(&n, 1).as_deref(), Some("child1"));
        assert_eq!(extract_name_from_children(&n, 5), None);
    }

    #[test]
    fn node_type_extraction() {
        let de = DataExtractor::new(ExtractionConfig::default());
        let n = Node {
            node_type: "Class".into(),
            ..Default::default()
        };
        assert_eq!(de.extract_node_type(Some(&n)).as_deref(), Some("Class"));
        assert_eq!(de.extract_node_type(None), None);
    }

    #[test]
    fn node_roles_extraction() {
        let de = DataExtractor::new(ExtractionConfig::default());
        let n = Node {
            roles: vec!["Declaration".into(), "Function".into()],
            ..Default::default()
        };
        let roles = de.extract_node_roles(&n).unwrap();
        assert_eq!(roles, vec!["Declaration", "Function"]);
        assert_eq!(de.extract_node_roles(&node()), None);
    }

    #[test]
    fn node_position_extraction() {
        let de = DataExtractor::new(ExtractionConfig::default());
        let n = Node {
            pos: Some(Positions {
                start_line: 10,
                end_line: 20,
                start_col: 5,
                end_col: 15,
                ..Default::default()
            }),
            ..Default::default()
        };
        let pos = de.extract_node_position(&n).unwrap();
        assert_eq!(pos.get("start_line"), Some(&Value::Uint(10)));
        assert_eq!(pos.get("end_line"), Some(&Value::Uint(20)));
    }

    #[test]
    fn node_properties_extraction() {
        let de = DataExtractor::new(ExtractionConfig::default());
        let n = Node {
            props: [
                ("key1".to_string(), "val1".to_string()),
                ("key2".to_string(), "val2".to_string()),
            ]
            .into(),
            ..Default::default()
        };
        let props = de.extract_node_properties(&n).unwrap();
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn child_count_extraction() {
        let de = DataExtractor::new(ExtractionConfig::default());
        let n = Node {
            children: vec![node(), node(), node()],
            ..Default::default()
        };
        assert_eq!(de.extract_child_count(Some(&n)), Some(3));
    }

    #[test]
    fn name_from_children_nil_child_via_empty_token() {
        // The Rust Node has no nullable child; the equivalent "no usable name"
        // case is a child with empty token and no name prop.
        let n = Node {
            children: vec![node()],
            ..Default::default()
        };
        assert_eq!(extract_name_from_children(&n, 0), None);
    }

    #[test]
    fn custom_extractor() {
        let mut config = ExtractionConfig::default();
        config.name_extractors.insert(
            "custom".into(),
            Box::new(|n: &Node| Some(format!("custom_{}", n.token))),
        );
        let de = DataExtractor::new(config);
        let n = Node {
            token: "test".into(),
            ..Default::default()
        };
        assert_eq!(de.extract_name(&n, "custom").as_deref(), Some("custom_test"));
    }
}
