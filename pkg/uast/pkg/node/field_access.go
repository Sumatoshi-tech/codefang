package node

import (
	"fmt"
	"slices"
	"strconv"
	"strings"
)

// FieldAccessManager handles all field access operations.
type FieldAccessManager struct {
	processorRegistry *FieldProcessorRegistry
	extractorRegistry *ValueExtractorRegistry
}

// NewFieldAccessManager creates a new FieldAccessManager with default registries.
func NewFieldAccessManager() *FieldAccessManager {
	return &FieldAccessManager{
		processorRegistry: NewFieldProcessorRegistry(),
		extractorRegistry: NewValueExtractorRegistry(),
	}
}

// ProcessFieldAccess processes field access for a given FieldNode and Node.
func (mgr *FieldAccessManager) ProcessFieldAccess(fieldNode *FieldNode, node *Node) []*Node {
	if len(fieldNode.Fields) == 0 {
		return nil
	}

	if len(fieldNode.Fields) == 1 {
		return mgr.ProcessSingleField(fieldNode.Fields[0], node)
	}

	return mgr.ProcessNestedField(fieldNode.Fields, node)
}

// ProcessSingleField processes a single field access on a node.
func (mgr *FieldAccessManager) ProcessSingleField(field string, node *Node) []*Node {
	return globalFieldAccessRegistry.Access(node, field)
}

// ProcessNestedField processes nested field access across multiple fields.
func (mgr *FieldAccessManager) ProcessNestedField(fields []string, node *Node) []*Node {
	if len(fields) == 0 {
		return nil
	}

	firstField := fields[0]
	remainingFields := fields[1:]

	processor := mgr.processorRegistry.Get(firstField)

	return processor.Process(node, remainingFields)
}

// GetFieldValue returns the raw value of a field on a node.
func (mgr *FieldAccessManager) GetFieldValue(node *Node, fieldName string) any {
	extractor := mgr.extractorRegistry.Get(fieldName)

	return extractor.Extract(node)
}

// GetFirstFieldValue returns the first element of a field value as a node slice.
func (mgr *FieldAccessManager) GetFirstFieldValue(node *Node, fieldName string) []*Node {
	value := mgr.GetFieldValue(node, fieldName)
	if value == nil {
		return nil
	}

	switch typedVal := value.(type) {
	case []*Node:
		if len(typedVal) > 0 {
			return []*Node{typedVal[0]}
		}
	case string:
		if typedVal != "" {
			return []*Node{NewLiteralNode(string(typedVal[0]))}
		}
	}

	return nil
}

// GetLastFieldValue returns the last element of a field value as a node slice.
func (mgr *FieldAccessManager) GetLastFieldValue(node *Node, fieldName string) []*Node {
	value := mgr.GetFieldValue(node, fieldName)
	if value == nil {
		return nil
	}

	switch typedVal := value.(type) {
	case []*Node:
		if len(typedVal) > 0 {
			return []*Node{typedVal[len(typedVal)-1]}
		}
	case string:
		if typedVal != "" {
			return []*Node{NewLiteralNode(string(typedVal[len(typedVal)-1]))}
		}
	}

	return nil
}

// CheckMembership checks whether left values are members of right values.
func (mgr *FieldAccessManager) CheckMembership(leftFunc, rightFunc QueryFunc, node *Node) string {
	leftVals := leftFunc([]*Node{node})
	rightVals := rightFunc(nil)

	if len(leftVals) == 0 || len(rightVals) == 0 {
		return boolFalse
	}

	if mgr.isRolesMembership(leftVals) {
		return mgr.checkRolesMembership(leftVals, rightVals)
	}

	return mgr.checkGeneralMembership(leftVals, rightVals)
}

func (mgr *FieldAccessManager) isRolesMembership(leftVals []*Node) bool {
	return len(leftVals) == 1 && leftVals[0].Type == UASTLiteral && mgr.isRolesString(leftVals[0].Token)
}

func (mgr *FieldAccessManager) checkRolesMembership(leftVals, rightVals []*Node) string {
	leftStr := leftVals[0].Token
	if !mgr.isRolesString(leftStr) {
		return boolFalse
	}

	roles := mgr.extractRoles(leftStr)

	return mgr.matchRoles(roles, rightVals)
}

