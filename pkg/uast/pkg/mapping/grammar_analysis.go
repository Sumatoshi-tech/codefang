package mapping

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

// ParseNodeTypes parses node-types.json and returns a slice of NodeTypeInfo.
func ParseNodeTypes(jsonData []byte) ([]NodeTypeInfo, error) {
	var raw []map[string]interface{}
	if err := json.Unmarshal(jsonData, &raw); err != nil {
		return nil, fmt.Errorf("failed to unmarshal node-types.json: %w", err)
	}
	nodes := make([]NodeTypeInfo, 0, len(raw))
	for _, entry := range raw {
		node, err := parseNodeTypeInfo(entry)
		if err != nil {
			return nil, err
		}
		nodes = append(nodes, node)
	}
	return nodes, nil
}

// ApplyHeuristicClassification applies heuristic rules to classify node types.
func ApplyHeuristicClassification(nodes []NodeTypeInfo) []NodeTypeInfo {
	for i := range nodes {
		nodes[i].Category = classifyNodeCategory(nodes[i])
	}
	return nodes
}

// CoverageAnalysis computes mapping coverage statistics.
func CoverageAnalysis(rules []MappingRule, nodeTypes []NodeTypeInfo) (float64, error) {
	mapped := make(map[string]bool)
	for _, rule := range rules {
		mapped[rule.Name] = true
	}
	total := len(nodeTypes)
	if total == 0 {
		return 0, fmt.Errorf("no node types to analyze")
	}
	covered := 0
	for _, n := range nodeTypes {
		if mapped[n.Name] {
			covered++
		}
	}
	return float64(covered) / float64(total), nil
}

