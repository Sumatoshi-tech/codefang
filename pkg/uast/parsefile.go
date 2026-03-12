package uast

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/iosafety"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// ParseFile reads a source file from disk and returns its UAST.
// If lang is non-empty, it overrides language detection derived from the file extension.
func (parser *Parser) ParseFile(ctx context.Context, path, lang string) (*node.Node, error) {
	code, resolvedPath, err := iosafety.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}

	filename := resolvedPath
	if lang != "" {
		ext := filepath.Ext(resolvedPath)
		filename = strings.TrimSuffix(resolvedPath, ext) + "." + lang
	}

	parsed, parseErr := parser.Parse(ctx, filename, code)
	if parseErr != nil {
		return nil, fmt.Errorf("parse %s: %w", path, parseErr)
	}

	return parsed, nil
}
