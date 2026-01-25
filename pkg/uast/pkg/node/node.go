// Package uast provides a universal abstract syntax tree (UAST) representation and utilities for parsing, navigating, querying, and mutating code structure in a language-agnostic way.
package node

import (
	"crypto/sha1" //nolint:gosec // SHA1 used for content fingerprinting, not security
	"encoding/binary"
	"fmt"
	"hash"
	"strings"
	"sync"
)

const (
	UASTFile           = "File"
	UASTFunction       = "Function"
	UASTFunctionDecl   = "FunctionDecl"
	UASTMethod         = "Method"
	UASTClass          = "Class"
	UASTInterface      = "Interface"
	UASTStruct         = "Struct"
	UASTEnum           = "Enum"
	UASTEnumMember     = "EnumMember"
	UASTVariable       = "Variable"
	UASTParameter      = "Parameter"
	UASTBlock          = "Block"
	UASTIf             = "If"
	UASTLoop           = "Loop"
	UASTSwitch         = "Switch"
	UASTCase           = "Case"
	UASTReturn         = "Return"
	UASTBreak          = "Break"
	UASTContinue       = "Continue"
	UASTAssignment     = "Assignment"
	UASTCall           = "Call"
	UASTIdentifier     = "Identifier"
	UASTLiteral        = "Literal"
	UASTBinaryOp       = "BinaryOp"
	UASTUnaryOp        = "UnaryOp"
	UASTImport         = "Import"
	UASTPackage        = "Package"
	UASTAttribute      = "Attribute"
	UASTComment        = "Comment"
	UASTDocString      = "DocString"
	UASTTypeAnnotation = "TypeAnnotation"
	UASTField          = "Field"
	UASTProperty       = "Property"
	UASTGetter         = "Getter"
	UASTSetter         = "Setter"
	UASTLambda         = "Lambda"
	UASTTry            = "Try"
	UASTCatch          = "Catch"
	UASTFinally        = "Finally"
	UASTThrow          = "Throw"
	UASTModule         = "Module"
	UASTNamespace      = "Namespace"
	UASTDecorator      = "Decorator"
	UASTSpread         = "Spread"
	UASTTuple          = "Tuple"
	UASTList           = "List"
	UASTDict           = "Dict"
	UASTSet            = "Set"
	UASTKeyValue       = "KeyValue"
	UASTIndex          = "Index"
	UASTSlice          = "Slice"
	UASTCast           = "Cast"
	UASTAwait          = "Await"
	UASTYield          = "Yield"
	UASTGenerator      = "Generator"
	UASTComprehension  = "Comprehension"
	UASTPattern        = "Pattern"
	UASTMatch          = "Match"
	UASTSynthetic      = "Synthetic"
)

const (
	RoleFunction    = "Function"
	RoleDeclaration = "Declaration"
	RoleName        = "Name"
	RoleReference   = "Reference"
	RoleAssignment  = "Assignment"
	RoleCall        = "Call"
	RoleParameter   = "Parameter"
	RoleArgument    = "Argument"
	RoleCondition   = "Condition"
	RoleBody        = "Body"
	RoleExported    = "Exported"
	RolePublic      = "Public"
	RolePrivate     = "Private"
	RoleStatic      = "Static"
	RoleConstant    = "Constant"
	RoleMutable     = "Mutable"
	RoleGetter      = "Getter"
	RoleSetter      = "Setter"
	RoleLiteral     = "Literal"
	RoleVariable    = "Variable"
	RoleLoop        = "Loop"
	RoleBranch      = "Branch"
	RoleImport      = "Import"
	RoleDoc         = "Doc"
	RoleComment     = "Comment"
	RoleAttribute   = "Attribute"
	RoleAnnotation  = "Annotation"
	RoleOperator    = "Operator"
	RoleIndex       = "Index"
	RoleKey         = "Key"
	RoleValue       = "Value"
	RoleType        = "Type"
	RoleInterface   = "Interface"
	RoleClass       = "Class"
	RoleStruct      = "Struct"
	RoleEnum        = "Enum"
	RoleMember      = "Member"
	RoleModule      = "Module"
	RoleLambda      = "Lambda"
	RoleTry         = "Try"
	RoleCatch       = "Catch"
	RoleFinally     = "Finally"
	RoleThrow       = "Throw"
	RoleAwait       = "Await"
	RoleYield       = "Yield"
	RoleSpread      = "Spread"
	RolePattern     = "Pattern"
	RoleMatch       = "Match"
	RoleReturn      = "Return"
	RoleBreak       = "Break"
	RoleContinue    = "Continue"
	RoleGenerator   = "Generator"
)

