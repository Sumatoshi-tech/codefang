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
	symbolNames     []string
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

	parser.symbolNames = buildSymbolNames(parser.language)

	return nil
}

func buildSymbolNames(language *sitter.Language) []string {
	symbolCount := min(language.SymbolCount(), maxSymbolID+1)

	symbolNames := make([]string, int(symbolCount))

	for symbolID := uint16(0); uint32(symbolID) < symbolCount; symbolID++ {
		symbolNames[symbolID] = language.SymbolName(sitter.Symbol(symbolID))
	}

	return symbolNames
}

// Extensions returns the supported file extensions for this parser.
func (parser *DSLParser) Extensions() []string {
	return parser.langInfo.Extensions
}

// Parse parses the given file content and returns the root UAST node.
func (parser *DSLParser) Parse(_ string, content []byte) (*node.Node, error) {
	tree, err := parser.parseTSTree(content)
	if err != nil {
		return nil, err
	}
	defer tree.Close()

	root := tree.RootNode()
	if root.IsNull() {
		return nil, errNoRootNode
	}

	ctx := parser.newParseContext(tree, content)
	canonical := ctx.toCanonicalNode(root, "")

	return canonical, nil
}

// parseTSTree parses source bytes into a tree-sitter Tree.
// The caller is responsible for calling tree.Close().
func (parser *DSLParser) parseTSTree(content []byte) (*sitter.Tree, error) {
	tsParser, ok := parser.tsParserPool.Get().(*sitter.Parser)
	if !ok {
		return nil, errPoolType
	}

	defer parser.tsParserPool.Put(tsParser)

	tree, err := tsParser.ParseString(context.Background(), nil, content)
	if err != nil {
		return nil, fmt.Errorf("dsl parser: failed to parse: %w", err)
	}

	return tree, nil
}

// Language returns the language name for this parser.
func (parser *DSLParser) Language() string {
	return parser.langInfo.Name
}

// GetOriginalDSL returns the original DSL content that was used to create this parser.
func (parser *DSLParser) GetOriginalDSL() string {
	return parser.originalDSL
}

// parseContext holds shared state for a single Parse() call.
// Per-node varying state (root, parentContext) is passed as function parameters,
// eliminating per-node heap allocations.
type parseContext struct {
	tree            *sitter.Tree
	alloc           *node.Allocator
	cursors         []*sitter.TreeCursor
	batchChildren   []batchChildInfo
	patternMatcher  *mapping.PatternMatcher
	language        string
	mappingRules    []mapping.Rule
	ruleIndex       map[string]int
	symbolNames     []string
	internedTypes   map[string]node.Type
	internedRoles   map[string]node.Role
	interner        map[string]string
	source          []byte
	includeUnmapped bool
}

// newParseContext creates a parseContext for a single Parse() call.
func (parser *DSLParser) newParseContext(tree *sitter.Tree, content []byte) *parseContext {
	return &parseContext{
		tree:            tree,
		alloc:           &node.Allocator{},
		language:        parser.langInfo.Name,
		source:          content,
		mappingRules:    parser.mappingRules,
		ruleIndex:       parser.ruleIndex,
		symbolNames:     parser.symbolNames,
		internedTypes:   parser.internedTypes,
		internedRoles:   parser.internedRoles,
		interner:        make(map[string]string, 128),  //nolint:mnd // initial capacity for per-parse string interner
		batchChildren:   make([]batchChildInfo, 0, 32), //nolint:mnd // small reusable child batch buffer per parse
		patternMatcher:  parser.patternMatcher,
		includeUnmapped: parser.IncludeUnmapped,
	}
}

// getCursor returns a TreeCursor from the pool, or creates a new one.
// Cursors are reused across recursive processChildren calls within a single parse.
func (ctx *parseContext) getCursor(root sitter.Node) *sitter.TreeCursor {
	poolLen := len(ctx.cursors)
	if poolLen > 0 {
		cursor := ctx.cursors[poolLen-1]
		ctx.cursors = ctx.cursors[:poolLen-1]

		cursor.Reset(root)

		return cursor
	}

	return sitter.NewTreeCursor(root)
}

