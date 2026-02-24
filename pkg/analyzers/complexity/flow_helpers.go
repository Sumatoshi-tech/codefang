package complexity

import (
	"strings"
	"unicode"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// functionSourceContext holds the original function source and absolute offset,
// allowing us to recover binary operators when UAST mappings omit operator tokens.
type functionSourceContext struct {
	source      []byte
	startOffset uint
}

func newFunctionSourceContext(fn *node.Node) functionSourceContext {
	if fn == nil || fn.Token == "" || fn.Pos == nil {
		return functionSourceContext{}
	}

	return functionSourceContext{
		source:      []byte(fn.Token),
		startOffset: fn.Pos.StartOffset,
	}
}

func (ctx functionSourceContext) binaryOperator(n *node.Node) string {
	if n == nil {
		return ""
	}

	if n.Token != "" {
		if op := normalizeOperatorText(n.Token); op != "" {
			return op
		}
	}

	if n.Props != nil {
		if op := normalizeOperatorText(n.Props["operator"]); op != "" {
			return op
		}
	}

	return ctx.binaryOperatorFromOffsets(n)
}

func (ctx functionSourceContext) binaryOperatorFromOffsets(n *node.Node) string {
	if len(ctx.source) == 0 || len(n.Children) < 2 {
		return ""
	}

	left, right := n.Children[0], n.Children[1]

	if left == nil || right == nil || left.Pos == nil || right.Pos == nil {
		return ""
	}

	if right.Pos.StartOffset <= left.Pos.EndOffset || left.Pos.EndOffset < ctx.startOffset {
		return ""
	}

	start, end := int(left.Pos.EndOffset-ctx.startOffset), int(right.Pos.StartOffset-ctx.startOffset)

	if start < 0 || end < 0 || start >= end || end > len(ctx.source) {
		return ""
	}

	segment := string(ctx.source[start:end])

	return normalizeOperatorText(segment)
}

func normalizeOperatorText(raw string) string {
	if raw == "" {
		return ""
	}

	trimmed := strings.TrimSpace(raw)
	compact := strings.Map(func(r rune) rune {
		if unicode.IsSpace(r) {
			return -1
		}

		return r
	}, trimmed)

	switch compact {
	case "&&", "||", "and", "or", "AND", "OR",
		"<", ">", "<=", ">=", "==", "!=":
		return compact
	default:
		return ""
	}
}

func isLogicalOperatorToken(operator string) bool {
	switch operator {
	case "&&", "||", "and", "or", "AND", "OR":
		return true
	default:
		return false
	}
}

func isElseIfNode(parent, child *node.Node, childIdx int) bool {
	if parent == nil || child == nil {
		return false
	}

	return parent.Type == node.UASTIf && child.Type == node.UASTIf && childIdx >= 2
}

func isDefaultCase(caseNode *node.Node) bool {
	if caseNode == nil {
		return false
	}

	token := strings.TrimSpace(strings.ToLower(caseNode.Token))
	if token == "" {
		return false
	}

	return strings.HasPrefix(token, "default")
}