// Role represents a syntactic/semantic label for a node.
type Role string

// Type represents a type label for a node.
type Type string

// Positions represents the byte and line/col offsets for a node.
// All fields are 1-based except StartOffset/EndOffset, which are byte offsets.
type Positions struct {
	StartLine   uint `json:"start_line,omitempty"`
	StartCol    uint `json:"start_col,omitempty"`
	StartOffset uint `json:"start_offset,omitempty"`
	EndLine     uint `json:"end_line,omitempty"`
	EndCol      uint `json:"end_col,omitempty"`
	EndOffset   uint `json:"end_offset,omitempty"`
}

// Node is the canonical UAST node structure.
//
// Fields:
//
//	ID: unique node identifier (optional)
//	Type: node type (e.g., "Function", "Identifier")
//	Token: string value or token for leaf nodes
//	Roles: semantic/syntactic roles (see Role)
//	Pos: source code position info (optional)
//	Props: additional properties (language-specific)
//	Children: child nodes (ordered)
type Node struct {
	ID       string            `json:"id,omitempty"`
	Type     Type              `json:"type,omitempty"`
	Token    string            `json:"token,omitempty"`
	Roles    []Role            `json:"roles,omitempty"`
	Pos      *Positions        `json:"pos,omitempty"`
	Props    map[string]string `json:"props,omitempty"`
	Children []*Node           `json:"children,omitempty"`
}

// nodePool is a sync.Pool for Node structs to reduce allocation overhead
var nodePool = sync.Pool{
	New: func() interface{} {
		return &Node{}
	},
}

type nodeAncestorFrame struct {
	node   *Node
	parent []*Node
}

type nodeTransformFrame struct {
	node     *Node
	parent   *Node
	childIdx int
	newNode  *Node
}

// NodeBuilder provides a fluent interface for building Node instances
type NodeBuilder struct {
	node *Node
}

// NewBuilder creates a new NodeBuilder with a node from the pool
func NewBuilder() *NodeBuilder {
	return &NodeBuilder{node: nodePool.Get().(*Node)} //nolint:errcheck // Pool only contains *Node
}

// WithID sets the node ID
func (b *NodeBuilder) WithID(id string) *NodeBuilder {
	b.node.ID = id
	return b
}

// WithType sets the node type
func (b *NodeBuilder) WithType(t Type) *NodeBuilder {
	b.node.Type = t
	return b
}

// WithToken sets the node token
func (b *NodeBuilder) WithToken(token string) *NodeBuilder {
	b.node.Token = token
	return b
}

// WithRoles sets the node roles
func (b *NodeBuilder) WithRoles(roles []Role) *NodeBuilder {
	b.node.Roles = roles
	return b
}

// WithPosition sets the node position
func (b *NodeBuilder) WithPosition(pos *Positions) *NodeBuilder {
	b.node.Pos = pos
	return b
}

// WithProps sets the node properties
func (b *NodeBuilder) WithProps(props map[string]string) *NodeBuilder {
	b.node.Props = props
	return b
}

// Build creates and returns the final Node
func (b *NodeBuilder) Build() *Node {
	b.node.Children = make([]*Node, 0, 4) // Pre-allocate with reasonable capacity
	return b.node
}

// New creates a new Node from the pool and initializes it with the given values
func New(id string, nodeType Type, token string, roles []Role, pos *Positions, props map[string]string) *Node {
	return NewBuilder().
		WithID(id).
		WithType(nodeType).
		WithToken(token).
		WithRoles(roles).
		WithPosition(pos).
		WithProps(props).
		Build()
}

