//! Operator / operand detection (`detector.go`).
//!
//! Halstead classifies every token as an *operator* or an *operand*. This module
//! reproduces the Go detector's classification tables and token-extraction rules
//! exactly.
//!
//! ## Node abstraction
//!
//! The Go detector reads five fields off a `*node.Node`: `Type`, `Token`,
//! `Roles`, `Props`, `Children`, plus the helper `HasAnyRole`. The real node
//! type lives in `cf-uast-node`. To keep this analyzer testable in isolation
//! (and to avoid hard-coupling to an API surface that is still settling), the
//! detector is generic over the [`HalNode`] trait; the production wiring
//! implements [`HalNode`] for `cf_uast_node::Node`.
//!
//! See the crate-level todos for the exact `cf_uast_node::Node` adapter that
//! must be supplied at integration time.

use std::collections::HashMap;

/// Minimal read-only view of a UAST node, covering exactly the fields the
/// Halstead detector inspects.
///
/// Implement this for `cf_uast_node::Node` in the integration layer. The string
/// values returned for `node_type`, `token`, role names, and prop keys must be
/// the same UAST canonical strings the Go `node` package emits (e.g. `"Function"`,
/// `"Identifier"`, role `"Operator"`, prop `"operator"`), because those strings
/// flow into the operator/operand maps and therefore into machine output.
pub trait HalNode {
    /// The node's UAST type string (`node.Type`), e.g. `"BinaryOp"`.
    fn node_type(&self) -> &str;
    /// The node's literal token text (`node.Token`), empty if none.
    fn token(&self) -> &str;
    /// True if the node carries any of the named roles (`HasAnyRole`).
    fn has_any_role(&self, roles: &[&str]) -> bool;
    /// Looks up a property by key (`node.Props[key]`).
    fn prop(&self, key: &str) -> Option<&str>;
    /// The node's children, in order (`node.Children`).
    fn children(&self) -> &[Self]
    where
        Self: Sized;
}

// --- UAST canonical strings the Go detector matches against ---
//
// These mirror the constants in `pkg/uast/pkg/node` referenced by detector.go.

/// UAST type strings classified as operators.
pub const OPERATOR_TYPES: &[&str] = &[
    "BinaryOp",   // UASTBinaryOp
    "UnaryOp",    // UASTUnaryOp
    "Assignment", // UASTAssignment
    "Call",       // UASTCall
    "Index",      // UASTIndex
    "Slice",      // UASTSlice
    "Return",     // UASTReturn
];

/// UAST role strings classified as operators.
pub const OPERATOR_ROLES: &[&str] = &[
    "Operator",   // RoleOperator
    "Assignment", // RoleAssignment
    "Call",       // RoleCall
    "Return",     // RoleReturn
];

/// UAST type strings classified as operands.
pub const OPERAND_TYPES: &[&str] = &[
    "Identifier", // UASTIdentifier
    "Literal",    // UASTLiteral
    "Field",      // UASTField
];

/// UAST role strings classified as operands.
pub const OPERAND_ROLES: &[&str] = &[
    "Name",     // RoleName
    "Literal",  // RoleLiteral
    "Variable", // RoleVariable
    "Argument", // RoleArgument
];

/// UAST type strings that represent declarations (parent context that suppresses
/// counting a declaration identifier as an operand).
pub const DECLARATION_TYPES: &[&str] = &[
    "Function",     // UASTFunction
    "FunctionDecl", // UASTFunctionDecl
    "Method",       // UASTMethod
    "Parameter",    // UASTParameter
    "Variable",     // UASTVariable
    "Field",        // UASTField
    "Import",       // UASTImport
    "Package",      // UASTPackage
    "Struct",       // UASTStruct
    "Class",        // UASTClass
    "Interface",    // UASTInterface
    "Enum",         // UASTEnum
];

/// Roles on the *parent* that mark a child name-identifier as a declaration.
const DECLARATION_PARENT_ROLES: &[&str] = &[
    "Declaration", // RoleDeclaration
    "Parameter",   // RoleParameter
    "Import",      // RoleImport
    "Type",        // RoleType
];

/// Exact operator tokens for O(1) membership testing (`tokenOperatorSet`).
const TOKEN_OPERATOR_SET: &[&str] = &[
    "===", "!==", "==", "!=", "<=", ">=", "&&", "||", "<<=", ">>=", "<<", ">>", "**", ":=", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "=", "<", ">", "&", "|",
    "^", "!",
];

/// Operators sorted longest-first for containment matching so `===` matches
/// before `==` (`tokenOperatorsByLength`). Order is load-bearing.
const TOKEN_OPERATORS_BY_LENGTH: &[&str] = &[
    "===", "!==", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "**", ":=", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "=", "<", ">", "&", "|",
    "^", "!",
];

/// Detects operators and operands in UAST nodes (`OperatorOperandDetector`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OperatorOperandDetector;

