//! Core [`Node`] / [`Positions`] types, the [`Builder`], constructors, and the
//! structural mutation/query methods. Ported from `node.go`.

use sha1::{Digest, Sha1};

/// A type label for a node (e.g. `"Function"`, `"Identifier"`).
///
/// Go declares `type Type string`; we use a transparent newtype so it keeps the
/// same comparison/ordering semantics while staying distinct from [`Role`].
pub type Type = String;

/// A syntactic/semantic label for a node (see [`roles`]).
///
/// Go declares `type Role string`; ported as a plain `String`.
pub type Role = String;

/// UAST node type constants. Ported verbatim from `node.go`'s `UAST*` block.
///
/// These are re-exported at the crate root so callers can write
/// `cf_uast_node::UAST_FUNCTION` analogous to Go's `node.UASTFunction`.
pub mod uast_types {
    /// `"File"`.
    pub const UAST_FILE: &str = "File";
    /// `"Function"`.
    pub const UAST_FUNCTION: &str = "Function";
    /// `"FunctionDecl"`.
    pub const UAST_FUNCTION_DECL: &str = "FunctionDecl";
    /// `"Method"`.
    pub const UAST_METHOD: &str = "Method";
    /// `"Class"`.
    pub const UAST_CLASS: &str = "Class";
    /// `"Interface"`.
    pub const UAST_INTERFACE: &str = "Interface";
    /// `"Struct"`.
    pub const UAST_STRUCT: &str = "Struct";
    /// `"Enum"`.
    pub const UAST_ENUM: &str = "Enum";
    /// `"EnumMember"`.
    pub const UAST_ENUM_MEMBER: &str = "EnumMember";
    /// `"Variable"`.
    pub const UAST_VARIABLE: &str = "Variable";
    /// `"Parameter"`.
    pub const UAST_PARAMETER: &str = "Parameter";
    /// `"Block"`.
    pub const UAST_BLOCK: &str = "Block";
    /// `"If"`.
    pub const UAST_IF: &str = "If";
    /// `"Loop"`.
    pub const UAST_LOOP: &str = "Loop";
    /// `"Switch"`.
    pub const UAST_SWITCH: &str = "Switch";
    /// `"Case"`.
    pub const UAST_CASE: &str = "Case";
    /// `"Return"`.
    pub const UAST_RETURN: &str = "Return";
    /// `"Break"`.
    pub const UAST_BREAK: &str = "Break";
    /// `"Continue"`.
    pub const UAST_CONTINUE: &str = "Continue";
    /// `"Assignment"`.
    pub const UAST_ASSIGNMENT: &str = "Assignment";
    /// `"Call"`.
    pub const UAST_CALL: &str = "Call";
    /// `"Identifier"`.
    pub const UAST_IDENTIFIER: &str = "Identifier";
    /// `"Literal"`.
    pub const UAST_LITERAL: &str = "Literal";
    /// `"BinaryOp"`.
    pub const UAST_BINARY_OP: &str = "BinaryOp";
    /// `"UnaryOp"`.
    pub const UAST_UNARY_OP: &str = "UnaryOp";
    /// `"Import"`.
    pub const UAST_IMPORT: &str = "Import";
    /// `"Package"`.
    pub const UAST_PACKAGE: &str = "Package";
    /// `"Attribute"`.
    pub const UAST_ATTRIBUTE: &str = "Attribute";
    /// `"Comment"`.
    pub const UAST_COMMENT: &str = "Comment";
    /// `"DocString"`.
    pub const UAST_DOC_STRING: &str = "DocString";
    /// `"TypeAnnotation"`.
    pub const UAST_TYPE_ANNOTATION: &str = "TypeAnnotation";
    /// `"Field"`.
    pub const UAST_FIELD: &str = "Field";
    /// `"Property"`.
    pub const UAST_PROPERTY: &str = "Property";
    /// `"Getter"`.
    pub const UAST_GETTER: &str = "Getter";
    /// `"Setter"`.
    pub const UAST_SETTER: &str = "Setter";
    /// `"Lambda"`.
    pub const UAST_LAMBDA: &str = "Lambda";
    /// `"Try"`.
    pub const UAST_TRY: &str = "Try";
    /// `"Catch"`.
    pub const UAST_CATCH: &str = "Catch";
    /// `"Finally"`.
    pub const UAST_FINALLY: &str = "Finally";
    /// `"Throw"`.
    pub const UAST_THROW: &str = "Throw";
    /// `"Module"`.
    pub const UAST_MODULE: &str = "Module";
    /// `"Namespace"`.
    pub const UAST_NAMESPACE: &str = "Namespace";
    /// `"Decorator"`.
    pub const UAST_DECORATOR: &str = "Decorator";
    /// `"Spread"`.
    pub const UAST_SPREAD: &str = "Spread";
    /// `"Tuple"`.
    pub const UAST_TUPLE: &str = "Tuple";
    /// `"List"`.
    pub const UAST_LIST: &str = "List";
    /// `"Dict"`.
    pub const UAST_DICT: &str = "Dict";
    /// `"Set"`.
    pub const UAST_SET: &str = "Set";
    /// `"KeyValue"`.
    pub const UAST_KEY_VALUE: &str = "KeyValue";
    /// `"Index"`.
    pub const UAST_INDEX: &str = "Index";
    /// `"Slice"`.
    pub const UAST_SLICE: &str = "Slice";
    /// `"Cast"`.
    pub const UAST_CAST: &str = "Cast";
    /// `"Await"`.
    pub const UAST_AWAIT: &str = "Await";
    /// `"Yield"`.
    pub const UAST_YIELD: &str = "Yield";
    /// `"Generator"`.
    pub const UAST_GENERATOR: &str = "Generator";
    /// `"Comprehension"`.
    pub const UAST_COMPREHENSION: &str = "Comprehension";
    /// `"Pattern"`.
    pub const UAST_PATTERN: &str = "Pattern";
    /// `"Match"`.
    pub const UAST_MATCH: &str = "Match";
    /// `"Synthetic"`.
    pub const UAST_SYNTHETIC: &str = "Synthetic";
}

