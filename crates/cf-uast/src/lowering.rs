//! Tree-sitter concrete-syntax-tree → UAST lowering.
//!
//! Given a parsed tree-sitter [`Tree`] plus the language's mapping [`Rule`]s,
//! it walks the named nodes and produces the canonical [`Node`] tree that
//! feeds `uast parse`'s JSON output.
//!
//! # Byte-parity notes (pinned by the differential gate)
//!
//! - **Node type resolution.** A node's type is its alias-aware kind:
//!   tree-sitter's [`TsNode::kind`] (`ts_node_type`) — but only when the node
//!   was constructed via a named-child API; see [`Lowering::node_type`].
//! - **Children.** Only *named* children are visited, in tree order.
//! - **Positions.** 1-based line/col (tree-sitter row/col + 1), 0-based byte
//!   offsets.
//! - **Synthetic collapsing.** An unmapped node with exactly one mapped child
//!   is replaced by that child; with several, a `Synthetic` node spanning them
//!   is produced; with none, it is dropped.
//! - **IDs.** Stable IDs are assigned in a later pass (the `uast parse`
//!   command calls `assign_stable_ids`); this lowering leaves `id` empty.

use std::collections::HashMap;
use std::sync::Arc;

use cf_uast_mapping::{PatternMatcher, Rule};
use cf_uast_node::{Node, Positions};
use tree_sitter::{Node as TsNode, Query, Tree};

/// A mapping [`Rule`] with its inheritance already merged and its pattern
/// already compiled.
///
/// Built once per language at loader init (see [`resolve_rules`]) so the
/// per-node lowering walk borrows a fully-resolved rule and reuses an
/// already-compiled [`Query`] — eliminating the per-node `Rule` clone and the
/// per-node mutex/`compile_and_cache` round-trip the lazy path performed.
pub(crate) struct ResolvedRule {
    /// The inheritance-merged rule.
    pub rule: Rule,
    /// The compiled pattern, or `None` when the rule has no pattern **or** the
    /// pattern fails to compile.
    ///
    /// Compile-failure parity: the lazy path surfaced a bad pattern at match
    /// time via `compile_and_cache(..).ok()? -> None`, so the node fell through
    /// with no captures. Storing `None` here for a pattern that fails to
    /// compile reproduces that fall-through exactly — init never starts
    /// erroring on patterns that previously only failed lazily.
    pub query: Option<Arc<Query>>,
}

/// Pre-resolves every rule's inheritance and pre-compiles every non-empty
/// pattern, producing the table the lowering walk borrows.
///
/// Inheritance is resolved up front (the embedded corpus uses no `extends`, so
/// this is a passthrough move there, but the general `extends` merge is kept).
/// Each non-empty pattern is compiled via `matcher`; a pattern that fails to
/// compile (or an absent matcher) yields `None`, matching the lazy path's
/// observable fall-through.
pub(crate) fn resolve_rules(
    rules: &[Rule],
    rule_index: &HashMap<String, usize>,
    matcher: Option<&PatternMatcher>,
) -> Vec<ResolvedRule> {
    rules
        .iter()
        .map(|r| {
            let rule = resolve_inheritance(rules, rule_index, r.clone());
            let query = if rule.pattern.is_empty() {
                None
            } else {
                matcher.and_then(|m| m.compile_and_cache(&rule.pattern).ok())
            };
            ResolvedRule { rule, query }
        })
        .collect()
}

/// Recursively merges a base rule's fields, with the child rule overriding
/// non-empty scalar fields and copying/extending the collection fields.
///
/// Moved verbatim from the former `Lowering::resolve_inheritance` so the merge
/// semantics are unchanged; it now runs once per rule at init rather than once
/// per matched node.
fn resolve_inheritance(rules: &[Rule], rule_index: &HashMap<String, usize>, rule: Rule) -> Rule {
    if rule.extends.is_empty() {
        return rule;
    }

    let Some(&base_idx) = rule_index.get(&rule.extends) else {
        return rule;
    };

    let mut merged = rules[base_idx].clone();

    if !rule.pattern.is_empty() {
        merged.pattern.clone_from(&rule.pattern);
    }
    if !rule.uast_spec.r#type.is_empty() {
        merged.uast_spec.r#type.clone_from(&rule.uast_spec.r#type);
    }
    if !rule.uast_spec.token.is_empty() {
        merged.uast_spec.token.clone_from(&rule.uast_spec.token);
    }
    if !rule.uast_spec.roles.is_empty() {
        merged.uast_spec.roles.clone_from(&rule.uast_spec.roles);
    }
    if let Some(child_props) = &rule.uast_spec.props {
        let dst = merged.uast_spec.props.get_or_insert_with(Default::default);
        for (k, v) in child_props {
            dst.insert(k.clone(), v.clone());
        }
    }
    if !rule.uast_spec.children.is_empty() {
        merged
            .uast_spec
            .children
            .clone_from(&rule.uast_spec.children);
    }
    if !rule.conditions.is_empty() {
        merged.conditions.extend(rule.conditions.iter().cloned());
    }

    resolve_inheritance(rules, rule_index, merged)
}

