package common //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestNewDataExtractor(t *testing.T) {
	t.Parallel()

	config := ExtractionConfig{
		DefaultExtractors: false,
	}

	extractor := NewDataExtractor(config)
	if extractor == nil {
		t.Fatal("NewDataExtractor returned nil")
	}
}

func TestNewDataExtractor_WithDefaults(t *testing.T) {
	t.Parallel()

	config := ExtractionConfig{
		DefaultExtractors: true,
	}

	extractor := NewDataExtractor(config)
	if extractor == nil {
		t.Fatal("NewDataExtractor returned nil")
	}
	// Verify default extractors are present.
	if len(extractor.config.NameExtractors) == 0 {
		t.Error("expected default name extractors")
	}

	if len(extractor.config.ValueExtractors) == 0 {
		t.Error("expected default value extractors")
	}
}

func TestNewDataExtractor_WithCustomExtractors(t *testing.T) {
	t.Parallel()

	customNameExtractor := func(_ *node.Node) (string, bool) {
		return "custom_name", true
	}
	customValueExtractor := func(_ *node.Node) (any, bool) {
		return "custom_value", true
	}

	config := ExtractionConfig{
		NameExtractors: map[string]NameExtractor{
			"custom": customNameExtractor,
		},
		ValueExtractors: map[string]ValueExtractor{
			"custom": customValueExtractor,
		},
		DefaultExtractors: true,
	}
	extractor := NewDataExtractor(config)

	// Verify custom extractor is merged with defaults.
	if _, exists := extractor.config.NameExtractors["custom"]; !exists {
		t.Error("expected custom name extractor")
	}

	if _, exists := extractor.config.NameExtractors["token"]; !exists {
		t.Error("expected default token extractor")
	}
}

func TestDataExtractor_ExtractName(t *testing.T) {
	t.Parallel()

	customExtractor := func(n *node.Node) (string, bool) {
		if n != nil && n.Token != "" {
			return n.Token, true
		}

		return "", false
	}

	config := ExtractionConfig{
		NameExtractors: map[string]NameExtractor{
			"custom": customExtractor,
		},
	}
	extractor := NewDataExtractor(config)

	testNode := &node.Node{Token: "test_token"}

	name, ok := extractor.ExtractName(testNode, "custom")
	if !ok || name != "test_token" {
		t.Errorf("expected 'test_token', got '%s', ok=%v", name, ok)
	}

	// Test non-existent extractor.
	name, ok = extractor.ExtractName(testNode, "nonexistent")
	if ok || name != "" {
		t.Error("expected empty result for non-existent extractor")
	}
}

func TestDataExtractor_ExtractValue(t *testing.T) {
	t.Parallel()

	customExtractor := func(n *node.Node) (any, bool) {
		if n != nil {
			return 42, true
		}

		return nil, false
	}

	config := ExtractionConfig{
		ValueExtractors: map[string]ValueExtractor{
			"custom": customExtractor,
		},
	}
	extractor := NewDataExtractor(config)

	testNode := &node.Node{}

	value, ok := extractor.ExtractValue(testNode, "custom")
	if !ok || value != 42 {
		t.Errorf("expected 42, got %v, ok=%v", value, ok)
	}

	// Test non-existent extractor.
	value, ok = extractor.ExtractValue(testNode, "nonexistent")
	if ok || value != nil {
		t.Error("expected nil result for non-existent extractor")
	}
}

func TestDataExtractor_ExtractNameFromProps(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	// Test with valid props.
	testNode := &node.Node{
		Props: map[string]string{
			"name": "test_name",
		},
	}

	name, ok := extractor.ExtractNameFromProps(testNode, "name")
	if !ok || name != "test_name" {
		t.Errorf("expected 'test_name', got '%s', ok=%v", name, ok)
	}

	// Test with missing key.
	name, ok = extractor.ExtractNameFromProps(testNode, "missing")
	if ok || name != "" {
		t.Error("expected empty result for missing key")
	}

	// Test with nil node.
	name, ok = extractor.ExtractNameFromProps(nil, "name")
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}

	// Test with nil props.
	emptyNode := &node.Node{}

	name, ok = extractor.ExtractNameFromProps(emptyNode, "name")
	if ok || name != "" {
		t.Error("expected empty result for nil props")
	}
}

