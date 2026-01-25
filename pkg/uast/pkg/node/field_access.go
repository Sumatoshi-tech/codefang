package node

import (
	"fmt"
	"slices"
	"strings"
)

// FieldAccessManager handles all field access operations
type FieldAccessManager struct {
	processorRegistry *FieldProcessorRegistry
	extractorRegistry *ValueExtractorRegistry
}

func NewFieldAccessManager() *FieldAccessManager {
	return &FieldAccessManager{
		processorRegistry: NewFieldProcessorRegistry(),
		extractorRegistry: NewValueExtractorRegistry(),
	}
}

func (m *FieldAccessManager) ProcessFieldAccess(n *FieldNode, node *Node) []*Node {
	if len(n.Fields) == 0 {
		return nil
	}
	if len(n.Fields) == 1 {
		return m.ProcessSingleField(n.Fields[0], node)
	}
	return m.ProcessNestedField(n.Fields, node)
}

func (m *FieldAccessManager) ProcessSingleField(field string, node *Node) []*Node {
	return globalFieldAccessRegistry.Access(node, field)
}

func (m *FieldAccessManager) ProcessNestedField(fields []string, node *Node) []*Node {
	if len(fields) == 0 {
		return nil
	}

	firstField := fields[0]
	remainingFields := fields[1:]

	processor := m.processorRegistry.Get(firstField)
	return processor.Process(node, remainingFields)
}

func (m *FieldAccessManager) GetFieldValue(node *Node, fieldName string) interface{} {
	extractor := m.extractorRegistry.Get(fieldName)
	return extractor.Extract(node)
}

func (m *FieldAccessManager) GetFirstFieldValue(node *Node, fieldName string) []*Node {
	value := m.GetFieldValue(node, fieldName)
	if value == nil {
		return nil
	}

	switch v := value.(type) {
	case []*Node:
		if len(v) > 0 {
			return []*Node{v[0]}
		}
	case string:
		if len(v) > 0 {
			return []*Node{NewLiteralNode(string(v[0]))}
		}
	}
	return nil
}

func (m *FieldAccessManager) GetLastFieldValue(node *Node, fieldName string) []*Node {
	value := m.GetFieldValue(node, fieldName)
	if value == nil {
		return nil
	}

	switch v := value.(type) {
	case []*Node:
		if len(v) > 0 {
			return []*Node{v[len(v)-1]}
		}
	case string:
		if len(v) > 0 {
			return []*Node{NewLiteralNode(string(v[len(v)-1]))}
		}
	}
	return nil
}

func (m *FieldAccessManager) CheckMembership(leftFunc, rightFunc QueryFunc, node *Node) string {
	leftVals := leftFunc([]*Node{node})
	rightVals := rightFunc(nil)

	if len(leftVals) == 0 || len(rightVals) == 0 {
		return "false"
	}

	if m.isRolesMembership(leftVals) {
		return m.checkRolesMembership(leftVals, rightVals)
	}

	return m.checkGeneralMembership(leftVals, rightVals)
}

func (m *FieldAccessManager) isRolesMembership(leftVals []*Node) bool {
	return len(leftVals) == 1 && leftVals[0].Type == "Literal" && m.isRolesString(leftVals[0].Token)
}

func (m *FieldAccessManager) checkRolesMembership(leftVals, rightVals []*Node) string {
	leftStr := leftVals[0].Token
	if !m.isRolesString(leftStr) {
		return "false"
	}

	roles := m.extractRoles(leftStr)
	return m.matchRoles(roles, rightVals)
}

func (m *FieldAccessManager) isRolesString(str string) bool {
	return len(str) > 2 && str[0] == '[' && str[len(str)-1] == ']'
}

func (m *FieldAccessManager) extractRoles(rolesStr string) []string {
	content := rolesStr[1 : len(rolesStr)-1]
	return strings.Fields(content)
}

func (m *FieldAccessManager) matchRoles(roles []string, rightVals []*Node) string {
	for _, rightVal := range rightVals {
		if rightVal.Type == "Literal" {
			if slices.Contains(roles, rightVal.Token) {
				return "true"
			}
		}
	}
	return "false"
}

