package mapping

import (
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
	"sync"
)

// Sentinel errors for DSL parsing.
var (
	errInvalidASTRoot    = errors.New("invalid AST root type")
	errNoLangDeclaration = errors.New("no language declaration found")
	errInvalidLangFormat = errors.New("invalid language declaration format")
	errInvalidRule       = errors.New("invalid mapping rule")
	errNoRules           = errors.New("no mapping rules found in DSL")
	errMissingUASTFields = errors.New("missing UAST field list")
)

// Minimum number of fields required when parsing an "extends" declaration.
const minExtendsFields = 3

// LanguageInfo represents language declaration information from mapping files.
type LanguageInfo struct {
	Name       string   `json:"name"`
	Extensions []string `json:"extensions"`
	Files      []string `json:"files"`
}

// extractLanguageDeclarationFromAST extracts language declaration from the parsed AST.
func extractLanguageDeclarationFromAST(ast any) (*LanguageInfo, error) {
	var langInfo *LanguageInfo

	var walk func(parseNode *node32)

	walk = func(parseNode *node32) {
		if parseNode == nil {
			return
		}

		// Look for language declaration nodes.
		if parseNode.pegRule == ruleLanguageDeclaration {
			info, extractErr := extractLanguageDeclaration(parseNode)
			if extractErr == nil {
				langInfo = info
			}

			return
		}

		// Continue walking the tree.
		for child := parseNode.up; child != nil; child = child.next {
			walk(child)
		}
	}

	switch typedAST := ast.(type) {
	case *node32:
		walk(typedAST)
	case []*node32:
		for _, child := range typedAST {
			walk(child)
		}
	default:
		return nil, fmt.Errorf("%w: %T", errInvalidASTRoot, ast)
	}

	if langInfo == nil {
		return nil, errNoLangDeclaration
	}

	return langInfo, nil
}

// extractLanguageDeclaration extracts language info from a language declaration node.
func extractLanguageDeclaration(parseNode *node32) (*LanguageInfo, error) {
	// The language declaration should have the structure:
	// [language "name", extensions: ".ext1", ".ext2"].
	text := extractText(parseNode)

	// Find the language name (between quotes after "language").
	langStart := strings.Index(text, `language "`)
	if langStart == -1 {
		return nil, errInvalidLangFormat
	}

	langStart += len(`language "`)

	langEnd := strings.Index(text[langStart:], `"`)
	if langEnd == -1 {
		return nil, errInvalidLangFormat
	}

	languageName := text[langStart : langStart+langEnd]

	// Check for extensions and files sections.
	extStart := strings.Index(text, "extensions:")
	filesStart := strings.Index(text, "files:")

	var extensions []string

	var files []string

	if extStart != -1 {
		// Parse extensions.
		extStart += len("extensions:")
		extText := text[extStart:]

		// Check if there's a files section after extensions.
		if filesStart != -1 && filesStart > extStart {
			// Parse extensions up to files section.
			extText = extText[:filesStart-extStart]
			extText = strings.TrimSpace(extText)
			extText = strings.Trim(extText, ",")
		}

		extensions = parseQuotedList(extText)
	}

	if filesStart != -1 {
		// Parse files.
		filesStart += len("files:")
		filesText := text[filesStart:]
		files = parseQuotedList(filesText)
	}

	return &LanguageInfo{
		Name:       languageName,
		Extensions: extensions,
		Files:      files,
	}, nil
}

// quotedListParser holds state for parsing a comma-separated list of quoted strings.
type quotedListParser struct {
	items    []string
	current  strings.Builder
	inQuotes bool
}

func (p *quotedListParser) processChar(ch byte) {
	if ch == '"' || ch == '\'' {
		p.handleQuote()

		return
	}

	if ch == ',' && !p.inQuotes {
		p.flushItem()

		return
	}

	p.current.WriteByte(ch)
}

func (p *quotedListParser) handleQuote() {
	if p.inQuotes {
		p.inQuotes = false
		p.flushItem()
	} else {
		p.inQuotes = true
	}
}

func (p *quotedListParser) flushItem() {
	if item := strings.TrimSpace(p.current.String()); item != "" {
		p.items = append(p.items, item)
	}

	p.current.Reset()
}

// parseQuotedList parses a comma-separated list of quoted strings.
// It handles both single and double quotes, and trims whitespace.
func parseQuotedList(text string) []string {
	text = strings.TrimSpace(text)
	text = strings.Trim(text, "[]")
	text = strings.TrimRight(text, ",")

	parser := &quotedListParser{}

	for idx := range len(text) {
		parser.processChar(text[idx])
	}

	parser.flushItem()

	return parser.items
}

// Parser parses the mapping DSL and returns validated mapping rules.
type Parser struct{}