func TestDataExtractor_ExtractNameFromToken(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	// Test with valid token.
	testNode := &node.Node{Token: "test_token"}

	name, ok := extractor.ExtractNameFromToken(testNode)
	if !ok || name != "test_token" {
		t.Errorf("expected 'test_token', got '%s', ok=%v", name, ok)
	}

	// Test with empty token.
	emptyNode := &node.Node{Token: ""}

	name, ok = extractor.ExtractNameFromToken(emptyNode)
	if ok || name != "" {
		t.Error("expected empty result for empty token")
	}

	// Test with nil node.
	name, ok = extractor.ExtractNameFromToken(nil)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}
}

func TestDataExtractor_ExtractNameFromChildren(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	// Test with child that has token.
	testNode := &node.Node{
		Children: []*node.Node{
			{Token: "child_token"},
		},
	}

	name, ok := extractor.ExtractNameFromChildren(testNode, 0)
	if !ok || name != "child_token" {
		t.Errorf("expected 'child_token', got '%s', ok=%v", name, ok)
	}

	// Test with child that has props.
	testNode2 := &node.Node{
		Children: []*node.Node{
			{Props: map[string]string{"name": "child_name"}},
		},
	}

	name, ok = extractor.ExtractNameFromChildren(testNode2, 0)
	if !ok || name != "child_name" {
		t.Errorf("expected 'child_name', got '%s', ok=%v", name, ok)
	}

	// Test with invalid index.
	name, ok = extractor.ExtractNameFromChildren(testNode, 10)
	if ok || name != "" {
		t.Error("expected empty result for invalid index")
	}

	// Test with nil child.
	testNode3 := &node.Node{
		Children: []*node.Node{nil},
	}

	name, ok = extractor.ExtractNameFromChildren(testNode3, 0)
	if ok || name != "" {
		t.Error("expected empty result for nil child")
	}

	// Test with nil node.
	name, ok = extractor.ExtractNameFromChildren(nil, 0)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}
}

func TestDataExtractor_ExtractNodeType(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	testNode := &node.Node{Type: "FunctionDeclaration"}

	nodeType, ok := extractor.ExtractNodeType(testNode)
	if !ok || nodeType != "FunctionDeclaration" {
		t.Errorf("expected 'FunctionDeclaration', got '%s', ok=%v", nodeType, ok)
	}

	// Test with nil node.
	nodeType, ok = extractor.ExtractNodeType(nil)
	if ok || nodeType != "" {
		t.Error("expected empty result for nil node")
	}
}

func TestDataExtractor_ExtractNodeRoles(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	testNode := &node.Node{
		Roles: []node.Role{"Function", "Declaration"},
	}

	roles, ok := extractor.ExtractNodeRoles(testNode)
	if !ok || len(roles) != 2 {
		t.Errorf("expected 2 roles, got %d, ok=%v", len(roles), ok)
	}

	// Test with empty roles.
	emptyNode := &node.Node{}

	roles, ok = extractor.ExtractNodeRoles(emptyNode)
	if ok || roles != nil {
		t.Error("expected nil result for empty roles")
	}

	// Test with nil node.
	roles, ok = extractor.ExtractNodeRoles(nil)
	if ok || roles != nil {
		t.Error("expected nil result for nil node")
	}
}

func TestDataExtractor_ExtractNodePosition(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	testNode := &node.Node{
		Pos: &node.Positions{
			StartLine:   10,
			EndLine:     20,
			StartCol:    5,
			EndCol:      25,
			StartOffset: 100,
			EndOffset:   200,
		},
	}

	pos, ok := extractor.ExtractNodePosition(testNode)
	if !ok || pos == nil {
		t.Fatal("expected position data")
	}

	startLine, ok := pos["start_line"].(uint)
	require.True(t, ok, "type assertion failed for start_line")

	if startLine != 10 {
		t.Errorf("expected start_line 10, got %v", startLine)
	}

	endLine, ok := pos["end_line"].(uint)
	require.True(t, ok, "type assertion failed for end_line")

	if endLine != 20 {
		t.Errorf("expected end_line 20, got %v", endLine)
	}

	// Test with nil pos.
	emptyNode := &node.Node{}

	pos, ok = extractor.ExtractNodePosition(emptyNode)
	if ok || pos != nil {
		t.Error("expected nil result for nil pos")
	}

	// Test with nil node.
	pos, ok = extractor.ExtractNodePosition(nil)
	if ok || pos != nil {
		t.Error("expected nil result for nil node")
	}
}