impl OperatorOperandDetector {
    /// Creates a new detector.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Recursively collects operators and operands from a node, accumulating
    /// counts into the supplied maps (`CollectOperatorsAndOperands`).
    pub fn collect<N: HalNode>(
        &self,
        node: &N,
        operators: &mut HashMap<String, i64>,
        operands: &mut HashMap<String, i64>,
    ) {
        self.walk(node, None, operators, operands);
    }

    fn walk<N: HalNode>(
        &self,
        node: &N,
        parent: Option<&N>,
        operators: &mut HashMap<String, i64>,
        operands: &mut HashMap<String, i64>,
    ) {
        if self.is_operator(node) {
            self.record_operator(node, operators);
        } else {
            self.record_operand(node, parent, operands);
        }

        for child in node.children() {
            self.walk(child, Some(node), operators, operands);
        }
    }

    fn record_operator<N: HalNode>(&self, node: &N, operators: &mut HashMap<String, i64>) {
        let operator = self.operator_name(node);
        if operator.is_empty() {
            return;
        }
        *operators.entry(operator).or_insert(0) += 1;
    }

    fn record_operand<N: HalNode>(
        &self,
        node: &N,
        parent: Option<&N>,
        operands: &mut HashMap<String, i64>,
    ) {
        if !self.is_operand(node) || !self.should_count_operand(node, parent) {
            return;
        }
        let operand = self.operand_name(node);
        if operand.is_empty() {
            return;
        }
        *operands.entry(operand).or_insert(0) += 1;
    }

    /// True if the node is an operator by type or role (`IsOperator`).
    #[must_use]
    pub fn is_operator<N: HalNode>(&self, node: &N) -> bool {
        OPERATOR_TYPES.contains(&node.node_type()) || node.has_any_role(OPERATOR_ROLES)
    }

    /// True if the node is an operand by type or role (`IsOperand`).
    #[must_use]
    pub fn is_operand<N: HalNode>(&self, node: &N) -> bool {
        OPERAND_TYPES.contains(&node.node_type()) || node.has_any_role(OPERAND_ROLES)
    }

    /// Extracts the operator name (`GetOperatorName`): prefer the `operator`
    /// prop, then a containment match inside the token, then the raw token, then
    /// the node type.
    #[must_use]
    pub fn operator_name<N: HalNode>(&self, node: &N) -> String {
        if let Some(op) = node.prop("operator") {
            return op.to_string();
        }
        if let Some(op) = extract_operator_from_token(node.token()) {
            return op.to_string();
        }
        if !node.token().is_empty() {
            return node.token().to_string();
        }
        node.node_type().to_string()
    }

    /// Extracts the operand name (`GetOperandName`): prefer the token, then the
    /// `name` prop, then the `value` prop, else empty.
    #[must_use]
    pub fn operand_name<N: HalNode>(&self, node: &N) -> String {
        if !node.token().is_empty() {
            return node.token().to_string();
        }
        if let Some(name) = node.prop("name") {
            return name.to_string();
        }
        if let Some(value) = node.prop("value") {
            return value.to_string();
        }
        String::new()
    }

    fn should_count_operand<N: HalNode>(&self, node: &N, parent: Option<&N>) -> bool {
        if is_declaration_identifier(node, parent) {
            return false;
        }
        !self.operand_name(node).is_empty()
    }
}

/// Public re-export of [`is_declaration_identifier`] for the streaming visitor,
/// which reproduces the detector's operand-counting guard inline.
#[must_use]
pub fn is_declaration_identifier_pub<N: HalNode>(node: &N, parent: Option<&N>) -> bool {
    is_declaration_identifier(node, parent)
}

/// True if `node` is a declaration's name identifier given its `parent`
/// (`isDeclarationIdentifier`). Such identifiers are NOT counted as operands.
fn is_declaration_identifier<N: HalNode>(node: &N, parent: Option<&N>) -> bool {
    let Some(parent) = parent else { return false };

    if node.node_type() != "Identifier" || !node.has_any_role(&["Name"]) {
        return false;
    }

    if parent.has_any_role(DECLARATION_PARENT_ROLES) {
        return true;
    }

    DECLARATION_TYPES.contains(&parent.node_type())
}

/// Extracts an operator from a free-form token (`extractOperatorFromToken`):
/// trims, tries an exact set match, then a longest-first containment match where
/// the operator is surrounded by spaces (`" op "`).
fn extract_operator_from_token(token: &str) -> Option<&'static str> {
    if token.trim().is_empty() {
        return None;
    }

    if let Some(&op) = TOKEN_OPERATOR_SET.iter().find(|&&op| op == token) {
        return Some(op);
    }

    for &op in TOKEN_OPERATORS_BY_LENGTH {
        let needle = format!(" {op} ");
        if token.contains(&needle) {
            return Some(op);
        }
    }

    None
}

#[cfg(test)]
pub(crate) mod test_support {
    //! A concrete in-crate node used by unit tests, mirroring how the Go tests
    //! construct `node.New(...)` literals.
    use super::HalNode;
    use std::collections::HashMap;

    /// A simple owned node for tests.
    #[derive(Debug, Clone, Default)]
    pub struct TestNode {
        pub node_type: String,
        pub token: String,
        pub roles: Vec<String>,
        pub props: HashMap<String, String>,
        pub children: Vec<TestNode>,
    }