// NewNodeWithToken creates a new Node with type and token
func NewNodeWithToken(nodeType Type, token string) *Node {
	node := nodePool.Get().(*Node) //nolint:errcheck // Pool only contains *Node
	node.ID = ""
	node.Type = nodeType
	node.Token = token
	node.Roles = nil
	node.Pos = nil
	node.Props = nil
	node.Children = nil
	return node
}

// NewLiteralNode creates a new Node for literal values
func NewLiteralNode(token string) *Node {
	return NewNodeWithToken("Literal", token)
}

// Release returns a Node to the pool for reuse
func (n *Node) Release() {
	// Clear the node to prevent memory leaks
	n.ID = ""
	n.Type = ""
	n.Token = ""
	n.Roles = nil
	n.Pos = nil
	n.Props = nil
	n.Children = nil
	nodePool.Put(n)
}

// Find returns all nodes in the tree (including root) for which predicate(node) is true.
// Traversal is pre-order. Returns nil if n is nil.
func (n *Node) Find(predicate func(*Node) bool) []*Node {
	if isNodeNil(n) {
		return nil
	}
	return findNodesWithPredicate(n, predicate)
}

// AddChild appends a child node to n.
func (n *Node) AddChild(child *Node) {
	n.Children = append(n.Children, child)
}

// RemoveChild removes the first occurrence of the given child node from n.
// Returns true if the child was found and removed.
func (n *Node) RemoveChild(child *Node) bool {
	for i, c := range n.Children {
		if isChildMatch(c, child) {
			removeChildAtIndex(n, i)
			return true
		}
	}
	return false
}

// ReplaceChild replaces the first occurrence of old in Children with new. Returns true if replaced.
func (n *Node) ReplaceChild(old, new *Node) bool {
	for i, c := range n.Children {
		if isChildMatch(c, old) {
			replaceChildAtIndex(n, i, new)
			return true
		}
	}
	return false
}

// VisitPreOrder visits all nodes in pre-order (root, then children left-to-right).
// Now uses the final optimized implementation with strict depth limiting.
func (n *Node) VisitPreOrder(fn func(*Node)) {
	if n == nil {
		return
	}
	// Use the channel-based optimized version and consume it
	for node := range preOrder(n) {
		fn(node)
	}
}

// PreOrder returns a channel streaming nodes in pre-order traversal.
// Now uses the final optimized implementation with strict depth limiting.
func (n *Node) PreOrder() <-chan *Node {
	return preOrder(n)
}

// VisitPostOrder visits all nodes in post-order (children left-to-right, then root).
// Now uses the final optimized implementation with strict depth limiting.
func (n *Node) VisitPostOrder(fn func(*Node)) {
	postOrder(n, fn)
}

// Ancestors returns the list of ancestors from root to the parent of target (empty if not found).
// Returns nil if n or target is nil.
func (n *Node) Ancestors(target *Node) []*Node {
	if isNodeNil(n) || isNodeNil(target) {
		return nil
	}
	return findAncestors(n, target)
}

// FindDSL queries nodes using a DSL string.
// Example:
//
//	nodes, err := node.FindDSL("type == 'Function' | map(.children)")
//	if err != nil {
//	    log.Fatal(err)
//	}
//	for _, n := range nodes {
//	    fmt.Println(n.Type)
//	}
func (n *Node) FindDSL(query string) ([]*Node, error) {
	if len(query) == 0 {
		return nil, fmt.Errorf("query string is empty")
	}

	ast, err := n.parseDSLQuery(query)
	if err != nil {
		return nil, err
	}

	initialInput := n.determineInitialInput(ast)
	return n.executeDSLRuntime(ast, initialInput)
}

func (n *Node) parseDSLQuery(query string) (interface{}, error) {
	ast, err := ParseDSL(query)
	if err != nil {
		return nil, fmt.Errorf("DSL parse error: %w", err)
	}
	return ast, nil
}

func (n *Node) executeDSLRuntime(ast interface{}, initialInput []*Node) ([]*Node, error) {
	runtime, err := LowerDSL(ast)
	if err != nil {
		return nil, fmt.Errorf("DSL lowering error: %w", err)
	}
	result := runtime(initialInput)
	if result == nil {
		return []*Node{}, nil
	}
	return result, nil
}