func TestDataExtractor_ExtractNodeProperties(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	testNode := &node.Node{
		Props: map[string]string{
			"key1": "value1",
			"key2": "value2",
		},
	}

	props, ok := extractor.ExtractNodeProperties(testNode)
	if !ok || len(props) != 2 {
		t.Errorf("expected 2 properties, got %d, ok=%v", len(props), ok)
	}

	if props["key1"] != "value1" {
		t.Errorf("expected 'value1', got '%s'", props["key1"])
	}

	// Test with nil props.
	emptyNode := &node.Node{}

	props, ok = extractor.ExtractNodeProperties(emptyNode)
	if ok || props != nil {
		t.Error("expected nil result for nil props")
	}

	// Test with nil node.
	props, ok = extractor.ExtractNodeProperties(nil)
	if ok || props != nil {
		t.Error("expected nil result for nil node")
	}
}

func TestDataExtractor_ExtractChildCount(t *testing.T) {
	t.Parallel()

	extractor := NewDataExtractor(ExtractionConfig{})

	testNode := &node.Node{
		Children: []*node.Node{{}, {}, {}},
	}

	count, ok := extractor.ExtractChildCount(testNode)
	if !ok || count != 3 {
		t.Errorf("expected 3, got %d, ok=%v", count, ok)
	}

	// Test with nil node.
	count, ok = extractor.ExtractChildCount(nil)
	if ok || count != 0 {
		t.Error("expected 0 for nil node")
	}
}

// Test standalone extraction functions.

func TestExtractFunctionName(t *testing.T) {
	t.Parallel()

	// Test from props.
	testNode := &node.Node{
		Props: map[string]string{"name": "myFunction"},
	}

	name, ok := ExtractFunctionName(testNode)
	if !ok || name != "myFunction" {
		t.Errorf("expected 'myFunction', got '%s', ok=%v", name, ok)
	}

	// Test from token.
	testNode2 := &node.Node{Token: "funcToken"}

	name, ok = ExtractFunctionName(testNode2)
	if !ok || name != "funcToken" {
		t.Errorf("expected 'funcToken', got '%s', ok=%v", name, ok)
	}

	// Test from children.
	testNode3 := &node.Node{
		Children: []*node.Node{{Token: "childName"}},
	}

	name, ok = ExtractFunctionName(testNode3)
	if !ok || name != "childName" {
		t.Errorf("expected 'childName', got '%s', ok=%v", name, ok)
	}

	// Test nil node.
	name, ok = ExtractFunctionName(nil)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}
}

func TestExtractVariableName(t *testing.T) {
	t.Parallel()

	// Test from props.
	testNode := &node.Node{
		Props: map[string]string{"name": "myVar"},
	}

	name, ok := ExtractVariableName(testNode)
	if !ok || name != "myVar" {
		t.Errorf("expected 'myVar', got '%s', ok=%v", name, ok)
	}

	// Test from token.
	testNode2 := &node.Node{Token: "varToken"}

	name, ok = ExtractVariableName(testNode2)
	if !ok || name != "varToken" {
		t.Errorf("expected 'varToken', got '%s', ok=%v", name, ok)
	}

	// Test nil node.
	name, ok = ExtractVariableName(nil)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}
}

func TestStandaloneExtractNameFromProps(t *testing.T) {
	t.Parallel()

	testNode := &node.Node{
		Props: map[string]string{"name": "propName"},
	}

	name, ok := ExtractNameFromProps(testNode, "name")
	if !ok || name != "propName" {
		t.Errorf("expected 'propName', got '%s', ok=%v", name, ok)
	}

	// Test nil node.
	name, ok = ExtractNameFromProps(nil, "name")
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}

	// Test nil props.
	emptyNode := &node.Node{}

	name, ok = ExtractNameFromProps(emptyNode, "name")
	if ok || name != "" {
		t.Error("expected empty result for nil props")
	}
}