/// Role constants for syntactic and semantic labeling. Ported from `node.go`.
pub mod roles {
    /// `"Function"`.
    pub const ROLE_FUNCTION: &str = "Function";
    /// `"Declaration"`.
    pub const ROLE_DECLARATION: &str = "Declaration";
    /// `"Name"`.
    pub const ROLE_NAME: &str = "Name";
    /// `"Reference"`.
    pub const ROLE_REFERENCE: &str = "Reference";
    /// `"Assignment"`.
    pub const ROLE_ASSIGNMENT: &str = "Assignment";
    /// `"Call"`.
    pub const ROLE_CALL: &str = "Call";
    /// `"Parameter"`.
    pub const ROLE_PARAMETER: &str = "Parameter";
    /// `"Argument"`.
    pub const ROLE_ARGUMENT: &str = "Argument";
    /// `"Condition"`.
    pub const ROLE_CONDITION: &str = "Condition";
    /// `"Body"`.
    pub const ROLE_BODY: &str = "Body";
    /// `"Exported"`.
    pub const ROLE_EXPORTED: &str = "Exported";
    /// `"Public"`.
    pub const ROLE_PUBLIC: &str = "Public";
    /// `"Private"`.
    pub const ROLE_PRIVATE: &str = "Private";
    /// `"Static"`.
    pub const ROLE_STATIC: &str = "Static";
    /// `"Constant"`.
    pub const ROLE_CONSTANT: &str = "Constant";
    /// `"Mutable"`.
    pub const ROLE_MUTABLE: &str = "Mutable";
    /// `"Getter"`.
    pub const ROLE_GETTER: &str = "Getter";
    /// `"Setter"`.
    pub const ROLE_SETTER: &str = "Setter";
    /// `"Literal"`.
    pub const ROLE_LITERAL: &str = "Literal";
    /// `"Variable"`.
    pub const ROLE_VARIABLE: &str = "Variable";
    /// `"Loop"`.
    pub const ROLE_LOOP: &str = "Loop";
    /// `"Branch"`.
    pub const ROLE_BRANCH: &str = "Branch";
    /// `"Import"`.
    pub const ROLE_IMPORT: &str = "Import";
    /// `"Doc"`.
    pub const ROLE_DOC: &str = "Doc";
    /// `"Comment"`.
    pub const ROLE_COMMENT: &str = "Comment";
    /// `"Attribute"`.
    pub const ROLE_ATTRIBUTE: &str = "Attribute";
    /// `"Annotation"`.
    pub const ROLE_ANNOTATION: &str = "Annotation";
    /// `"Operator"`.
    pub const ROLE_OPERATOR: &str = "Operator";
    /// `"Index"`.
    pub const ROLE_INDEX: &str = "Index";
    /// `"Key"`.
    pub const ROLE_KEY: &str = "Key";
    /// `"Value"`.
    pub const ROLE_VALUE: &str = "Value";
    /// `"Type"`.
    pub const ROLE_TYPE: &str = "Type";
    /// `"Interface"`.
    pub const ROLE_INTERFACE: &str = "Interface";
    /// `"Class"`.
    pub const ROLE_CLASS: &str = "Class";
    /// `"Struct"`.
    pub const ROLE_STRUCT: &str = "Struct";
    /// `"Enum"`.
    pub const ROLE_ENUM: &str = "Enum";
    /// `"Member"`.
    pub const ROLE_MEMBER: &str = "Member";
    /// `"Module"`.
    pub const ROLE_MODULE: &str = "Module";
    /// `"Lambda"`.
    pub const ROLE_LAMBDA: &str = "Lambda";
    /// `"Try"`.
    pub const ROLE_TRY: &str = "Try";
    /// `"Catch"`.
    pub const ROLE_CATCH: &str = "Catch";
    /// `"Finally"`.
    pub const ROLE_FINALLY: &str = "Finally";
    /// `"Throw"`.
    pub const ROLE_THROW: &str = "Throw";
    /// `"Await"`.
    pub const ROLE_AWAIT: &str = "Await";
    /// `"Yield"`.
    pub const ROLE_YIELD: &str = "Yield";
    /// `"Spread"`.
    pub const ROLE_SPREAD: &str = "Spread";
    /// `"Pattern"`.
    pub const ROLE_PATTERN: &str = "Pattern";
    /// `"Match"`.
    pub const ROLE_MATCH: &str = "Match";
    /// `"Return"`.
    pub const ROLE_RETURN: &str = "Return";
    /// `"Break"`.
    pub const ROLE_BREAK: &str = "Break";
    /// `"Continue"`.
    pub const ROLE_CONTINUE: &str = "Continue";
    /// `"Generator"`.
    pub const ROLE_GENERATOR: &str = "Generator";
}