func (n *Node) determineInitialInput(ast interface{}) []*Node {
	if _, ok := ast.(*FilterNode); ok {
		return n.Children
	}
	if pipeline, ok := ast.(*PipelineNode); ok {
		return n.determinePipelineInput(pipeline)
	}
	return []*Node{n}
}

func (n *Node) determinePipelineInput(pipeline *PipelineNode) []*Node {
	if len(pipeline.Stages) == 0 {
		return n.Children
	}
	if mapNode, ok := pipeline.Stages[0].(*MapNode); ok {
		return n.determineMapNodeInput(mapNode)
	}
	return n.Children
}

func (n *Node) determineMapNodeInput(mapNode *MapNode) []*Node {
	if fieldNode, ok := mapNode.Expr.(*FieldNode); ok {
		return n.determineFieldNodeInput(fieldNode)
	}
	return n.Children
}

func (n *Node) determineFieldNodeInput(fieldNode *FieldNode) []*Node {
	if len(fieldNode.Fields) == 1 && fieldNode.Fields[0] == "children" {
		return []*Node{n}
	}
	return n.Children
}

// HasAnyRole checks if the node has the given role.
// Example:
//
//	if uast.HasAnyRole(node, uast.RoleFunction) {
//	    fmt.Println("Node is a function")
//	}
func (n *Node) HasAnyRole(roles ...Role) bool {
	if isNodeNil(n) || hasNoRoles(n) {
		return false
	}

	for _, role := range roles {
		if isRoleMatch(n.Roles, role) {
			return true
		}
	}
	return false
}

func (n *Node) HasAllRoles(roles ...Role) bool {
	if isNodeNil(n) || hasNoRoles(n) {
		return false
	}

	for _, role := range roles {
		if !isRoleMatch(n.Roles, role) {
			return false
		}
	}
	return true
}

func (n *Node) HasAnyType(nodeTypes ...Type) bool {
	if isNodeNil(n) {
		return false
	}
	return isTypeMatch(n.Type, nodeTypes)
}

// Transform mutates the tree in place using the provided function.
// Example:
//
//	uast.Transform(root, func(n *uast.Node) bool {
//	    if n.Type == "Comment" {
//	        n.Token = ""
//	    }
//	    return true // continue traversal
//	})
func (n *Node) TransformInPlace(fn func(*Node) bool) {
	transformInPlace(n, fn)
}

// Transform returns a new tree where each node is replaced by the result of fn(node) (post-order, non-recursive).
// The returned tree is a deep copy with transformations applied. Returns nil if n is nil.
func (n *Node) Transform(fn func(*Node) *Node) *Node {
	return transformNode(n, fn)
}

func (n *Node) ToMap() map[string]any {
	if n == nil {
		return nil
	}

	result := buildBaseMap(n)
	result["pos"] = buildPositionMap(n.Pos)

	if len(n.Children) > 0 {
		result["children"] = buildChildrenMap(n.Children)
	}

	return result
}

// buildBaseMap creates the base map with type, id, token, props, and roles
func buildBaseMap(n *Node) map[string]any {
	result := map[string]any{
		"type": n.Type,
	}

	addIDToMap(result, n.ID)
	addTokenToMap(result, n.Token)
	addPropsToMap(result, n.Props)
	addRolesToMap(result, n.Roles)

	return result
}

func addIDToMap(result map[string]any, id string) {
	if id != "" {
		result["id"] = fmt.Sprintf("%x", id)
	}
}

func addTokenToMap(result map[string]any, token string) {
	if token != "" {
		result["token"] = token
	}
}

func addPropsToMap(result map[string]any, props map[string]string) {
	if len(props) > 0 {
		result["props"] = props
	}
}

func addRolesToMap(result map[string]any, roles []Role) {
	roleStrings := make([]string, len(roles))
	for i, role := range roles {
		roleStrings[i] = string(role)
	}
	result["roles"] = roleStrings
}