func isValidIdentifier(name string) bool {
	if len(name) == 0 {
		return false
	}
	if !(('a' <= name[0] && name[0] <= 'z') || ('A' <= name[0] && name[0] <= 'Z') || name[0] == '_') {
		return false
	}
	for i := 1; i < len(name); i++ {
		c := name[i]
		if !(('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || c == '_') {
			return false
		}
	}
	return true
}

// CanonicalTypeRoleMap maps node name patterns to canonical UAST types and roles.
var CanonicalTypeRoleMap = []struct {
	Pattern string
	Type    string
	Roles   []string
}{
	{"function", "Function", []string{"Function", "Declaration"}},
	{"method", "Method", []string{"Function", "Declaration", "Member"}},
	{"class", "Class", []string{"Class", "Declaration"}},
	{"interface", "Interface", []string{"Interface", "Declaration"}},
	{"struct", "Struct", []string{"Struct", "Declaration"}},
	{"enum", "Enum", []string{"Enum", "Declaration"}},
	{"enum_member", "EnumMember", []string{"Member"}},
	{"variable", "Variable", []string{"Variable", "Declaration"}},
	{"parameter", "Parameter", []string{"Parameter"}},
	{"block", "Block", []string{"Body"}},
	{"if", "If", []string{}},
	{"loop", "Loop", []string{"Loop"}},
	{"for", "Loop", []string{"Loop"}},
	{"while", "Loop", []string{"Loop"}},
	{"switch", "Switch", []string{}},
	{"case", "Case", []string{"Branch"}},
	{"return", "Return", []string{"Return"}},
	{"break", "Break", []string{"Break"}},
	{"continue", "Continue", []string{"Continue"}},
	{"assignment", "Assignment", []string{"Assignment"}},
	{"call", "Call", []string{"Call"}},
	{"identifier", "Identifier", []string{"Name"}},
	{"literal", "Literal", []string{"Literal"}},
	{"binary_op", "BinaryOp", []string{"Operator"}},
	{"unary_op", "UnaryOp", []string{"Operator"}},
	{"import", "Import", []string{"Import"}},
	{"package", "Package", []string{"Module"}},
	{"attribute", "Attribute", []string{"Attribute"}},
	{"comment", "Comment", []string{"Comment"}},
	{"docstring", "DocString", []string{"Doc"}},
	{"type_annotation", "TypeAnnotation", []string{"Type"}},
	{"field", "Field", []string{"Member"}},
	{"property", "Property", []string{"Member"}},
	{"getter", "Getter", []string{"Getter"}},
	{"setter", "Setter", []string{"Setter"}},
	{"lambda", "Lambda", []string{"Lambda"}},
	{"try", "Try", []string{"Try"}},
	{"catch", "Catch", []string{"Catch"}},
	{"finally", "Finally", []string{"Finally"}},
	{"throw", "Throw", []string{"Throw"}},
	{"module", "Module", []string{"Module"}},
	{"namespace", "Namespace", []string{"Module"}},
	{"decorator", "Decorator", []string{"Attribute"}},
	{"spread", "Spread", []string{"Spread"}},
	{"tuple", "Tuple", []string{}},
	{"list", "List", []string{}},
	{"dict", "Dict", []string{}},
	{"set", "Set", []string{}},
	{"key_value", "KeyValue", []string{"Key", "Value"}},
	{"index", "Index", []string{"Index"}},
	{"slice", "Slice", []string{}},
	{"cast", "Cast", []string{}},
	{"await", "Await", []string{"Await"}},
	{"yield", "Yield", []string{"Yield"}},
	{"generator", "Generator", []string{"Generator"}},
	{"comprehension", "Comprehension", []string{}},
	{"pattern", "Pattern", []string{"Pattern"}},
	{"match", "Match", []string{"Match"}},
}

func guessUASTTypeAndRoles(name string) (string, []string) {
	lname := strings.ToLower(name)
	for _, entry := range CanonicalTypeRoleMap {
		if strings.Contains(lname, entry.Pattern) {
			return entry.Type, entry.Roles
		}
	}
	return "Synthetic", nil
}

func guessTokenField(n NodeTypeInfo) string {
	for fname := range n.Fields {
		if fname == "name" {
			return "@name"
		}
	}
	return ""
}

// parseNodeTypeInfo parses a single node type entry from node-types.json.
func parseNodeTypeInfo(entry map[string]interface{}) (NodeTypeInfo, error) {
	name, _ := entry["type"].(string)
	isNamed, _ := entry["named"].(bool)
	fields := parseFields(entry["fields"])
	children := parseChildren(entry["children"])
	return NodeTypeInfo{
		Name:     name,
		IsNamed:  isNamed,
		Fields:   fields,
		Children: children,
	}, nil
}

// parseFields parses the fields section of a node type entry.
func parseFields(raw interface{}) map[string]FieldInfo {
	fields := make(map[string]FieldInfo)
	m, ok := raw.(map[string]interface{})
	if !ok {
		return fields
	}
	for fname, fval := range m {
		info := FieldInfo{Name: fname}
		fmap, ok := fval.(map[string]interface{})
		if !ok {
			continue
		}
		info.Required, _ = fmap["required"].(bool)
		info.Types = parseFieldTypes(fmap["types"])
		info.Multiple = isFieldMultiple(fmap)
		fields[fname] = info
	}
	return fields
}

// parseFieldTypes extracts type names from the types array.
func parseFieldTypes(raw interface{}) []string {
	arr, ok := raw.([]interface{})
	if !ok {
		return nil
	}
	types := make([]string, 0, len(arr))
	for _, t := range arr {
		m, ok := t.(map[string]interface{})
		if !ok {
			continue
		}
		typeName, _ := m["type"].(string)
		if typeName != "" {
			types = append(types, typeName)
		}
	}
	return types
}

// isFieldMultiple infers if a field can have multiple values.
func isFieldMultiple(fmap map[string]interface{}) bool {
	// Heuristic: if types is an array with more than one entry, or if a 'multiple' flag exists
	if arr, ok := fmap["types"].([]interface{}); ok && len(arr) > 1 {
		return true
	}
	if mult, ok := fmap["multiple"].(bool); ok {
		return mult
	}
	return false
}

// parseChildren parses the children section of a node type entry.
func parseChildren(raw interface{}) []ChildInfo {
	arr, ok := raw.([]interface{})
	if !ok {
		return nil
	}
	children := make([]ChildInfo, 0, len(arr))
	for _, c := range arr {
		m, ok := c.(map[string]interface{})
		if !ok {
			continue
		}
		typeName, _ := m["type"].(string)
		named, _ := m["named"].(bool)
		children = append(children, ChildInfo{Type: typeName, Named: named})
	}
	return children
}

// classifyNodeCategory applies heuristic rules to classify a node type.
func classifyNodeCategory(n NodeTypeInfo) NodeCategory {
	if isLeafNode(n) {
		return Leaf
	}
	if isOperatorNode(n) {
		return Operator
	}
	return Container
}

// isLeafNode returns true if the node is a leaf (no children, no fields).
func isLeafNode(n NodeTypeInfo) bool {
	return len(n.Children) == 0 && len(n.Fields) == 0
}

// isOperatorNode returns true if the node is likely an operator (by name or pattern).
func isOperatorNode(n NodeTypeInfo) bool {
	return hasOperatorPattern(n.Name)
}

// hasOperatorPattern checks if the node name matches known operator patterns.
func hasOperatorPattern(name string) bool {
	return isPatternMatch(name, []string{"_operator", "_op", "operator", "binary_expression", "unary_expression"})
}

// isPatternMatch checks if the name matches any of the given patterns.
func isPatternMatch(name string, patterns []string) bool {
	for _, p := range patterns {
		if contains(name, p) {
			return true
		}
	}
	return false
}

// contains is a helper for substring matching.
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || (len(substr) > 0 && (len(s) > len(substr) && (s[:len(substr)] == substr || s[len(s)-len(substr):] == substr))) || (len(substr) > 0 && (len(s) > len(substr) && (s[len(s)-len(substr):] == substr))))
}