func (m *FieldAccessManager) checkGeneralMembership(leftVals, rightVals []*Node) string {
	for _, leftVal := range leftVals {
		for _, rightVal := range rightVals {
			if leftVal.Token == rightVal.Token {
				return "true"
			}
		}
	}
	return "false"
}

// FieldProcessorRegistry manages field processors
type FieldProcessorRegistry struct {
	processors map[string]FieldProcessor
}

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

func (r *FieldProcessorRegistry) Register(name string, processor FieldProcessor) {
	r.processors[name] = processor
}

func (r *FieldProcessorRegistry) Get(field string) FieldProcessor {
	if processor, exists := r.processors[field]; exists {
		return processor
	}
	return &DefaultFieldProcessor{}
}

// ValueExtractorRegistry manages value extractors
type ValueExtractorRegistry struct {
	extractors map[string]ValueExtractor
}

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

func (r *ValueExtractorRegistry) Register(name string, extractor ValueExtractor) {
	r.extractors[name] = extractor
}

func (r *ValueExtractorRegistry) Get(fieldName string) ValueExtractor {
	if extractor, exists := r.extractors[fieldName]; exists {
		return extractor
	}
	return &PropsValueExtractor{fieldName: fieldName}
}

type FieldProcessor interface {
	Process(node *Node, remainingFields []string) []*Node
}

type ChildrenFieldProcessor struct{}

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

type TokenFieldProcessor struct{}

func (p *TokenFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.Token, remainingFields)
	}
	return []*Node{NewLiteralNode(node.Token)}
}

type IDFieldProcessor struct{}

func (p *IDFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.ID, remainingFields)
	}
	return []*Node{NewLiteralNode(node.ID)}
}

type RolesFieldProcessor struct{}

func (p *RolesFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(fmt.Sprintf("%v", node.Roles), remainingFields)
	}
	return []*Node{NewLiteralNode(fmt.Sprintf("%v", node.Roles))}
}

type TypeFieldProcessor struct{}

func (p *TypeFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValue(node.Type, remainingFields)
	}
	return []*Node{NewLiteralNode(string(node.Type))}
}

type PropsFieldProcessor struct{}

func (p *PropsFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	if len(remainingFields) > 0 {
		return getNestedFieldValueFromProps(node, remainingFields)
	}
	return nil
}

type DefaultFieldProcessor struct{}

func (p *DefaultFieldProcessor) Process(node *Node, remainingFields []string) []*Node {
	return globalFieldAccessRegistry.Access(node, "default")
}

type ValueExtractor interface {
	Extract(node *Node) interface{}
}

type ChildrenValueExtractor struct{}

func (e *ChildrenValueExtractor) Extract(node *Node) interface{} {
	return node.Children
}

type TokenValueExtractor struct{}

func (e *TokenValueExtractor) Extract(node *Node) interface{} {
	return node.Token
}

type IDValueExtractor struct{}

func (e *IDValueExtractor) Extract(node *Node) interface{} {
	return node.ID
}

type RolesValueExtractor struct{}

func (e *RolesValueExtractor) Extract(node *Node) interface{} {
	return node.Roles
}

type TypeValueExtractor struct{}

func (e *TypeValueExtractor) Extract(node *Node) interface{} {
	return node.Type
}

type PropsValueExtractor struct {
	fieldName string
}

func (e *PropsValueExtractor) Extract(node *Node) interface{} {
	if hasProp(node, e.fieldName) {
		return node.Props[e.fieldName]
	}
	return nil
}

var globalFieldAccessRegistry = NewFieldAccessStrategyRegistry()

// Helper functions for production usage
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

func getNestedFieldValue(value interface{}, fields []string) []*Node {
	if len(fields) == 0 {
		return []*Node{NewLiteralNode(fmt.Sprintf("%v", value))}
	}

	if str, ok := value.(string); ok {
		if len(fields) == 1 && fields[0] == "length" {
			return []*Node{NewLiteralNode(fmt.Sprintf("%d", len(str)))}
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