// putCursor returns a TreeCursor to the pool for reuse.
func (ctx *parseContext) putCursor(cursor *sitter.TreeCursor) {
	ctx.cursors = append(ctx.cursors, cursor)
}

// nodeType resolves node type via unsafe symbol lookup and parser symbol table.
// It falls back to the CGO Type() path for inline or invalid symbol cases.
func (ctx *parseContext) nodeType(root sitter.Node) string {
	symbolID := readSymbol(unsafe.Pointer(&root))
	if symbolID == invalidSymbolID {
		// Inline subtrees do not expose heap symbol data. Use grammar symbol ID
		// (numeric CGO call) to avoid string allocations from ts_node_type.
		symbolID = uint16(root.GrammarSymbol())
	}

	symbolIndex := int(symbolID)
	if symbolIndex < len(ctx.symbolNames) {
		symbolName := ctx.symbolNames[symbolIndex]
		if symbolName != "" {
			return symbolName
		}
	}

	return root.Type()
}

// toCanonicalNode converts a tree-sitter node to a canonical UAST Node.
func (ctx *parseContext) toCanonicalNode(root sitter.Node, parentContext string) *node.Node {
	nodeType := ctx.nodeType(root)
	mappingRule := ctx.findMappingRule(nodeType)

	children := ctx.processChildren(root, mappingRule)
	if ctx.shouldSkipNode(root, mappingRule) {
		return nil
	}

	if ctx.shouldSkipEmptyFile(nodeType, children) {
		return nil
	}

	var roles []node.Role

	if mappingRule != nil {
		return ctx.createMappedNode(root, mappingRule, children, roles)
	}

	return ctx.createUnmappedNode(root, parentContext, nodeType, roles)
}

// findMappingRule finds a mapping rule for the given node type, resolving inheritance and merging fields.
func (ctx *parseContext) findMappingRule(nodeType string) *mapping.Rule {
	idx, ok := ctx.ruleIndex[nodeType]
	if !ok {
		return nil
	}

	return ctx.resolveInheritance(&ctx.mappingRules[idx])
}

// resolveInheritance recursively merges base rule fields if Extends is set.
func (ctx *parseContext) resolveInheritance(rule *mapping.Rule) *mapping.Rule {
	if rule.Extends == "" {
		return rule
	}

	baseIdx, ok := ctx.ruleIndex[rule.Extends]
	if !ok {
		return rule
	}

	base := &ctx.mappingRules[baseIdx]

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
	return ctx.resolveInheritance(&merged)
}

// cursorThreshold is the minimum named child count at which cursor-based
// iteration is faster than NamedChild(idx). Below this, the per-child CGO
// overhead of cursor operations exceeds the O(N*C) savings.
const cursorThreshold = 8

// processChildren processes all children of the node.
func (ctx *parseContext) processChildren(root sitter.Node, mappingRule *mapping.Rule) []*node.Node {
	childCount := readNamedChildCount(unsafe.Pointer(&root))
	children := make([]*node.Node, 0, childCount)

	if childCount < cursorThreshold {
		return ctx.processChildrenDirect(root, childCount, mappingRule, children)
	}

	return ctx.processChildrenBatch(root, childCount, mappingRule, children)
}

// processChildrenDirect iterates children via NamedChild(idx). Efficient for
// nodes with few children where cursor allocation overhead exceeds O(N*C) savings.
func (ctx *parseContext) processChildrenDirect(
	root sitter.Node, childCount uint32, mappingRule *mapping.Rule, children []*node.Node,
) []*node.Node {
	for idx := range childCount {
		child := root.NamedChild(idx)

		if ctx.shouldExcludeChild(child, mappingRule) {
			continue
		}

		childParentCtx := ctx.deriveParentContext(root, mappingRule)

		canonical := ctx.toCanonicalNode(child, childParentCtx)
		if canonical != nil {
			children = append(children, canonical)
		}
	}

	return children
}