func (mgr *FieldAccessManager) isRolesString(str string) bool {
	return len(str) > 2 && str[0] == '[' && str[len(str)-1] == ']'
}

func (mgr *FieldAccessManager) extractRoles(rolesStr string) []string {
	content := rolesStr[1 : len(rolesStr)-1]

	return strings.Fields(content)
}

func (mgr *FieldAccessManager) matchRoles(roles []string, rightVals []*Node) string {
	for _, rightVal := range rightVals {
		if rightVal.Type == UASTLiteral {
			if slices.Contains(roles, rightVal.Token) {
				return "true"
			}
		}
	}

	return boolFalse
}

func (mgr *FieldAccessManager) checkGeneralMembership(leftVals, rightVals []*Node) string {
	for _, leftVal := range leftVals {
		for _, rightVal := range rightVals {
			if leftVal.Token == rightVal.Token {
				return "true"
			}
		}
	}

	return boolFalse
}

// FieldProcessorRegistry manages field processors.
type FieldProcessorRegistry struct {
	processors map[string]FieldProcessor
}

// NewFieldProcessorRegistry creates a new FieldProcessorRegistry with default processors.
func NewFieldProcessorRegistry() *FieldProcessorRegistry {
	registry := &FieldProcessorRegistry{
		processors: make(map[string]FieldProcessor),
	}

	registry.Register("children", &ChildrenFieldProcessor{})
	registry.Register("token", &TokenFieldProcessor{})
	registry.Register("id", &IDFieldProcessor{})
	registry.Register("roles", &RolesFieldProcessor{})
	registry.Register("type", &TypeFieldProcessor{})
	registry.Register("props", &PropsFieldProcessor{})

	return registry
}

// Register registers a field processor under the given name.
func (r *FieldProcessorRegistry) Register(name string, processor FieldProcessor) {
	r.processors[name] = processor
}

// Get returns the field processor for the given field name.
func (r *FieldProcessorRegistry) Get(field string) FieldProcessor {
	if processor, exists := r.processors[field]; exists {
		return processor
	}

	return &DefaultFieldProcessor{}
}

// ValueExtractorRegistry manages value extractors.
type ValueExtractorRegistry struct {
	extractors map[string]ValueExtractor
}

// NewValueExtractorRegistry creates a new ValueExtractorRegistry with default extractors.
func NewValueExtractorRegistry() *ValueExtractorRegistry {
	registry := &ValueExtractorRegistry{
		extractors: make(map[string]ValueExtractor),
	}

	registry.Register("children", &ChildrenValueExtractor{})
	registry.Register("token", &TokenValueExtractor{})
	registry.Register("id", &IDValueExtractor{})
	registry.Register("roles", &RolesValueExtractor{})
	registry.Register("type", &TypeValueExtractor{})

	return registry
}

// Register registers a value extractor under the given name.
func (r *ValueExtractorRegistry) Register(name string, extractor ValueExtractor) {
	r.extractors[name] = extractor
}

// Get returns the value extractor for the given field name.
func (r *ValueExtractorRegistry) Get(fieldName string) ValueExtractor {
	if extractor, exists := r.extractors[fieldName]; exists {
		return extractor
	}

	return &PropsValueExtractor{fieldName: fieldName}
}

// FieldProcessor defines the interface for processing fields on a node.
type FieldProcessor interface {
	Process(node *Node, remainingFields []string) []*Node
}

// ChildrenFieldProcessor processes the children field of a node.
type ChildrenFieldProcessor struct{}

// Process returns results from traversing child nodes with remaining fields.
func (p *ChildrenFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	var results []*Node

	for _, child := range node.Children {
		if len(remainingFields) > 0 {
			manager := NewFieldAccessManager()
			childResults := manager.ProcessNestedField(remainingFields, child)
			results = append(results, childResults...)
		} else {
			results = append(results, child)
		}
	}

	return results
}

// TokenFieldProcessor processes the token field of a node.
type TokenFieldProcessor struct{}

// Process returns the token value, optionally traversing nested fields.
func (p *TokenFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.Token, remainingFields)
	}

	return []*Node{NewLiteralNode(node.Token)}
}

// IDFieldProcessor processes the ID field of a node.
type IDFieldProcessor struct{}

// Process returns the ID value, optionally traversing nested fields.
func (p *IDFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.ID, remainingFields)
	}

	return []*Node{NewLiteralNode(node.ID)}
}