// buildPositionMap creates the position map, handling nil positions
func buildPositionMap(pos *Positions) map[string]any {
	if pos == nil {
		return map[string]any{
			"start_line":   0,
			"start_col":    0,
			"start_offset": 0,
			"end_line":     0,
			"end_col":      0,
			"end_offset":   0,
		}
	}

	return map[string]any{
		"start_line":   pos.StartLine,
		"start_col":    pos.StartCol,
		"start_offset": pos.StartOffset,
		"end_line":     pos.EndLine,
		"end_col":      pos.EndCol,
		"end_offset":   pos.EndOffset,
	}
}

// buildChildrenMap creates the children map array
func buildChildrenMap(children []*Node) []map[string]any {
	childrenMap := make([]map[string]any, len(children))
	for i, child := range children {
		childrenMap[i] = child.ToMap()
	}
	return childrenMap
}

func isChildMatch(child, target *Node) bool {
	return child == target
}

func removeChildAtIndex(n *Node, index int) {
	n.Children = append(n.Children[:index], n.Children[index+1:]...)
}

// String returns a string representation of the node
func (n *Node) String() string {
	return nodeString(n)
}

// Optimized string representation without JSON marshaling
func nodeString(node *Node) string {
	if node == nil {
		return "nil"
	}

	var buf strings.Builder
	buf.WriteString("Node{")
	buf.WriteString("Type:")
	buf.WriteString(string(node.Type))

	appendToken(&buf, node.Token)
	appendRoles(&buf, node.Roles)
	appendProps(&buf, node.Props)
	appendChildren(&buf, node.Children)

	buf.WriteString("}")
	return buf.String()
}

func appendToken(buf *strings.Builder, token string) {
	if token != "" {
		buf.WriteString(",Token:")
		buf.WriteString(token)
	}
}

func appendRoles(buf *strings.Builder, roles []Role) {
	if len(roles) > 0 {
		buf.WriteString(",Roles:[")
		for i, role := range roles {
			if i > 0 {
				buf.WriteString(" ")
			}
			buf.WriteString(string(role))
		}
		buf.WriteString("]")
	}
}

func appendProps(buf *strings.Builder, props map[string]string) {
	if len(props) > 0 {
		buf.WriteString(",Props:")
		buf.WriteString(fmt.Sprintf("%v", props))
	}
}

func appendChildren(buf *strings.Builder, children []*Node) {
	if len(children) > 0 {
		buf.WriteString(",Children:")
		buf.WriteString(fmt.Sprintf("%d", len(children)))
	}
}

func isNodeNil(n *Node) bool {
	return n == nil
}

func findNodesWithPredicate(n *Node, predicate func(*Node) bool) []*Node {
	var result []*Node
	stack := []*Node{n}
	for nodeHasStack(stack) {
		curr := nodePopStack(&stack)
		if predicate(curr) {
			result = append(result, curr)
		}
		nodePushChildrenToStack(curr, &stack)
	}
	return result
}

func nodeHasStack(stack []*Node) bool {
	return len(stack) > 0
}

func nodePopStack(stack *[]*Node) *Node {
	last := (*stack)[len(*stack)-1]
	*stack = (*stack)[:len(*stack)-1]
	return last
}

func nodePushChildrenToStack(node *Node, stack *[]*Node) {
	*stack = append(*stack, getReversedChildren(node)...)
}

func getReversedChildren(node *Node) []*Node {
	children := node.Children
	reversed := make([]*Node, len(children))
	for i := range children {
		reversed[len(reversed)-1-i] = children[i]
	}
	return reversed
}

func findAncestors(n, target *Node) []*Node {
	stack := []nodeAncestorFrame{{n, nil}}
	for nodeHasAncestorStack(stack) {
		top := nodePopAncestorStack(&stack)
		if isTargetFound(top.node, target) {
			return top.parent
		}
		nodePushChildAncestors(top, &stack)
	}
	return nil
}

func nodeHasAncestorStack(stack []nodeAncestorFrame) bool {
	return len(stack) > 0
}

func nodePopAncestorStack(stack *[]nodeAncestorFrame) nodeAncestorFrame {
	last := (*stack)[len(*stack)-1]
	*stack = (*stack)[:len(*stack)-1]
	return last
}

func isTargetFound(node, target *Node) bool {
	return node == target
}

