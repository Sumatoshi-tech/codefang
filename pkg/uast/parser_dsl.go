package uast

import (
	"context"
	"errors"
	"fmt"
	"io"
	"maps"
	"strings"
	"sync"
	"unsafe"

	forest "github.com/alexaandru/go-sitter-forest"
	sitter "github.com/alexaandru/go-tree-sitter-bare"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Sentinel errors for DSL parser operations.
var (
	errLanguageNotAvailable = errors.New("tree-sitter language not available")
	errNoRootNode           = errors.New("dsl parser: no root node")
	errPoolType             = errors.New("dsl parser: pool returned unexpected type")
)

// Token extraction source constants.
const (
	tokenSourceFieldsName = "fields.name"
	tokenSourcePropsName  = "props.name"
	tokenSourceText       = "text"
	comparisonParts       = 2
)

// maxInternLen is the maximum string length eligible for per-parse interning.
// Strings longer than this are unlikely to repeat within a single file.
const maxInternLen = 32

// DSLParser implements the UAST LanguageParser interface using DSL-based mappings.
type DSLParser struct {
	reader          io.Reader
	language        *sitter.Language
	patternMatcher  *mapping.PatternMatcher
	langInfo        *mapping.LanguageInfo
	originalDSL     string
	mappingRules    []mapping.Rule
	ruleIndex       map[string]int
	internedTypes   map[string]node.Type // pre-interned Type values from DSL rules
	internedRoles   map[string]node.Role // pre-interned Role values from DSL rules
	tsParserPool    sync.Pool
	IncludeUnmapped bool
}

// NewDSLParser creates a new DSL-based parser with the given language and mapping rules.
func NewDSLParser(reader io.Reader) *DSLParser {
	return &DSLParser{
		reader: reader,
	}
}

// Load reads and parses the DSL content, initializing the parser.
func (parser *DSLParser) Load() error {
	// Read the content first.
	content, err := io.ReadAll(parser.reader)
	if err != nil {
		return fmt.Errorf("reading DSL content: %w", err)
	}

	// Store the original DSL content.
	parser.originalDSL = string(content)

	// Parse the mapping.
	rules, langInfo, parseErr := (&mapping.Parser{}).ParseMapping(
		strings.NewReader(parser.originalDSL),
	)
	if parseErr != nil {
		return parseErr
	}

	parser.mappingRules = rules
	parser.langInfo = langInfo

	return parser.initializeLanguage()
}

// initializeLanguage initializes the tree-sitter language and pattern matcher.
func (parser *DSLParser) initializeLanguage() error {
	// Get the tree-sitter language with panic recovery.
	var lang *sitter.Language

	func() {
		defer func() {
			_ = recover() //nolint:errcheck // recover() returns any, not error
		}()

		lang = forest.GetLanguage(parser.langInfo.Name)
	}()

	if lang == nil {
		return fmt.Errorf("%w: %s", errLanguageNotAvailable, parser.langInfo.Name)
	}

	parser.language = lang
	parser.patternMatcher = mapping.NewPatternMatcher(parser.language)
	parser.tsParserPool = sync.Pool{
		New: func() any {
			tsParser := sitter.NewParser()
			tsParser.SetLanguage(lang)

			return tsParser
		},
	}

	// Build O(1) rule lookup index (first occurrence wins, matching
	// the original linear-scan semantics of findMappingRule).
	parser.ruleIndex = make(map[string]int, len(parser.mappingRules))

	for i, r := range parser.mappingRules {
		if _, exists := parser.ruleIndex[r.Name]; !exists {
			parser.ruleIndex[r.Name] = i
		}
	}

	// Pre-intern Type and Role strings from DSL rules. These are fixed per language
	// and repeat for every parsed file, so deduplication saves significant allocations.
	parser.internedTypes = make(map[string]node.Type, len(parser.mappingRules))
	parser.internedRoles = make(map[string]node.Role)

	for _, r := range parser.mappingRules {
		if r.UASTSpec.Type != "" {
			parser.internedTypes[r.UASTSpec.Type] = node.Type(r.UASTSpec.Type)
		}

		for _, role := range r.UASTSpec.Roles {
			if _, exists := parser.internedRoles[role]; !exists {
				parser.internedRoles[role] = node.Role(role)
			}
		}
	}

	return nil
}

// Extensions returns the supported file extensions for this parser.
func (parser *DSLParser) Extensions() []string {
	return parser.langInfo.Extensions
}

// Parse parses the given file content and returns the root UAST node.
func (parser *DSLParser) Parse(_ string, content []byte) (*node.Node, error) {
	tsParser, ok := parser.tsParserPool.Get().(*sitter.Parser)
	if !ok {
		return nil, errPoolType
	}

	defer parser.tsParserPool.Put(tsParser)

	tree, err := tsParser.ParseString(context.Background(), nil, content)
	if err != nil {
		return nil, fmt.Errorf("dsl parser: failed to parse: %w", err)
	}
	defer tree.Close()

	root := tree.RootNode()
	if root.IsNull() {
		return nil, errNoRootNode
	}

	interner := make(map[string]string, 128) //nolint:mnd // initial capacity for per-parse string interner
	dslNode := parser.createDSLNode(root, tree, content, interner)
	canonical := dslNode.ToCanonicalNode()

	return canonical, nil
}

// Language returns the language name for this parser.
func (parser *DSLParser) Language() string {
	return parser.langInfo.Name
}

// GetOriginalDSL returns the original DSL content that was used to create this parser.
func (parser *DSLParser) GetOriginalDSL() string {
	return parser.originalDSL
}

// DSLNode wraps a Tree-sitter node for conversion to UAST using DSL mappings.
type DSLNode struct {
	Root            sitter.Node
	Tree            *sitter.Tree
	PatternMatcher  *mapping.PatternMatcher
	Language        string
	ParentContext   string
	MappingRules    []mapping.Rule
	ruleIndex       map[string]int
	internedTypes   map[string]node.Type // shared from DSLParser (read-only)
	internedRoles   map[string]node.Role // shared from DSLParser (read-only)
	interner        map[string]string    // per-parse string interner for short tokens
	Source          []byte
	IncludeUnmapped bool
}

// ToCanonicalNode converts the DSLNode to a canonical UAST Node.
func (dn *DSLNode) ToCanonicalNode() *node.Node {
	nodeType := dn.Root.Type()
	mappingRule := dn.findMappingRule(nodeType)

	children := dn.processChildren(mappingRule)
	if dn.shouldSkipNode(mappingRule) {
		return nil
	}

	if dn.shouldSkipEmptyFile(nodeType, children) {
		return nil
	}

	var roles []node.Role

	if mappingRule != nil {
		return dn.createMappedNode(mappingRule, children, roles)
	}

	return dn.createUnmappedNode(nodeType, roles)
}

// createDSLNode creates a new DSLNode with the given parameters.
func (parser *DSLParser) createDSLNode(root sitter.Node, tree *sitter.Tree, content []byte, interner map[string]string) *DSLNode {
	return &DSLNode{
		Root:            root,
		Tree:            tree,
		Language:        parser.langInfo.Name,
		Source:          content,
		MappingRules:    parser.mappingRules,
		ruleIndex:       parser.ruleIndex,
		internedTypes:   parser.internedTypes,
		internedRoles:   parser.internedRoles,
		interner:        interner,
		PatternMatcher:  parser.patternMatcher,
		IncludeUnmapped: parser.IncludeUnmapped,
		ParentContext:   "",
	}
}

// findMappingRule finds a mapping rule for the given node type, resolving inheritance and merging fields.
func (dn *DSLNode) findMappingRule(nodeType string) *mapping.Rule {
	idx, ok := dn.ruleIndex[nodeType]
	if !ok {
		return nil
	}

	return dn.resolveInheritance(&dn.MappingRules[idx])
}

// resolveInheritance recursively merges base rule fields if Extends is set.
func (dn *DSLNode) resolveInheritance(rule *mapping.Rule) *mapping.Rule {
	if rule.Extends == "" {
		return rule
	}

	baseIdx, ok := dn.ruleIndex[rule.Extends]
	if !ok {
		return rule
	}

	base := &dn.MappingRules[baseIdx]

	merged := *base // Shallow copy.

	// Merge/override with child rule fields.
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

	// Recursively resolve further inheritance.
	return dn.resolveInheritance(&merged)
}

// processChildren processes all children of the node.
func (dn *DSLNode) processChildren(mappingRule *mapping.Rule) []*node.Node {
	childCount := dn.Root.NamedChildCount()
	children := make([]*node.Node, 0, childCount)

	for idx := range childCount {
		child := dn.Root.NamedChild(idx)
		childNode := dn.createChildNode(child, mappingRule)

		if dn.shouldExcludeChild(childNode, mappingRule) {
			continue
		}

		canonical := childNode.ToCanonicalNode()
		if canonical != nil {
			children = append(children, canonical)
		}
	}

	return children
}

// createChildNode creates a child DSLNode with proper parent context.
func (dn *DSLNode) createChildNode(child sitter.Node, mappingRule *mapping.Rule) *DSLNode {
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
		ruleIndex:       dn.ruleIndex,
		internedTypes:   dn.internedTypes,
		internedRoles:   dn.internedRoles,
		interner:        dn.interner,
		PatternMatcher:  dn.PatternMatcher,
		IncludeUnmapped: dn.IncludeUnmapped,
		ParentContext:   parentContext,
	}
}

