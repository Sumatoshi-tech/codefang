package uast

import (
	"context"
	"errors"
	"fmt"
	"io"
	"maps"
	"strings"

	forest "github.com/alexaandru/go-sitter-forest"
	sitter "github.com/alexaandru/go-tree-sitter-bare"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// DSLParser implements the UAST LanguageParser interface using DSL-based mappings.
type DSLParser struct {
	language        *sitter.Language
	mappingRules    []mapping.MappingRule
	patternMatcher  *mapping.PatternMatcher
	IncludeUnmapped bool
	reader          io.Reader
	langInfo        *mapping.LanguageInfo
	originalDSL     string // Store the original DSL content
}

// NewDSLParser creates a new DSL-based parser with the given language and mapping rules.
func NewDSLParser(reader io.Reader) *DSLParser {
	return &DSLParser{
		reader: reader,
	}
}

func (p *DSLParser) Load() error {
	// Read the content first
	content, err := io.ReadAll(p.reader)
	if err != nil {
		return err
	}

	// Store the original DSL content
	p.originalDSL = string(content)

	// Parse the mapping
	rules, langInfo, err := (&mapping.MappingParser{}).ParseMapping(strings.NewReader(p.originalDSL))

	if err != nil {
		return err
	}

	p.mappingRules = rules
	p.langInfo = langInfo

	return p.initializeLanguage()
}

// initializeLanguage initializes the tree-sitter language and pattern matcher
func (p *DSLParser) initializeLanguage() error {
	// Get the tree-sitter language with panic recovery
	var lang *sitter.Language
	func() {
		defer func() {
			if r := recover(); r != nil { //nolint:staticcheck // intentionally empty - panic recovery sets lang to nil
			}
		}()
		lang = forest.GetLanguage(p.langInfo.Name)
	}()

	if lang == nil {
		return fmt.Errorf("tree-sitter language not available for: %s", p.langInfo.Name)
	}

	p.language = lang
	p.patternMatcher = mapping.NewPatternMatcher(p.language)

	return nil
}

func (p *DSLParser) Extensions() []string {
	return p.langInfo.Extensions
}

// Parse parses the given file content and returns the root UAST node.
func (p *DSLParser) Parse(filename string, content []byte) (*node.Node, error) {
	parser := sitter.NewParser()
	parser.SetLanguage(p.language)
	tree, err := parser.ParseString(context.Background(), nil, content)
	if err != nil {
		return nil, fmt.Errorf("dsl parser: failed to parse: %w", err)
	}
	root := tree.RootNode()
	if root.IsNull() {
		return nil, errors.New("dsl parser: no root node")
	}

	dslNode := p.createDSLNode(root, tree, content)
	canonical := dslNode.ToCanonicalNode()
	if canonical == nil {
		return nil, nil
	}
	return canonical, nil
}

// Language returns the language name for this parser.
func (p *DSLParser) Language() string {
	return p.langInfo.Name
}

// GetOriginalDSL returns the original DSL content that was used to create this parser
func (p *DSLParser) GetOriginalDSL() string {
	return p.originalDSL
}

// DSLNode wraps a Tree-sitter node for conversion to UAST using DSL mappings.
type DSLNode struct {
	Root            sitter.Node
	Tree            *sitter.Tree
	Language        string
	Source          []byte
	MappingRules    []mapping.MappingRule
	PatternMatcher  *mapping.PatternMatcher
	IncludeUnmapped bool
	ParentContext   string
}

// ToCanonicalNode converts the DSLNode to a canonical UAST Node.
func (dn *DSLNode) ToCanonicalNode() *node.Node {
	nodeType := dn.Root.Type()
	mappingRule := dn.findMappingRule(nodeType)

	children := dn.processChildren(mappingRule)
	if dn.shouldSkipNode(mappingRule) {
		return nil
	}

	if dn.shouldSkipEmptyFile(nodeType, mappingRule, children) {
		return nil
	}

	props := map[string]string{}
	var roles []node.Role

	if mappingRule != nil {
		return dn.createMappedNode(mappingRule, children, props, roles)
	}

	return dn.createUnmappedNode(nodeType, props, roles)
}

// createDSLNode creates a new DSLNode with the given parameters
func (p *DSLParser) createDSLNode(root sitter.Node, tree *sitter.Tree, content []byte) *DSLNode {
	return &DSLNode{
		Root:            root,
		Tree:            tree,
		Language:        p.langInfo.Name,
		Source:          content,
		MappingRules:    p.mappingRules,
		PatternMatcher:  p.patternMatcher,
		IncludeUnmapped: p.IncludeUnmapped,
	}
}

// findMappingRule finds a mapping rule for the given node type, resolving inheritance and merging fields
func (dn *DSLNode) findMappingRule(nodeType string) *mapping.MappingRule {
	var rule *mapping.MappingRule
	for i := range dn.MappingRules {
		if dn.MappingRules[i].Name == nodeType {
			rule = &dn.MappingRules[i]
			break
		}
	}
	if rule == nil {
		return nil
	}
	return dn.resolveInheritance(rule)
}

// resolveInheritance recursively merges base rule fields if Extends is set
func (dn *DSLNode) resolveInheritance(rule *mapping.MappingRule) *mapping.MappingRule {
	if rule.Extends == "" {
		return rule
	}
	var base *mapping.MappingRule
	for i := range dn.MappingRules {
		if dn.MappingRules[i].Name == rule.Extends {
			base = &dn.MappingRules[i]
			break
		}
	}
	if base == nil {
		return rule
	}
	merged := *base // shallow copy
	// Merge/override with child rule fields
	if rule.Pattern != "" {
		merged.Pattern = rule.Pattern
	}
	if rule.UASTSpec.Type != "" {
		merged.UASTSpec.Type = rule.UASTSpec.Type
	}
	if rule.UASTSpec.Token != "" {
		merged.UASTSpec.Token = rule.UASTSpec.Token
	}
	if len(rule.UASTSpec.Roles) > 0 {
		merged.UASTSpec.Roles = rule.UASTSpec.Roles
	}
	if len(rule.UASTSpec.Props) > 0 {
		if merged.UASTSpec.Props == nil {
			merged.UASTSpec.Props = map[string]string{}
		}
		maps.Copy(merged.UASTSpec.Props, rule.UASTSpec.Props)
	}
	if len(rule.UASTSpec.Children) > 0 {
		merged.UASTSpec.Children = rule.UASTSpec.Children
	}
	if len(rule.Conditions) > 0 {
		merged.Conditions = append(merged.Conditions, rule.Conditions...)
	}
	// Recursively resolve further inheritance
	return dn.resolveInheritance(&merged)
}

// processChildren processes all children of the node
func (dn *DSLNode) processChildren(mappingRule *mapping.MappingRule) []*node.Node {
	childCount := dn.Root.NamedChildCount()
	children := make([]*node.Node, 0, childCount)

	for i := range childCount {
		child := dn.Root.NamedChild(i)
		childNode := dn.createChildNode(child, mappingRule)

		if dn.shouldExcludeChild(childNode, mappingRule) {
			continue
		}

		c := childNode.ToCanonicalNode()
		if c != nil && dn.shouldIncludeChild(child) {
			children = append(children, c)
		}
	}

	return children
}

// createChildNode creates a child DSLNode with proper parent context
func (dn *DSLNode) createChildNode(child sitter.Node, mappingRule *mapping.MappingRule) *DSLNode {
	parentContext := ""
	if mappingRule != nil {
		parentContext = mappingRule.UASTSpec.Type
	}
	if parentContext == "" {
		parentContext = dn.Root.Type()
	}
	return &DSLNode{
		Root:            child,
		Tree:            dn.Tree,
		Language:        dn.Language,
		Source:          dn.Source,
		MappingRules:    dn.MappingRules,
		PatternMatcher:  dn.PatternMatcher,
		IncludeUnmapped: dn.IncludeUnmapped,
		ParentContext:   parentContext,
	}
}

// --- Pattern Matching and Capture Extraction ---

// matchPattern returns the capture map for the current node and mapping rule, or nil if no match
func (dn *DSLNode) matchPattern(mappingRule *mapping.MappingRule) map[string]string {
	if mappingRule == nil || mappingRule.Pattern == "" {
		return nil
	}
	query, err := dn.PatternMatcher.CompileAndCache(mappingRule.Pattern)
	if err != nil {
		return nil
	}
	captures, err := dn.PatternMatcher.MatchPattern(query, &dn.Root, dn.Source)
	if err != nil {
		return nil
	}
	return captures
}

// extractCaptureText extracts text for a named capture using the pattern matcher and mapping rule pattern
func (dn *DSLNode) extractCaptureText(captureName string) string {
	mappingRule := dn.findMappingRule(dn.Root.Type())
	captures := dn.matchPattern(mappingRule)
	if captures != nil {
		if val, ok := captures[captureName]; ok {
			return val
		}
	}
	// Fallback: try to extract by field name
	fieldNode := dn.Root.ChildByFieldName(captureName)
	if !fieldNode.IsNull() {
		if fieldNode.ChildCount() == 0 {
			return dn.extractNodeText(fieldNode)
		}
		// Not a leaf: return node type as placeholder
		return fieldNode.Type()
	}
	// Recursively search for a descendant node of the given type
	desc := findDescendantByType(dn.Root, captureName)
	if !desc.IsNull() {
		return dn.extractNodeText(desc)
	}
	return ""
}

func findDescendantByType(node sitter.Node, typ string) sitter.Node {
	if node.Type() == typ {
		return node
	}
	for i := uint32(0); i < node.NamedChildCount(); i++ {
		child := node.NamedChild(i)
		found := findDescendantByType(child, typ)
		if !found.IsNull() {
			return found
		}
	}
	return sitter.Node{}
}

// --- Condition Evaluation ---

// evaluateConditions returns true if all conditions are satisfied for the current node
func (dn *DSLNode) evaluateConditions(mappingRule *mapping.MappingRule) bool {
	if mappingRule == nil || len(mappingRule.Conditions) == 0 {
		return true
	}
	captures := dn.matchPattern(mappingRule)
	for _, cond := range mappingRule.Conditions {
		if !dn.evaluateCondition(cond.Expr, captures) {
			return false
		}
	}
	return true
}

// evaluateCondition evaluates a single condition expression (very basic implementation)
func (dn *DSLNode) evaluateCondition(expr string, captures map[string]string) bool {
	// Only support simple equality: field == "value" or field != "value"
	expr = strings.TrimSpace(expr)
	if strings.Contains(expr, "==") {
		return dn.evaluateComparisonOp(expr, "==", captures, func(a, b string) bool { return a == b })
	}
	if strings.Contains(expr, "!=") {
		return dn.evaluateComparisonOp(expr, "!=", captures, func(a, b string) bool { return a != b })
	}
	return false
}

// evaluateComparisonOp is a helper that evaluates comparison expressions with the given operator and comparator.
func (dn *DSLNode) evaluateComparisonOp(expr, op string, captures map[string]string, compare func(a, b string) bool) bool {
	parts := strings.SplitN(expr, op, 2)
	if len(parts) != 2 {
		return false
	}
	field := strings.TrimSpace(parts[0])
	val := strings.Trim(strings.TrimSpace(parts[1]), "\"")

	// Check captures first
	if v, ok := captures[field]; ok {
		return compare(v, val)
	}

	// Check field names in the AST
	if fieldNode := dn.Root.ChildByFieldName(field); !fieldNode.IsNull() {
		fieldText := fieldNode.Content(dn.Source)
		return compare(fieldText, val)
	}

	// Check if field is a child type
	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		child := dn.Root.NamedChild(i)
		if child.Type() == field {
			childText := child.Content(dn.Source)
			return compare(childText, val)
		}
	}

	return false
}

