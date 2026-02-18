package observability_test

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// blockedAttrPrefixes are attribute key prefixes that must never appear.
var blockedAttrPrefixes = []string{
	"user.",
	"user_",
	"password",
	"token",
	"secret",
	"credential",
}

// blockedAttrKeys are exact attribute keys that must never appear.
var blockedAttrKeys = map[string]bool{
	"email":         true,
	"request.body":  true,
	"response.body": true,
	"user_id":       true,
	"user_email":    true,
}

// TestTelemetryLint_NoHighCardinalityAttributes scans Go source files for
// attribute.String/Int/Bool/Float64 calls with blocked key patterns.
func TestTelemetryLint_NoHighCardinalityAttributes(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)
	fset := token.NewFileSet()

	var violations []string

	err := filepath.Walk(root, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}

		// Skip vendor, third_party, testdata, and hidden directories.
		if info.IsDir() {
			base := filepath.Base(path)
			if base == "vendor" || base == "third_party" || base == ".git" || base == "testdata" {
				return filepath.SkipDir
			}

			return nil
		}

		// Only scan .go files, skip tests.
		if !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}

		file, parseErr := parser.ParseFile(fset, path, nil, 0)
		if parseErr != nil {
			return fmt.Errorf("parse %s: %w", path, parseErr)
		}

		ast.Inspect(file, func(n ast.Node) bool {
			call, ok := n.(*ast.CallExpr)
			if !ok {
				return true
			}

			sel, ok := call.Fun.(*ast.SelectorExpr)
			if !ok {
				return true
			}

			// Match attribute.String, attribute.Int, attribute.Bool, attribute.Float64.
			ident, ok := sel.X.(*ast.Ident)
			if !ok {
				return true
			}

			if ident.Name != "attribute" {
				return true
			}

			switch sel.Sel.Name {
			case "String", "Int", "Bool", "Float64":
			default:
				return true
			}

			if len(call.Args) == 0 {
				return true
			}

			lit, ok := call.Args[0].(*ast.BasicLit)
			if !ok || lit.Kind != token.STRING {
				return true
			}

			// Remove quotes from the string literal.
			key := strings.Trim(lit.Value, "\"")

			if isBlockedKey(key) {
				pos := fset.Position(lit.Pos())

				rel, relErr := filepath.Rel(root, pos.Filename)
				if relErr != nil {
					rel = pos.Filename
				}

				violations = append(violations, rel+":"+key)
			}

			return true
		})

		return nil
	})

	require.NoError(t, err)
	assert.Empty(t, violations, "found high-cardinality or PII attribute keys: %v", violations)
}

func isBlockedKey(key string) bool {
	lower := strings.ToLower(key)

	if blockedAttrKeys[lower] {
		return true
	}

	for _, prefix := range blockedAttrPrefixes {
		if strings.HasPrefix(lower, prefix) {
			return true
		}
	}

	return false
}

// projectRoot walks up from the current file to find the go.mod root.
func projectRoot(t *testing.T) string {
	t.Helper()

	dir, err := os.Getwd()
	require.NoError(t, err)

	for {
		_, statErr := os.Stat(filepath.Join(dir, "go.mod"))
		if statErr == nil {
			return dir
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("could not find project root (go.mod)")
		}

		dir = parent
	}
}
