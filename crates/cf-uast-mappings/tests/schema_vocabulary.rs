//! `uast validate` must accept everything `uast parse` can emit: every UAST
//! type and role any mapping table produces — plus the roles the lowering
//! adds on its own — is a member of the embedded JSON schema's `NodeType` /
//! `Role` enums. Guards the schema against drifting behind the tables.

use std::collections::BTreeSet;

use serde_json::Value;

/// Roles the lowering attaches without a table rule
/// (`cf_uast::lowering`, shape contract).
const LOWERING_ROLES: &[&str] = &[cf_uast_node::ROLE_ELSE];

fn schema_enum(schema: &Value, definition: &str) -> BTreeSet<String> {
    schema["definitions"][definition]["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("schema definition {definition} has an enum"))
        .iter()
        .map(|v| v.as_str().expect("enum member is a string").to_string())
        .collect()
}

#[test]
fn schema_enums_cover_every_emitted_type_and_role() {
    let schema: Value = serde_json::from_str(cf_uast_spec::schema()).expect("schema is JSON");
    let types = schema_enum(&schema, "NodeType");
    let roles = schema_enum(&schema, "Role");

    let mut missing: Vec<String> = Vec::new();
    for (stem, mapping) in cf_uast_mappings::ALL {
        for rule in mapping.rules {
            let t = rule.uast_type.as_str();
            if !types.contains(t) {
                missing.push(format!("{stem}/{}: type {t}", rule.name));
            }
            for r in rule.roles {
                if !roles.contains(r.as_str()) {
                    missing.push(format!("{stem}/{}: role {}", rule.name, r.as_str()));
                }
            }
        }
    }
    for r in LOWERING_ROLES {
        if !roles.contains(*r) {
            missing.push(format!("lowering: role {r}"));
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "schema enums lack {} emitted value(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}