// --- Node/Child Inclusion/Exclusion ---

// shouldSkipNode checks if the current node should be skipped based on mapping rule conditions
func (dn *DSLNode) shouldSkipNode(mappingRule *mapping.MappingRule) bool {
	if mappingRule == nil {
		return false
	}
	return !dn.evaluateConditions(mappingRule)
}

// shouldExcludeChild checks if a child should be excluded based on mapping rules and conditions
func (dn *DSLNode) shouldExcludeChild(childNode *DSLNode, mappingRule *mapping.MappingRule) bool {
	if mappingRule == nil {
		return false
	}
	childRule := childNode.findMappingRule(childNode.Root.Type())
	if childRule == nil {
		return false
	}
	return !childNode.evaluateConditions(childRule)
}

// shouldIncludeChild checks if a child should be included in the result
func (dn *DSLNode) shouldIncludeChild(child sitter.Node) bool {
	childMappingRule := dn.findMappingRule(child.Type())
	if childMappingRule == nil {
		return true // Include unmapped children
	}
	childNode := dn.createChildNode(child, childMappingRule)
	return childNode.evaluateConditions(childMappingRule)
}

// shouldSkipEmptyFile checks if an empty file should be skipped
func (dn *DSLNode) shouldSkipEmptyFile(nodeType string, _ *mapping.MappingRule, children []*node.Node) bool {
	return nodeType == "source_file" && len(children) == 0 && len(dn.Source) == 0
}