// RolesFieldProcessor processes the roles field of a node.
type RolesFieldProcessor struct{}

// Process returns the roles value, optionally traversing nested fields.
func (p *RolesFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(fmt.Sprintf("%v", node.Roles), remainingFields)
	}

	return []*Node{NewLiteralNode(fmt.Sprintf("%v", node.Roles))}
}

// TypeFieldProcessor processes the type field of a node.
type TypeFieldProcessor struct{}

// Process returns the type value, optionally traversing nested fields.
func (p *TypeFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.Type, remainingFields)
	}

	return []*Node{NewLiteralNode(string(node.Type))}
}

// PropsFieldProcessor processes the props field of a node.
type PropsFieldProcessor struct{}

// Process returns property values from nested fields.
func (p *PropsFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValueFromProps(node, remainingFields)
	}

	return nil
}

// DefaultFieldProcessor is the fallback processor for unknown fields.
type DefaultFieldProcessor struct{}

// Process uses the global field access registry for default processing.
func (p *DefaultFieldProcessor) Process(node *Node, _ []string) []*Node {
	return globalFieldAccessRegistry.Access(node, "default")
}

// ValueExtractor defines the interface for extracting values from a node.
type ValueExtractor interface {
	Extract(node *Node) any
}

// ChildrenValueExtractor extracts child nodes from a node.
type ChildrenValueExtractor struct{}

// Extract returns the children of the node.
func (e *ChildrenValueExtractor) Extract(node *Node) any {
	return node.Children
}

// TokenValueExtractor extracts the token from a node.
type TokenValueExtractor struct{}

// Extract returns the token of the node.
func (e *TokenValueExtractor) Extract(node *Node) any {
	return node.Token
}

// IDValueExtractor extracts the ID from a node.
type IDValueExtractor struct{}

// Extract returns the ID of the node.
func (e *IDValueExtractor) Extract(node *Node) any {
	return node.ID
}

// RolesValueExtractor extracts the roles from a node.
type RolesValueExtractor struct{}

// Extract returns the roles of the node.
func (e *RolesValueExtractor) Extract(node *Node) any {
	return node.Roles
}

// TypeValueExtractor extracts the type from a node.
type TypeValueExtractor struct{}

// Extract returns the type of the node.
func (e *TypeValueExtractor) Extract(node *Node) any {
	return node.Type
}

// PropsValueExtractor extracts a named property from a node.
type PropsValueExtractor struct {
	fieldName string
}

// Extract returns the property value if it exists on the node.
func (e *PropsValueExtractor) Extract(node *Node) any {
	if hasProp(node, e.fieldName) {
		return node.Props[e.fieldName]
	}

	return nil
}

var globalFieldAccessRegistry = NewFieldAccessStrategyRegistry()

// Helper functions for production usage.
func processFieldAccess(n *FieldNode, node *Node) []*Node {
	manager := NewFieldAccessManager()

	return manager.ProcessFieldAccess(n, node)
}

func checkMembership(leftFunc, rightFunc QueryFunc, node *Node) string {
	manager := NewFieldAccessManager()

	return manager.CheckMembership(leftFunc, rightFunc, node)
}

func getFirstFieldValue(node *Node, fieldName string) []*Node {
	manager := NewFieldAccessManager()

	return manager.GetFirstFieldValue(node, fieldName)
}

func getLastFieldValue(node *Node, fieldName string) []*Node {
	manager := NewFieldAccessManager()

	return manager.GetLastFieldValue(node, fieldName)
}

func getNestedFieldValue(value any, fields []string) []*Node {
	if len(fields) == 0 {
		return []*Node{NewLiteralNode(fmt.Sprintf("%v", value))}
	}

	if str, ok := value.(string); ok {
		if len(fields) == 1 && fields[0] == "length" {
			return []*Node{NewLiteralNode(strconv.Itoa(len(str)))}
		}
	}

	return []*Node{NewLiteralNode(fmt.Sprintf("%v", value))}
}

func getNestedFieldValueFromProps(node *Node, fields []string) []*Node {
	if len(fields) == 0 {
		return nil
	}

	if len(fields) == 1 {
		if value, exists := node.Props[fields[0]]; exists {
			return []*Node{NewLiteralNode(value)}
		}

		return nil
	}

	return nil
}