// ParseMapping parses the mapping DSL input and returns mapping rules.
func (parser *Parser) ParseMapping(reader io.Reader) ([]Rule, *LanguageInfo, error) {
	content, err := io.ReadAll(reader)
	if err != nil {
		return nil, nil, fmt.Errorf("reading input: %w", err)
	}

	input := string(content)
	input = strings.ReplaceAll(input, "\r\n", "\n")
	input = strings.ReplaceAll(input, "\r", "\n")

	// Serialize access to the global nodeTextBuffer used during parsing and AST traversal.
	nodeTextMu.Lock()
	defer nodeTextMu.Unlock()

	ast, parseErr := parseMappingDSL(input)
	if parseErr != nil {
		return nil, nil, parseErr
	}

	rules, buildErr := buildRulesFromAST(ast)
	if buildErr != nil {
		return nil, nil, buildErr
	}

	langInfo, langErr := extractLanguageDeclarationFromAST(ast)
	if langErr != nil {
		return nil, nil, langErr
	}

	return rules, langInfo, nil
}

// parseMappingDSL uses the generated PEG parser to parse the input DSL.
func parseMappingDSL(input string) (any, error) {
	nodeTextBuffer = input

	parser := &MappingDSL{Buffer: input}

	initErr := parser.Init()
	if initErr != nil {
		return nil, fmt.Errorf("mapping DSL init error: %w", initErr)
	}

	parseErr := parser.Parse()
	if parseErr != nil {
		return nil, fmt.Errorf("mapping DSL parse error: %w", parseErr)
	}

	return parser.AST(), nil
}

// buildRulesFromAST converts the PEG AST to []Rule.
func buildRulesFromAST(ast any) ([]Rule, error) {
	var rules []Rule

	var walk func(parseNode *node32)

	walk = func(parseNode *node32) {
		if parseNode == nil {
			return
		}

		if parseNode.pegRule == ruleRule {
			rule, extractErr := extractRule(parseNode)
			if extractErr == nil {
				rules = append(rules, rule)
			}
		}

		// Skip language declarations - they're handled separately.
		if parseNode.pegRule == ruleLanguageDeclaration {
			return
		}

		for child := parseNode.up; child != nil; child = child.next {
			walk(child)
		}
	}

	switch typedAST := ast.(type) {
	case *node32:
		walk(typedAST)
	case []*node32:
		for _, child := range typedAST {
			walk(child)
		}
	default:
		return nil, fmt.Errorf("%w: %T", errInvalidASTRoot, ast)
	}

	if len(rules) == 0 {
		return nil, errNoRules
	}

	return rules, nil
}

// findInheritanceNode searches for an inheritance comment node in or after the rule node.
// It first looks among direct children, then checks the sibling node.
func findInheritanceNode(ruleNode *node32) *node32 {
	for child := ruleNode.up; child != nil; child = child.next {
		if child.pegRule == ruleInheritanceComment {
			return child
		}
	}

	if ruleNode.next != nil && ruleNode.next.pegRule == ruleInheritanceComment {
		return ruleNode.next
	}

	return nil
}

func extractRule(ruleNode *node32) (Rule, error) {
	var rule Rule

	nameNode := findChild(ruleNode, ruleIdentifier)
	patternNode := findChild(ruleNode, rulePattern)
	uastSpecNode := findChild(ruleNode, ruleUASTSpec)
	whenNode := findChild(ruleNode, ruleConditionList)
	inheritanceNode := findInheritanceNode(ruleNode)

	extends := ""

	var inheritanceConditions []Condition

	if inheritanceNode != nil {
		extends, inheritanceConditions = extractInheritanceAndConditions(inheritanceNode)
	}

	if nameNode != nil {
		rule.Name = extractText(nameNode)
	}

	if patternNode != nil {
		rule.Pattern = extractPattern(patternNode)
	}

	if uastSpecNode != nil {
		spec, specErr := extractUASTSpec(uastSpecNode)
		if specErr == nil {
			rule.UASTSpec = spec
		}
	}

	var allConditions []Condition

	if whenNode != nil {
		allConditions = append(allConditions, extractConditions(whenNode)...)
	}

	if len(inheritanceConditions) > 0 {
		allConditions = append(allConditions, inheritanceConditions...)
	}

	rule.Conditions = allConditions
	rule.Extends = extends

	broken := rule.Name == "" || rule.Pattern == "" || rule.UASTSpec.Type == ""
	if broken {
		return rule, errInvalidRule
	}

	return rule, nil
}

func extractConditions(condNode *node32) []Condition {
	var conds []Condition

	for child := condNode.up; child != nil; child = child.next {
		if child.pegRule == ruleCondition {
			conds = append(conds, Condition{Expr: extractText(child)})
		}
	}

	return conds
}