// processChildrenCursor iterates children via TreeCursor. Efficient for nodes
// with many children where O(C) cursor traversal beats O(N*C) NamedChild(idx).
func (ctx *parseContext) processChildrenCursor(
	root sitter.Node, mappingRule *mapping.Rule, children []*node.Node,
) []*node.Node {
	cursor := ctx.getCursor(root)

	if !cursor.GoToFirstChild() {
		ctx.putCursor(cursor)

		return children
	}

	for {
		child := cursor.CurrentNode()

		if child.IsNamed() {
			if !ctx.shouldExcludeChild(child, mappingRule) {
				childParentCtx := ctx.deriveParentContext(root, mappingRule)

				canonical := ctx.toCanonicalNode(child, childParentCtx)
				if canonical != nil {
					children = append(children, canonical)
				}
			}
		}

		if !cursor.GoToNextSibling() {
			break
		}
	}

	ctx.putCursor(cursor)

	return children
}

func (ctx *parseContext) ensureBatchChildren(size uint32) []batchChildInfo {
	needed := safeconv.MustUintToInt(uint(size))

	if cap(ctx.batchChildren) < needed {
		ctx.batchChildren = make([]batchChildInfo, needed)
	} else {
		ctx.batchChildren = ctx.batchChildren[:needed]
	}

	return ctx.batchChildren
}

func (ctx *parseContext) processChildrenBatch(
	root sitter.Node,
	childCount uint32,
	mappingRule *mapping.Rule,
	children []*node.Node,
) []*node.Node {
	batchChildren := ctx.ensureBatchChildren(childCount)

	written, total := readNamedChildrenBatch(unsafe.Pointer(&root), batchChildren)
	if total != childCount || written != childCount {
		return ctx.processChildrenCursor(root, mappingRule, children)
	}

	for idx := range written {
		child := batchChildToNode(batchChildren[idx])
		if child.IsNull() || !child.IsNamed() {
			return ctx.processChildrenCursor(root, mappingRule, children)
		}

		if ctx.shouldExcludeChild(child, mappingRule) {
			continue
		}

		childParentCtx := ctx.deriveParentContext(root, mappingRule)

		canonical := ctx.toCanonicalNode(child, childParentCtx)
		if canonical != nil {
			children = append(children, canonical)
		}
	}

	return children
}

// deriveParentContext computes the parent context string for child nodes.
func (ctx *parseContext) deriveParentContext(root sitter.Node, mappingRule *mapping.Rule) string {
	if mappingRule != nil && mappingRule.UASTSpec.Type != "" {
		return mappingRule.UASTSpec.Type
	}

	return ctx.nodeType(root)
}

// Pattern Matching and Capture Extraction.

// matchPattern returns the capture map for the current node and mapping rule, or nil if no match.
func (ctx *parseContext) matchPattern(root sitter.Node, mappingRule *mapping.Rule) map[string]string {
	if mappingRule == nil || mappingRule.Pattern == "" {
		return nil
	}

	query, err := ctx.patternMatcher.CompileAndCache(mappingRule.Pattern)
	if err != nil {
		return nil
	}

	captures, err := ctx.patternMatcher.MatchPattern(query, &root, ctx.source)
	if err != nil {
		return nil
	}

	return captures
}