func TestStandaloneExtractNameFromToken(t *testing.T) {
	t.Parallel()

	testNode := &node.Node{Token: "tokenName"}

	name, ok := ExtractNameFromToken(testNode)
	if !ok || name != "tokenName" {
		t.Errorf("expected 'tokenName', got '%s', ok=%v", name, ok)
	}

	// Test nil node.
	name, ok = ExtractNameFromToken(nil)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}

	// Test empty token.
	emptyNode := &node.Node{Token: ""}

	name, ok = ExtractNameFromToken(emptyNode)
	if ok || name != "" {
		t.Error("expected empty result for empty token")
	}
}

func TestStandaloneExtractNameFromChildren(t *testing.T) {
	t.Parallel()

	// Test child with token.
	testNode := &node.Node{
		Children: []*node.Node{{Token: "childToken"}},
	}

	name, ok := ExtractNameFromChildren(testNode, 0)
	if !ok || name != "childToken" {
		t.Errorf("expected 'childToken', got '%s', ok=%v", name, ok)
	}

	// Test child with props.
	testNode2 := &node.Node{
		Children: []*node.Node{{Props: map[string]string{"name": "childPropName"}}},
	}

	name, ok = ExtractNameFromChildren(testNode2, 0)
	if !ok || name != "childPropName" {
		t.Errorf("expected 'childPropName', got '%s', ok=%v", name, ok)
	}

	// Test nil node.
	name, ok = ExtractNameFromChildren(nil, 0)
	if ok || name != "" {
		t.Error("expected empty result for nil node")
	}

	// Test invalid index.
	name, ok = ExtractNameFromChildren(testNode, 10)
	if ok || name != "" {
		t.Error("expected empty result for invalid index")
	}

	// Test nil child.
	testNode3 := &node.Node{Children: []*node.Node{nil}}

	name, ok = ExtractNameFromChildren(testNode3, 0)
	if ok || name != "" {
		t.Error("expected empty result for nil child")
	}

	// Test child without name.
	testNode4 := &node.Node{Children: []*node.Node{{}}}

	name, ok = ExtractNameFromChildren(testNode4, 0)
	if ok || name != "" {
		t.Error("expected empty result for child without name")
	}
}

func TestMergeNameExtractors(t *testing.T) {
	t.Parallel()

	defaults := map[string]NameExtractor{
		"default1": func(_ *node.Node) (string, bool) { return "default", true },
	}
	custom := map[string]NameExtractor{
		"custom1": func(_ *node.Node) (string, bool) { return "custom", true },
	}

	result := mergeNameExtractors(custom, defaults)
	if len(result) != 2 {
		t.Errorf("expected 2 extractors, got %d", len(result))
	}

	if _, exists := result["default1"]; !exists {
		t.Error("expected default1 extractor")
	}

	if _, exists := result["custom1"]; !exists {
		t.Error("expected custom1 extractor")
	}

	// Test with nil custom.
	result = mergeNameExtractors(nil, defaults)
	if len(result) != 1 {
		t.Errorf("expected 1 extractor from defaults, got %d", len(result))
	}
}

func TestMergeValueExtractors(t *testing.T) {
	t.Parallel()

	defaults := map[string]ValueExtractor{
		"default1": func(_ *node.Node) (any, bool) { return "default", true },
	}
	custom := map[string]ValueExtractor{
		"custom1": func(_ *node.Node) (any, bool) { return "custom", true },
	}

	result := mergeValueExtractors(custom, defaults)
	if len(result) != 2 {
		t.Errorf("expected 2 extractors, got %d", len(result))
	}

	// Test with nil custom.
	result = mergeValueExtractors(nil, defaults)
	if len(result) != 1 {
		t.Errorf("expected 1 extractor from defaults, got %d", len(result))
	}
}