// Pattern Matching and Capture Extraction.

// matchPattern returns the capture map for the current node and mapping rule, or nil if no match.
func (dn *DSLNode) matchPattern(mappingRule *mapping.Rule) map[string]string {
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

// extractCaptureText extracts text for a named capture using the pattern matcher.
func (dn *DSLNode) extractCaptureText(captureName string) string {
	mappingRule := dn.findMappingRule(dn.Root.Type())
	captures := dn.matchPattern(mappingRule)

	if captures != nil {
		if val, ok := captures[captureName]; ok {
			return val
		}
	}

	// Fallback: try to extract by field name.
	fieldNode := dn.Root.ChildByFieldName(captureName)
	if !fieldNode.IsNull() {
		if fieldNode.ChildCount() == 0 {
			return dn.extractNodeText(fieldNode)
		}

		// Not a leaf: return node type as placeholder.
		return fieldNode.Type()
	}

	// Recursively search for a descendant node of the given type.
	desc := findDescendantByType(dn.Root, captureName)
	if !desc.IsNull() {
		return dn.extractNodeText(desc)
	}

	return ""
}

func findDescendantByType(tsNode sitter.Node, typ string) sitter.Node {
	if tsNode.Type() == typ {
		return tsNode
	}

	for idx := range tsNode.NamedChildCount() {
		child := tsNode.NamedChild(idx)

		found := findDescendantByType(child, typ)
		if !found.IsNull() {
			return found
		}
	}

	return sitter.Node{}
}

// Condition Evaluation.

// evaluateConditions returns true if all conditions are satisfied for the current node.
func (dn *DSLNode) evaluateConditions(mappingRule *mapping.Rule) bool {
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

// evaluateCondition evaluates a single condition expression (very basic implementation).
func (dn *DSLNode) evaluateCondition(expr string, captures map[string]string) bool {
	// Only support simple equality: field == "value" or field != "value".
	expr = strings.TrimSpace(expr)

	if strings.Contains(expr, "==") {
		return dn.evaluateComparisonOp(
			expr, "==", captures,
			func(left, right string) bool { return left == right },
		)
	}

	if strings.Contains(expr, "!=") {
		return dn.evaluateComparisonOp(
			expr, "!=", captures,
			func(left, right string) bool { return left != right },
		)
	}

	return false
}

// evaluateComparisonOp is a helper that evaluates comparison expressions
// with the given operator and comparator.
func (dn *DSLNode) evaluateComparisonOp(expr, op string, captures map[string]string, compare func(left, right string) bool) bool {
	parts := strings.SplitN(expr, op, comparisonParts)
	if len(parts) != comparisonParts {
		return false
	}

	field := strings.TrimSpace(parts[0])
	val := strings.Trim(strings.TrimSpace(parts[1]), "\"")

	// Check captures first.
	if capturedVal, ok := captures[field]; ok {
		return compare(capturedVal, val)
	}

	// Check field names in the AST (zero-copy: comparison only).
	if fieldNode := dn.Root.ChildByFieldName(field); !fieldNode.IsNull() {
		fieldText := dn.unsafeNodeText(fieldNode)

		return compare(fieldText, val)
	}

	// Check if field is a child type (zero-copy: comparison only).
	for idx := range dn.Root.NamedChildCount() {
		child := dn.Root.NamedChild(idx)
		if child.Type() == field {
			childText := dn.unsafeNodeText(child)

			return compare(childText, val)
		}
	}

	return false
}

// Node/Child Inclusion/Exclusion.

// shouldSkipNode checks if the current node should be skipped based on mapping rule conditions.
func (dn *DSLNode) shouldSkipNode(mappingRule *mapping.Rule) bool {
	if mappingRule == nil {
		return false
	}

	return !dn.evaluateConditions(mappingRule)
}

// shouldExcludeChild checks if a child should be excluded based on mapping rules and conditions.
func (dn *DSLNode) shouldExcludeChild(childNode *DSLNode, mappingRule *mapping.Rule) bool {
	if mappingRule == nil {
		return false
	}

	childRule := childNode.findMappingRule(childNode.Root.Type())
	if childRule == nil {
		return false
	}

	return !childNode.evaluateConditions(childRule)
}

// shouldSkipEmptyFile checks if an empty file should be skipped.
func (dn *DSLNode) shouldSkipEmptyFile(nodeType string, children []*node.Node) bool {
	return nodeType == "source_file" && len(children) == 0 && len(dn.Source) == 0
}

// createMappedNode creates a UAST node from a mapped Tree-sitter node.
func (dn *DSLNode) createMappedNode(
	mappingRule *mapping.Rule, children []*node.Node,
	roles []node.Role,
) *node.Node {
	dn.extractRoles(mappingRule, &roles)

	// Lazily allocate props only when properties or names are present.
	var props map[string]string
	if mappingRule.UASTSpec.Props != nil {
		props = make(map[string]string, len(mappingRule.UASTSpec.Props))
	}

	dn.extractProperties(mappingRule, props)
	props = dn.extractName(mappingRule, props)

	// Use pre-interned type string if available, avoiding repeated allocation.
	nodeType := dn.internedTypes[mappingRule.UASTSpec.Type]
	if nodeType == "" {
		nodeType = node.Type(mappingRule.UASTSpec.Type)
	}

	uastNode := node.New(
		"", nodeType,
		dn.extractTokenText(mappingRule), roles, dn.extractPositions(), props,
	)
	uastNode.Children = children

	dn.extractToken(mappingRule, uastNode)

	return uastNode
}

// extractRoles extracts roles from the mapping rule, using pre-interned values.
func (dn *DSLNode) extractRoles(mappingRule *mapping.Rule, roles *[]node.Role) {
	if mappingRule == nil {
		return
	}

	for _, roleStr := range mappingRule.UASTSpec.Roles {
		if interned, ok := dn.internedRoles[roleStr]; ok {
			*roles = append(*roles, interned)
		} else {
			*roles = append(*roles, node.Role(roleStr))
		}
	}
}

// extractName extracts name from the node if specified in mapping.
// Returns the updated props map (may allocate if name is found and props is nil).
func (dn *DSLNode) extractName(mappingRule *mapping.Rule, props map[string]string) map[string]string {
	if mappingRule == nil {
		return props
	}

	name := dn.extractNameFromNode(tokenSourceFieldsName)
	if name != "" {
		if props == nil {
			props = make(map[string]string, 1)
		}

		props["name"] = name
	}

	return props
}

// extractNameFromNode extracts a name from a node using the specified source.
func (dn *DSLNode) extractNameFromNode(source string) string {
	switch source {
	case tokenSourceFieldsName:
		return dn.extractNameFromField("name")
	case tokenSourcePropsName:
		return dn.extractNameFromProps()
	case tokenSourceText:
		return dn.extractNameFromText()
	default:
		return ""
	}
}

// extractNameFromField extracts a name from a specific field using Tree-sitter's field API.
func (dn *DSLNode) extractNameFromField(fieldName string) string {
	fieldNode := dn.Root.ChildByFieldName(fieldName)
	if !fieldNode.IsNull() {
		return dn.extractNodeText(fieldNode)
	}

	return ""
}

// extractNameFromText extracts name from node text.
func (dn *DSLNode) extractNameFromText() string {
	if dn.Root.ChildCount() == 0 {
		return dn.extractNodeText(dn.Root)
	}

	return ""
}

// extractNameFromProps extracts name from node properties (legacy).
func (dn *DSLNode) extractNameFromProps() string {
	return dn.extractNameFromText()
}

// extractProperties extracts properties from the mapping rule.
func (dn *DSLNode) extractProperties(mappingRule *mapping.Rule, props map[string]string) {
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

// extractPropertyValue extracts a property value from the node.
func (dn *DSLNode) extractPropertyValue(propStr string) string {
	if strings.HasPrefix(propStr, "@") && len(propStr) > 1 {
		// Property references a capture.
		return dn.extractCaptureText(propStr[1:])
	}

	if after, ok := strings.CutPrefix(propStr, "descendant:"); ok {
		return dn.extractTokenFromDescendant(after)
	}

	return dn.extractDirectChildProperty(propStr)
}

// extractDirectChildProperty extracts a direct child property.
func (dn *DSLNode) extractDirectChildProperty(propStr string) string {
	for idx := range dn.Root.NamedChildCount() {
		child := dn.Root.NamedChild(idx)

		childKind := child.Type()
		if childKind == propStr {
			return dn.extractChildText(child)
		}
	}

	return ""
}

// extractChildText extracts text from a child node.
// Short strings are interned within the current parse.
func (dn *DSLNode) extractChildText(child sitter.Node) string {
	start := child.StartByte()
	end := child.EndByte()

	if safeconv.MustUintToInt(end) <= len(dn.Source) {
		s := string(dn.Source[start:end])

		if len(s) <= maxInternLen && dn.interner != nil {
			if interned, ok := dn.interner[s]; ok {
				return interned
			}

			dn.interner[s] = s
		}

		return s
	}

	return ""
}

// extractToken extracts token from the mapping rule.
func (dn *DSLNode) extractToken(mappingRule *mapping.Rule, uastNode *node.Node) {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return
	}

	token := dn.extractTokenFromNode(mappingRule.UASTSpec.Token)
	if token != "" {
		uastNode.Token = token
	}
}

// extractTokenFromNode extracts a token from a node using the specified source.
func (dn *DSLNode) extractTokenFromNode(source string) string {
	switch source {
	case tokenSourceFieldsName:
		return dn.extractNameFromField("name")
	case tokenSourcePropsName:
		return dn.extractNameFromProps()
	case tokenSourceText:
		return dn.extractNameFromText()
	default:
		// Try to extract as a field name.
		if after, ok := strings.CutPrefix(source, "fields."); ok {
			return dn.extractNameFromField(after)
		}

		return ""
	}
}

// extractTokenFromChildType finds the first child of the specified type and extracts its token.
func (dn *DSLNode) extractTokenFromChildType(nodeType string) string {
	for idx := range dn.Root.NamedChildCount() {
		child := dn.Root.NamedChild(idx)
		if child.Type() == nodeType {
			return dn.extractNodeText(child)
		}
	}

	return ""
}

// extractTokenFromDescendant finds the first descendant of the specified type and extracts its token.
func (dn *DSLNode) extractTokenFromDescendant(nodeType string) string {
	return dn.findDescendantToken(nodeType)
}

// findDescendantToken recursively searches for a descendant of the specified type.
func (dn *DSLNode) findDescendantToken(nodeType string) string {
	if dn.Root.Type() == nodeType {
		return dn.extractNodeText(dn.Root)
	}

	for idx := range dn.Root.NamedChildCount() {
		child := dn.Root.NamedChild(idx)

		childNode := &DSLNode{
			Root:            child,
			Tree:            dn.Tree,
			Language:        dn.Language,
			Source:          dn.Source,
			MappingRules:    dn.MappingRules,
			ruleIndex:       dn.ruleIndex,
			internedTypes:   dn.internedTypes,
			internedRoles:   dn.internedRoles,
			interner:        dn.interner,
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

// extractTokenText extracts the token text based on the mapping rule.
func (dn *DSLNode) extractTokenText(mappingRule *mapping.Rule) string {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return ""
	}

	tokenSpec := mappingRule.UASTSpec.Token

	if captureName, ok := strings.CutPrefix(tokenSpec, "@"); ok {
		// Extract from capture.
		return dn.extractCaptureText(captureName)
	}

	switch tokenSpec {
	case "self", tokenSourceText:
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

// extractPositions extracts position information from the Tree-sitter node.
func (dn *DSLNode) extractPositions() *node.Positions {
	start := dn.Root.StartPoint()
	end := dn.Root.EndPoint()

	return node.NewPositions(
		start.Row+1,
		start.Column+1,
		dn.Root.StartByte(),
		end.Row+1,
		end.Column+1,
		dn.Root.EndByte(),
	)
}

// createUnmappedNode creates a UAST node for unmapped Tree-sitter nodes.
func (dn *DSLNode) createUnmappedNode(nodeType string, roles []node.Role) *node.Node {
	mappedChildren := dn.processUnmappedChildren()

	if dn.IncludeUnmapped {
		return dn.createIncludeUnmappedNode(nodeType, mappedChildren, roles)
	}

	return dn.createSyntheticNode(mappedChildren)
}

// processUnmappedChildren processes children for unmapped nodes.
func (dn *DSLNode) processUnmappedChildren() []*node.Node {
	var mappedChildren []*node.Node

	for idx := range dn.Root.NamedChildCount() {
		child := dn.Root.NamedChild(idx)
		childNode := dn.createUnmappedChildNode(child)

		canonical := childNode.ToCanonicalNode()
		if canonical != nil {
			mappedChildren = append(mappedChildren, canonical)
		}
	}

	return mappedChildren
}

// createUnmappedChildNode creates a child node for unmapped nodes.
func (dn *DSLNode) createUnmappedChildNode(child sitter.Node) *DSLNode {
	return &DSLNode{
		Root:            child,
		Tree:            dn.Tree,
		Language:        dn.Language,
		Source:          dn.Source,
		MappingRules:    dn.MappingRules,
		ruleIndex:       dn.ruleIndex,
		internedTypes:   dn.internedTypes,
		internedRoles:   dn.internedRoles,
		interner:        dn.interner,
		PatternMatcher:  dn.PatternMatcher,
		IncludeUnmapped: dn.IncludeUnmapped,
		ParentContext:   dn.ParentContext,
	}
}

// createIncludeUnmappedNode creates a node when IncludeUnmapped is true.
func (dn *DSLNode) createIncludeUnmappedNode(
	nodeType string, mappedChildren []*node.Node,
	roles []node.Role,
) *node.Node {
	uastNode := node.New(
		"", node.Type(dn.Language+":"+nodeType),
		dn.Token(), roles, dn.Positions(), nil,
	)
	uastNode.Children = mappedChildren

	return uastNode
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

	if len(mappedChildren) == 0 {
		return nil
	}

	pos := computeChildrenSpan(mappedChildren)

	synth := node.New("", "Synthetic", "", nil, pos, nil)
	synth.Children = mappedChildren

	return synth
}

// positionBounds tracks min/max position values while scanning children.
type positionBounds struct {
	minStartLine, minStartCol, minStartOffset uint
	maxEndLine, maxEndCol, maxEndOffset       uint
	found                                     bool
}

func newPositionBounds() positionBounds {
	const maxUint = ^uint(0)

	return positionBounds{
		minStartLine: maxUint, minStartCol: maxUint, minStartOffset: maxUint,
	}
}

func (b *positionBounds) update(pos *node.Positions) {
	b.found = true

	if pos.StartLine < b.minStartLine {
		b.minStartLine = pos.StartLine
	}

	if pos.StartCol < b.minStartCol {
		b.minStartCol = pos.StartCol
	}

	if pos.StartOffset < b.minStartOffset {
		b.minStartOffset = pos.StartOffset
	}

	if pos.EndLine > b.maxEndLine {
		b.maxEndLine = pos.EndLine
	}

	if pos.EndCol > b.maxEndCol {
		b.maxEndCol = pos.EndCol
	}

	if pos.EndOffset > b.maxEndOffset {
		b.maxEndOffset = pos.EndOffset
	}
}

func (b *positionBounds) toPositions() *node.Positions {
	if !b.found {
		return nil
	}

	return node.NewPositions(
		b.minStartLine,
		b.minStartCol,
		b.minStartOffset,
		b.maxEndLine,
		b.maxEndCol,
		b.maxEndOffset,
	)
}

// computeChildrenSpan computes the bounding span across children positions.
func computeChildrenSpan(children []*node.Node) *node.Positions {
	bounds := newPositionBounds()

	for _, child := range children {
		if child.Pos != nil {
			bounds.update(child.Pos)
		}
	}

	return bounds.toPositions()
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
	start := root.StartPoint()
	end := root.EndPoint()

	return node.NewPositions(
		start.Row+1,
		start.Column+1,
		root.StartByte(),
		end.Row+1,
		end.Column+1,
		root.EndByte(),
	)
}

// extractNodeText extracts text from a Tree-sitter node (allocating copy).
// Use for values that are stored in Node.Token or Node.Props.
// Short strings (≤ maxInternLen) are interned within the current parse to
// deduplicate repeated identifiers, keywords, and operators.
func (dn *DSLNode) extractNodeText(tsNode sitter.Node) string {
	start := tsNode.StartByte()
	end := tsNode.EndByte()

	if safeconv.MustUintToInt(end) <= len(dn.Source) {
		s := string(dn.Source[start:end])

		if len(s) <= maxInternLen && dn.interner != nil {
			if interned, ok := dn.interner[s]; ok {
				return interned
			}

			dn.interner[s] = s
		}

		return s
	}

	return ""
}

// unsafeNodeText returns a zero-copy string view of a Tree-sitter node's text.
// The returned string shares the Source []byte backing array and must NOT be
// stored beyond the current parse call (i.e., not in Node.Token or Node.Props).
// Safe for comparison, filtering, and condition evaluation within Parse().
func (dn *DSLNode) unsafeNodeText(tsNode sitter.Node) string {
	start := tsNode.StartByte()
	end := tsNode.EndByte()

	if end <= uint(len(dn.Source)) {
		return unsafe.String(&dn.Source[start], safeconv.MustUintToInt(end-start))
	}

	return ""
}