// extractCaptureText extracts text for a named capture using the pattern matcher.
func (ctx *parseContext) extractCaptureText(root sitter.Node, captureName string) string {
	mappingRule := ctx.findMappingRule(ctx.nodeType(root))
	captures := ctx.matchPattern(root, mappingRule)

	if captures != nil {
		if val, ok := captures[captureName]; ok {
			return val
		}
	}

	// Fallback: try to extract by field name.
	fieldNode := root.ChildByFieldName(captureName)
	if !fieldNode.IsNull() {
		if fieldNode.ChildCount() == 0 {
			return ctx.extractNodeText(fieldNode)
		}

		// Not a leaf: return node type as placeholder.
		return ctx.nodeType(fieldNode)
	}

	// Recursively search for a descendant node of the given type.
	desc := findDescendantByType(root, captureName)
	if !desc.IsNull() {
		return ctx.extractNodeText(desc)
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
func (ctx *parseContext) evaluateConditions(root sitter.Node, mappingRule *mapping.Rule) bool {
	if mappingRule == nil || len(mappingRule.Conditions) == 0 {
		return true
	}

	captures := ctx.matchPattern(root, mappingRule)

	for _, cond := range mappingRule.Conditions {
		if !ctx.evaluateCondition(root, cond.Expr, captures) {
			return false
		}
	}

	return true
}

// evaluateCondition evaluates a single condition expression (very basic implementation).
func (ctx *parseContext) evaluateCondition(root sitter.Node, expr string, captures map[string]string) bool {
	// Only support simple equality: field == "value" or field != "value".
	expr = strings.TrimSpace(expr)

	if strings.Contains(expr, "==") {
		return ctx.evaluateComparisonOp(
			root, expr, "==", captures,
			func(left, right string) bool { return left == right },
		)
	}

	if strings.Contains(expr, "!=") {
		return ctx.evaluateComparisonOp(
			root, expr, "!=", captures,
			func(left, right string) bool { return left != right },
		)
	}

	return false
}

// evaluateComparisonOp is a helper that evaluates comparison expressions
// with the given operator and comparator.
func (ctx *parseContext) evaluateComparisonOp(
	root sitter.Node, expr, op string,
	captures map[string]string, compare func(left, right string) bool,
) bool {
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
	if fieldNode := root.ChildByFieldName(field); !fieldNode.IsNull() {
		fieldText := ctx.unsafeNodeText(fieldNode)

		return compare(fieldText, val)
	}

	// Check if field is a child type (zero-copy: comparison only).
	cursor := ctx.getCursor(root)

	if cursor.GoToFirstChild() {
		for {
			child := cursor.CurrentNode()

			if child.IsNamed() && ctx.nodeType(child) == field {
				childText := ctx.unsafeNodeText(child)
				ctx.putCursor(cursor)

				return compare(childText, val)
			}

			if !cursor.GoToNextSibling() {
				break
			}
		}
	}

	ctx.putCursor(cursor)

	return false
}

// Node/Child Inclusion/Exclusion.

// shouldSkipNode checks if the current node should be skipped based on mapping rule conditions.
func (ctx *parseContext) shouldSkipNode(root sitter.Node, mappingRule *mapping.Rule) bool {
	if mappingRule == nil {
		return false
	}

	return !ctx.evaluateConditions(root, mappingRule)
}

// shouldExcludeChild checks if a child should be excluded based on mapping rules and conditions.
func (ctx *parseContext) shouldExcludeChild(child sitter.Node, mappingRule *mapping.Rule) bool {
	if mappingRule == nil {
		return false
	}

	childRule := ctx.findMappingRule(ctx.nodeType(child))
	if childRule == nil {
		return false
	}

	return !ctx.evaluateConditions(child, childRule)
}

// shouldSkipEmptyFile checks if an empty file should be skipped.
func (ctx *parseContext) shouldSkipEmptyFile(nodeType string, children []*node.Node) bool {
	return nodeType == "source_file" && len(children) == 0 && len(ctx.source) == 0
}

// createMappedNode creates a UAST node from a mapped Tree-sitter node.
func (ctx *parseContext) createMappedNode(
	root sitter.Node, mappingRule *mapping.Rule,
	children []*node.Node, roles []node.Role,
) *node.Node {
	ctx.extractRoles(mappingRule, &roles)

	// Lazily allocate props only when properties or names are present.
	var props map[string]string
	if mappingRule.UASTSpec.Props != nil {
		props = make(map[string]string, len(mappingRule.UASTSpec.Props))
	}

	ctx.extractProperties(root, mappingRule, props)
	props = ctx.extractName(root, mappingRule, props)

	// Use pre-interned type string if available, avoiding repeated allocation.
	nodeType := ctx.internedTypes[mappingRule.UASTSpec.Type]
	if nodeType == "" {
		nodeType = node.Type(mappingRule.UASTSpec.Type)
	}

	uastNode := ctx.alloc.NewNode(
		"", nodeType,
		ctx.extractTokenText(root, mappingRule), roles, ctx.extractPositions(root), props,
	)
	uastNode.Children = children

	ctx.extractToken(root, mappingRule, uastNode)

	return uastNode
}

// extractRoles extracts roles from the mapping rule, using pre-interned values.
func (ctx *parseContext) extractRoles(mappingRule *mapping.Rule, roles *[]node.Role) {
	if mappingRule == nil {
		return
	}

	for _, roleStr := range mappingRule.UASTSpec.Roles {
		if interned, ok := ctx.internedRoles[roleStr]; ok {
			*roles = append(*roles, interned)
		} else {
			*roles = append(*roles, node.Role(roleStr))
		}
	}
}

// extractName extracts name from the node if specified in mapping.
// Returns the updated props map (may allocate if name is found and props is nil).
func (ctx *parseContext) extractName(root sitter.Node, mappingRule *mapping.Rule, props map[string]string) map[string]string {
	if mappingRule == nil {
		return props
	}

	name := ctx.extractNameFromNode(root, tokenSourceFieldsName)
	if name != "" {
		if props == nil {
			props = make(map[string]string, 1)
		}

		props["name"] = name
	}

	return props
}

// extractNameFromNode extracts a name from a node using the specified source.
func (ctx *parseContext) extractNameFromNode(root sitter.Node, source string) string {
	switch source {
	case tokenSourceFieldsName:
		return ctx.extractNameFromField(root, "name")
	case tokenSourcePropsName:
		return ctx.extractNameFromText(root)
	case tokenSourceText:
		return ctx.extractNameFromText(root)
	default:
		return ""
	}
}

// extractNameFromField extracts a name from a specific field using Tree-sitter's field API.
func (ctx *parseContext) extractNameFromField(root sitter.Node, fieldName string) string {
	fieldNode := root.ChildByFieldName(fieldName)
	if !fieldNode.IsNull() {
		return ctx.extractNodeText(fieldNode)
	}

	return ""
}

// extractNameFromText extracts name from node text.
func (ctx *parseContext) extractNameFromText(root sitter.Node) string {
	if root.ChildCount() == 0 {
		return ctx.extractNodeText(root)
	}

	return ""
}

// extractProperties extracts properties from the mapping rule.
func (ctx *parseContext) extractProperties(root sitter.Node, mappingRule *mapping.Rule, props map[string]string) {
	if mappingRule == nil || mappingRule.UASTSpec.Props == nil {
		return
	}

	for key, value := range mappingRule.UASTSpec.Props {
		extractedValue := ctx.extractPropertyValue(root, value)
		if extractedValue != "" {
			props[key] = extractedValue
		}
	}
}

// extractPropertyValue extracts a property value from the node.
func (ctx *parseContext) extractPropertyValue(root sitter.Node, propStr string) string {
	if strings.HasPrefix(propStr, "@") && len(propStr) > 1 {
		// Property references a capture.
		return ctx.extractCaptureText(root, propStr[1:])
	}

	if after, ok := strings.CutPrefix(propStr, "descendant:"); ok {
		return ctx.findDescendantToken(root, after)
	}

	return ctx.extractDirectChildProperty(root, propStr)
}

// extractDirectChildProperty extracts a direct child property.
func (ctx *parseContext) extractDirectChildProperty(root sitter.Node, propStr string) string {
	for idx := range root.NamedChildCount() {
		child := root.NamedChild(idx)

		childKind := ctx.nodeType(child)
		if childKind == propStr {
			return ctx.extractChildText(child)
		}
	}

	return ""
}

// extractChildText extracts text from a child node.
// Short strings are interned within the current parse.
func (ctx *parseContext) extractChildText(child sitter.Node) string {
	start := child.StartByte()
	end := child.EndByte()

	if safeconv.MustUintToInt(end) <= len(ctx.source) {
		s := string(ctx.source[start:end])

		if len(s) <= maxInternLen && ctx.interner != nil {
			if interned, ok := ctx.interner[s]; ok {
				return interned
			}

			ctx.interner[s] = s
		}

		return s
	}

	return ""
}

// extractToken extracts token from the mapping rule.
func (ctx *parseContext) extractToken(root sitter.Node, mappingRule *mapping.Rule, uastNode *node.Node) {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return
	}

	token := ctx.extractTokenFromNode(root, mappingRule.UASTSpec.Token)
	if token != "" {
		uastNode.Token = token
	}
}

// extractTokenFromNode extracts a token from a node using the specified source.
func (ctx *parseContext) extractTokenFromNode(root sitter.Node, source string) string {
	switch source {
	case tokenSourceFieldsName:
		return ctx.extractNameFromField(root, "name")
	case tokenSourcePropsName:
		return ctx.extractNameFromText(root)
	case tokenSourceText:
		return ctx.extractNameFromText(root)
	default:
		// Try to extract as a field name.
		if after, ok := strings.CutPrefix(source, "fields."); ok {
			return ctx.extractNameFromField(root, after)
		}

		return ""
	}
}

// extractTokenFromChildType finds the first child of the specified type and extracts its token.
func (ctx *parseContext) extractTokenFromChildType(root sitter.Node, nodeType string) string {
	for idx := range root.NamedChildCount() {
		child := root.NamedChild(idx)
		if ctx.nodeType(child) == nodeType {
			return ctx.extractNodeText(child)
		}
	}

	return ""
}

// findDescendantToken recursively searches for a descendant of the specified type.
func (ctx *parseContext) findDescendantToken(root sitter.Node, nodeType string) string {
	if ctx.nodeType(root) == nodeType {
		return ctx.extractNodeText(root)
	}

	for idx := range root.NamedChildCount() {
		child := root.NamedChild(idx)

		if result := ctx.findDescendantToken(child, nodeType); result != "" {
			return result
		}
	}

	return ""
}

// extractTokenText extracts the token text based on the mapping rule.
func (ctx *parseContext) extractTokenText(root sitter.Node, mappingRule *mapping.Rule) string {
	if mappingRule == nil || mappingRule.UASTSpec.Token == "" {
		return ""
	}

	tokenSpec := mappingRule.UASTSpec.Token

	if captureName, ok := strings.CutPrefix(tokenSpec, "@"); ok {
		// Extract from capture.
		return ctx.extractCaptureText(root, captureName)
	}

	switch tokenSpec {
	case "self", tokenSourceText:
		return ctx.extractNodeText(root)
	default:
		if after, ok := strings.CutPrefix(tokenSpec, "child:"); ok {
			return ctx.extractTokenFromChildType(root, after)
		}

		if after, ok := strings.CutPrefix(tokenSpec, "descendant:"); ok {
			return ctx.findDescendantToken(root, after)
		}

		return tokenSpec
	}
}

// extractPositions extracts position information from the Tree-sitter node.
// Start values are read directly from the TSNode struct via unsafe (0 CGO calls).
// End values use a dedicated C helper (1 CGO call).
// Total: 1 CGO call per node, down from 2.
func (ctx *parseContext) extractPositions(root sitter.Node) *node.Positions {
	startByte, startRow, startCol := readStartPositions(unsafe.Pointer(&root))
	endByte, endRow, endCol := readEndPositions(unsafe.Pointer(&root))

	return ctx.alloc.NewPositions(
		startRow+1,
		startCol+1,
		startByte,
		endRow+1,
		endCol+1,
		endByte,
	)
}

// createUnmappedNode creates a UAST node for unmapped Tree-sitter nodes.
func (ctx *parseContext) createUnmappedNode(root sitter.Node, parentContext, nodeType string, roles []node.Role) *node.Node {
	mappedChildren := ctx.processUnmappedChildren(root, parentContext)

	if ctx.includeUnmapped {
		return ctx.createIncludeUnmappedNode(root, nodeType, mappedChildren, roles)
	}

	return ctx.createSyntheticNode(mappedChildren)
}

// processUnmappedChildren processes children for unmapped nodes.
func (ctx *parseContext) processUnmappedChildren(root sitter.Node, parentContext string) []*node.Node {
	childCount := readNamedChildCount(unsafe.Pointer(&root))

	if childCount < cursorThreshold {
		return ctx.processUnmappedChildrenDirect(root, childCount, parentContext)
	}

	return ctx.processUnmappedChildrenCursor(root, parentContext)
}

func (ctx *parseContext) processUnmappedChildrenDirect(
	root sitter.Node, childCount uint32, parentContext string,
) []*node.Node {
	var mappedChildren []*node.Node

	for idx := range childCount {
		child := root.NamedChild(idx)

		canonical := ctx.toCanonicalNode(child, parentContext)
		if canonical != nil {
			mappedChildren = append(mappedChildren, canonical)
		}
	}

	return mappedChildren
}

func (ctx *parseContext) processUnmappedChildrenCursor(root sitter.Node, parentContext string) []*node.Node {
	var mappedChildren []*node.Node

	cursor := ctx.getCursor(root)

	if !cursor.GoToFirstChild() {
		ctx.putCursor(cursor)

		return mappedChildren
	}

	for {
		child := cursor.CurrentNode()

		if child.IsNamed() {
			canonical := ctx.toCanonicalNode(child, parentContext)
			if canonical != nil {
				mappedChildren = append(mappedChildren, canonical)
			}
		}

		if !cursor.GoToNextSibling() {
			break
		}
	}

	ctx.putCursor(cursor)

	return mappedChildren
}

// createIncludeUnmappedNode creates a node when IncludeUnmapped is true.
func (ctx *parseContext) createIncludeUnmappedNode(
	root sitter.Node, nodeType string,
	mappedChildren []*node.Node, roles []node.Role,
) *node.Node {
	uastNode := ctx.alloc.NewNode(
		"", node.Type(ctx.language+":"+nodeType),
		ctx.tokenText(root), roles, ctx.extractPositions(root), nil,
	)
	uastNode.Children = mappedChildren

	return uastNode
}

// tokenText returns the string token for a node, if it is a leaf.
func (ctx *parseContext) tokenText(root sitter.Node) string {
	if root.ChildCount() == 0 {
		return ctx.extractNodeText(root)
	}

	return ""
}

// createSyntheticNode creates a synthetic node for multiple children.
// All position fields use uint. Children without positions are skipped
// for span computation.
// If only one child is passed, it is returned as-is.
// If no position can be computed, pos is left nil.
func (ctx *parseContext) createSyntheticNode(mappedChildren []*node.Node) *node.Node {
	if len(mappedChildren) == 1 {
		return mappedChildren[0]
	}

	if len(mappedChildren) == 0 {
		return nil
	}

	pos := ctx.computeChildrenSpan(mappedChildren)

	synth := ctx.alloc.NewNode("", "Synthetic", "", nil, pos, nil)
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

func (b *positionBounds) toPositions(alloc *node.Allocator) *node.Positions {
	if !b.found {
		return nil
	}

	return alloc.NewPositions(
		b.minStartLine,
		b.minStartCol,
		b.minStartOffset,
		b.maxEndLine,
		b.maxEndCol,
		b.maxEndOffset,
	)
}

// computeChildrenSpan computes the bounding span across children positions.
func (ctx *parseContext) computeChildrenSpan(children []*node.Node) *node.Positions {
	bounds := newPositionBounds()

	for _, child := range children {
		if child.Pos != nil {
			bounds.update(child.Pos)
		}
	}

	return bounds.toPositions(ctx.alloc)
}

// extractNodeText extracts text from a Tree-sitter node (allocating copy).
// Use for values that are stored in Node.Token or Node.Props.
// Short strings (≤ maxInternLen) are interned within the current parse to
// deduplicate repeated identifiers, keywords, and operators.
func (ctx *parseContext) extractNodeText(tsNode sitter.Node) string {
	start := tsNode.StartByte()
	end := tsNode.EndByte()

	if safeconv.MustUintToInt(end) <= len(ctx.source) {
		s := string(ctx.source[start:end])

		if len(s) <= maxInternLen && ctx.interner != nil {
			if interned, ok := ctx.interner[s]; ok {
				return interned
			}

			ctx.interner[s] = s
		}

		return s
	}

	return ""
}

// unsafeNodeText returns a zero-copy string view of a Tree-sitter node's text.
// The returned string shares the Source []byte backing array and must NOT be
// stored beyond the current parse call (i.e., not in Node.Token or Node.Props).
// Safe for comparison, filtering, and condition evaluation within Parse().
func (ctx *parseContext) unsafeNodeText(tsNode sitter.Node) string {
	start := tsNode.StartByte()
	end := tsNode.EndByte()

	if end <= uint(len(ctx.source)) {
		return unsafe.String(&ctx.source[start], safeconv.MustUintToInt(end-start))
	}

	return ""
}
