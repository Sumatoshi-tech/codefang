//! Tree-sitter concrete-syntax-tree → UAST lowering.
//!
//! Direct port of the conversion half of Go `pkg/uast/parser_dsl.go`
//! (`parseContext` and `toCanonicalNode` and friends). Given a parsed
//! tree-sitter [`Tree`] plus the language's mapping [`Rule`]s, it walks the named
//! nodes and produces the canonical [`Node`] tree that feeds `uast parse`'s JSON
//! output.
//!
//! # Byte-parity notes
//!
//! - **Node type resolution.** Go resolves a node's type via an alias-aware
//!   symbol read (`readSymbol`) mapped through `language.SymbolName`, falling back
//!   to `node.Type()`. tree-sitter's [`tsnode::Node::kind`] is exactly
//!   `ts_node_type`, which is alias-aware and identical to the Go result, so it
//!   is used directly.
//! - **Children.** Only *named* children are visited, in tree order, exactly as
//!   the Go `processChildren` family does (the cursor/batch/direct variants in Go
//!   are performance-only and visit the same named children in the same order).
//! - **Positions.** 1-based line/col, 0-based byte offsets — Go adds 1 to the
//!   tree-sitter row/col (`startRow+1`, `startCol+1`).
//! - **Synthetic collapsing.** An unmapped node with exactly one mapped child is
//!   replaced by that child; with several, a `Synthetic` node spanning them is
//!   produced; with none, it is dropped (`createSyntheticNode`).
//! - **IDs.** Go assigns stable IDs in a later pass (the `uast parse` command
//!   calls `AssignStableIDs`); this lowering leaves `id` empty, matching the Go
//!   `Parse` return.

use std::collections::HashMap;

use cf_uast_mapping::{PatternMatcher, Rule};
use cf_uast_node::{Node, Positions};
use tree_sitter::{Node as TsNode, Tree};

/// The minimum named-child count at which Go switches from `NamedChild(idx)` to
/// cursor iteration. Behavioral-only in Go (same nodes, same order); kept as a
/// doc constant for fidelity but not needed in Rust, which always uses the
/// child iterator.
#[allow(dead_code)]
const CURSOR_THRESHOLD: usize = 8;

