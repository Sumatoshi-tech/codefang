//go:build e2e

// Package e2e_test contains end-to-end acceptance tests for codefang features.
//
// Tests are organized by feature spec — one file per spec or feature area.
// They exercise real analysis on real source files and assert the output
// contract. New specs add new *_test.go files; shared infrastructure lives
// in helpers_test.go.
//
// Build tag: e2e (excluded from `go test ./...` by default).
//
// Run all e2e tests:
//
//	make test-e2e
//
// Run a specific feature:
//
//	make test-e2e RUN=TestPerFile
package e2e_test

import (
	"os"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
)

func TestMain(m *testing.M) {
	renderer.RegisterPlotRenderer()
	devs.RegisterDevPlotSections()
	couples.RegisterPlotSections()
	os.Exit(m.Run())
}