// createMappedNode creates a UAST node from a mapped Tree-sitter node
func (dn *DSLNode) createMappedNode(mappingRule *mapping.MappingRule, children []*node.Node, props map[string]string, roles []node.Role) *node.Node {
	dn.extractRoles(mappingRule, &roles)
	dn.extractProperties(mappingRule, props)
	dn.extractName(mappingRule, props)

	uastNode := node.New("", node.Type(mappingRule.UASTSpec.Type), dn.extractTokenText(mappingRule), roles, dn.extractPositions(), props)
	uastNode.Children = children

	dn.extractToken(mappingRule, uastNode)
	return uastNode
}

// extractRoles extracts roles from the mapping rule
func (dn *DSLNode) extractRoles(mappingRule *mapping.MappingRule, roles *[]node.Role) {
	if mappingRule == nil {
		return
	}
	for _, roleStr := range mappingRule.UASTSpec.Roles {
		*roles = append(*roles, node.Role(roleStr))
	}
}

// extractName extracts name from the node if specified in mapping
func (dn *DSLNode) extractName(mappingRule *mapping.MappingRule, props map[string]string) {
	// For now, extract name from the first identifier child if available
	if mappingRule == nil {
		return
	}
	name := dn.extractNameFromNode("fields.name")
	if name != "" {
		props["name"] = name
	}
}

