package uast

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// TestParser_DeterministicAcrossParses guards against state leaking through
// the parseContext [sync.Pool] — most notably the shared ctx.batchChildren
// backing array, which previously caused recursive processChildrenBatch
// calls to overwrite outer-loop entries the parent had not yet read.
//
// The input is a Go file whose root and inner blocks each have well over
// the cursorThreshold of named children, exercising both the batch and
// recursive paths. Parsing the same content with the same *Parser must
// produce structurally identical trees on every call.
func TestParser_DeterministicAcrossParses(t *testing.T) {
	t.Parallel()

	src := []byte(`package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"
)

func a() {}
func b() {}
func c() {}
func d() {}
func e() {}
func f() {}
func g() {}
func h() {}
func i() {}
func j() {}

func work(ctx context.Context, w io.Writer) error {
	if ctx == nil {
		return errors.New("nil ctx")
	}

	var mu sync.Mutex
	mu.Lock()
	defer mu.Unlock()

	parts := []string{"a", "b", "c", "d", "e", "f", "g", "h", "i", "j"}
	out := strings.Join(parts, ",")

	for idx, p := range parts {
		if p == "" {
			continue
		}
		fmt.Fprintf(w, "%d:%s\n", idx, p)
	}

	now := time.Now()
	if _, err := fmt.Fprintln(w, out, now); err != nil {
		return err
	}
	if _, err := fmt.Fprintln(os.Stderr, "done"); err != nil {
		return err
	}
	return nil
}
`)

	parser, err := NewParser()
	require.NoError(t, err)

	const runs = 8

	first, err := parser.Parse(context.Background(), "main.go", src)
	require.NoError(t, err)
	require.NotNil(t, first)

	wantNodes := countAllNodes(first)
	wantFuncs := countFunctionNodes(first)
	node.ReleaseTree(first)

	require.Positive(t, wantNodes, "baseline tree must be non-empty")
	require.GreaterOrEqual(t, wantFuncs, 11, "expected at least 11 functions in the fixture")

	for run := 2; run <= runs; run++ {
		tree, parseErr := parser.Parse(context.Background(), "main.go", src)
		require.NoErrorf(t, parseErr, "parse run %d failed", run)
		require.NotNil(t, tree)

		gotNodes := countAllNodes(tree)
		gotFuncs := countFunctionNodes(tree)
		node.ReleaseTree(tree)

		assert.Equalf(t, wantNodes, gotNodes,
			"node count drift on run %d: want %d, got %d (parseContext buffer corruption?)",
			run, wantNodes, gotNodes)
		assert.Equalf(t, wantFuncs, gotFuncs,
			"function count drift on run %d: want %d, got %d (parseContext buffer corruption?)",
			run, wantFuncs, gotFuncs)
	}
}

func countAllNodes(n *node.Node) int {
	if n == nil {
		return 0
	}

	total := 1
	for _, child := range n.Children {
		total += countAllNodes(child)
	}

	return total
}

func countFunctionNodes(n *node.Node) int {
	if n == nil {
		return 0
	}

	count := 0
	if n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration) {
		count = 1
	}

	for _, child := range n.Children {
		count += countFunctionNodes(child)
	}

	return count
}
