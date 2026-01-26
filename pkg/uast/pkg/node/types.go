package node

// Node Types.
// These types represent the building blocks for DSL (Domain Specific Language) operations.

// DSLNode represents any node in the DSL tree.
type DSLNode any

// MapNode represents a map operation in the DSL.
type MapNode struct{ Expr DSLNode }

// FilterNode represents a filter operation in the DSL.
type FilterNode struct{ Expr DSLNode }

// ReduceNode represents a reduce operation in the DSL.
type ReduceNode struct{ Expr DSLNode }

// FieldNode represents field access in the DSL.
type FieldNode struct{ Fields []string }

// LiteralNode represents a literal value in the DSL.
type LiteralNode struct{ Value any }

// CallNode represents a function call in the DSL.
type CallNode struct {
	Name string
	Args []DSLNode
}

// PipelineNode represents a pipeline of operations in the DSL.
type PipelineNode struct{ Stages []DSLNode }

// RMapNode represents a reverse map operation in the DSL.
type RMapNode struct{ Expr DSLNode }

// RFilterNode represents a reverse filter operation in the DSL.
type RFilterNode struct{ Expr DSLNode }

// QueryFunc represents a function that processes a slice of nodes and returns a slice of nodes.
type QueryFunc func([]*Node) []*Node

// DSLNodeType represents the type of a DSL node.
type DSLNodeType string

// DSL node type constants.
const (
	PipelineType DSLNodeType = "Pipeline"
	MapType      DSLNodeType = "Map"
	FilterType   DSLNodeType = "Filter"
	ReduceType   DSLNodeType = "Reduce"
	FieldType    DSLNodeType = "Field"
	LiteralType  DSLNodeType = "Literal"
	CallType     DSLNodeType = "Call"
	RMapType     DSLNodeType = "RMap"
	RFilterType  DSLNodeType = "RFilter"
)

// Lowering Interfaces.
// These interfaces handle the conversion of DSL nodes to executable functions.

// DSLNodeLowerer defines the interface for lowering DSL nodes to query functions.
type DSLNodeLowerer interface {
	Lower(node DSLNode) (QueryFunc, error)
}

// DSLNodeLowererRegistry manages a collection of DSL node lowerers.
type DSLNodeLowererRegistry struct {
	lowerers map[DSLNodeType]DSLNodeLowerer
}

// Field Access Interfaces.
// These interfaces handle field access strategies for nodes.

// FieldAccessStrategy defines the interface for accessing node fields.
type FieldAccessStrategy interface {
	Access(node *Node) []*Node
}

// FieldAccessStrategyRegistry manages a collection of field access strategies.
type FieldAccessStrategyRegistry struct {
	strategies map[string]FieldAccessStrategy
}

// NewFieldAccessStrategyRegistry creates and initializes a new field access strategy registry.
func NewFieldAccessStrategyRegistry() *FieldAccessStrategyRegistry {
	registry := &FieldAccessStrategyRegistry{
		strategies: make(map[string]FieldAccessStrategy),
	}

	registry.registerDefaultStrategies()

	return registry
}

// registerDefaultStrategies registers the default field access strategies.
func (registry *FieldAccessStrategyRegistry) registerDefaultStrategies() {
	registry.strategies["children"] = &ChildrenFieldStrategy{}
	registry.strategies["token"] = &TokenFieldStrategy{}
	registry.strategies["id"] = &IDFieldStrategy{}
	registry.strategies["roles"] = &RolesFieldStrategy{}
	registry.strategies["type"] = &TypeFieldStrategy{}
	registry.strategies["first"] = &FirstFieldStrategy{}
	registry.strategies["last"] = &LastFieldStrategy{}
}

// Access retrieves nodes using the specified field access strategy.
func (registry *FieldAccessStrategyRegistry) Access(fieldNode *Node, fieldName string) []*Node {
	if strategy, exists := registry.strategies[fieldName]; exists {
		return strategy.Access(fieldNode)
	}

	if hasProp(fieldNode, fieldName) {
		return []*Node{NewLiteralNode(fieldNode.Props[fieldName])}
	}

	return nil
}

// hasProp checks if a node has a property with the given name.
func hasProp(fieldNode *Node, fieldName string) bool {
	_, exists := fieldNode.Props[fieldName]

	return exists
}

// Field Access Strategy Implementations.
// These structs implement specific field access strategies.

// ChildrenFieldStrategy accesses the children of a node.
type ChildrenFieldStrategy struct{}

// Access returns the children of the given node.
func (strategy *ChildrenFieldStrategy) Access(fieldNode *Node) []*Node {
	return fieldNode.Children
}

// TokenFieldStrategy accesses the token of a node.
type TokenFieldStrategy struct{}

// Access returns the token of the given node as a literal node.
func (strategy *TokenFieldStrategy) Access(fieldNode *Node) []*Node {
	return []*Node{NewLiteralNode(fieldNode.Token)}
}

// IDFieldStrategy accesses the ID of a node.
type IDFieldStrategy struct{}

// Access returns the ID of the given node as a literal node.
func (strategy *IDFieldStrategy) Access(fieldNode *Node) []*Node {
	return []*Node{NewLiteralNode(fieldNode.ID)}
}

// RolesFieldStrategy accesses the roles of a node.
type RolesFieldStrategy struct{}

// Access returns each role of the given node as a separate literal node.
func (strategy *RolesFieldStrategy) Access(fieldNode *Node) []*Node {
	out := make([]*Node, 0, len(fieldNode.Roles))

	for _, role := range fieldNode.Roles {
		out = append(out, NewLiteralNode(string(role)))
	}

	return out
}

// TypeFieldStrategy accesses the type of a node.
type TypeFieldStrategy struct{}

// Access returns the type of the given node as a literal node if it exists.
func (strategy *TypeFieldStrategy) Access(fieldNode *Node) []*Node {
	if fieldNode.Type != "" {
		return []*Node{NewLiteralNode(string(fieldNode.Type))}
	}

	return nil
}

// FirstFieldStrategy accesses the first field value of a node.
type FirstFieldStrategy struct{}

// Access returns the first field value of the given node.
func (strategy *FirstFieldStrategy) Access(fieldNode *Node) []*Node {
	return getFirstFieldValue(fieldNode, "first")
}

// LastFieldStrategy accesses the last field value of a node.
type LastFieldStrategy struct{}

// Access returns the last field value of the given node.
func (strategy *LastFieldStrategy) Access(fieldNode *Node) []*Node {
	return getLastFieldValue(fieldNode, "last")
}