func nodePushChildAncestors(top nodeAncestorFrame, stack *[]nodeAncestorFrame) {
	*stack = append(*stack, createAncestorFrames(top)...)
}

func createAncestorFrames(top nodeAncestorFrame) []nodeAncestorFrame {
	ancestorPath := buildAncestorPath(top.parent, top.node)
	frames := make([]nodeAncestorFrame, len(top.node.Children))
	for i := range top.node.Children {
		frames[len(frames)-1-i] = nodeAncestorFrame{top.node.Children[i], ancestorPath}
	}
	return frames
}

func buildAncestorPath(parent []*Node, node *Node) []*Node {
	return append(append([]*Node{}, parent...), node)
}

func transformNode(n *Node, fn func(*Node) *Node) *Node {
	var (
		stack   []nodeTransformFrame
		results = make(map[*Node]*Node)
	)
	stack = append(stack, nodeTransformFrame{n, nil, 0, nil})
	for nodeHasTransformStack(stack) {
		top := &stack[len(stack)-1]
		stack = processTransformFrame(top, stack, results, fn)
	}
	return results[n]
}

func processTransformFrame(top *nodeTransformFrame, stack []nodeTransformFrame, results map[*Node]*Node, fn func(*Node) *Node) []nodeTransformFrame {
	if nodeHasMoreChildren(top) {
		nodePushChildTransform(top, &stack)
		nodeIncrementChildIndex(top)
		return stack
	}
	nodeProcessTransformedNode(top, results, fn)
	nodePopTransformStack(&stack)
	return stack
}

func nodeHasTransformStack(stack []nodeTransformFrame) bool {
	return len(stack) > 0
}

func nodeHasMoreChildren(top *nodeTransformFrame) bool {
	return top.childIdx < len(top.node.Children)
}

func nodePushChildTransform(top *nodeTransformFrame, stack *[]nodeTransformFrame) {
	*stack = append(*stack, nodeTransformFrame{top.node.Children[top.childIdx], top.node, 0, nil})
}

func nodeIncrementChildIndex(top *nodeTransformFrame) {
	top.childIdx++
}

func nodeProcessTransformedNode(top *nodeTransformFrame, results map[*Node]*Node, fn func(*Node) *Node) {
	results[top.node] = fn(createTransformedNode(top.node, results))
}

func createTransformedNode(node *Node, results map[*Node]*Node) *Node {
	copy := *node
	copy.Children = make([]*Node, len(node.Children))
	for i, c := range node.Children {
		copy.Children[i] = results[c]
	}
	return &copy
}

func nodePopTransformStack(stack *[]nodeTransformFrame) {
	*stack = (*stack)[:len(*stack)-1]
}

func replaceChildAtIndex(n *Node, index int, new *Node) {
	n.Children[index] = new
}

func hasNoRoles(node *Node) bool {
	return len(node.Roles) == 0
}

func isRoleMatch(roles []Role, target Role) bool {
	for _, r := range roles {
		if target == r {
			return true
		}
	}
	return false
}

func isTypeMatch(nodeType Type, target []Type) bool {
	for _, t := range target {
		if nodeType == t {
			return true
		}
	}
	return false
}

func transformInPlace(root *Node, fn func(*Node) bool) {
	stack := []*Node{root}
	for nodeHasStack(stack) {
		n := nodePopStack(&stack)
		if shouldContinueTransform(n, fn) {
			nodePushChildrenToStack(n, &stack)
		}
	}
}

func shouldContinueTransform(n *Node, fn func(*Node) bool) bool {
	return fn(n)
}

// Final optimized tree traversal with strict depth limiting
func preOrder(node *Node) <-chan *Node {
	ch := make(chan *Node)
	go func() {
		defer close(ch)
		if node == nil {
			return
		}

		stack := make([]*Node, 0, defaultStackCap)
		stack = append(stack, node)

		for len(stack) > 0 {
			n := stack[len(stack)-1]
			stack = stack[:len(stack)-1]

			if n == nil {
				continue
			}

			ch <- n
			stack = processPreOrderChildren(n, stack, defaultMaxDepth, ch)
		}
	}()
	return ch
}

