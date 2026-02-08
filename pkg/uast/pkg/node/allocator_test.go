package node //nolint:testpackage // Tests need access to internal types.

import "testing"

func TestAllocator_GetNode_ReturnsNonNil(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	n := alloc.GetNode()

	if n == nil {
		t.Fatal("GetNode returned nil")
	}

	if n.Type != "" {
		t.Errorf("expected empty Type, got %q", n.Type)
	}

	if n.Token != "" {
		t.Errorf("expected empty Token, got %q", n.Token)
	}
}

func TestAllocator_PutNode_Reuses(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	n := alloc.GetNode()
	n.Type = "Function"
	n.Token = "hello"

	alloc.PutNode(n)

	reused := alloc.GetNode()

	if reused != n {
		t.Fatal("expected PutNode'd node to be reused by GetNode")
	}

	if reused.Type != "" {
		t.Errorf("expected cleared Type, got %q", reused.Type)
	}

	if reused.Token != "" {
		t.Errorf("expected cleared Token, got %q", reused.Token)
	}
}

func TestAllocator_GetPositions_ReturnsNonNil(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	p := alloc.GetPositions()

	if p == nil {
		t.Fatal("GetPositions returned nil")
	}

	if p.StartLine != 0 {
		t.Errorf("expected zero StartLine, got %d", p.StartLine)
	}
}

func TestAllocator_PutPositions_Reuses(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	p := alloc.GetPositions()
	p.StartLine = 42

	alloc.PutPositions(p)

	reused := alloc.GetPositions()

	if reused != p {
		t.Fatal("expected PutPositions'd position to be reused")
	}

	if reused.StartLine != 0 {
		t.Errorf("expected cleared StartLine, got %d", reused.StartLine)
	}
}

func TestAllocator_NewNode_FieldsSet(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	roles := []Role{"Declaration"}
	pos := alloc.NewPositions(1, 1, 0, 5, 1, 40)
	n := alloc.NewNode("", "Function", "hello", roles, pos, nil)

	if n.Type != "Function" {
		t.Errorf("expected Type=Function, got %q", n.Type)
	}

	if n.Token != "hello" {
		t.Errorf("expected Token=hello, got %q", n.Token)
	}

	if n.Pos != pos {
		t.Error("expected Pos to be the provided position")
	}

	if len(n.Roles) != 1 || n.Roles[0] != "Declaration" {
		t.Errorf("unexpected roles: %v", n.Roles)
	}
}

func TestAllocator_NewPositions_FieldsSet(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}
	p := alloc.NewPositions(10, 5, 100, 20, 1, 200)

	if p.StartLine != 10 {
		t.Errorf("expected StartLine=10, got %d", p.StartLine)
	}

	if p.EndOffset != 200 {
		t.Errorf("expected EndOffset=200, got %d", p.EndOffset)
	}
}

func TestAllocator_ReleaseTree(t *testing.T) {
	t.Parallel()

	alloc := &Allocator{}

	child := alloc.NewNode("", "Identifier", "x", nil, alloc.NewPositions(2, 1, 10, 2, 2, 11), nil)
	parent := alloc.NewNode("", "Function", "f", nil, alloc.NewPositions(1, 1, 0, 3, 1, 20), nil)
	parent.Children = []*Node{child}

	alloc.ReleaseTree(parent)

	if len(alloc.nodes) != 2 {
		t.Errorf("expected 2 nodes in free list, got %d", len(alloc.nodes))
	}

	if len(alloc.pos) != 2 {
		t.Errorf("expected 2 positions in free list, got %d", len(alloc.pos))
	}
}