/// The byte and line/col offsets for a node.
///
/// All fields are 1-based except `start_offset`/`end_offset`, which are byte
/// offsets. Mirrors Go's `Positions` struct (`uint` fields, `omitempty` JSON).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Positions {
    /// 1-based start line.
    pub start_line: u64,
    /// 1-based start column.
    pub start_col: u64,
    /// Byte offset of the start.
    pub start_offset: u64,
    /// 1-based end line.
    pub end_line: u64,
    /// 1-based end column.
    pub end_col: u64,
    /// Byte offset of the end.
    pub end_offset: u64,
}

/// The canonical UAST node structure.
///
/// Field meanings mirror the Go `Node` struct exactly:
/// - `id`: unique/stable node identifier (optional; raw bytes — see
///   [`Node::assign_stable_ids`]).
/// - `node_type`: node type (e.g. `"Function"`, `"Identifier"`).
/// - `token`: string value or token for leaf nodes.
/// - `roles`: semantic/syntactic roles.
/// - `pos`: source code position info (optional).
/// - `props`: additional, language-specific properties.
/// - `children`: child nodes (ordered).
///
/// Note: Go stores `ID` as a `string` even though [`Node::assign_stable_ids`]
/// fills it with 8 raw SHA-1 bytes (not valid UTF-8). To reproduce that exactly
/// without lossy conversion, `id` is `Vec<u8>` here; [`Node::to_map`] hex-encodes
/// it just like Go's `fmt.Sprintf("%x", nodeID)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    /// Unique node identifier (raw bytes; empty means "no ID").
    pub id: Vec<u8>,
    /// String value or token for leaf nodes.
    pub token: String,
    /// Node type.
    pub node_type: Type,
    /// Semantic/syntactic roles.
    pub roles: Vec<Role>,
    /// Source code position info.
    pub pos: Option<Positions>,
    /// Additional language-specific properties.
    pub props: std::collections::HashMap<String, String>,
    /// Child nodes, in order.
    pub children: Vec<Node>,
}

