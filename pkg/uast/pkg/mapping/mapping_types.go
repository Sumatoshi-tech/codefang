package mapping

// NodeTypeInfo holds metadata for a Tree-Sitter node type.
type NodeTypeInfo struct {
	Name     string
	IsNamed  bool
	Fields   map[string]FieldInfo
	Children []ChildInfo
	Category NodeCategory // Leaf, Container, Operator
}

type FieldInfo struct {
	Name     string
	Required bool
	Types    []string
	Multiple bool
}

type ChildInfo struct {
	Type  string
	Named bool
}

type NodeCategory int

const (
	Leaf NodeCategory = iota
	Container
	Operator
)

// MappingRule represents a mapping from a Tree-Sitter pattern to a UAST specification.
type MappingRule struct {
	Name       string
	Pattern    string // S-expression or DSL
	UASTSpec   UASTSpec
	Extends    string      // Optional: inheritance
	Conditions []Condition // Optional: conditional logic
}

type Condition struct {
	Expr string // The condition expression as parsed from DSL
}

type UASTSpec struct {
	Type     string
	Token    string
	Roles    []string
	Props    map[string]string
	Children []string
}
