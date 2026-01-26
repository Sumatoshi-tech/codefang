// Package node provides the canonical UAST node structure and operations for
// tree traversal, querying, and transformation.
package node

import (
	"crypto/sha1" //nolint:gosec // SHA1 used for content fingerprinting, not security.
	"encoding/binary"
	"errors"
	"fmt"
	"hash"
	"slices"
	"strconv"
	"strings"
	"sync"
)

// UAST node type constants.
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

// Role constants for syntactic and semantic labeling.
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
//	ID: unique node identifier (optional).
//	Type: node type (e.g., "Function", "Identifier").
//	Token: string value or token for leaf nodes.
//	Roles: semantic/syntactic roles (see Role).
//	Pos: source code position info (optional).
//	Props: additional properties (language-specific).
//	Children: child nodes (ordered).
type Node struct {
	ID       string            `json:"id,omitempty"`
	Token    string            `json:"token,omitempty"`
	Type     Type              `json:"type,omitempty"`
	Roles    []Role            `json:"roles,omitempty"`
	Pos      *Positions        `json:"pos,omitempty"`
	Props    map[string]string `json:"props,omitempty"`
	Children []*Node           `json:"children,omitempty"`
}

// nodePool is a [sync.Pool] for Node structs to reduce allocation overhead.
//
//nolint:gochecknoglobals // Shared pool for node allocation performance.
var nodePool = sync.Pool{
	New: func() any {
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
	newNode  *Node
	childIdx int
}

// Sentinel errors for DSL operations.
var errEmptyQuery = errors.New("query string is empty")

// NodeBuilder provides a fluent interface for building Node instances.
type NodeBuilder struct {
	node *Node
}

// Allocation constants.
const (
	initialChildCap = 4
)

// NewBuilder creates a new NodeBuilder with a node from the pool.
func NewBuilder() *NodeBuilder {
	poolNode, ok := nodePool.Get().(*Node)
	if !ok {
		poolNode = &Node{}
	}

	return &NodeBuilder{node: poolNode}
}

// WithID sets the node ID.
func (builder *NodeBuilder) WithID(nodeID string) *NodeBuilder {
	builder.node.ID = nodeID

	return builder
}

// WithType sets the node type.
func (builder *NodeBuilder) WithType(nodeType Type) *NodeBuilder {
	builder.node.Type = nodeType

	return builder
}

// WithToken sets the node token.
func (builder *NodeBuilder) WithToken(token string) *NodeBuilder {
	builder.node.Token = token

	return builder
}

// WithRoles sets the node roles.
func (builder *NodeBuilder) WithRoles(roles []Role) *NodeBuilder {
	builder.node.Roles = roles

	return builder
}

// WithPosition sets the node position.
func (builder *NodeBuilder) WithPosition(pos *Positions) *NodeBuilder {
	builder.node.Pos = pos

	return builder
}

// WithProps sets the node properties.
func (builder *NodeBuilder) WithProps(props map[string]string) *NodeBuilder {
	builder.node.Props = props

	return builder
}

// Build creates and returns the final Node.
func (builder *NodeBuilder) Build() *Node {
	builder.node.Children = make([]*Node, 0, initialChildCap)

	return builder.node
}

// New creates a new Node from the pool and initializes it with the given values.
func New(nodeID string, nodeType Type, token string, roles []Role, pos *Positions, props map[string]string) *Node {
	return NewBuilder().
		WithID(nodeID).
		WithType(nodeType).
		WithToken(token).
		WithRoles(roles).
		WithPosition(pos).
		WithProps(props).
		Build()
}

// NewNodeWithToken creates a new Node with type and token.
func NewNodeWithToken(nodeType Type, token string) *Node {
	poolNode, ok := nodePool.Get().(*Node)
	if !ok {
		poolNode = &Node{}
	}

	poolNode.ID = ""
	poolNode.Type = nodeType
	poolNode.Token = token
	poolNode.Roles = nil
	poolNode.Pos = nil
	poolNode.Props = nil
	poolNode.Children = nil

	return poolNode
}

// NewLiteralNode creates a new Node for literal values.
func NewLiteralNode(token string) *Node {
	return NewNodeWithToken("Literal", token)
}

// Release returns a Node to the pool for reuse.
func (targetNode *Node) Release() {
	// Clear the node to prevent memory leaks.
	targetNode.ID = ""
	targetNode.Type = ""
	targetNode.Token = ""
	targetNode.Roles = nil
	targetNode.Pos = nil
	targetNode.Props = nil
	targetNode.Children = nil
	nodePool.Put(targetNode)
}

// Find returns all nodes in the tree (including root) for which predicate(node) is true.
// Traversal is pre-order. Returns nil if n is nil.
func (targetNode *Node) Find(predicate func(*Node) bool) []*Node {
	if targetNode == nil {
		return nil
	}

	return findNodesWithPredicate(targetNode, predicate)
}

// AddChild appends a child node to n.
func (targetNode *Node) AddChild(child *Node) {
	targetNode.Children = append(targetNode.Children, child)
}

// RemoveChild removes the first occurrence of the given child node from n.
// Returns true if the child was found and removed.
func (targetNode *Node) RemoveChild(child *Node) bool {
	for idx, candidate := range targetNode.Children {
		if candidate == child {
			removeChildAtIndex(targetNode, idx)

			return true
		}
	}

	return false
}

// ReplaceChild replaces the first occurrence of old in Children with replacement.
// Returns true if replaced.
func (targetNode *Node) ReplaceChild(old, replacement *Node) bool {
	for idx, candidate := range targetNode.Children {
		if candidate == old {
			replaceChildAtIndex(targetNode, idx, replacement)

			return true
		}
	}

	return false
}

// VisitPreOrder visits all nodes in pre-order (root, then children left-to-right).
// Now uses the final optimized implementation with strict depth limiting.
func (targetNode *Node) VisitPreOrder(fn func(*Node)) {
	if targetNode == nil {
		return
	}

	// Use the channel-based optimized version and consume it.
	for visitNode := range preOrder(targetNode) {
		fn(visitNode)
	}
}

// PreOrder returns a channel streaming nodes in pre-order traversal.
// Now uses the final optimized implementation with strict depth limiting.
func (targetNode *Node) PreOrder() <-chan *Node {
	return preOrder(targetNode)
}

// VisitPostOrder visits all nodes in post-order (children left-to-right, then root).
// Now uses the final optimized implementation with strict depth limiting.
func (targetNode *Node) VisitPostOrder(fn func(*Node)) {
	postOrder(targetNode, fn)
}

// Ancestors returns the list of ancestors from root to the parent of target (empty if not found).
// Returns nil if n or target is nil.
func (targetNode *Node) Ancestors(target *Node) []*Node {
	if targetNode == nil || target == nil {
		return nil
	}

	return findAncestors(targetNode, target)
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
func (targetNode *Node) FindDSL(query string) ([]*Node, error) {
	if query == "" {
		return nil, errEmptyQuery
	}

	ast, err := targetNode.parseDSLQuery(query)
	if err != nil {
		return nil, err
	}

	initialInput := targetNode.determineInitialInput(ast)

	return targetNode.executeDSLRuntime(ast, initialInput)
}

func (targetNode *Node) parseDSLQuery(query string) (any, error) {
	ast, err := ParseDSL(query)
	if err != nil {
		return nil, fmt.Errorf("DSL parse error: %w", err)
	}

	return ast, nil
}

func (targetNode *Node) executeDSLRuntime(ast any, initialInput []*Node) ([]*Node, error) {
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

func (targetNode *Node) determineInitialInput(ast any) []*Node {
	if _, ok := ast.(*FilterNode); ok {
		return targetNode.Children
	}

	if pipeline, ok := ast.(*PipelineNode); ok {
		return targetNode.determinePipelineInput(pipeline)
	}

	return []*Node{targetNode}
}

func (targetNode *Node) determinePipelineInput(pipeline *PipelineNode) []*Node {
	if len(pipeline.Stages) == 0 {
		return targetNode.Children
	}

	if mapNode, ok := pipeline.Stages[0].(*MapNode); ok {
		return targetNode.determineMapNodeInput(mapNode)
	}

	return targetNode.Children
}

func (targetNode *Node) determineMapNodeInput(mapNode *MapNode) []*Node {
	if fieldNode, ok := mapNode.Expr.(*FieldNode); ok {
		return targetNode.determineFieldNodeInput(fieldNode)
	}

	return targetNode.Children
}

func (targetNode *Node) determineFieldNodeInput(fieldNode *FieldNode) []*Node {
	if len(fieldNode.Fields) == 1 && fieldNode.Fields[0] == "children" {
		return []*Node{targetNode}
	}

	return targetNode.Children
}

// HasAnyRole checks if the node has any of the given roles.
// Example:
//
//	if uast.HasAnyRole(node, uast.RoleFunction) {
//	    fmt.Println("Node is a function")
//	}
func (targetNode *Node) HasAnyRole(roles ...Role) bool {
	if targetNode == nil || len(targetNode.Roles) == 0 {
		return false
	}

	for _, role := range roles {
		if slices.Contains(targetNode.Roles, role) {
			return true
		}
	}

	return false
}

// HasAllRoles checks if the node has all of the given roles.
func (targetNode *Node) HasAllRoles(roles ...Role) bool {
	if targetNode == nil || len(targetNode.Roles) == 0 {
		return false
	}

	for _, role := range roles {
		if !slices.Contains(targetNode.Roles, role) {
			return false
		}
	}

	return true
}

// HasAnyType checks if the node has any of the given types.
func (targetNode *Node) HasAnyType(nodeTypes ...Type) bool {
	if targetNode == nil {
		return false
	}

	return slices.Contains(nodeTypes, targetNode.Type)
}

// TransformInPlace mutates the tree in place using the provided function.
// Example:
//
//	uast.Transform(root, func(n *uast.Node) bool {
//	    if n.Type == "Comment" {
//	        n.Token = ""
//	    }
//	    return true // continue traversal
//	})
func (targetNode *Node) TransformInPlace(fn func(*Node) bool) {
	transformInPlace(targetNode, fn)
}

// Transform returns a new tree where each node is replaced by the result of
// fn(node) (post-order, non-recursive).
// The returned tree is a deep copy with transformations applied. Returns nil if n is nil.
func (targetNode *Node) Transform(fn func(*Node) *Node) *Node {
	return transformNode(targetNode, fn)
}

// ToMap converts the node to a map representation.
func (targetNode *Node) ToMap() map[string]any {
	if targetNode == nil {
		return nil
	}

	result := buildBaseMap(targetNode)
	result["pos"] = buildPositionMap(targetNode.Pos)

	if len(targetNode.Children) > 0 {
		result["children"] = buildChildrenMap(targetNode.Children)
	}

	return result
}

// buildBaseMap creates the base map with type, id, token, props, and roles.
func buildBaseMap(targetNode *Node) map[string]any {
	result := map[string]any{
		"type": targetNode.Type,
	}

	addIDToMap(result, targetNode.ID)
	addTokenToMap(result, targetNode.Token)
	addPropsToMap(result, targetNode.Props)
	addRolesToMap(result, targetNode.Roles)

	return result
}

func addIDToMap(result map[string]any, nodeID string) {
	if nodeID != "" {
		result["id"] = fmt.Sprintf("%x", nodeID)
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

	for idx, role := range roles {
		roleStrings[idx] = string(role)
	}

	result["roles"] = roleStrings
}

// buildPositionMap creates the position map, handling nil positions.
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

// buildChildrenMap creates the children map array.
func buildChildrenMap(children []*Node) []map[string]any {
	childrenMap := make([]map[string]any, len(children))

	for idx, child := range children {
		childrenMap[idx] = child.ToMap()
	}

	return childrenMap
}

func removeChildAtIndex(targetNode *Node, index int) {
	targetNode.Children = append(targetNode.Children[:index], targetNode.Children[index+1:]...)
}

// String returns a string representation of the node.
func (targetNode *Node) String() string {
	return nodeString(targetNode)
}

// Optimized string representation without JSON marshaling.
func nodeString(targetNode *Node) string {
	if targetNode == nil {
		return "nil"
	}

	var buf strings.Builder

	buf.WriteString("Node{")
	buf.WriteString("Type:")
	buf.WriteString(string(targetNode.Type))

	appendToken(&buf, targetNode.Token)
	appendRoles(&buf, targetNode.Roles)
	appendProps(&buf, targetNode.Props)
	appendChildren(&buf, targetNode.Children)

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

		for idx, role := range roles {
			if idx > 0 {
				buf.WriteString(" ")
			}

			buf.WriteString(string(role))
		}

		buf.WriteString("]")
	}
}

func appendProps(buf *strings.Builder, props map[string]string) {
	if len(props) > 0 {
		fmt.Fprintf(buf, ",Props:%v", props)
	}
}

func appendChildren(buf *strings.Builder, children []*Node) {
	if len(children) > 0 {
		buf.WriteString(",Children:")
		buf.WriteString(strconv.Itoa(len(children)))
	}
}

func findNodesWithPredicate(targetNode *Node, predicate func(*Node) bool) []*Node {
	var result []*Node

	stack := []*Node{targetNode}

	for len(stack) > 0 {
		curr := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		if predicate(curr) {
			result = append(result, curr)
		}

		pushReversedChildren(curr, &stack)
	}

	return result
}

func pushReversedChildren(targetNode *Node, stack *[]*Node) {
	children := targetNode.Children
	reversed := make([]*Node, len(children))

	for idx := range children {
		reversed[len(reversed)-1-idx] = children[idx]
	}

	*stack = append(*stack, reversed...)
}

func findAncestors(targetNode, target *Node) []*Node {
	stack := []nodeAncestorFrame{{node: targetNode, parent: nil}}

	for len(stack) > 0 {
		top := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		if top.node == target {
			return top.parent
		}

		ancestorPath := append(append([]*Node{}, top.parent...), top.node)

		for idx := len(top.node.Children) - 1; idx >= 0; idx-- {
			stack = append(stack, nodeAncestorFrame{
				node:   top.node.Children[idx],
				parent: ancestorPath,
			})
		}
	}

	return nil
}

func transformNode(targetNode *Node, fn func(*Node) *Node) *Node {
	var stack []nodeTransformFrame

	results := make(map[*Node]*Node)

	stack = append(stack, nodeTransformFrame{
		node: targetNode, parent: nil, newNode: nil, childIdx: 0,
	})

	for len(stack) > 0 {
		top := &stack[len(stack)-1]

		if top.childIdx < len(top.node.Children) {
			stack = append(stack, nodeTransformFrame{
				node:     top.node.Children[top.childIdx],
				parent:   top.node,
				newNode:  nil,
				childIdx: 0,
			})

			top.childIdx++

			continue
		}

		nodeCopy := *top.node
		nodeCopy.Children = make([]*Node, len(top.node.Children))

		for idx, child := range top.node.Children {
			nodeCopy.Children[idx] = results[child]
		}

		results[top.node] = fn(&nodeCopy)
		stack = stack[:len(stack)-1]
	}

	return results[targetNode]
}

func replaceChildAtIndex(targetNode *Node, index int, replacement *Node) {
	targetNode.Children[index] = replacement
}

func transformInPlace(root *Node, fn func(*Node) bool) {
	stack := []*Node{root}

	for len(stack) > 0 {
		curr := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		if fn(curr) {
			pushReversedChildren(curr, &stack)
		}
	}
}

// Traversal depth and capacity constants.
const (
	defaultMaxDepth       = 25
	defaultStackCap       = 64
	depthLimitedMaxDepth  = 10
	stackCapGrowth        = 32
	iterativeQueueInitCap = 32
)

// Final optimized tree traversal with strict depth limiting.
func preOrder(targetNode *Node) <-chan *Node {
	ch := make(chan *Node)

	go func() {
		defer close(ch)

		if targetNode == nil {
			return
		}

		stack := make([]*Node, 0, defaultStackCap)
		stack = append(stack, targetNode)

		for len(stack) > 0 {
			curr := stack[len(stack)-1]
			stack = stack[:len(stack)-1]

			if curr == nil {
				continue
			}

			ch <- curr

			stack = processPreOrderChildren(curr, stack, defaultMaxDepth, ch)
		}
	}()

	return ch
}

func processPreOrderChildren(targetNode *Node, stack []*Node, maxAllowedDepth int, ch chan<- *Node) []*Node {
	if len(targetNode.Children) > 0 {
		if len(stack) >= maxAllowedDepth {
			processRemainingNodesDepthLimited(targetNode, ch, 0, depthLimitedMaxDepth)

			return stack
		}

		stack = ensureStackCapacity(stack, len(targetNode.Children))
		stack = pushChildrenToStackReversed(stack, targetNode.Children)
	}

	return stack
}

// ensureStackCapacity ensures the stack has enough capacity for additional children.
func ensureStackCapacity(stack []*Node, childCount int) []*Node {
	if cap(stack) < len(stack)+childCount {
		newStack := make([]*Node, len(stack), len(stack)+childCount+stackCapGrowth)
		copy(newStack, stack)

		return newStack
	}

	return stack
}

// pushChildrenToStackReversed pushes children to the stack in reverse order for pre-order traversal.
func pushChildrenToStackReversed(stack, children []*Node) []*Node {
	for idx := len(children) - 1; idx >= 0; idx-- {
		stack = append(stack, children[idx])
	}

	return stack
}

// processRemainingNodesDepthLimited processes remaining nodes with depth-limited recursion.
func processRemainingNodesDepthLimited(targetNode *Node, ch chan<- *Node, depth, maxDepth int) {
	if depth >= maxDepth {
		processRemainingNodesIterative(targetNode, ch)

		return
	}

	ch <- targetNode

	for _, child := range targetNode.Children {
		processRemainingNodesDepthLimited(child, ch, depth+1, maxDepth)
	}
}

// processRemainingNodesIterative processes remaining nodes iteratively.
func processRemainingNodesIterative(targetNode *Node, ch chan<- *Node) {
	queue := make([]*Node, 0, iterativeQueueInitCap)
	queue = append(queue, targetNode)

	for len(queue) > 0 {
		curr := queue[0]
		queue = queue[1:]

		if curr == nil {
			continue
		}

		ch <- curr

		queue = append(queue, curr.Children...)
	}
}

// postOrderFrame represents a frame in the post-order traversal stack.
type postOrderFrame struct {
	node  *Node
	index int
}

// Final optimized post-order traversal with strict depth limiting.
func postOrder(targetNode *Node, fn func(*Node)) {
	if targetNode == nil {
		return
	}

	stack := make([]postOrderFrame, 0, defaultStackCap)
	stack = append(stack, postOrderFrame{node: targetNode, index: 0})

	for len(stack) > 0 {
		if len(stack) >= defaultMaxDepth {
			processRemainingNodesPostOrderDepthLimited(targetNode, fn, 0, depthLimitedMaxDepth)

			break
		}

		top := &stack[len(stack)-1]

		if top.index == 0 && len(top.node.Children) > 0 {
			stack = ensurePostOrderStackCapacity(stack, len(top.node.Children))
			stack = pushChildrenToPostOrderStack(stack, top.node.Children)
			top.index = 1

			continue
		}

		fn(top.node)

		stack = stack[:len(stack)-1]
	}
}

// ensurePostOrderStackCapacity ensures the post-order stack has enough capacity.
func ensurePostOrderStackCapacity(stack []postOrderFrame, childCount int) []postOrderFrame {
	if cap(stack) < len(stack)+childCount {
		newStack := make([]postOrderFrame, len(stack), len(stack)+childCount+stackCapGrowth)
		copy(newStack, stack)

		return newStack
	}

	return stack
}

// pushChildrenToPostOrderStack pushes children to the post-order stack in reverse order.
func pushChildrenToPostOrderStack(stack []postOrderFrame, children []*Node) []postOrderFrame {
	for idx := len(children) - 1; idx >= 0; idx-- {
		stack = append(stack, postOrderFrame{node: children[idx], index: 0})
	}

	return stack
}

// processRemainingNodesPostOrderDepthLimited processes remaining nodes for post-order with depth limiting.
func processRemainingNodesPostOrderDepthLimited(targetNode *Node, fn func(*Node), depth, maxDepth int) {
	if depth >= maxDepth {
		// Switch to iterative approach.
		processRemainingNodesPostOrderIterative(targetNode, fn)

		return
	}

	for _, child := range targetNode.Children {
		processRemainingNodesPostOrderDepthLimited(child, fn, depth+1, maxDepth)
	}

	fn(targetNode)
}

// processRemainingNodesPostOrderIterative processes remaining nodes for post-order iteratively.
func processRemainingNodesPostOrderIterative(targetNode *Node, fn func(*Node)) {
	stack, visited := initializePostOrderIterative(targetNode)

	for len(stack) > 0 {
		curr := stack[len(stack)-1]

		if visited[curr] {
			fn(curr)

			stack = stack[:len(stack)-1]

			continue
		}

		visited[curr] = true

		for idx := len(curr.Children) - 1; idx >= 0; idx-- {
			stack = append(stack, curr.Children[idx])
		}
	}
}

//nolint:gocritic // unnamedResult: named returns conflict with nonamedreturns linter.
func initializePostOrderIterative(targetNode *Node) ([]*Node, map[*Node]bool) {
	stack := make([]*Node, 0, iterativeQueueInitCap)
	visited := make(map[*Node]bool)

	stack = append(stack, targetNode)

	return stack, visited
}

// AssignStableIDs assigns a stable id to each node in the tree based on its content and position.
func (targetNode *Node) AssignStableIDs() {
	if targetNode == nil {
		return
	}

	assignStableIDRecursive(targetNode)
}

// Hash buffer size constants.
const (
	hashBufSize  = 8
	posBufFields = 6
)

// assignStableIDRecursive recursively assigns stable IDs to nodes.
func assignStableIDRecursive(targetNode *Node) {
	if targetNode == nil {
		return
	}

	// G401: SHA1 is used here for content-based fingerprinting to generate stable node IDs,
	// not for security purposes. Collision resistance is not required for this use case.
	hasher := sha1.New() //nolint:gosec // SHA1 used for content fingerprinting, not security.

	writeNodeContentToHash(hasher, targetNode)

	// Process children first to get their IDs.
	for _, child := range targetNode.Children {
		assignStableIDRecursive(child)
		writeChildIDToHash(hasher, child)
	}

	// Use first 8 bytes of SHA1 as uint64 id.
	idBytes := hasher.Sum(nil)[:hashBufSize]
	targetNode.ID = string(idBytes)
}

// writeNodeContentToHash writes node content to the hash.
func writeNodeContentToHash(hasher hash.Hash, targetNode *Node) {
	hasher.Write([]byte(targetNode.Type))
	hasher.Write([]byte(targetNode.Token))

	if targetNode.Pos != nil {
		writePositionToHash(hasher, targetNode.Pos)
	}

	for _, role := range targetNode.Roles {
		hasher.Write([]byte(role))
	}
}

// writePositionToHash writes position information to the hash.
func writePositionToHash(hasher hash.Hash, pos *Positions) {
	buf := make([]byte, hashBufSize*posBufFields)

	writeStartPosition(buf, pos)
	writeEndPosition(buf, pos)

	hasher.Write(buf)
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

// writeChildIDToHash writes child ID to the hash.
func writeChildIDToHash(hasher hash.Hash, child *Node) {
	hasher.Write([]byte(child.ID))
}
