package analyze

import "github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"

// NodeVisitor defines the interface for UAST visitors.
type NodeVisitor interface {
	OnEnter(n *node.Node, depth int)
	OnExit(n *node.Node, depth int)
}

// AnalysisVisitor extends NodeVisitor to provide analysis results.
type AnalysisVisitor interface {
	NodeVisitor
	GetReport() Report
}
