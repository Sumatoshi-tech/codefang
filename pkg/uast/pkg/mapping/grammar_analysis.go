package mapping

import (
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
)

// errNoNodeTypes is returned when there are no node types to analyze.
var errNoNodeTypes = errors.New("no node types to analyze")

// ParseNodeTypes parses node-types.json and returns a slice of NodeTypeInfo.
func ParseNodeTypes(jsonData []byte) ([]NodeTypeInfo, error) {
	var raw []map[string]any

	err := json.Unmarshal(jsonData, &raw)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal node-types.json: %w", err)
	}

	nodes := make([]NodeTypeInfo, 0, len(raw))

	for _, entry := range raw {
		parsedNode := parseNodeTypeInfo(entry)
		nodes = append(nodes, parsedNode)
	}

	return nodes, nil
}

// ApplyHeuristicClassification applies heuristic rules to classify node types.
func ApplyHeuristicClassification(nodes []NodeTypeInfo) []NodeTypeInfo {
	for idx := range nodes {
		nodes[idx].Category = classifyNodeCategory(nodes[idx])
	}

	return nodes
}

// CoverageAnalysis computes mapping coverage statistics.
func CoverageAnalysis(rules []Rule, nodeTypes []NodeTypeInfo) (float64, error) {
	mapped := make(map[string]bool)

	for _, rule := range rules {
		mapped[rule.Name] = true
	}

	total := len(nodeTypes)
	if total == 0 {
		return 0, errNoNodeTypes
	}

	covered := 0

	for _, nodeType := range nodeTypes {
		if mapped[nodeType.Name] {
			covered++
		}
	}

	return float64(covered) / float64(total), nil
}

func isValidIdentifier(name string) bool {
	if name == "" {
		return false
	}

	first := name[0]
	if (first < 'a' || first > 'z') && (first < 'A' || first > 'Z') && first != '_' {
		return false
	}

	for idx := 1; idx < len(name); idx++ {
		ch := name[idx]

		if (ch < 'a' || ch > 'z') && (ch < 'A' || ch > 'Z') && (ch < '0' || ch > '9') && ch != '_' {
			return false
		}
	}

	return true
}

// canonicalTypeRole maps a node name pattern to a canonical UAST type and roles.
type canonicalTypeRole struct {
	Pattern string
	Type    string
	Roles   []string
}

var canonicalTypeRoleMap = []canonicalTypeRole{
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
	{"if", "If", nil},
	{"loop", "Loop", []string{"Loop"}},
	{"for", "Loop", []string{"Loop"}},
	{"while", "Loop", []string{"Loop"}},
	{"switch", "Switch", nil},
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
	{"tuple", "Tuple", nil},
	{"list", "List", nil},
	{"dict", "Dict", nil},
	{"set", "Set", nil},
	{"key_value", "KeyValue", []string{"Key", "Value"}},
	{"index", "Index", []string{"Index"}},
	{"slice", "Slice", nil},
	{"cast", "Cast", nil},
	{"await", "Await", []string{"Await"}},
	{"yield", "Yield", []string{"Yield"}},
	{"generator", "Generator", []string{"Generator"}},
	{"comprehension", "Comprehension", nil},
	{"pattern", "Pattern", []string{"Pattern"}},
	{"match", "Match", []string{"Match"}},
}

func guessUASTTypeAndRoles(name string) (uastType string, roles []string) {
	lname := strings.ToLower(name)

	for _, entry := range canonicalTypeRoleMap {
		if strings.Contains(lname, entry.Pattern) {
			return entry.Type, entry.Roles
		}
	}

	return "Synthetic", nil
}

func guessTokenField(nodeInfo NodeTypeInfo) string {
	for fname := range nodeInfo.Fields {
		if fname == "name" {
			return "@name"
		}
	}

	return ""
}

// parseNodeTypeInfo parses a single node type entry from node-types.json.
func parseNodeTypeInfo(entry map[string]any) NodeTypeInfo {
	name, nameOK := entry["type"].(string)
	if !nameOK {
		name = ""
	}

	isNamed, namedOK := entry["named"].(bool)
	if !namedOK {
		isNamed = false
	}

	fields := parseFields(entry["fields"])
	children := parseChildren(entry["children"])

	return NodeTypeInfo{
		Name:     name,
		Fields:   fields,
		Children: children,
		Category: 0,
		IsNamed:  isNamed,
	}
}

// parseFields parses the fields section of a node type entry.
func parseFields(raw any) map[string]FieldInfo {
	fields := make(map[string]FieldInfo)

	fieldMap, ok := raw.(map[string]any)
	if !ok {
		return fields
	}

	for fname, fval := range fieldMap {
		info := FieldInfo{Name: fname, Types: nil, Required: false, Multiple: false}

		fmap, fmapOK := fval.(map[string]any)
		if !fmapOK {
			continue
		}

		if req, reqOK := fmap["required"].(bool); reqOK {
			info.Required = req
		}

		info.Types = parseFieldTypes(fmap["types"])
		info.Multiple = isFieldMultiple(fmap)
		fields[fname] = info
	}

	return fields
}