// extractNameFromNode extracts a name from a node using the specified source
func (dn *DSLNode) extractNameFromNode(source string) string {
	switch source {
	case "fields.name":
		return dn.extractNameFromField("name")
	case "props.name":
		return dn.extractNameFromProps()
	case "text":
		return dn.extractNameFromText()
	default:
		return ""
	}
}

// extractNameFromField extracts a name from a specific field using Tree-sitter's field API
func (dn *DSLNode) extractNameFromField(fieldName string) string {
	fieldNode := dn.Root.ChildByFieldName(fieldName)
	if !fieldNode.IsNull() {
		return dn.extractNodeText(fieldNode)
	}
	return dn.extractNameFromChildType(fieldName)
}

// extractNameFromChildType extracts name from a child with the field name as its type
func (dn *DSLNode) extractNameFromChildType(fieldName string) string {
	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		child := dn.Root.NamedChild(i)
		if child.Type() == fieldName {
			return dn.extractNodeText(child)
		}
	}
	return ""
}

// extractNameFromText extracts name from node text
func (dn *DSLNode) extractNameFromText() string {
	if dn.Root.ChildCount() == 0 {
		return dn.extractNodeText(dn.Root)
	}
	return ""
}

// extractNameFromProps extracts name from node properties (legacy)
func (dn *DSLNode) extractNameFromProps() string {
	return dn.extractNameFromText()
}

// extractProperties extracts properties from the mapping rule
func (dn *DSLNode) extractProperties(mappingRule *mapping.MappingRule, props map[string]string) {
	if mappingRule == nil || mappingRule.UASTSpec.Props == nil {
		return
	}
	for key, value := range mappingRule.UASTSpec.Props {
		extractedValue := dn.extractPropertyValue(value)
		if extractedValue != "" {
			props[key] = extractedValue
		}
	}
}

// extractPropertyValue extracts a property value from the node
func (dn *DSLNode) extractPropertyValue(propStr string) string {
	if strings.HasPrefix(propStr, "@") && len(propStr) > 1 {
		// Property references a capture
		return dn.extractCaptureText(propStr[1:])
	}
	if dn.isDescendantProperty(propStr) {
		return dn.extractDescendantProperty(propStr)
	}
	return dn.extractDirectChildProperty(propStr)
}

// isDescendantProperty checks if the property is a descendant property
func (dn *DSLNode) isDescendantProperty(propStr string) bool {
	_, ok := strings.CutPrefix(propStr, "descendant:")
	return ok
}

// extractDescendantProperty extracts a descendant property
func (dn *DSLNode) extractDescendantProperty(propStr string) string {
	after, _ := strings.CutPrefix(propStr, "descendant:")
	return dn.extractTokenFromDescendant(after)
}

// extractDirectChildProperty extracts a direct child property
func (dn *DSLNode) extractDirectChildProperty(propStr string) string {
	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		c := dn.Root.NamedChild(i)
		childKind := c.Type()
		if childKind == propStr {
			return dn.extractChildText(c)
		}
	}
	return ""
}