func TestDefaultNameExtractors(t *testing.T) {
	t.Parallel()

	extractors := getDefaultNameExtractors()
	if len(extractors) != 3 {
		t.Errorf("expected 3 default name extractors, got %d", len(extractors))
	}

	// Test token extractor.
	tokenExtractor := extractors["token"]
	testNode := &node.Node{Token: "test"}

	name, ok := tokenExtractor(testNode)
	if !ok || name != "test" {
		t.Error("token extractor failed")
	}

	_, ok = tokenExtractor(nil)
	if ok {
		t.Error("token extractor should return false for nil")
	}

	_, ok = tokenExtractor(&node.Node{Token: ""})
	if ok {
		t.Error("token extractor should return false for empty token")
	}

	// Test props_name extractor.
	propsNameExtractor := extractors["props_name"]
	testNode2 := &node.Node{Props: map[string]string{"name": "propName"}}

	name, ok = propsNameExtractor(testNode2)
	if !ok || name != "propName" {
		t.Error("props_name extractor failed")
	}

	_, ok = propsNameExtractor(nil)
	if ok {
		t.Error("props_name extractor should return false for nil")
	}

	_, ok = propsNameExtractor(&node.Node{})
	if ok {
		t.Error("props_name extractor should return false for nil props")
	}

	// Test props_id extractor.
	propsIDExtractor := extractors["props_id"]
	testNode3 := &node.Node{Props: map[string]string{"id": "propID"}}

	name, ok = propsIDExtractor(testNode3)
	if !ok || name != "propID" {
		t.Error("props_id extractor failed")
	}

	_, ok = propsIDExtractor(nil)
	if ok {
		t.Error("props_id extractor should return false for nil")
	}
}

func TestDefaultValueExtractors(t *testing.T) { //nolint:cyclop,gocognit,gocyclo // cyclomatic complexity is acceptable for this function.
	t.Parallel()

	extractors := getDefaultValueExtractors()
	if len(extractors) != 5 {
		t.Errorf("expected 5 default value extractors, got %d", len(extractors))
	}

	// Test type extractor.
	typeExtractor := extractors["type"]
	testNode := &node.Node{Type: "FunctionDeclaration"}

	value, ok := typeExtractor(testNode)
	if !ok || value != "FunctionDeclaration" {
		t.Error("type extractor failed")
	}

	_, ok = typeExtractor(nil)
	if ok {
		t.Error("type extractor should return false for nil")
	}

	// Test roles extractor.
	rolesExtractor := extractors["roles"]
	testNode2 := &node.Node{Roles: []node.Role{"Function"}}

	value, ok = rolesExtractor(testNode2)
	if !ok {
		t.Error("roles extractor failed")
	}

	roles, ok := value.([]string)
	require.True(t, ok, "type assertion failed for roles")

	if len(roles) != 1 || roles[0] != "Function" {
		t.Error("roles extractor returned wrong value")
	}

	_, ok = rolesExtractor(nil)
	if ok {
		t.Error("roles extractor should return false for nil")
	}

	_, ok = rolesExtractor(&node.Node{})
	if ok {
		t.Error("roles extractor should return false for empty roles")
	}

	// Test position extractor.
	posExtractor := extractors["position"]
	testNode3 := &node.Node{Pos: &node.Positions{StartLine: 10, EndLine: 20}}

	_, ok = posExtractor(testNode3)
	if !ok {
		t.Error("position extractor failed")
	}

	_, ok = posExtractor(nil)
	if ok {
		t.Error("position extractor should return false for nil")
	}

	_, ok = posExtractor(&node.Node{})
	if ok {
		t.Error("position extractor should return false for nil pos")
	}

	// Test properties extractor.
	propsExtractor := extractors["properties"]
	testNode4 := &node.Node{Props: map[string]string{"key": "value"}}

	_, ok = propsExtractor(testNode4)
	if !ok {
		t.Error("properties extractor failed")
	}

	_, ok = propsExtractor(nil)
	if ok {
		t.Error("properties extractor should return false for nil")
	}

	_, ok = propsExtractor(&node.Node{})
	if ok {
		t.Error("properties extractor should return false for nil props")
	}

	// Test child_count extractor.
	childCountExtractor := extractors["child_count"]
	testNode5 := &node.Node{Children: []*node.Node{{}, {}}}

	value, ok = childCountExtractor(testNode5)
	if !ok || value != 2 {
		t.Error("child_count extractor failed")
	}

	_, ok = childCountExtractor(nil)
	if ok {
		t.Error("child_count extractor should return false for nil")
	}
}