/// The minimum named-child count at which `process_unmapped_children`
/// switches from per-index `named_child(idx)` construction to a raw cursor
/// walk — an observable threshold, because the two traversals differ in alias
/// assignment for "extra" nodes (see [`Lowering::node_type`]).
const CURSOR_THRESHOLD: usize = 8;

/// Per-parse lowering state.
pub struct Lowering<'a> {
    source: &'a [u8],
    /// Pre-resolved rules (inheritance merged, pattern compiled) — borrowed,
    /// never cloned per node.
    rules: &'a [ResolvedRule],
    /// First-occurrence-wins rule index keyed by tree-sitter node type.
    rule_index: &'a HashMap<String, usize>,
    /// Compiled-pattern matcher for the language. Used only to run an
    /// already-compiled `@capture` query against a node; pattern *compilation*
    /// happens once at init, not per node.
    pattern_matcher: &'a PatternMatcher,
    language: &'a str,
    include_unmapped: bool,
}

impl<'a> Lowering<'a> {
    pub(crate) const fn new(
        source: &'a [u8],
        rules: &'a [ResolvedRule],
        rule_index: &'a HashMap<String, usize>,
        pattern_matcher: &'a PatternMatcher,
        language: &'a str,
        include_unmapped: bool,
    ) -> Self {
        Lowering {
            source,
            rules,
            rule_index,
            pattern_matcher,
            language,
            include_unmapped,
        }
    }

    /// Lowers a parsed tree into the canonical UAST root. Returns `None` only
    /// when the root collapses to nothing (e.g. an empty `source_file`).
    pub(crate) fn lower(&self, tree: &Tree) -> Option<Node> {
        let root = tree.root_node();
        self.to_canonical_node(root, "")
    }

    /// The alias-aware node type — `kind()` (`ts_node_type`) — **provided the
    /// node was constructed by a named-child-style API**.
    ///
    /// Aliases are assigned at node-construction time from the parent
    /// production's alias sequence. `ts_node_named_child` assigns them only to
    /// true production children — an **extra** (e.g. a comment) never receives
    /// one and keeps its raw kind. A raw `TreeCursor` walk instead smears
    /// pending aliases onto extras: in multi-document YAML the comment after a
    /// `---` reports `kind() == "document"` from a cursor but `"comment"` from
    /// `named_child`, which is the difference between missing and matching the
    /// `comment` mapping rule (kubernetes `nodelocaldns.yaml` lines 192-193
    /// diverged this way against the reference binary). So every traversal
    /// that feeds this function must construct nodes via
    /// `named_children`/`named_child`, never via a raw cursor walk — except
    /// where the cursor walk itself is the frozen behavior (see
    /// [`Self::process_unmapped_children`]).
    fn node_type(&self, node: TsNode<'_>) -> &'static str {
        node.kind()
    }

    /// Lowers one tree-sitter node (and its subtree) to a canonical node.
    fn to_canonical_node(&self, root: TsNode<'_>, parent_context: &str) -> Option<Node> {
        let node_type = self.node_type(root);
        let mapping_rule = self.find_mapping_rule(node_type);

        let children = self.process_children(root, mapping_rule);

        if self.should_skip_node(root, mapping_rule) {
            return None;
        }

        if self.should_skip_empty_file(node_type, &children) {
            return None;
        }

        match mapping_rule {
            Some(resolved) => Some(self.create_mapped_node(root, resolved, children)),
            None => self.create_unmapped_node(root, parent_context, node_type),
        }
    }

    /// Looks up the node type's pre-resolved mapping rule. Returns a borrow
    /// into the init-time resolution table — no clone, no inheritance merge,
    /// no pattern compile per node.
    fn find_mapping_rule(&self, node_type: &str) -> Option<&'a ResolvedRule> {
        let idx = *self.rule_index.get(node_type)?;
        Some(&self.rules[idx])
    }