// extractChildText extracts text from a child node
func (dn *DSLNode) extractChildText(child sitter.Node) string {
	start := child.StartByte()
	end := child.EndByte()
	if safeconv.MustUintToInt(end) <= len(dn.Source) {
		return string(dn.Source[start:end])
	}
	return ""
}

// extractToken extracts token from the mapping rule
func (dn *DSLNode) extractToken(mappingRule *mapping.MappingRule, uastNode *node.Node) {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return
	}
	token := dn.extractTokenFromNode(mappingRule.UASTSpec.Token)
	if token != "" {
		uastNode.Token = token
	}
}

// extractTokenFromNode extracts a token from a node using the specified source
func (dn *DSLNode) extractTokenFromNode(source string) string {
	switch source {
	case "fields.name":
		return dn.extractNameFromField("name")
	case "props.name":
		return dn.extractNameFromProps()
	case "text":
		return dn.extractNameFromText()
	default:
		// Try to extract as a field name
		if strings.HasPrefix(source, "fields.") {
			fieldName := strings.TrimPrefix(source, "fields.")
			return dn.extractNameFromField(fieldName)
		}
		return ""
	}
}

// extractTokenFromChildType finds the first child of the specified type and extracts its token
func (dn *DSLNode) extractTokenFromChildType(nodeType string) string {
	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		child := dn.Root.NamedChild(i)
		if child.Type() == nodeType {
			return dn.extractNodeText(child)
		}
	}
	return ""
}

// extractTokenFromDescendant finds the first descendant of the specified type and extracts its token
func (dn *DSLNode) extractTokenFromDescendant(nodeType string) string {
	return dn.findDescendantToken(nodeType)
}

// findDescendantToken recursively searches for a descendant of the specified type
func (dn *DSLNode) findDescendantToken(nodeType string) string {
	if dn.Root.Type() == nodeType {
		return dn.extractNodeText(dn.Root)
	}

	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		child := dn.Root.NamedChild(i)
		childNode := &DSLNode{
			Root:            child,
			Tree:            dn.Tree,
			Language:        dn.Language,
			Source:          dn.Source,
			MappingRules:    dn.MappingRules,
			PatternMatcher:  dn.PatternMatcher,
			IncludeUnmapped: dn.IncludeUnmapped,
			ParentContext:   dn.ParentContext,
		}
		if result := childNode.findDescendantToken(nodeType); result != "" {
			return result
		}
	}

	return ""
}

// extractTokenText extracts the token text based on the mapping rule
func (dn *DSLNode) extractTokenText(mappingRule *mapping.MappingRule) string {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return ""
	}

	tokenSpec := mappingRule.UASTSpec.Token
	if strings.HasPrefix(tokenSpec, "@") {
		// Extract from capture
		captureName := strings.TrimPrefix(tokenSpec, "@")
		return dn.extractCaptureText(captureName)
	}

	switch tokenSpec {
	case "self":
		return dn.extractNodeText(dn.Root)
	case "text":
		return dn.extractNodeText(dn.Root)
	default:

		if after, ok := strings.CutPrefix(tokenSpec, "child:"); ok {
			return dn.extractTokenFromChildType(after)
		}

		if after, ok := strings.CutPrefix(tokenSpec, "descendant:"); ok {
			return dn.extractTokenFromDescendant(after)
		}

		return tokenSpec
	}
}

// extractPositions extracts position information from the Tree-sitter node
func (dn *DSLNode) extractPositions() *node.Positions {
	return &node.Positions{
		StartLine:   dn.Root.StartPoint().Row + 1,
		StartCol:    dn.Root.StartPoint().Column + 1,
		StartOffset: dn.Root.StartByte(),
		EndLine:     dn.Root.EndPoint().Row + 1,
		EndCol:      dn.Root.EndPoint().Column + 1,
		EndOffset:   dn.Root.EndByte(),
	}
}

// createUnmappedNode creates a UAST node for unmapped Tree-sitter nodes
func (dn *DSLNode) createUnmappedNode(nodeType string, props map[string]string, roles []node.Role) *node.Node {
	mappedChildren := dn.processUnmappedChildren()

	if dn.IncludeUnmapped {
		return dn.createIncludeUnmappedNode(nodeType, mappedChildren, props, roles)
	}

	return dn.createSyntheticNode(mappedChildren)
}

