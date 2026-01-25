package mapping

import (
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
)

// LanguageInfo represents language declaration information from mapping files
type LanguageInfo struct {
	Name       string   `json:"name"`
	Extensions []string `json:"extensions"`
	Files      []string `json:"files"`
}

// extractLanguageDeclarationFromAST extracts language declaration from the parsed AST
func extractLanguageDeclarationFromAST(ast interface{}) (*LanguageInfo, error) {
	var langInfo *LanguageInfo

	var walk func(n *node32)
	walk = func(n *node32) {
		if n == nil {
			return
		}

		// Look for language declaration nodes
		if n.pegRule == ruleLanguageDeclaration {
			info, err := extractLanguageDeclaration(n)
			if err == nil {
				langInfo = info
			}
			return
		}

		// Continue walking the tree
		for child := n.up; child != nil; child = child.next {
			walk(child)
		}
	}

	switch n := ast.(type) {
	case *node32:
		walk(n)
	case []*node32:
		for _, child := range n {
			walk(child)
		}
	default:
		return nil, fmt.Errorf("invalid AST root type: %T", ast)
	}

	if langInfo == nil {
		return nil, fmt.Errorf("no language declaration found")
	}

	return langInfo, nil
}

// extractLanguageDeclaration extracts language info from a language declaration node
func extractLanguageDeclaration(node *node32) (*LanguageInfo, error) {
	// The language declaration should have the structure:
	// [language "name", extensions: ".ext1", ".ext2"]

	text := extractText(node)

	// Find the language name (between quotes after "language")
	langStart := strings.Index(text, `language "`)
	if langStart == -1 {
		return nil, fmt.Errorf("invalid language declaration format")
	}

	langStart += len(`language "`)
	langEnd := strings.Index(text[langStart:], `"`)
	if langEnd == -1 {
		return nil, fmt.Errorf("invalid language declaration format")
	}

	languageName := text[langStart : langStart+langEnd]

	// Check for extensions and files sections
	extStart := strings.Index(text, "extensions:")
	filesStart := strings.Index(text, "files:")

	var extensions []string
	var files []string

	if extStart != -1 {
		// Parse extensions
		extStart += len("extensions:")
		extText := text[extStart:]

		// Check if there's a files section after extensions
		if filesStart != -1 && filesStart > extStart {
			// Parse extensions up to files section
			extText = extText[:filesStart-extStart]
			extText = strings.TrimSpace(extText)
			extText = strings.Trim(extText, ",")
		}

		extensions = parseExtensionsFromText(extText)
	}

	if filesStart != -1 {
		// Parse files
		filesStart += len("files:")
		filesText := text[filesStart:]
		files = parseFilesFromText(filesText)
	}

	return &LanguageInfo{
		Name:       languageName,
		Extensions: extensions,
		Files:      files,
	}, nil
}

// parseQuotedList parses a comma-separated list of quoted strings.
// It handles both single and double quotes, and trims whitespace.
func parseQuotedList(text string) []string {
	text = strings.TrimSpace(text)
	text = strings.Trim(text, "[]")
	// Also remove trailing comma if present
	text = strings.TrimRight(text, ",")

	var items []string
	var current strings.Builder
	inQuotes := false
	for i := 0; i < len(text); i++ {
		c := text[i]
		if c == '"' || c == '\'' {
			if inQuotes {
				// End of quoted item
				inQuotes = false
				if current.Len() > 0 {
					item := current.String()
					if item := strings.TrimSpace(item); item != "" {
						items = append(items, item)
					}
					current.Reset()
				}
			} else {
				// Start of quoted item
				inQuotes = true
			}
			continue
		}
		if c == ',' && !inQuotes {
			// End of unquoted item
			if item := strings.TrimSpace(current.String()); item != "" {
				items = append(items, item)
			}
			current.Reset()
			continue
		}
		current.WriteByte(c)
	}
	// Add last item
	if item := strings.TrimSpace(current.String()); item != "" {
		items = append(items, item)
	}
	return items
}

// parseExtensionsFromText parses extensions from the extensions part of language declaration
func parseExtensionsFromText(extText string) []string {
	return parseQuotedList(extText)
}

// parseFilesFromText parses files from the files part of language declaration
func parseFilesFromText(filesText string) []string {
	return parseQuotedList(filesText)
}

// MappingParser parses the mapping DSL and returns validated mapping rules.
type MappingParser struct{}

// ParseMapping parses the mapping DSL input and returns mapping rules.
func (p *MappingParser) ParseMapping(reader io.Reader) ([]MappingRule, *LanguageInfo, error) {
	content, err := io.ReadAll(reader)
	if err != nil {
		return nil, nil, err
	}

	input := string(content)
	input = strings.ReplaceAll(input, "\r\n", "\n")
	input = strings.ReplaceAll(input, "\r", "\n")
	ast, err := parseMappingDSL(input)

	if err != nil {
		return nil, nil, err
	}

	rules, err := buildMappingRulesFromAST(ast)

	if err != nil {
		return nil, nil, err
	}

	langInfo, err := extractLanguageDeclarationFromAST(ast)

	if err != nil {
		return nil, nil, err
	}

	return rules, langInfo, nil
}

// parseMappingDSL uses the generated PEG parser to parse the input DSL.
func parseMappingDSL(input string) (any, error) {
	nodeTextBuffer = input
	parser := &MappingDSL{Buffer: input}
	parser.Init() //nolint:errcheck // Init does not return an error in this PEG parser implementation
	if err := parser.Parse(); err != nil {
		return nil, fmt.Errorf("mapping DSL parse error: %w", err)
	}
	return parser.AST(), nil
}

