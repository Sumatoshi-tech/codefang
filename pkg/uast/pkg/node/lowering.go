package node

import (
	"errors"
	"fmt"
)

// errUnsupportedDSLNodeType is returned for unknown DSL node types.
var errUnsupportedDSLNodeType = errors.New("unsupported DSL node type")

// LowerDSL lowers a DSLNode AST to a QueryFunc.
func LowerDSL(ast DSLNode) (QueryFunc, error) {
	return globalLowererRegistry.Lower(ast)
}

//nolint:gochecknoglobals // Singleton lowerer registry.
var globalLowererRegistry *DSLNodeLowererRegistry

//nolint:gochecknoinits // Required to break initialization cycle with LowerDSL.
func init() {
	globalLowererRegistry = NewDSLNodeLowererRegistry()
	globalLowererRegistry.Register(PipelineType, &PipelineLowerer{})
	globalLowererRegistry.Register(MapType, &MapLowerer{})
	globalLowererRegistry.Register(FilterType, &FilterLowerer{})
	globalLowererRegistry.Register(ReduceType, &ReduceLowerer{})
	globalLowererRegistry.Register(FieldType, &FieldLowerer{})
	globalLowererRegistry.Register(LiteralType, &LiteralLowerer{})
	globalLowererRegistry.Register(CallType, &CallLowerer{})
	globalLowererRegistry.Register(RMapType, &RMapLowerer{})
	globalLowererRegistry.Register(RFilterType, &RFilterLowerer{})
}

// NewDSLNodeLowererRegistry creates a new DSLNodeLowererRegistry.
func NewDSLNodeLowererRegistry() *DSLNodeLowererRegistry {
	return &DSLNodeLowererRegistry{
		lowerers: make(map[DSLNodeType]DSLNodeLowerer),
	}
}

// Register adds a lowerer for the given node type.
func (registry *DSLNodeLowererRegistry) Register(nodeType DSLNodeType, lowerer DSLNodeLowerer) {
	registry.lowerers[nodeType] = lowerer
}

// Lower dispatches a DSLNode to the appropriate lowerer.
func (registry *DSLNodeLowererRegistry) Lower(dslNode DSLNode) (QueryFunc, error) {
	nodeType := ClassifyDSLNode(dslNode)

	lowerer, exists := registry.lowerers[nodeType]
	if !exists {
		return nil, fmt.Errorf("%w: %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerer.Lower(dslNode)
}

// Lowerer implementations.
// Note: Type assertions below are safe because the registry routes by node type.
// Each lowerer is only called with nodes of the correct type.

// PipelineLowerer lowers pipeline nodes.
type PipelineLowerer struct{}

// Lower converts a PipelineNode to a QueryFunc.
func (lowerer *PipelineLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	pipelineNode, ok := dslNode.(*PipelineNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected PipelineNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerPipeline(pipelineNode)
}

// MapLowerer lowers map nodes.
type MapLowerer struct{}

// Lower converts a MapNode to a QueryFunc.
func (lowerer *MapLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	mapNode, ok := dslNode.(*MapNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected MapNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerMap(mapNode)
}

// FilterLowerer lowers filter nodes.
type FilterLowerer struct{}

// Lower converts a FilterNode to a QueryFunc.
func (lowerer *FilterLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	filterNode, ok := dslNode.(*FilterNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected FilterNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerFilter(filterNode)
}

// ReduceLowerer lowers reduce nodes.
type ReduceLowerer struct{}

// Lower converts a ReduceNode to a QueryFunc.
func (lowerer *ReduceLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	reduceNode, ok := dslNode.(*ReduceNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected ReduceNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerReduce(reduceNode)
}

// FieldLowerer lowers field access nodes.
type FieldLowerer struct{}

// Lower converts a FieldNode to a QueryFunc.
func (lowerer *FieldLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	fieldNode, ok := dslNode.(*FieldNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected FieldNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerField(fieldNode)
}

// LiteralLowerer lowers literal nodes.
type LiteralLowerer struct{}

// Lower converts a LiteralNode to a QueryFunc.
func (lowerer *LiteralLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	literalNode, ok := dslNode.(*LiteralNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected LiteralNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerLiteral(literalNode)
}

// CallLowerer lowers call nodes.
type CallLowerer struct{}

// Lower converts a CallNode to a QueryFunc.
func (lowerer *CallLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	callNode, ok := dslNode.(*CallNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected CallNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerCall(callNode)
}

// RMapLowerer lowers recursive map nodes.
type RMapLowerer struct{}

// Lower converts an RMapNode to a QueryFunc.
func (lowerer *RMapLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	rmapNode, ok := dslNode.(*RMapNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected RMapNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerRMap(rmapNode)
}

// RFilterLowerer lowers recursive filter nodes.
type RFilterLowerer struct{}

// Lower converts an RFilterNode to a QueryFunc.
func (lowerer *RFilterLowerer) Lower(dslNode DSLNode) (QueryFunc, error) {
	rfilterNode, ok := dslNode.(*RFilterNode)
	if !ok {
		return nil, fmt.Errorf("%w: expected RFilterNode, got %T", errUnsupportedDSLNodeType, dslNode)
	}

	return lowerRFilter(rfilterNode)
}

func lowerCall(callNode *CallNode) (QueryFunc, error) {
	return globalOperatorRegistry.Handle(callNode)
}

// Core lowering functions.
func lowerField(fieldNode *FieldNode) (QueryFunc, error) {
	return func(nodes []*Node) []*Node {
		var out []*Node

		for _, targetNode := range nodes {
			result := processFieldAccess(fieldNode, targetNode)

			if result != nil {
				out = append(out, result...)
			}
		}

		return out
	}, nil
}

func lowerLiteral(literalNode *LiteralNode) (QueryFunc, error) {
	return func(_ []*Node) []*Node {
		return []*Node{NewLiteralNode(fmt.Sprint(literalNode.Value))}
	}, nil
}
