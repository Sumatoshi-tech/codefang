// Package mapping provides Tree-Sitter to UAST mapping rules and grammar analysis.
package mapping

// NodeTypeInfo holds metadata for a Tree-Sitter node type.
type NodeTypeInfo struct {
	Name     string
	Fields   map[string]FieldInfo
	Children []ChildInfo
	Category NodeCategory // Leaf, Container, Operator.
	IsNamed  bool
}

// FieldInfo describes a field within a Tree-Sitter node type.
type FieldInfo struct {
	Name     string
	Types    []string
	Required bool
	Multiple bool
}

// ChildInfo describes a child node type.
type ChildInfo struct {
	Type  string
	Named bool
}

// NodeCategory classifies a Tree-Sitter node as Leaf, Container, or Operator.
type NodeCategory int

// Node category constants.
const (
	Leaf NodeCategory = iota
	Container
	Operator
)

// Rule represents a mapping from a Tree-Sitter pattern to a UAST specification.
type Rule struct {
	Name       string
	Pattern    string // S-expression or DSL.
	Extends    string // Optional: inheritance.
	UASTSpec   UASTSpec
	Conditions []Condition // Optional: conditional logic.
}

// Condition represents a conditional expression in a mapping rule.
type Condition struct {
	Expr string // The condition expression as parsed from DSL.
}

// UASTSpec defines the target UAST node structure for a mapping rule.
type UASTSpec struct {
	Type     string
	Token    string
	Roles    []string
	Props    map[string]string
	Children []string
}