// processUnmappedChildren processes children for unmapped nodes
func (dn *DSLNode) processUnmappedChildren() []*node.Node {
	var mappedChildren []*node.Node
	for i := uint32(0); i < dn.Root.NamedChildCount(); i++ {
		child := dn.Root.NamedChild(i)
		childNode := dn.createUnmappedChildNode(child)
		c := childNode.ToCanonicalNode()
		if c != nil {
			mappedChildren = append(mappedChildren, c)
		}
	}
	return mappedChildren
}

// createUnmappedChildNode creates a child node for unmapped nodes
func (dn *DSLNode) createUnmappedChildNode(child sitter.Node) *DSLNode {
	return &DSLNode{
		Root:            child,
		Tree:            dn.Tree,
		Language:        dn.Language,
		Source:          dn.Source,
		MappingRules:    dn.MappingRules,
		PatternMatcher:  dn.PatternMatcher,
		IncludeUnmapped: dn.IncludeUnmapped,
		ParentContext:   dn.ParentContext,
	}
}

// createIncludeUnmappedNode creates a node when IncludeUnmapped is true
func (dn *DSLNode) createIncludeUnmappedNode(nodeType string, mappedChildren []*node.Node, props map[string]string, roles []node.Role) *node.Node {
	node := node.New("", node.Type(dn.Language+":"+nodeType), dn.Token(), roles, dn.Positions(), props)
	node.Children = mappedChildren
	return node
}

// createSyntheticNode creates a synthetic node for multiple children.
// All position fields use uint. Children without positions are skipped
// for span computation.
// If only one child is passed, it is returned as-is.
// If no position can be computed, pos is left nil.
func (dn *DSLNode) createSyntheticNode(mappedChildren []*node.Node) *node.Node {
	if len(mappedChildren) == 1 {
		return mappedChildren[0]
	}
	if len(mappedChildren) > 1 {
		// Initial values: max uint for "min", 0 for "max"
		const um = ^uint(0) // max value for uint
		minStartLine, minStartCol, minStartOffset := um, um, um
		maxEndLine, maxEndCol, maxEndOffset := uint(0), uint(0), uint(0)
		found := false

		for _, c := range mappedChildren {
			if c.Pos == nil {
				continue
			}
			found = true
			if c.Pos.StartLine < minStartLine {
				minStartLine = c.Pos.StartLine
			}
			if c.Pos.StartCol < minStartCol {
				minStartCol = c.Pos.StartCol
			}
			if c.Pos.StartOffset < minStartOffset {
				minStartOffset = c.Pos.StartOffset
			}
			if c.Pos.EndLine > maxEndLine {
				maxEndLine = c.Pos.EndLine
			}
			if c.Pos.EndCol > maxEndCol {
				maxEndCol = c.Pos.EndCol
			}
			if c.Pos.EndOffset > maxEndOffset {
				maxEndOffset = c.Pos.EndOffset
			}
		}

		var pos *node.Positions
		if found {
			pos = &node.Positions{
				StartLine:   minStartLine,
				StartCol:    minStartCol,
				StartOffset: minStartOffset,
				EndLine:     maxEndLine,
				EndCol:      maxEndCol,
				EndOffset:   maxEndOffset,
			}
		} else {
			pos = nil
		}

		synth := node.New("", "Synthetic", "", nil, pos, nil)
		synth.Children = mappedChildren
		return synth
	}
	return nil
}

// Token returns the string token for this node, if any.
func (dn *DSLNode) Token() string {
	if dn.Root.ChildCount() == 0 {
		return dn.extractNodeText(dn.Root)
	}
	return ""
}

// Positions returns the source code positions for this node, using uint fields as per UAST spec.
func (dn *DSLNode) Positions() *node.Positions {
	root := dn.Root
	return &node.Positions{
		StartLine:   root.StartPoint().Row + 1,
		StartCol:    root.StartPoint().Column + 1,
		StartOffset: root.StartByte(),
		EndLine:     root.EndPoint().Row + 1,
		EndCol:      root.EndPoint().Column + 1,
		EndOffset:   root.EndByte(),
	}
}

// extractNodeText extracts text from a Tree-sitter node
func (dn *DSLNode) extractNodeText(node sitter.Node) string {
	start := node.StartByte()
	end := node.EndByte()
	if safeconv.MustUintToInt(end) <= len(dn.Source) {
		return string(dn.Source[start:end])
	}
	return ""
}
