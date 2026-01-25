package node

// ClassifyDSLNode classifies DSL nodes by type
func ClassifyDSLNode(node DSLNode) DSLNodeType {
	switch node.(type) {
	case *PipelineNode:
		return PipelineType
	case *MapNode:
		return MapType
	case *FilterNode:
		return FilterType
	case *ReduceNode:
		return ReduceType
	case *FieldNode:
		return FieldType
	case *LiteralNode:
		return LiteralType
	case *CallNode:
		return CallType
	case *RMapNode:
		return RMapType
	case *RFilterNode:
		return RFilterType
	default:
		return ""
	}
}

// isLiteralNode checks if a node is a literal node
func isLiteralNode(n DSLNode) bool {
	_, ok := n.(*LiteralNode)
	return ok
}