// parseFieldTypes extracts type names from the types array.
func parseFieldTypes(raw any) []string {
	arr, ok := raw.([]any)
	if !ok {
		return nil
	}

	types := make([]string, 0, len(arr))

	for _, typeEntry := range arr {
		typeMap, mapOK := typeEntry.(map[string]any)
		if !mapOK {
			continue
		}

		typeName, typeOK := typeMap["type"].(string)
		if typeOK && typeName != "" {
			types = append(types, typeName)
		}
	}

	return types
}

// isFieldMultiple infers if a field can have multiple values.
func isFieldMultiple(fmap map[string]any) bool {
	// Heuristic: if types is an array with more than one entry, or if a 'multiple' flag exists.
	if arr, ok := fmap["types"].([]any); ok && len(arr) > 1 {
		return true
	}

	if mult, ok := fmap["multiple"].(bool); ok {
		return mult
	}

	return false
}

// parseChildren parses the children section of a node type entry.
func parseChildren(raw any) []ChildInfo {
	arr, ok := raw.([]any)
	if !ok {
		return nil
	}

	children := make([]ChildInfo, 0, len(arr))

	for _, childEntry := range arr {
		childMap, mapOK := childEntry.(map[string]any)
		if !mapOK {
			continue
		}

		typeName, typeNameOK := childMap["type"].(string)
		if !typeNameOK {
			typeName = ""
		}

		named, namedOK := childMap["named"].(bool)
		if !namedOK {
			named = false
		}

		children = append(children, ChildInfo{Type: typeName, Named: named})
	}

	return children
}

// classifyNodeCategory applies heuristic rules to classify a node type.
func classifyNodeCategory(nodeInfo NodeTypeInfo) NodeCategory {
	if len(nodeInfo.Children) == 0 && len(nodeInfo.Fields) == 0 {
		return Leaf
	}

	isOperatorNode := strings.Contains(nodeInfo.Name, "_operator") ||
		strings.Contains(nodeInfo.Name, "_op") ||
		strings.Contains(nodeInfo.Name, "operator") ||
		strings.Contains(nodeInfo.Name, "binary_expression") ||
		strings.Contains(nodeInfo.Name, "unary_expression")

	if isOperatorNode {
		return Operator
	}

	return Container
}

// writeLanguageDeclaration writes the language declaration section to the builder.
func writeLanguageDeclaration(sb *strings.Builder, language string, extensions []string) {
	if language == "" || len(extensions) == 0 {
		return
	}

	fmt.Fprintf(sb, "[language %q, extensions: ", language)

	for idx, ext := range extensions {
		if idx > 0 {
			sb.WriteString(", ")
		}

		fmt.Fprintf(sb, "%q", ext)
	}

	sb.WriteString("]\n\n")
}

// collectChildTypes collects and returns sorted child type names from a node's children and fields.
func collectChildTypes(nodeInfo NodeTypeInfo) []string {
	childTypes := make(map[string]struct{})

	for _, child := range nodeInfo.Children {
		if isValidIdentifier(child.Type) {
			childTypes[child.Type] = struct{}{}
		}
	}

	for _, field := range nodeInfo.Fields {
		for _, fieldType := range field.Types {
			if isValidIdentifier(fieldType) {
				childTypes[fieldType] = struct{}{}
			}
		}
	}

	if len(childTypes) == 0 {
		return nil
	}

	keys := make([]string, 0, len(childTypes))

	for key := range childTypes {
		keys = append(keys, key)
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

	for idx, role := range roles {
		if idx > 0 {
			sb.WriteString(", ")
		}

		fmt.Fprintf(sb, "%q", role)
	}
}

// writeChildrenSection writes the children section if children are present.
func writeChildrenSection(sb *strings.Builder, childTypes []string) {
	if len(childTypes) == 0 {
		return
	}

	sb.WriteString(",\n    children: ")

	for idx, childType := range childTypes {
		if idx > 0 {
			sb.WriteString(", ")
		}

		fmt.Fprintf(sb, "%q", childType)
	}
}

// writeNodeMapping writes a single node's mapping DSL to the builder.
func writeNodeMapping(sb *strings.Builder, nodeInfo NodeTypeInfo) {
	uastType, roles := guessUASTTypeAndRoles(nodeInfo.Name)
	isLeaf := len(nodeInfo.Children) == 0 && len(nodeInfo.Fields) == 0

	fmt.Fprintf(sb, "%s <- (%s) => uast(\n", nodeInfo.Name, nodeInfo.Name)
	fmt.Fprintf(sb, "    type: %q", uastType)

	if isLeaf {
		if token := guessTokenField(nodeInfo); token != "" {
			fmt.Fprintf(sb, ",\n    token: %q", token)
		}
	}

	writeRolesSection(sb, roles)
	writeChildrenSection(sb, collectChildTypes(nodeInfo))

	sb.WriteString("\n)\n\n")
}

// GenerateMappingDSL emits mapping DSL for a set of node types, using canonical UAST types/roles.
func GenerateMappingDSL(nodes []NodeTypeInfo, language string, extensions []string) string {
	var sb strings.Builder

	writeLanguageDeclaration(&sb, language, extensions)

	for _, nodeInfo := range nodes {
		if !isValidIdentifier(nodeInfo.Name) {
			continue
		}

		writeNodeMapping(&sb, nodeInfo)
	}

	return sb.String()
}