    /// Visits named children in order, skipping those excluded by their own
    /// rule's conditions, recursing into each.
    fn process_children(&self, root: TsNode<'_>, mapping_rule: Option<&ResolvedRule>) -> Vec<Node> {
        let count = root.named_child_count();
        let mut children = Vec::with_capacity(count);
        // Index-based `named_child(idx)`, NOT a cursor walk and NOT the
        // cursor-backed `named_children` iterator: only per-index construction
        // assigns production aliases correctly (extras keep their raw kind) —
        // see [`Self::node_type`].
        for idx in 0..count {
            let Some(child) = root.named_child(idx) else {
                continue;
            };
            if !self.should_exclude_child(child, mapping_rule) {
                let child_ctx = self.derive_parent_context(root, mapping_rule);
                if let Some(canonical) = self.to_canonical_node(child, &child_ctx) {
                    children.push(canonical);
                }
            }
        }
        children
    }

    /// The context string a child sees: the parent rule's UAST type, or the
    /// parent's raw node type when unmapped.
    fn derive_parent_context(
        &self,
        root: TsNode<'_>,
        mapping_rule: Option<&ResolvedRule>,
    ) -> String {
        if let Some(resolved) = mapping_rule {
            if !resolved.rule.uast_spec.r#type.is_empty() {
                return resolved.rule.uast_spec.r#type.clone();
            }
        }
        self.node_type(root).to_string()
    }

    // ---- pattern matching / captures ----------------------------------------

    /// Runs the rule's pre-compiled pattern and returns the first match's
    /// captures, or `None` when there is no pattern / it failed to compile at
    /// init / no match.
    ///
    /// The pre-compiled `Option<Arc<Query>>` reproduces the lazy path exactly:
    /// an empty pattern stored `None` (no query), and a pattern that failed to
    /// compile also stored `None` — both fall through to `None` here, just as
    /// `compile_and_cache(..).ok()?` did per node.
    fn match_pattern(
        &self,
        root: TsNode<'_>,
        mapping_rule: Option<&ResolvedRule>,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        let query = mapping_rule?.query.as_ref()?;
        self.pattern_matcher
            .match_pattern(query, root, self.source)
            .ok()
    }

    /// Resolves a `@capture` reference: query captures first, then a field
    /// with that name, then a descendant of that type.
    fn extract_capture_text(&self, root: TsNode<'_>, capture_name: &str) -> String {
        let mapping_rule = self.find_mapping_rule(self.node_type(root));
        if let Some(captures) = self.match_pattern(root, mapping_rule) {
            if let Some(val) = captures.get(capture_name) {
                return val.clone();
            }
        }

        // Fallback: field name.
        if let Some(field_node) = root.child_by_field_name(capture_name) {
            if field_node.child_count() == 0 {
                return self.extract_node_text(field_node);
            }
            return self.node_type(field_node).to_string();
        }

        // Fallback: descendant of the given type.
        if let Some(desc) = find_descendant_by_type(root, capture_name) {
            return self.extract_node_text(desc);
        }

        String::new()
    }

    // ---- conditions ----------------------------------------------------------

    /// Evaluates a rule's conditions; vacuously true without conditions.
    fn evaluate_conditions(&self, root: TsNode<'_>, mapping_rule: Option<&ResolvedRule>) -> bool {
        let rule = match mapping_rule {
            Some(r) if !r.rule.conditions.is_empty() => &r.rule,
            _ => return true,
        };
        let captures = self.match_pattern(root, mapping_rule);
        for cond in &rule.conditions {
            if !self.evaluate_condition(root, &cond.expr, captures.as_ref()) {
                return false;
            }
        }
        true
    }

    /// Evaluates one condition (simple `field == "v"` / `field != "v"`).
    fn evaluate_condition(
        &self,
        root: TsNode<'_>,
        expr: &str,
        captures: Option<&std::collections::BTreeMap<String, String>>,
    ) -> bool {
        let expr = expr.trim();
        if expr.contains("==") {
            return self.evaluate_comparison_op(root, expr, "==", captures, |l, r| l == r);
        }
        if expr.contains("!=") {
            return self.evaluate_comparison_op(root, expr, "!=", captures, |l, r| l != r);
        }
        false
    }

    /// Evaluates one comparison: captured value first, then a field with that
    /// name, then a named child of that type.
    fn evaluate_comparison_op(
        &self,
        root: TsNode<'_>,
        expr: &str,
        op: &str,
        captures: Option<&std::collections::BTreeMap<String, String>>,
        compare: impl Fn(&str, &str) -> bool,
    ) -> bool {
        let parts: Vec<&str> = expr.splitn(2, op).collect();
        if parts.len() != 2 {
            return false;
        }
        let field = parts[0].trim();
        let val = parts[1].trim().trim_matches('"');

        if let Some(caps) = captures {
            if let Some(captured) = caps.get(field) {
                return compare(captured, val);
            }
        }

        if let Some(field_node) = root.child_by_field_name(field) {
            return compare(&self.extract_node_text(field_node), val);
        }

        // Child scanning here uses the CURSOR for all counts — cursor alias
        // semantics included (frozen reference-implementation behavior).
        let mut cursor = root.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.is_named() && self.node_type(child) == field {
                    return compare(&self.extract_node_text(child), val);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    // ---- inclusion / exclusion ------------------------------------------------

    /// A mapped node whose conditions fail is skipped.
    fn should_skip_node(&self, root: TsNode<'_>, mapping_rule: Option<&ResolvedRule>) -> bool {
        if mapping_rule.is_none() {
            return false;
        }
        !self.evaluate_conditions(root, mapping_rule)
    }

    /// A child with a failing-conditions rule of its own is excluded.
    fn should_exclude_child(&self, child: TsNode<'_>, mapping_rule: Option<&ResolvedRule>) -> bool {
        if mapping_rule.is_none() {
            return false;
        }
        let child_rule = self.find_mapping_rule(self.node_type(child));
        if child_rule.is_none() {
            return false;
        }
        !self.evaluate_conditions(child, child_rule)
    }

    /// An empty `source_file` with no children lowers to nothing.
    fn should_skip_empty_file(&self, node_type: &str, children: &[Node]) -> bool {
        node_type == "source_file" && children.is_empty() && self.source.is_empty()
    }

    // ---- mapped node construction ---------------------------------------------

    /// Builds the canonical node for a rule-mapped tree-sitter node.
    fn create_mapped_node(
        &self,
        root: TsNode<'_>,
        resolved: &ResolvedRule,
        children: Vec<Node>,
    ) -> Node {
        let rule = &resolved.rule;
        let roles = self.extract_roles(rule);

        // An empty props map serializes identically to an absent one under
        // `omitempty`, so eagerly building (and only conditionally filling)
        // one preserves bytes.
        let mut props: HashMap<String, String> = HashMap::new();
        self.extract_properties(root, rule, &mut props);
        self.extract_name(root, rule, &mut props);

        let pos = self.extract_positions(root);
        let token_text = self.extract_token_text(root, rule);

        let mut node = Node::new(
            Vec::new(),
            rule.uast_spec.r#type.clone(),
            token_text,
            roles,
            pos,
            props,
        );
        node.children = children;

        // The post-construction pass overrides the token if the rule's token
        // spec resolves to a `fields.name`/`text` source.
        self.extract_token(root, rule, &mut node);

        node
    }

    /// The rule's roles, verbatim.
    fn extract_roles(&self, rule: &Rule) -> Vec<String> {
        rule.uast_spec.roles.clone()
    }

    /// Fills `props["name"]` from the node's `name` field, when present.
    fn extract_name(&self, root: TsNode<'_>, _rule: &Rule, props: &mut HashMap<String, String>) {
        let name = self.extract_name_from_field(root, "name");
        if !name.is_empty() {
            props.insert("name".to_string(), name);
        }
    }

    /// The text of the node's `field_name` field, or empty.
    fn extract_name_from_field(&self, root: TsNode<'_>, field_name: &str) -> String {
        match root.child_by_field_name(field_name) {
            Some(field_node) => self.extract_node_text(field_node),
            None => String::new(),
        }
    }

    /// The node's own text, leaf nodes only.
    fn extract_name_from_text(&self, root: TsNode<'_>) -> String {
        if root.child_count() == 0 {
            return self.extract_node_text(root);
        }
        String::new()
    }

    /// Extracts the rule's props into the node's props map.
    fn extract_properties(
        &self,
        root: TsNode<'_>,
        rule: &Rule,
        props: &mut HashMap<String, String>,
    ) {
        let rule_props = match &rule.uast_spec.props {
            Some(p) => p,
            None => return,
        };
        for (key, value) in rule_props {
            let extracted = self.extract_property_value(root, value);
            if !extracted.is_empty() {
                props.insert(key.clone(), extracted);
            }
        }
    }

    /// Resolves one prop value: `@capture`, `descendant:<type>`, or a direct
    /// child type.
    fn extract_property_value(&self, root: TsNode<'_>, prop: &str) -> String {
        if let Some(capture) = prop.strip_prefix('@') {
            if !capture.is_empty() {
                return self.extract_capture_text(root, capture);
            }
        }
        if let Some(after) = prop.strip_prefix("descendant:") {
            return self.find_descendant_token(root, after);
        }
        self.extract_direct_child_property(root, prop)
    }

    /// The text of the first named child whose type equals `prop`.
    fn extract_direct_child_property(&self, root: TsNode<'_>, prop: &str) -> String {
        let count = root.named_child_count();
        for idx in 0..count {
            if let Some(child) = root.named_child(idx) {
                if self.node_type(child) == prop {
                    return self.extract_node_text(child);
                }
            }
        }
        String::new()
    }

    /// Post-construction token override (see [`Self::create_mapped_node`]).
    fn extract_token(&self, root: TsNode<'_>, rule: &Rule, node: &mut Node) {
        if rule.uast_spec.token.is_empty() {
            return;
        }
        let token = self.extract_token_from_node(root, &rule.uast_spec.token);
        if !token.is_empty() {
            node.token = token;
        }
    }

    /// Resolves a `fields.*`/`props.name`/`text` token source.
    fn extract_token_from_node(&self, root: TsNode<'_>, source: &str) -> String {
        match source {
            "fields.name" => self.extract_name_from_field(root, "name"),
            "props.name" | "text" => self.extract_name_from_text(root),
            _ => {
                if let Some(field) = source.strip_prefix("fields.") {
                    return self.extract_name_from_field(root, field);
                }
                String::new()
            }
        }
    }

    /// The text of the first named child of the given type.
    fn extract_token_from_child_type(&self, root: TsNode<'_>, node_type: &str) -> String {
        let count = root.named_child_count();
        for idx in 0..count {
            if let Some(child) = root.named_child(idx) {
                if self.node_type(child) == node_type {
                    return self.extract_node_text(child);
                }
            }
        }
        String::new()
    }

    /// The text of the first descendant (pre-order, including self) of the
    /// given type.
    fn find_descendant_token(&self, root: TsNode<'_>, node_type: &str) -> String {
        if self.node_type(root) == node_type {
            return self.extract_node_text(root);
        }
        let count = root.named_child_count();
        for idx in 0..count {
            if let Some(child) = root.named_child(idx) {
                let result = self.find_descendant_token(child, node_type);
                if !result.is_empty() {
                    return result;
                }
            }
        }
        String::new()
    }

    /// The token computed *before* construction ([`Self::extract_token`] may
    /// later override it).
    fn extract_token_text(&self, root: TsNode<'_>, rule: &Rule) -> String {
        let token_spec = &rule.uast_spec.token;
        if token_spec.is_empty() {
            return String::new();
        }

        if let Some(capture) = token_spec.strip_prefix('@') {
            return self.extract_capture_text(root, capture);
        }

        match token_spec.as_str() {
            "self" | "text" => self.extract_node_text(root),
            _ => {
                if let Some(after) = token_spec.strip_prefix("child:") {
                    return self.extract_token_from_child_type(root, after);
                }
                if let Some(after) = token_spec.strip_prefix("descendant:") {
                    return self.find_descendant_token(root, after);
                }
                token_spec.clone()
            }
        }
    }

    /// 1-based line/col, 0-based byte offsets.
    fn extract_positions(&self, root: TsNode<'_>) -> Option<Positions> {
        let start = root.start_position();
        let end = root.end_position();
        Some(Positions {
            start_line: start.row as u64 + 1,
            start_col: start.column as u64 + 1,
            start_offset: root.start_byte() as u64,
            end_line: end.row as u64 + 1,
            end_col: end.column as u64 + 1,
            end_offset: root.end_byte() as u64,
        })
    }

    // ---- unmapped node construction ---------------------------------------------

    /// Builds the result for a tree-sitter node with no mapping rule.
    fn create_unmapped_node(
        &self,
        root: TsNode<'_>,
        parent_context: &str,
        node_type: &str,
    ) -> Option<Node> {
        let mapped_children = self.process_unmapped_children(root, parent_context);

        if self.include_unmapped {
            return Some(self.create_include_unmapped_node(root, node_type, mapped_children));
        }
        self.create_synthetic_node(mapped_children)
    }

    /// Recurses into named children carrying the same `parent_context`.
    fn process_unmapped_children(&self, root: TsNode<'_>, parent_context: &str) -> Vec<Node> {
        let count = root.named_child_count();
        let mut mapped = Vec::new();
        // The traversal dispatches on CURSOR_THRESHOLD = 8: below it,
        // per-index `named_child(idx)` — clean production-alias semantics
        // (extras keep their raw kind); at or above it, the raw CURSOR — whose
        // alias smearing onto extras is observable, frozen behavior (pinned by
        // the differential gate). Both halves must be kept exactly; see
        // [`Self::node_type`]. (Note `process_children` differs: it uses the
        // clean per-index construction for all sizes.)
        if count < CURSOR_THRESHOLD {
            for idx in 0..count {
                let Some(child) = root.named_child(idx) else {
                    continue;
                };
                if let Some(canonical) = self.to_canonical_node(child, parent_context) {
                    mapped.push(canonical);
                }
            }
            return mapped;
        }
        let mut cursor = root.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.is_named() {
                    if let Some(canonical) = self.to_canonical_node(child, parent_context) {
                        mapped.push(canonical);
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        mapped
    }

    /// Builds the `<language>:<node_type>` passthrough node used when
    /// unmapped nodes are included.
    fn create_include_unmapped_node(
        &self,
        root: TsNode<'_>,
        node_type: &str,
        mapped_children: Vec<Node>,
    ) -> Node {
        let mut node = Node::new(
            Vec::new(),
            format!("{}:{}", self.language, node_type),
            self.token_text(root),
            Vec::new(),
            self.extract_positions(root),
            HashMap::new(),
        );
        node.children = mapped_children;
        node
    }

    /// The node's own text, leaf nodes only.
    fn token_text(&self, root: TsNode<'_>) -> String {
        if root.child_count() == 0 {
            return self.extract_node_text(root);
        }
        String::new()
    }

    /// Collapses unmapped children: one child passes through, several get a
    /// spanning `Synthetic` parent, none vanishes.
    fn create_synthetic_node(&self, mapped_children: Vec<Node>) -> Option<Node> {
        match mapped_children.len() {
            1 => mapped_children.into_iter().next(),
            0 => None,
            _ => {
                let pos = compute_children_span(&mapped_children);
                let mut synth = Node::new(
                    Vec::new(),
                    "Synthetic".to_string(),
                    String::new(),
                    Vec::new(),
                    pos,
                    HashMap::new(),
                );
                synth.children = mapped_children;
                Some(synth)
            }
        }
    }

    /// The node's source slice (an owned copy).
    fn extract_node_text(&self, ts_node: TsNode<'_>) -> String {
        let start = ts_node.start_byte();
        let end = ts_node.end_byte();
        if end <= self.source.len() {
            // Source bytes are valid UTF-8 for source files; the lossy decode
            // is a no-op for valid UTF-8 and total for anything else.
            return String::from_utf8_lossy(&self.source[start..end]).into_owned();
        }
        String::new()
    }
}

/// The first descendant (pre-order, including self) of the given type.
fn find_descendant_by_type<'tree>(node: TsNode<'tree>, typ: &str) -> Option<TsNode<'tree>> {
    if node.kind() == typ {
        return Some(node);
    }
    let count = node.named_child_count();
    for idx in 0..count {
        if let Some(child) = node.named_child(idx) {
            if let Some(found) = find_descendant_by_type(child, typ) {
                return Some(found);
            }
        }
    }
    None
}

/// The bounding span across all children that carry a position; `None` when
/// no child has one.
fn compute_children_span(children: &[Node]) -> Option<Positions> {
    let mut found = false;
    let mut min_start_line = u64::MAX;
    let mut min_start_col = u64::MAX;
    let mut min_start_offset = u64::MAX;
    let mut max_end_line = 0u64;
    let mut max_end_col = 0u64;
    let mut max_end_offset = 0u64;

    for child in children {
        if let Some(pos) = &child.pos {
            found = true;
            min_start_line = min_start_line.min(pos.start_line);
            min_start_col = min_start_col.min(pos.start_col);
            min_start_offset = min_start_offset.min(pos.start_offset);
            max_end_line = max_end_line.max(pos.end_line);
            max_end_col = max_end_col.max(pos.end_col);
            max_end_offset = max_end_offset.max(pos.end_offset);
        }
    }

    if !found {
        return None;
    }
    Some(Positions {
        start_line: min_start_line,
        start_col: min_start_col,
        start_offset: min_start_offset,
        end_line: max_end_line,
        end_col: max_end_col,
        end_offset: max_end_offset,
    })
}