// writeLanguageDeclaration writes the language declaration section to the builder.
func writeLanguageDeclaration(sb *strings.Builder, language string, extensions []string) {
	if language == "" || len(extensions) == 0 {
		return
	}
	sb.WriteString(fmt.Sprintf("[language \"%s\", extensions: ", language))
	for i, ext := range extensions {
		if i > 0 {
			sb.WriteString(", ")
		}
		sb.WriteString(fmt.Sprintf("\"%s\"", ext))
	}
	sb.WriteString("]\n\n")
}

// collectChildTypes collects and returns sorted child type names from a node's children and fields.
func collectChildTypes(n NodeTypeInfo) []string {
	childTypes := make(map[string]struct{})
	for _, c := range n.Children {
		if isValidIdentifier(c.Type) {
			childTypes[c.Type] = struct{}{}
		}
	}
	for _, f := range n.Fields {
		for _, t := range f.Types {
			if isValidIdentifier(t) {
				childTypes[t] = struct{}{}
			}
		}
	}
	if len(childTypes) == 0 {
		return nil
	}
	keys := make([]string, 0, len(childTypes))
	for k := range childTypes {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// writeRolesSection writes the roles section if roles are present.
func writeRolesSection(sb *strings.Builder, roles []string) {
	if len(roles) == 0 {
		return
	}
	sb.WriteString(",\n    roles: ")
	for i, r := range roles {
		if i > 0 {
			sb.WriteString(", ")
		}
		sb.WriteString(fmt.Sprintf("\"%s\"", r))
	}
}

// writeChildrenSection writes the children section if children are present.
func writeChildrenSection(sb *strings.Builder, childTypes []string) {
	if len(childTypes) == 0 {
		return
	}
	sb.WriteString(",\n    children: ")
	for i, k := range childTypes {
		if i > 0 {
			sb.WriteString(", ")
		}
		sb.WriteString(fmt.Sprintf("\"%s\"", k))
	}
}

// writeNodeMapping writes a single node's mapping DSL to the builder.
func writeNodeMapping(sb *strings.Builder, n NodeTypeInfo) {
	uastType, roles := guessUASTTypeAndRoles(n.Name)
	isLeaf := len(n.Children) == 0 && len(n.Fields) == 0

	sb.WriteString(fmt.Sprintf("%s <- (%s) => uast(\n", n.Name, n.Name))
	sb.WriteString(fmt.Sprintf("    type: \"%s\"", uastType))

	if isLeaf {
		if token := guessTokenField(n); token != "" {
			sb.WriteString(fmt.Sprintf(",\n    token: \"%s\"", token))
		}
	}

	writeRolesSection(sb, roles)
	writeChildrenSection(sb, collectChildTypes(n))

	sb.WriteString("\n)\n\n")
}

// GenerateMappingDSL emits mapping DSL for a set of node types, using canonical UAST types/roles.
func GenerateMappingDSL(nodes []NodeTypeInfo, language string, extensions []string) string {
	var sb strings.Builder

	writeLanguageDeclaration(&sb, language, extensions)

	for _, n := range nodes {
		if !isValidIdentifier(n.Name) {
			continue
		}
		writeNodeMapping(&sb, n)
	}
	return sb.String()
}