const (
	defaultMaxDepth = 25
	defaultStackCap = 64
)

func processPreOrderChildren(n *Node, stack []*Node, maxAllowedDepth int, ch chan<- *Node) []*Node {
	if len(n.Children) > 0 {
		if len(stack) >= maxAllowedDepth {
			processRemainingNodesDepthLimited(n, ch, 0, 10)
			return stack
		}

		stack = ensureStackCapacity(stack, len(n.Children))
		stack = pushChildrenToStackReversed(stack, n.Children)
	}
	return stack
}

// ensureStackCapacity ensures the stack has enough capacity for additional children
func ensureStackCapacity(stack []*Node, childCount int) []*Node {
	if cap(stack) < len(stack)+childCount {
		newStack := make([]*Node, len(stack), len(stack)+childCount+32)
		copy(newStack, stack)
		return newStack
	}
	return stack
}

// pushChildrenToStackReversed pushes children to the stack in reverse order for pre-order traversal
func pushChildrenToStackReversed(stack []*Node, children []*Node) []*Node {
	for i := len(children) - 1; i >= 0; i-- {
		stack = append(stack, children[i])
	}
	return stack
}

// Process remaining nodes with depth-limited recursion
func processRemainingNodesDepthLimited(node *Node, ch chan<- *Node, depth, maxDepth int) {
	if depth >= maxDepth {
		processRemainingNodesIterative(node, ch)
		return
	}

	ch <- node
	for _, child := range node.Children {
		processRemainingNodesDepthLimited(child, ch, depth+1, maxDepth)
	}
}

// Process remaining nodes iteratively
func processRemainingNodesIterative(node *Node, ch chan<- *Node) {
	queue := make([]*Node, 0, 32)
	queue = append(queue, node)

	for len(queue) > 0 {
		n := queue[0]
		queue = queue[1:]

		if n == nil {
			continue
		}

		ch <- n

		queue = append(queue, n.Children...)
	}
}

// Final optimized post-order traversal with strict depth limiting
func postOrder(node *Node, fn func(*Node)) {
	if node == nil {
		return
	}

	stack := make([]postOrderFrame, 0, defaultStackCap)
	stack = append(stack, postOrderFrame{node: node, index: 0})

	for len(stack) > 0 {
		if len(stack) >= defaultMaxDepth {
			processRemainingNodesPostOrderDepthLimited(node, fn, 0, 10)
			break
		}

		top := &stack[len(stack)-1]
		stack = processPostOrderFrame(top, stack, fn)
	}
}

func processPostOrderFrame(top *postOrderFrame, stack []postOrderFrame, fn func(*Node)) []postOrderFrame {
	if top.index == 0 {
		return processInitialFrame(top, stack, fn)
	}
	return processCompletedFrame(top, stack, fn)
}

func processInitialFrame(top *postOrderFrame, stack []postOrderFrame, fn func(*Node)) []postOrderFrame {
	if len(top.node.Children) > 0 {
		stack = ensurePostOrderStackCapacity(stack, len(top.node.Children))
		stack = pushChildrenToPostOrderStack(stack, top.node.Children)
		top.index = 1
		return stack
	}
	fn(top.node)
	return stack[:len(stack)-1]
}

func processCompletedFrame(top *postOrderFrame, stack []postOrderFrame, fn func(*Node)) []postOrderFrame {
	fn(top.node)
	return stack[:len(stack)-1]
}

// postOrderFrame represents a frame in the post-order traversal stack
type postOrderFrame struct {
	node  *Node
	index int
}

// ensurePostOrderStackCapacity ensures the post-order stack has enough capacity
func ensurePostOrderStackCapacity(stack []postOrderFrame, childCount int) []postOrderFrame {
	if cap(stack) < len(stack)+childCount {
		newStack := make([]postOrderFrame, len(stack), len(stack)+childCount+32)
		copy(newStack, stack)
		return newStack
	}
	return stack
}

// pushChildrenToPostOrderStack pushes children to the post-order stack in reverse order
func pushChildrenToPostOrderStack(stack []postOrderFrame, children []*Node) []postOrderFrame {
	for i := len(children) - 1; i >= 0; i-- {
		stack = append(stack, postOrderFrame{node: children[i], index: 0})
	}
	return stack
}