    impl TestNode {
        pub fn new(node_type: &str) -> Self {
            Self {
                node_type: node_type.to_string(),
                ..Default::default()
            }
        }

        pub fn with_token(mut self, token: &str) -> Self {
            self.token = token.to_string();
            self
        }

        pub fn with_roles(mut self, roles: &[&str]) -> Self {
            self.roles = roles.iter().map(|r| (*r).to_string()).collect();
            self
        }

        pub fn with_prop(mut self, key: &str, value: &str) -> Self {
            self.props.insert(key.to_string(), value.to_string());
            self
        }

        pub fn child(mut self, child: TestNode) -> Self {
            self.children.push(child);
            self
        }
    }

    impl HalNode for TestNode {
        fn node_type(&self) -> &str {
            &self.node_type
        }
        fn token(&self) -> &str {
            &self.token
        }
        fn has_any_role(&self, roles: &[&str]) -> bool {
            self.roles.iter().any(|r| roles.contains(&r.as_str()))
        }
        fn prop(&self, key: &str) -> Option<&str> {
            self.props.get(key).map(String::as_str)
        }
        fn children(&self) -> &[Self] {
            &self.children
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TestNode;
    use super::*;

    /// Ported from `TestAnalyzer_OperatorOperandDetection`.
    #[test]
    fn operator_and_operand_detection() {
        let d = OperatorOperandDetector::new();

        let op = TestNode::new("BinaryOp")
            .with_token("+")
            .with_roles(&["Operator"])
            .with_prop("operator", "+");
        assert!(d.is_operator(&op));

        let operand = TestNode::new("Identifier")
            .with_token("x")
            .with_roles(&["Variable"])
            .with_prop("name", "x");
        assert!(d.is_operand(&operand));

        let lit = TestNode::new("Literal")
            .with_token("42")
            .with_roles(&["Literal"])
            .with_prop("value", "42");
        assert!(d.is_operand(&lit));
    }

    /// Ported from `TestExtractOperatorFromToken`-style benchmark inputs.
    #[test]
    fn extract_operator_from_token_cases() {
        assert_eq!(extract_operator_from_token("=="), Some("=="));
        assert_eq!(extract_operator_from_token("a + b"), Some("+"));
        assert_eq!(extract_operator_from_token("x"), None);
        assert_eq!(extract_operator_from_token("  "), None);
        // longest-first: "===" must win over "==" on an exact token.
        assert_eq!(extract_operator_from_token("==="), Some("==="));
    }

    /// `RealAggregation`-style structure: `x = (y + 5)` yields 2 operators
    /// (`=`,`+`) and 3 operands (`x`,`y`,`5`).
    #[test]
    fn collect_simple_function() {
        let function = TestNode::new("Function")
            .with_roles(&["Function", "Declaration"])
            .with_prop("name", "testFunction")
            .child(
                TestNode::new("Assignment")
                    .with_token("=")
                    .with_roles(&["Assignment"])
                    .with_prop("operator", "=")
                    .child(
                        TestNode::new("Identifier")
                            .with_token("x")
                            .with_roles(&["Variable"])
                            .with_prop("name", "x"),
                    )
                    .child(
                        TestNode::new("BinaryOp")
                            .with_token("+")
                            .with_roles(&["Operator"])
                            .with_prop("operator", "+")
                            .child(
                                TestNode::new("Identifier")
                                    .with_token("y")
                                    .with_roles(&["Variable"])
                                    .with_prop("name", "y"),
                            )
                            .child(
                                TestNode::new("Literal")
                                    .with_token("5")
                                    .with_roles(&["Literal"])
                                    .with_prop("value", "5"),
                            ),
                    ),
            );

        let mut operators = HashMap::new();
        let mut operands = HashMap::new();
        OperatorOperandDetector::new().collect(&function, &mut operators, &mut operands);

        assert_eq!(operators.len(), 2);
        assert_eq!(operators.get("="), Some(&1));
        assert_eq!(operators.get("+"), Some(&1));

        assert_eq!(operands.len(), 3);
        assert_eq!(operands.get("x"), Some(&1));
        assert_eq!(operands.get("y"), Some(&1));
        assert_eq!(operands.get("5"), Some(&1));
    }

    /// A declaration name-identifier under a declaration parent is NOT an
    /// operand (mirrors `TestAnalyzer_DoesNotCountParameterNodeAsOperand`).
    #[test]
    fn declaration_identifier_not_counted() {
        let function = TestNode::new("Function")
            .with_roles(&["Function", "Declaration"])
            .child(
                TestNode::new("Identifier")
                    .with_token("foo")
                    .with_roles(&["Name"])
                    .with_prop("name", "foo"),
            );

        let mut operators = HashMap::new();
        let mut operands = HashMap::new();
        OperatorOperandDetector::new().collect(&function, &mut operators, &mut operands);

        // The name identifier under the Function (a declaration type) is skipped.
        assert!(operands.is_empty(), "declaration name must not be an operand");
    }
}