/// Per-parse lowering state (Go `parseContext`).
pub(crate) struct Lowering<'a> {
    source: &'a [u8],
    rules: &'a [Rule],
    /// First-occurrence-wins rule index keyed by tree-sitter node type
    /// (Go `ruleIndex`).
    rule_index: &'a HashMap<String, usize>,
    /// Compiled-pattern matcher for the language (Go `patternMatcher`). Used only
    /// for `@capture` token/prop extraction and conditions; the embedded Go
    /// mappings exercise neither, but the path is ported for fidelity.
    pattern_matcher: &'a PatternMatcher,
    language: &'a str,
    include_unmapped: bool,
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(
        source: &'a [u8],
        rules: &'a [Rule],
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

    /// Lowers a parsed tree into the canonical UAST root (Go `Parse` body after
    /// `RootNode`). Returns `None` only when the root collapses to nothing (e.g.
    /// an empty `source_file`), matching Go's possible `nil` return.
    pub(crate) fn lower(&self, tree: &Tree) -> Option<Node> {
        let root = tree.root_node();
        self.to_canonical_node(root, "")
    }

    /// Go `nodeType`: the alias-aware node type — `kind()` (`ts_node_type`) —
    /// **provided the node was constructed by a named-child-style API**.
    ///
    /// Aliases are assigned at node-construction time from the parent
    /// production's alias sequence. `ts_node_named_child` (Go's `NamedChild` /
    /// its CGO batch) assigns them only to true production children — an
    /// **extra** (e.g. a comment) never receives one and keeps its raw kind. A
    /// raw `TreeCursor` walk instead smears pending aliases onto extras: in
    /// multi-document YAML the comment after a `---` reports `kind() ==
    /// "document"` from a cursor but `"comment"` from `named_child`, which is
    /// the difference between missing and matching the `comment` mapping rule
    /// (kubernetes `nodelocaldns.yaml` lines 192-193 diverged this way). So
    /// every traversal that feeds this function must construct nodes via
    /// `named_children`/`named_child`, never via a raw cursor walk.
    fn node_type(&self, node: TsNode<'_>) -> &'static str {
        node.kind()
    }

    /// Go `toCanonicalNode`.
    fn to_canonical_node(&self, root: TsNode<'_>, parent_context: &str) -> Option<Node> {
        let node_type = self.node_type(root);
        let mapping_rule = self.find_mapping_rule(node_type);

        let children = self.process_children(root, mapping_rule.as_ref());

        if self.should_skip_node(root, mapping_rule.as_ref()) {
            return None;
        }

        if self.should_skip_empty_file(node_type, &children) {
            return None;
        }

        match mapping_rule {
            Some(rule) => Some(self.create_mapped_node(root, &rule, children)),
            None => self.create_unmapped_node(root, parent_context, node_type),
        }
    }

    /// Go `findMappingRule` + `resolveInheritance`. Returns an owned, fully
    /// inheritance-merged rule so callers see the same fields Go's merged
    /// `*mapping.Rule` exposes.
    fn find_mapping_rule(&self, node_type: &str) -> Option<Rule> {
        let idx = *self.rule_index.get(node_type)?;
        Some(self.resolve_inheritance(self.rules[idx].clone()))
    }

    /// Go `resolveInheritance`: recursively merges a base rule's fields, with the
    /// child rule overriding non-empty scalar fields and copying/extending the
    /// collection fields.
    fn resolve_inheritance(&self, rule: Rule) -> Rule {
        if rule.extends.is_empty() {
            return rule;
        }

        let base_idx = match self.rule_index.get(&rule.extends) {
            Some(&i) => i,
            None => return rule,
        };

        let mut merged = self.rules[base_idx].clone();

        if !rule.pattern.is_empty() {
            merged.pattern = rule.pattern.clone();
        }
        if !rule.uast_spec.r#type.is_empty() {
            merged.uast_spec.r#type = rule.uast_spec.r#type.clone();
        }
        if !rule.uast_spec.token.is_empty() {
            merged.uast_spec.token = rule.uast_spec.token.clone();
        }
        if !rule.uast_spec.roles.is_empty() {
            merged.uast_spec.roles = rule.uast_spec.roles.clone();
        }
        if let Some(child_props) = &rule.uast_spec.props {
            let dst = merged.uast_spec.props.get_or_insert_with(Default::default);
            for (k, v) in child_props {
                dst.insert(k.clone(), v.clone());
            }
        }
        if !rule.uast_spec.children.is_empty() {
            merged.uast_spec.children = rule.uast_spec.children.clone();
        }
        if !rule.conditions.is_empty() {
            merged.conditions.extend(rule.conditions.iter().cloned());
        }

        self.resolve_inheritance(merged)
    }

    /// Go `processChildren` (collapsing the perf-only direct/cursor/batch
    /// variants): visit named children in order, skipping those excluded by their
    /// own rule's conditions, recursing into each.
    fn process_children(&self, root: TsNode<'_>, mapping_rule: Option<&Rule>) -> Vec<Node> {
        let count = root.named_child_count();
        let mut children = Vec::with_capacity(count);
        // Index-based `named_child(idx)` (Go `processChildrenDirect`), NOT a
        // cursor walk and NOT the cursor-backed `named_children` iterator: only
        // per-index construction assigns production aliases correctly (extras
        // keep their raw kind) — see [`Self::node_type`].
        for idx in 0..count {
            let Some(child) = root.named_child(idx) else { continue };
            if !self.should_exclude_child(child, mapping_rule) {
                let child_ctx = self.derive_parent_context(root, mapping_rule);
                if let Some(canonical) = self.to_canonical_node(child, &child_ctx) {
                    children.push(canonical);
                }
            }
        }
        children
    }

    /// Go `deriveParentContext`.
    fn derive_parent_context(&self, root: TsNode<'_>, mapping_rule: Option<&Rule>) -> String {
        if let Some(rule) = mapping_rule {
            if !rule.uast_spec.r#type.is_empty() {
                return rule.uast_spec.r#type.clone();
            }
        }
        self.node_type(root).to_string()
    }

    // ---- pattern matching / captures (Go matchPattern & friends) -----------

    /// Go `matchPattern`: compile the rule's pattern and return the first match's
    /// captures, or `None` when there is no pattern / no match.
    fn match_pattern(
        &self,
        root: TsNode<'_>,
        mapping_rule: Option<&Rule>,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        let rule = mapping_rule?;
        if rule.pattern.is_empty() {
            return None;
        }
        let query = self.pattern_matcher.compile_and_cache(&rule.pattern).ok()?;
        self.pattern_matcher
            .match_pattern(&query, root, self.source)
            .ok()
    }

    /// Go `extractCaptureText`.
    fn extract_capture_text(&self, root: TsNode<'_>, capture_name: &str) -> String {
        let mapping_rule = self.find_mapping_rule(self.node_type(root));
        if let Some(captures) = self.match_pattern(root, mapping_rule.as_ref()) {
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

    // ---- conditions (Go evaluateConditions) --------------------------------

    /// Go `evaluateConditions`.
    fn evaluate_conditions(&self, root: TsNode<'_>, mapping_rule: Option<&Rule>) -> bool {
        let rule = match mapping_rule {
            Some(r) if !r.conditions.is_empty() => r,
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

    /// Go `evaluateCondition` (simple `field == "v"` / `field != "v"`).
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

    /// Go `evaluateComparisonOp`.
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

        // Index-based `named_child(idx)` for alias-correct kinds — see
        // [`Self::node_type`].
        for idx in 0..root.named_child_count() {
            let Some(child) = root.named_child(idx) else { continue };
            if self.node_type(child) == field {
                return compare(&self.extract_node_text(child), val);
            }
        }
        false
    }

    // ---- inclusion / exclusion (Go shouldSkip*) ----------------------------

    /// Go `shouldSkipNode`.
    fn should_skip_node(&self, root: TsNode<'_>, mapping_rule: Option<&Rule>) -> bool {
        if mapping_rule.is_none() {
            return false;
        }
        !self.evaluate_conditions(root, mapping_rule)
    }

    /// Go `shouldExcludeChild`.
    fn should_exclude_child(&self, child: TsNode<'_>, mapping_rule: Option<&Rule>) -> bool {
        if mapping_rule.is_none() {
            return false;
        }
        let child_rule = self.find_mapping_rule(self.node_type(child));
        if child_rule.is_none() {
            return false;
        }
        !self.evaluate_conditions(child, child_rule.as_ref())
    }

    /// Go `shouldSkipEmptyFile`.
    fn should_skip_empty_file(&self, node_type: &str, children: &[Node]) -> bool {
        node_type == "source_file" && children.is_empty() && self.source.is_empty()
    }

    // ---- mapped node construction (Go createMappedNode) --------------------

    /// Go `createMappedNode`.
    fn create_mapped_node(&self, root: TsNode<'_>, rule: &Rule, children: Vec<Node>) -> Node {
        let roles = self.extract_roles(rule);

        // Go lazily allocates the props map only when the rule has props or a
        // name is found; an empty map serializes identically to a nil map under
        // `omitempty`, so building one only when non-empty preserves bytes.
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

        // Go `extractToken` runs after construction and overrides the token if
        // the rule's token spec resolves to a `fields.name`/`text` source.
        self.extract_token(root, rule, &mut node);

        node
    }

    /// Go `extractRoles`.
    fn extract_roles(&self, rule: &Rule) -> Vec<String> {
        rule.uast_spec.roles.clone()
    }

    /// Go `extractName` (always sourced from `fields.name`).
    fn extract_name(&self, root: TsNode<'_>, _rule: &Rule, props: &mut HashMap<String, String>) {
        let name = self.extract_name_from_field(root, "name");
        if !name.is_empty() {
            props.insert("name".to_string(), name);
        }
    }

    /// Go `extractNameFromField`.
    fn extract_name_from_field(&self, root: TsNode<'_>, field_name: &str) -> String {
        match root.child_by_field_name(field_name) {
            Some(field_node) => self.extract_node_text(field_node),
            None => String::new(),
        }
    }

    /// Go `extractNameFromText`.
    fn extract_name_from_text(&self, root: TsNode<'_>) -> String {
        if root.child_count() == 0 {
            return self.extract_node_text(root);
        }
        String::new()
    }

    /// Go `extractProperties`.
    fn extract_properties(&self, root: TsNode<'_>, rule: &Rule, props: &mut HashMap<String, String>) {
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

    /// Go `extractPropertyValue`.
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

    /// Go `extractDirectChildProperty`.
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

    /// Go `extractToken` (post-construction override).
    fn extract_token(&self, root: TsNode<'_>, rule: &Rule, node: &mut Node) {
        if rule.uast_spec.token.is_empty() {
            return;
        }
        let token = self.extract_token_from_node(root, &rule.uast_spec.token);
        if !token.is_empty() {
            node.token = token;
        }
    }

    /// Go `extractTokenFromNode`.
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

    /// Go `extractTokenFromChildType`.
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

    /// Go `findDescendantToken`.
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

    /// Go `extractTokenText` (the token computed *before* construction and passed
    /// to `NewNode`; `extractToken` may later override it).
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

    /// Go `extractPositions`: 1-based line/col, 0-based byte offsets.
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

    // ---- unmapped node construction (Go createUnmappedNode) ----------------

    /// Go `createUnmappedNode`.
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

    /// Go `processUnmappedChildren` (perf variants collapsed): recurse into named
    /// children carrying the same `parent_context`.
    fn process_unmapped_children(&self, root: TsNode<'_>, parent_context: &str) -> Vec<Node> {
        let mut mapped = Vec::new();
        // Index-based `named_child(idx)` for alias-correct kinds — see
        // [`Self::node_type`].
        for idx in 0..root.named_child_count() {
            let Some(child) = root.named_child(idx) else { continue };
            if let Some(canonical) = self.to_canonical_node(child, parent_context) {
                mapped.push(canonical);
            }
        }
        mapped
    }

    /// Go `createIncludeUnmappedNode`.
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

    /// Go `tokenText` (leaf-only text).
    fn token_text(&self, root: TsNode<'_>) -> String {
        if root.child_count() == 0 {
            return self.extract_node_text(root);
        }
        String::new()
    }

    /// Go `createSyntheticNode`.
    fn create_synthetic_node(&self, mapped_children: Vec<Node>) -> Option<Node> {
        match mapped_children.len() {
            1 => mapped_children.into_iter().next(),
            0 => None,
            _ => {
                let pos = compute_children_span(&mapped_children);
                let mut synth =
                    Node::new(Vec::new(), "Synthetic".to_string(), String::new(), Vec::new(), pos, HashMap::new());
                synth.children = mapped_children;
                Some(synth)
            }
        }
    }

    /// Go `extractNodeText`: the node's source slice (an owned copy).
    fn extract_node_text(&self, ts_node: TsNode<'_>) -> String {
        let start = ts_node.start_byte();
        let end = ts_node.end_byte();
        if end <= self.source.len() {
            // Go does `string(source[start:end])`, a lossy-free byte copy. The
            // working-tree bytes are valid UTF-8 for source files; mirror Go's
            // copy with a lossy decode that is a no-op for valid UTF-8.
            return String::from_utf8_lossy(&self.source[start..end]).into_owned();
        }
        String::new()
    }
}

/// Go `findDescendantByType`.
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

/// Go `computeChildrenSpan` + `positionBounds`: the bounding span across all
/// children that carry a position; `None` when no child has one.
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