/// Initial capacity for the children slice (`initialChildCap` in Go).
const INITIAL_CHILD_CAP: usize = 4;

/// SHA-1 prefix length used for stable IDs (`hashBufSize` in Go).
const HASH_BUF_SIZE: usize = 8;

/// A fluent builder for [`Node`]. Mirrors Go's `Builder`.
#[derive(Debug, Default)]
pub struct Builder {
    node: Node,
}

impl Builder {
    /// Creates a new builder around a zero-valued node.
    pub fn new() -> Self {
        Builder { node: Node::default() }
    }

    /// Sets the node ID (raw bytes).
    pub fn with_id(mut self, id: impl Into<Vec<u8>>) -> Self {
        self.node.id = id.into();
        self
    }

    /// Sets the node type.
    pub fn with_type(mut self, node_type: impl Into<Type>) -> Self {
        self.node.node_type = node_type.into();
        self
    }

    /// Sets the node token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.node.token = token.into();
        self
    }

    /// Sets the node roles.
    pub fn with_roles(mut self, roles: Vec<Role>) -> Self {
        self.node.roles = roles;
        self
    }

    /// Sets the node position.
    pub fn with_position(mut self, pos: Option<Positions>) -> Self {
        self.node.pos = pos;
        self
    }

    /// Sets the node properties.
    pub fn with_props(mut self, props: std::collections::HashMap<String, String>) -> Self {
        self.node.props = props;
        self
    }

    /// Consumes the builder and returns the node. Children are left empty.
    pub fn build(self) -> Node {
        self.node
    }
}

impl Node {
    /// Creates a new node with all leaf fields set (children left empty).
    /// Mirrors Go's `node.New(...)`.
    pub fn new(
        id: impl Into<Vec<u8>>,
        node_type: impl Into<Type>,
        token: impl Into<String>,
        roles: Vec<Role>,
        pos: Option<Positions>,
        props: std::collections::HashMap<String, String>,
    ) -> Node {
        Builder::new()
            .with_id(id)
            .with_type(node_type)
            .with_token(token)
            .with_roles(roles)
            .with_position(pos)
            .with_props(props)
            .build()
    }

    /// Creates a node with a type and token (`node.NewNodeWithToken`).
    pub fn with_token(node_type: impl Into<Type>, token: impl Into<String>) -> Node {
        Node::new(Vec::new(), node_type, token, Vec::new(), None, Default::default())
    }

    /// Creates a literal node (`node.NewLiteralNode`).
    pub fn literal(token: impl Into<String>) -> Node {
        Node::with_token(uast_types::UAST_LITERAL, token)
    }

    /// Appends a child node. Mirrors Go's `AddChild` (lazy slice init).
    pub fn add_child(&mut self, child: Node) {
        if self.children.is_empty() {
            self.children.reserve(INITIAL_CHILD_CAP);
        }
        self.children.push(child);
    }

    /// Removes the first child structurally equal to `child`.
    /// Returns `true` if one was removed. Mirrors Go's `RemoveChild` (which
    /// compares by pointer; here we compare by structural equality since Rust
    /// `Node`s are values).
    pub fn remove_child(&mut self, child: &Node) -> bool {
        if let Some(idx) = self.children.iter().position(|c| c == child) {
            self.children.remove(idx);
            return true;
        }
        false
    }

