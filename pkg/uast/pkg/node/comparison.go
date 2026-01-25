package node

import "fmt"

func tokensCompare(left, right []*Node, op string) bool {
	if len(left) == 0 || len(right) == 0 {
		return false
	}
	leftToken, rightToken := left[0].Token, right[0].Token
	leftFloat, leftErr := parseFloat(leftToken)
	rightFloat, rightErr := parseFloat(rightToken)

	if leftErr == nil && rightErr == nil {
		return compareFloatWithOp(leftFloat, rightFloat, op)
	}
	return compareStringWithOp(leftToken, rightToken, op)
}

func parseFloat(s string) (float64, error) {
	var f float64
	_, err := fmt.Sscanf(s, "%f", &f)
	return f, err
}

func compareFloatWithOp(left, right float64, op string) bool {
	switch op {
	case ">":
		return left > right
	case ">=":
		return left >= right
	case "<":
		return left < right
	case "<=":
		return left <= right
	default:
		return false
	}
}

func compareStringWithOp(left, right string, op string) bool {
	switch op {
	case ">":
		return left > right
	case ">=":
		return left >= right
	case "<":
		return left < right
	case "<=":
		return left <= right
	default:
		return false
	}
}