func extractInheritanceAndConditions(inheritanceNode *node32) (string, []Condition) {
	// Format: # Extends base_rule [when field == "val" and other_field != "bad"].
	text := extractText(inheritanceNode)
	trimmed := strings.TrimSpace(text)

	if !strings.HasPrefix(trimmed, "# Extends ") {
		return "", nil
	}

	parts := strings.Fields(trimmed)
	base := ""

	if len(parts) >= minExtendsFields {
		base = parts[2]
	}

	// Look for 'when' and condition expressions.
	_, condExpr, found := strings.Cut(text, "when ")
	if !found {
		return base, nil
	}

	condExpr = strings.TrimSpace(condExpr)
	if condExpr == "" {
		return base, nil
	}

	var conds []Condition

	// Split on 'and' for multiple conditions.
	for condStr := range strings.SplitSeq(condExpr, " and ") {
		condStr = strings.TrimSpace(condStr)
		if condStr != "" {
			conds = append(conds, Condition{Expr: condStr})
		}
	}

	return base, conds
}

func findChild(parseNode *node32, target pegRule) *node32 {
	for child := parseNode.up; child != nil; child = child.next {
		if child.pegRule == target {
			return child
		}
	}

	return nil
}

func extractText(parseNode *node32) string {
	if parseNode == nil {
		return ""
	}

	return string([]rune(nodeTextBuffer)[parseNode.begin:parseNode.end])
}

var (
	nodeTextBuffer string
	nodeTextMu     sync.Mutex
)

func extractPattern(patternNode *node32) string {
	return extractText(patternNode)
}

func extractUASTField(fieldNode *node32) (fname string, fvals []string) {
	for child := fieldNode.up; child != nil; child = child.next {
		switch child.pegRule {
		case ruleUASTFieldName:
			fname = extractText(child)
		case ruleUASTFieldValue:
			fvals = extractFieldValues(child)
		default:
			// Other PEG rules are not relevant for field extraction.
		}
	}

	return fname, fvals
}

func extractFieldValues(valueNode *node32) []string {
	var fvals []string

	for valNode := valueNode.up; valNode != nil; valNode = valNode.next {
		switch valNode.pegRule {
		case ruleCapture, ruleIdentifier:
			fvals = append(fvals, extractText(valNode))
		case ruleString:
			val := extractText(valNode)

			unq, unqErr := strconv.Unquote(val)
			if unqErr == nil {
				val = unq
			}

			fvals = append(fvals, val)
		case ruleMultipleCaptures:
			fvals = append(fvals, extractMultipleCaptures(valNode)...)
		case ruleMultipleStrings:
			fvals = append(fvals, extractMultipleStrings(valNode)...)
		default:
			// Other PEG rules are not relevant for value extraction.
		}
	}

	return fvals
}

func extractMultipleCaptures(multiNode *node32) []string {
	var captures []string

	for valNode := multiNode.up; valNode != nil; valNode = valNode.next {
		if valNode.pegRule == ruleCapture {
			captures = append(captures, extractText(valNode))
		}
	}

	return captures
}

func extractMultipleStrings(multiNode *node32) []string {
	var strs []string

	for valNode := multiNode.up; valNode != nil; valNode = valNode.next {
		if valNode.pegRule == ruleString {
			val := extractText(valNode)

			unq, unqErr := strconv.Unquote(val)
			if unqErr == nil {
				val = unq
			}

			strs = append(strs, val)
		}
	}

	return strs
}

func extractUASTSpec(uastSpecNode *node32) (UASTSpec, error) {
	var spec UASTSpec

	fieldsNode := findChild(uastSpecNode, ruleUASTFields)
	if fieldsNode == nil {
		return spec, errMissingUASTFields
	}

	for entryNode := fieldsNode.up; entryNode != nil; entryNode = entryNode.next {
		if entryNode.pegRule != ruleUASTField {
			continue
		}

		fname, fvals := extractUASTField(entryNode)
		applyUASTField(&spec, fname, fvals)
	}

	return spec, nil
}

func applyUASTField(spec *UASTSpec, fname string, fvals []string) {
	switch fname {
	case "type":
		if len(fvals) > 0 {
			spec.Type = fvals[0]
		}
	case "token":
		if len(fvals) > 0 {
			spec.Token = fvals[0]
		}
	case "roles":
		spec.Roles = append(spec.Roles, fvals...)
	case "children":
		spec.Children = append(spec.Children, fvals...)
	default:
		if spec.Props == nil {
			spec.Props = make(map[string]string)
		}

		if len(fvals) > 0 {
			spec.Props[fname] = fvals[0]
		}
	}
}