    /// Replaces the first child structurally equal to `old` with `replacement`.
    /// Returns `true` if a replacement happened. Mirrors Go's `ReplaceChild`.
    pub fn replace_child(&mut self, old: &Node, replacement: Node) -> bool {
        if let Some(idx) = self.children.iter().position(|c| c == old) {
            self.children[idx] = replacement;
            return true;
        }
        false
    }

    /// Returns `true` if the node has any of the given roles. Mirrors `HasAnyRole`.
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        if self.roles.is_empty() {
            return false;
        }
        roles.iter().any(|r| self.roles.iter().any(|nr| nr == r))
    }

    /// Returns `true` if the node has all of the given roles. Mirrors `HasAllRoles`.
    pub fn has_all_roles(&self, roles: &[&str]) -> bool {
        if self.roles.is_empty() {
            return false;
        }
        roles.iter().all(|r| self.roles.iter().any(|nr| nr == r))
    }

    /// Returns `true` if the node's type is any of the given types. Mirrors `HasAnyType`.
    pub fn has_any_type(&self, types: &[&str]) -> bool {
        types.iter().any(|t| self.node_type == *t)
    }

    /// Assigns a stable, content-addressed ID to every node in the tree.
    ///
    /// Reproduces Go's `AssignStableIDs` / `assignStableIDRecursive` byte-for-byte:
    /// for each node a SHA-1 digest is computed over the node type, token, the
    /// six little-endian `u64` position fields (only when a position is present),
    /// each role string, and — after recursing into children first — each child's
    /// already-assigned ID. The node's ID becomes the first 8 bytes of the digest.
    pub fn assign_stable_ids(&mut self) {
        let mut hasher = Sha1::new();
        hasher.update(self.node_type.as_bytes());
        hasher.update(self.token.as_bytes());

        if let Some(pos) = self.pos {
            let mut buf = [0u8; HASH_BUF_SIZE * 6];
            buf[0..8].copy_from_slice(&pos.start_line.to_le_bytes());
            buf[8..16].copy_from_slice(&pos.start_col.to_le_bytes());
            buf[16..24].copy_from_slice(&pos.start_offset.to_le_bytes());
            buf[24..32].copy_from_slice(&pos.end_line.to_le_bytes());
            buf[32..40].copy_from_slice(&pos.end_col.to_le_bytes());
            buf[40..48].copy_from_slice(&pos.end_offset.to_le_bytes());
            hasher.update(buf);
        }

        for role in &self.roles {
            hasher.update(role.as_bytes());
        }

        // Process children first so their IDs are available, then fold them in.
        for child in &mut self.children {
            child.assign_stable_ids();
            hasher.update(&child.id);
        }

        let digest = hasher.finalize();
        self.id = digest[..HASH_BUF_SIZE].to_vec();
    }

    /// Returns the `Node{...}` debug string used by Go's `String()` method.
    ///
    /// Reproduces `nodeString`: `Node{Type:<t>[,Token:<tok>][,Roles:[a b]]
    /// [,Props:map[...]][,Children:<n>]}`. The `Props` rendering matches Go's
    /// `fmt.Sprintf("%v", map)` which prints `map[k:v k2:v2]` with keys sorted.
    pub fn to_display_string(&self) -> String {
        let mut buf = String::new();
        buf.push_str("Node{");
        buf.push_str("Type:");
        buf.push_str(&self.node_type);

        if !self.token.is_empty() {
            buf.push_str(",Token:");
            buf.push_str(&self.token);
        }

        if !self.roles.is_empty() {
            buf.push_str(",Roles:[");
            for (idx, role) in self.roles.iter().enumerate() {
                if idx > 0 {
                    buf.push(' ');
                }
                buf.push_str(role);
            }
            buf.push(']');
        }

        if !self.props.is_empty() {
            // Go: fmt.Fprintf(buf, ",Props:%v", props) => "map[k:v k2:v2]" with
            // keys in sorted order (Go's fmt sorts map keys since Go 1.12).
            buf.push_str(",Props:map[");
            let mut keys: Vec<&String> = self.props.keys().collect();
            keys.sort();
            for (idx, k) in keys.iter().enumerate() {
                if idx > 0 {
                    buf.push(' ');
                }
                buf.push_str(k);
                buf.push(':');
                buf.push_str(&self.props[*k]);
            }
            buf.push(']');
        }

        if !self.children.is_empty() {
            buf.push_str(",Children:");
            buf.push_str(&self.children.len().to_string());
        }

        buf.push('}');
        buf
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_display_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_all_fields() {
        let n = Builder::new()
            .with_type("Function")
            .with_token("foo")
            .with_roles(vec!["Declaration".into(), "Name".into()])
            .build();
        assert_eq!(n.node_type, "Function");
        assert_eq!(n.token, "foo");
        assert_eq!(n.roles, vec!["Declaration", "Name"]);
    }

    #[test]
    fn literal_node_has_literal_type() {
        let n = Node::literal("42");
        assert_eq!(n.node_type, "Literal");
        assert_eq!(n.token, "42");
    }

    #[test]
    fn add_child_appends() {
        // Mirrors Go TestNode_AddChild.
        let mut n = Node::with_token("File", "");
        n.add_child(Node::with_token("Function", ""));
        assert_eq!(n.children.len(), 1);
    }

    #[test]
    fn remove_child_removes_first_match() {
        // Mirrors Go TestNode_RemoveChild.
        let child1 = Node::with_token("Function", "a");
        let child2 = Node::with_token("Function", "b");
        let mut root = Builder::new()
            .with_type("File")
            .build();
        root.children = vec![child1.clone(), child2];
        assert!(root.remove_child(&child1));
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn replace_child_replaces_first_match() {
        let old = Node::with_token("Function", "a");
        let mut root = Builder::new().with_type("File").build();
        root.children = vec![old.clone()];
        let new = Node::with_token("Method", "b");
        assert!(root.replace_child(&old, new.clone()));
        assert_eq!(root.children[0], new);
    }

    #[test]
    fn has_any_and_all_roles() {
        let n = Builder::new()
            .with_type("Function")
            .with_roles(vec!["Declaration".into(), "Name".into()])
            .build();
        assert!(n.has_any_role(&["Name"]));
        assert!(!n.has_any_role(&["Body"]));
        assert!(n.has_all_roles(&["Declaration", "Name"]));
        assert!(!n.has_all_roles(&["Declaration", "Body"]));
        let empty = Node::with_token("Function", "");
        assert!(!empty.has_any_role(&["Name"]));
        assert!(!empty.has_all_roles(&["Name"]));
    }

    #[test]
    fn has_any_type() {
        let n = Node::with_token("Function", "");
        assert!(n.has_any_type(&["Method", "Function"]));
        assert!(!n.has_any_type(&["Class"]));
    }

    #[test]
    fn display_matches_go_format() {
        // Mirrors Go TestNode_String.
        assert_eq!(
            Node::with_token("Function", "foo").to_display_string(),
            "Node{Type:Function,Token:foo}"
        );
        let with_roles = Builder::new()
            .with_type("Function")
            .with_roles(vec!["Declaration".into()])
            .build();
        assert_eq!(with_roles.to_display_string(), "Node{Type:Function,Roles:[Declaration]}");
    }

    #[test]
    fn assign_stable_ids_is_deterministic_and_8_bytes() {
        let mut a = Builder::new().with_type("Function").with_token("foo").build();
        a.add_child(Node::with_token("Identifier", "x"));
        let mut b = a.clone();
        a.assign_stable_ids();
        b.assign_stable_ids();
        assert_eq!(a.id.len(), 8);
        assert_eq!(a.id, b.id);
        assert_eq!(a.children[0].id.len(), 8);
    }

    #[test]
    fn assign_stable_ids_known_vector() {
        // Golden SHA-1 prefix: a leaf node Type="Function", Token="foo", no
        // position, no roles, no children => SHA1("Functionfoo")[:8].
        // SHA1("Functionfoo") = 8d9f...; computed independently below.
        let mut n = Node::with_token("Function", "foo");
        n.assign_stable_ids();
        let mut hasher = Sha1::new();
        hasher.update(b"Functionfoo");
        let expect = hasher.finalize()[..8].to_vec();
        assert_eq!(n.id, expect);
    }
}