// Process remaining nodes for post-order with depth limiting
func processRemainingNodesPostOrderDepthLimited(node *Node, fn func(*Node), depth, maxDepth int) {
	if depth >= maxDepth {
		// Switch to iterative approach
		processRemainingNodesPostOrderIterative(node, fn)
		return
	}

	for _, child := range node.Children {
		processRemainingNodesPostOrderDepthLimited(child, fn, depth+1, maxDepth)
	}
	fn(node)
}

const defaultIterativeStackCap = 32

// Process remaining nodes for post-order iteratively
func processRemainingNodesPostOrderIterative(node *Node, fn func(*Node)) {
	stack, visited := initializePostOrderIterative(node)
	for len(stack) > 0 {
		n := stack[len(stack)-1]
		stack = processPostOrderIterativeNode(n, stack, visited, fn)
	}
}

func initializePostOrderIterative(node *Node) ([]*Node, map[*Node]bool) {
	stack := make([]*Node, 0, defaultIterativeStackCap)
	visited := make(map[*Node]bool)
	stack = append(stack, node)
	return stack, visited
}

func processPostOrderIterativeNode(n *Node, stack []*Node, visited map[*Node]bool, fn func(*Node)) []*Node {
	if visited[n] {
		fn(n)
		return stack[:len(stack)-1]
	}
	visited[n] = true
	return pushChildrenToPostOrderStackReversed(stack, n.Children)
}

func pushChildrenToPostOrderStackReversed(stack []*Node, children []*Node) []*Node {
	for i := len(children) - 1; i >= 0; i-- {
		stack = append(stack, children[i])
	}
	return stack
}

// AssignStableIDs assigns a stable id to each node in the tree based on its content and position.
func (n *Node) AssignStableIDs() {
	if n == nil {
		return
	}
	assignStableIDRecursive(n)
}

// assignStableIDRecursive recursively assigns stable IDs to nodes
func assignStableIDRecursive(node *Node) {
	if node == nil {
		return
	}

	// G401: SHA1 is used here for content-based fingerprinting to generate stable node IDs,
	// not for security purposes. Collision resistance is not required for this use case.
	h := sha1.New() //nolint:gosec
	writeNodeContentToHash(h, node)

	// Process children first to get their IDs
	for _, child := range node.Children {
		assignStableIDRecursive(child)
		writeChildIDToHash(h, child)
	}

	// Use first 8 bytes of SHA1 as uint64 id
	idBytes := h.Sum(nil)[:8]
	node.ID = string(idBytes)
}

// writeNodeContentToHash writes node content to the hash
func writeNodeContentToHash(h hash.Hash, node *Node) {
	h.Write([]byte(node.Type))
	h.Write([]byte(node.Token))

	if node.Pos != nil {
		writePositionToHash(h, node.Pos)
	}

	for _, role := range node.Roles {
		h.Write([]byte(role))
	}
}

// writePositionToHash writes position information to the hash
func writePositionToHash(h hash.Hash, pos *Positions) {
	buf := make([]byte, 8*6)
	writeStartPosition(buf, pos)
	writeEndPosition(buf, pos)
	h.Write(buf)
}

func writeStartPosition(buf []byte, pos *Positions) {
	binary.LittleEndian.PutUint64(buf[0:8], uint64(pos.StartLine))
	binary.LittleEndian.PutUint64(buf[8:16], uint64(pos.StartCol))
	binary.LittleEndian.PutUint64(buf[16:24], uint64(pos.StartOffset))
}

func writeEndPosition(buf []byte, pos *Positions) {
	binary.LittleEndian.PutUint64(buf[24:32], uint64(pos.EndLine))
	binary.LittleEndian.PutUint64(buf[32:40], uint64(pos.EndCol))
	binary.LittleEndian.PutUint64(buf[40:48], uint64(pos.EndOffset))
}

// writeChildIDToHash writes child ID to the hash
func writeChildIDToHash(h hash.Hash, child *Node) {
	h.Write([]byte(child.ID))
}