// buildMappingRulesFromAST converts the PEG AST to []MappingRule.
func buildMappingRulesFromAST(ast interface{}) ([]MappingRule, error) {
	var rules []MappingRule

	var walk func(n *node32)
	walk = func(n *node32) {
		if n == nil {
			return
		}
		if n.pegRule == ruleRule {
			rule, err := extractMappingRule(n)
			if err == nil {
				rules = append(rules, rule)
			}
		}
		// Skip language declarations - they're handled separately
		if n.pegRule == ruleLanguageDeclaration {
			return
		}
		for child := n.up; child != nil; child = child.next {
			walk(child)
		}
	}

	switch n := ast.(type) {
	case *node32:
		walk(n)
	case []*node32:
		for _, child := range n {
			walk(child)
		}
	default:
		fmt.Printf("DEBUG: AST root type: %T, value: %#v\n", ast, ast)
		return nil, fmt.Errorf("invalid AST root type: %T", ast)
	}

	if len(rules) == 0 {
		if n, ok := ast.(*node32); ok && n != nil {
			fmt.Println("DEBUG: AST structure:")
			n.Print(os.Stdout, nodeTextBuffer)
		}
		return nil, fmt.Errorf("no mapping rules found in DSL")
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

func extractMappingRule(ruleNode *node32) (MappingRule, error) {
	var rule MappingRule
	nameNode := findChild(ruleNode, ruleIdentifier)
	patternNode := findChild(ruleNode, rulePattern)
	uastSpecNode := findChild(ruleNode, ruleUASTSpec)
	whenNode := findChild(ruleNode, ruleConditionList)
	inheritanceNode := findInheritanceNode(ruleNode)

	extends := ""
	inheritanceConditions := []Condition{}
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
		spec, err := extractUASTSpec(uastSpecNode)
		if err == nil {
			rule.UASTSpec = spec
		}
	}
	allConditions := []Condition{}
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
		return rule, fmt.Errorf("invalid mapping rule")
	}
	return rule, nil
}

func extractConditions(condNode *node32) []Condition {
	var conds []Condition
	for c := condNode.up; c != nil; c = c.next {
		if c.pegRule == ruleCondition {
			cond := Condition{Expr: extractText(c)}
			conds = append(conds, cond)
		}
	}
	return conds
}

func extractInheritanceAndConditions(inheritanceNode *node32) (string, []Condition) {
	// Format: # Extends base_rule [when field == "val" and other_field != "bad"]
	text := extractText(inheritanceNode)
	base := ""
	conds := []Condition{}
	if strings.HasPrefix(strings.TrimSpace(text), "# Extends ") {
		parts := strings.Fields(strings.TrimSpace(text))
		if len(parts) >= 3 {
			base = parts[2]
		}
		// Look for 'when' and condition expressions
		whenIdx := strings.Index(text, "when ")
		if whenIdx != -1 {
			condExpr := strings.TrimSpace(text[whenIdx+5:])
			if condExpr != "" {
				// Split on 'and' for multiple conditions
				for _, c := range strings.Split(condExpr, " and ") {
					c = strings.TrimSpace(c)
					if c != "" {
						conds = append(conds, Condition{Expr: c})
					}
				}
			}
		}
	}
	return base, conds
}

func findChild(node *node32, target pegRule) *node32 {
	for child := node.up; child != nil; child = child.next {
		if child.pegRule == target {
			return child
		}
	}
	return nil
}

func extractText(node *node32) string {
	if node == nil {
		return ""
	}
	return string([]rune(nodeTextBuffer)[node.begin:node.end])
}

// nodeTextBuffer is set by parseMappingDSL for text extraction
var nodeTextBuffer string

func extractPattern(patternNode *node32) string {
	return extractText(patternNode)
}

func extractUASTField(fieldNode *node32) (string, []string) {
	var fname string
	var fvals []string

	for child := fieldNode.up; child != nil; child = child.next {
		switch child.pegRule {
		case ruleUASTFieldName:
			fname = extractText(child)
		case ruleUASTFieldValue:
			for valNode := child.up; valNode != nil; valNode = valNode.next {
				switch valNode.pegRule {
				case ruleCapture, ruleIdentifier:
					val := extractText(valNode)
					fvals = append(fvals, val)
				case ruleString:
					val := extractText(valNode)
					if unq, err := strconv.Unquote(val); err == nil {
						val = unq
					}
					fvals = append(fvals, val)
				case ruleMultipleCaptures:
					for valNode := valNode.up; valNode != nil; valNode = valNode.next {
						if valNode.pegRule == ruleCapture {
							val := extractText(valNode)
							fvals = append(fvals, val)
						}
					}
				case ruleMultipleStrings:
					for valNode := valNode.up; valNode != nil; valNode = valNode.next {
						if valNode.pegRule == ruleString {
							val := extractText(valNode)
							if unq, err := strconv.Unquote(val); err == nil {
								val = unq
							}
							fvals = append(fvals, val)
						}
					}
				}
			}
		}
	}
	return fname, fvals
}

func extractUASTSpec(uastSpecNode *node32) (UASTSpec, error) {
	var spec UASTSpec
	fieldsNode := findChild(uastSpecNode, ruleUASTFields)
	if fieldsNode == nil {
		return spec, fmt.Errorf("missing UAST field list")
	}
	for entryNode := fieldsNode.up; entryNode != nil; entryNode = entryNode.next {
		if entryNode.pegRule != ruleUASTField {
			continue
		}
		fieldNode := entryNode
		fname, fvals := extractUASTField(fieldNode)
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
		case "props":
			if spec.Props == nil {
				spec.Props = make(map[string]string)
			}
			if len(fvals) > 0 {
				spec.Props[fname] = fvals[0]
			}
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
	return spec, nil
}
