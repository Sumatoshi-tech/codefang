package node

import (
	"fmt"
	"reflect"
	"strings"
	"testing"
)

func TestNodeEdgeCases(t *testing.T) {
	n := &Node{}
	if n.Pos != nil {
		t.Errorf("Default Pos should be nil")
	}
	if len(n.Roles) != 0 {
		t.Errorf("Default Roles should be empty")
	}
	if len(n.Props) != 0 {
		t.Errorf("Default Props should be empty")
	}
	if len(n.Children) != 0 {
		t.Errorf("Default Children should be empty")
	}
}

func makeTestTree() *Node {
	//      root
	//     / |  \
	//   c1 c2  c3
	//  /      /  \
	// gc1   gc2 gc3
	root := &Node{ID: "1", Type: "Root"}
	c1 := &Node{ID: "2", Type: "Child", Token: "c1"}
	c2 := &Node{ID: "3", Type: "Child", Token: "c2"}
	c3 := &Node{ID: "4", Type: "Child", Token: "c3"}
	gc1 := &Node{ID: "5", Type: "Grandchild", Token: "gc1"}
	gc2 := &Node{ID: "6", Type: "Grandchild", Token: "gc2"}
	gc3 := &Node{ID: "7", Type: "Grandchild", Token: "gc3"}
	c1.Children = []*Node{gc1}
	c3.Children = []*Node{gc2, gc3}
	root.Children = []*Node{c1, c2, c3}
	return root
}

func TestNodeFind(t *testing.T) {
	tree := makeTestTree()
	tests := []struct {
		name      string
		predicate func(*Node) bool
		expectIDs []string
	}{
		{"Find all", func(n *Node) bool { return true }, []string{"1", "2", "5", "3", "4", "6", "7"}},
		{"Find children", func(n *Node) bool { return n.Type == "Child" }, []string{"2", "3", "4"}},
		{"Find none", func(n *Node) bool { return false }, nil},
		{"Find leaf", func(n *Node) bool { return n.Token == "gc2" }, []string{"6"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got []string
			for _, n := range tree.Find(tt.predicate) {
				got = append(got, n.ID)
			}
			if !reflect.DeepEqual(got, tt.expectIDs) {
				t.Errorf("Find: got %v, want %v", got, tt.expectIDs)
			}
		})
	}
}

func TestNodePreOrder(t *testing.T) {
	tree := makeTestTree()
	var got []string
	tree.VisitPreOrder(func(n *Node) { got = append(got, n.ID) })
	want := []string{"1", "2", "5", "3", "4", "6", "7"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("PreOrder: got %v, want %v", got, want)
	}
}

func TestNodePostOrder(t *testing.T) {
	tree := makeTestTree()
	var got []string
	tree.VisitPostOrder(func(n *Node) { got = append(got, n.ID) })
	want := []string{"5", "2", "3", "6", "7", "4", "1"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("PostOrder: got %v, want %v", got, want)
	}
}

func TestNodeAncestors(t *testing.T) {
	tree := makeTestTree()
	gc2 := tree.Children[2].Children[0] // gc2
	anc := tree.Ancestors(gc2)
	got := make([]string, 0, len(anc))
	for _, n := range anc {
		got = append(got, n.ID)
	}
	want := []string{"1", "4"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Ancestors: got %v, want %v", got, want)
	}
	// Not found
	fake := &Node{ID: "999"}
	anc = tree.Ancestors(fake)
	if len(anc) != 0 {
		t.Errorf("Ancestors: expected empty for not found, got %v", anc)
	}
}

func TestNodeTransform(t *testing.T) {
	tree := makeTestTree()
	newTree := tree.Transform(func(n *Node) *Node {
		copy := *n
		copy.Type = "X" + n.Type
		return &copy
	})
	if newTree.Type != "XRoot" || newTree.Children[0].Type != "XChild" {
		t.Errorf("Transform: did not apply transformation correctly")
	}
	if newTree == tree || newTree.Children[0] == tree.Children[0] {
		t.Errorf("Transform: did not deep copy nodes")
	}
}

func TestNodeReplaceRemoveChild(t *testing.T) {
	parent := &Node{Type: "P"}
	c1 := &Node{Type: "C1"}
	c2 := &Node{Type: "C2"}
	parent.Children = []*Node{c1, c2}
	c3 := &Node{Type: "C3"}
	ok := parent.ReplaceChild(c2, c3)
	if !ok || parent.Children[1] != c3 {
		t.Errorf("ReplaceChild failed")
	}
	ok = parent.RemoveChild(c1)
	if !ok || len(parent.Children) != 1 || parent.Children[0] != c3 {
		t.Errorf("RemoveChild failed")
	}
	ok = parent.RemoveChild(&Node{Type: "X"})
	if ok {
		t.Errorf("RemoveChild should fail for non-existent child")
	}
}

func TestNodeNavigationEdgeCases(t *testing.T) {
	empty := &Node{}
	if len(empty.Find(func(*Node) bool { return true })) != 1 {
		t.Errorf("Find on single node should return itself")
	}
	var called int
	empty.VisitPreOrder(func(*Node) { called++ })
	if called != 1 {
		t.Errorf("PreOrder on single node should call once")
	}
	called = 0
	empty.VisitPostOrder(func(*Node) { called++ })
	if called != 1 {
		t.Errorf("PostOrder on single node should call once")
	}
	anc := empty.Ancestors(&Node{ID: "999"})
	if len(anc) != 0 {
		t.Errorf("Ancestors on single node should be empty")
	}
}

func TestFindDSL_BasicAndMembership(t *testing.T) {
	tree := &Node{
		Type: "Root",
		Children: []*Node{
			{Type: "Function", Roles: []Role{"Declaration"}, Token: "foo"},
			{Type: "Function", Roles: []Role{"Private"}, Token: "bar"},
			{Type: "String", Token: "hello"},
		},
	}
	tests := []struct {
		name  string
		query string
		want  []string // expected tokens
	}{
		{"all exported functions", "filter(.type == \"Function\" && .roles has \"Declaration\")", []string{"foo"}},
		{"all functions", "filter(.type == \"Function\")", []string{"foo", "bar"}},
		{"all strings", "filter(.type == \"String\")", []string{"hello"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := tree.FindDSL(tt.query)
			if err != nil {
				t.Fatalf("FindDSL error: %v", err)
			}
			var tokens []string
			for _, n := range got {
				tokens = append(tokens, n.Token)
			}
			if !reflect.DeepEqual(tokens, tt.want) {
				t.Errorf("FindDSL(%q) = %v, want %v", tt.query, tokens, tt.want)
			}
		})
	}
	// Add a minimal test for membership parsing
	t.Run("membership parsing", func(t *testing.T) {
		query := ".roles has \"Declaration\""
		_, err := ParseDSL(query)
		if err != nil {
			t.Fatalf("ParseDSL error: %v", err)
		}
	})
}

func TestPreOrder_Stream(t *testing.T) {
	root := &Node{Type: "Root"}
	child1 := &Node{Type: "A"}
	child2 := &Node{Type: "B"}
	root.Children = []*Node{child1, child2}
	got := make([]string, 0, 3)
	for n := range root.PreOrder() {
		got = append(got, string(n.Type))
	}
	want := []string{"Root", "A", "B"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("PreOrder = %v, want %v", got, want)
	}
}

func TestPreOrder_Comprehensive(t *testing.T) {
	t.Run("nil root", func(t *testing.T) {
		var root *Node
		count := 0
		for range root.PreOrder() {
			count++
		}
		if count != 0 {
			t.Errorf("expected 0 nodes for nil root, got %d", count)
		}
	})

	t.Run("empty tree", func(t *testing.T) {
		root := &Node{}
		var got []*Node
		for n := range root.PreOrder() {
			got = append(got, n)
		}
		if len(got) != 1 || got[0] != root {
			t.Errorf("expected only root node, got %v", got)
		}
	})

	t.Run("single node", func(t *testing.T) {
		root := &Node{Type: "A"}
		var got []string
		for n := range root.PreOrder() {
			got = append(got, string(n.Type))
		}
		if len(got) != 1 || got[0] != "A" {
			t.Errorf("expected [A], got %v", got)
		}
	})

	t.Run("multi-level tree", func(t *testing.T) {
		root := &Node{Type: "Root"}
		c1 := &Node{Type: "C1"}
		c2 := &Node{Type: "C2"}
		gc1 := &Node{Type: "GC1"}
		gc2 := &Node{Type: "GC2"}
		c1.Children = []*Node{gc1}
		c2.Children = []*Node{gc2}
		root.Children = []*Node{c1, c2}
		var got []string
		for n := range root.PreOrder() {
			got = append(got, string(n.Type))
		}
		want := []string{"Root", "C1", "GC1", "C2", "GC2"}
		if !reflect.DeepEqual(got, want) {
			t.Errorf("expected %v, got %v", want, got)
		}
	})

	t.Run("deep tree (stack safety)", func(t *testing.T) {
		const depth = 10000
		root := &Node{Type: "root"}
		curr := root
		for i := 0; i < depth; i++ {
			n := &Node{Type: Type(fmt.Sprintf("n%d", i))}
			curr.Children = []*Node{n}
			curr = n
		}
		count := 0
		for range root.PreOrder() {
			count++
		}
		if count != depth+1 {
			t.Errorf("expected %d nodes, got %d", depth+1, count)
		}
	})

	t.Run("mutation during traversal (should not panic)", func(t *testing.T) {
		root := &Node{Type: "root"}
		c1 := &Node{Type: "c1"}
		c2 := &Node{Type: "c2"}
		root.Children = []*Node{c1, c2}
		count := 0
		defer func() {
			if r := recover(); r != nil {
				t.Errorf("panicked during mutation: %v", r)
			}
		}()
		for n := range root.PreOrder() {
			count++
			if n == c1 {
				// Remove c2 during traversal
				root.Children = root.Children[:1]
			}
		}
		if count < 2 {
			t.Errorf("expected at least 2 nodes, got %d", count)
		}
	})
}

func TestHasRole(t *testing.T) {
	n := &Node{Roles: []Role{"Exported", "Test"}}
	if !n.HasAnyRole("Exported") {
		t.Error("HasRole should return true for present role")
	}
	if n.HasAnyRole("Missing") {
		t.Error("HasRole should return false for absent role")
	}

	// Test nil node
	var nilNode *Node
	if nilNode.HasAnyRole("Exported") {
		t.Error("HasRole should return false for nil node")
	}
}

func TestTransform_Mutation(t *testing.T) {
	root := &Node{Type: "Root", Children: []*Node{{Type: "String", Token: "  hello  "}}}
	root.TransformInPlace(func(n *Node) bool {
		if n.Type == "String" {
			n.Token = strings.TrimSpace(n.Token)
		}
		return true
	})
	if got := root.Children[0].Token; got != "hello" {
		t.Errorf("Transform did not mutate string: got %q, want %q", got, "hello")
	}
}

func TestTransform_Comprehensive(t *testing.T) {
	t.Run("empty tree", func(t *testing.T) {
		root := &Node{}
		count := 0
		root.TransformInPlace(func(n *Node) bool {
			count++
			return true
		})
		if count != 1 {
			t.Errorf("expected 1 node, got %d", count)
		}
	})

	t.Run("single node", func(t *testing.T) {
		root := &Node{Token: "a"}
		root.TransformInPlace(func(n *Node) bool {
			n.Token = "b"
			return true
		})
		if root.Token != "b" {
			t.Errorf("expected token 'b', got %q", root.Token)
		}
	})

	t.Run("multi-level tree", func(t *testing.T) {
		root := &Node{Token: "root"}
		c1 := &Node{Token: "c1"}
		c2 := &Node{Token: "c2"}
		gc1 := &Node{Token: "gc1"}
		gc2 := &Node{Token: "gc2"}
		c1.Children = []*Node{gc1}
		c2.Children = []*Node{gc2}
		root.Children = []*Node{c1, c2}
		var tokens []string
		root.TransformInPlace(func(n *Node) bool {
			tokens = append(tokens, n.Token)
			return true
		})
		want := []string{"root", "c1", "gc1", "c2", "gc2"}
		if !reflect.DeepEqual(tokens, want) {
			t.Errorf("expected %v, got %v", want, tokens)
		}
	})

	t.Run("deep tree (stack safety)", func(t *testing.T) {
		const depth = 10000
		root := &Node{Token: "root"}
		curr := root
		for i := 0; i < depth; i++ {
			n := &Node{Token: fmt.Sprintf("n%d", i)}
			curr.Children = []*Node{n}
			curr = n
		}
		count := 0
		root.TransformInPlace(func(n *Node) bool {
			count++
			return true
		})
		if count != depth+1 {
			t.Errorf("expected %d nodes, got %d", depth+1, count)
		}
	})

	t.Run("mutation of children during traversal", func(t *testing.T) {
		root := &Node{Token: "root"}
		c1 := &Node{Token: "c1"}
		c2 := &Node{Token: "c2"}
		root.Children = []*Node{c1, c2}
		count := 0
		root.TransformInPlace(func(n *Node) bool {
			count++
			if n == root {
				n.Children = n.Children[:1] // remove c2 during traversal
			}
			return true
		})
		if count < 2 {
			t.Errorf("expected at least 2 nodes, got %d", count)
		}
	})

	t.Run("skipping children by returning false", func(t *testing.T) {
		root := &Node{Token: "root"}
		c1 := &Node{Token: "c1"}
		c2 := &Node{Token: "c2"}
		gc1 := &Node{Token: "gc1"}
		c1.Children = []*Node{gc1}
		root.Children = []*Node{c1, c2}
		var tokens []string
		root.TransformInPlace(func(n *Node) bool {
			tokens = append(tokens, n.Token)
			if n == c1 {
				return false // skip gc1
			}
			return true
		})
		want := []string{"root", "c1", "c2"}
		if !reflect.DeepEqual(tokens, want) {
			t.Errorf("expected %v, got %v", want, tokens)
		}
	})

	t.Run("mutation during traversal (should not panic)", func(t *testing.T) {
		root := &Node{Token: "root"}
		c1 := &Node{Token: "c1"}
		c2 := &Node{Token: "c2"}
		root.Children = []*Node{c1, c2}
		defer func() {
			if r := recover(); r != nil {
				t.Errorf("panicked during mutation: %v", r)
			}
		}()
		root.TransformInPlace(func(n *Node) bool {
			if n == c1 {
				root.Children = root.Children[:1]
			}
			return true
		})
	})
}

func TestNode_FindDSL(t *testing.T) {
	tree := &Node{
		Type: "file",
		Children: []*Node{
			{
				Type:  "function",
				Token: "func add",
				Props: map[string]string{"name": "add"},
				Children: []*Node{
					{Type: "param", Token: "a"},
					{Type: "param", Token: "b"},
				},
			},
			{
				Type:  "function",
				Token: "func sub",
				Props: map[string]string{"name": "sub"},
				Children: []*Node{
					{Type: "param", Token: "x"},
					{Type: "param", Token: "y"},
				},
			},
			{
				Type:  "var",
				Token: "z",
			},
		},
	}

	tests := []struct {
		name    string
		query   string
		want    []string // expected tokens of result nodes
		wantErr bool
	}{
		{
			name:  "map children",
			query: "map(.children)",
			want:  []string{"func add", "func sub", "z"},
		},
		{
			name:  "filter functions",
			query: "map(.children) |> filter(.type == \"function\")",
			want:  []string{"func add", "func sub"},
		},
		{
			name:  "reduce count",
			query: "map(.children) |> filter(.type == \"function\") |> reduce(count)",
			want:  []string{"2"}, // reduce returns a node with Token = count as string
		},
		{
			name:  "field access",
			query: ".token",
			want:  []string{""}, // root node token is empty
		},
		{
			name:  "literal",
			query: "42",
			want:  []string{"42"},
		},
		{
			name:  "composition",
			query: "map(.children) |> filter(.type == \"var\")",
			want:  []string{"z"},
		},
		{
			name:    "invalid syntax",
			query:   "@#$",
			wantErr: true,
		},
		{
			name:  "unknown field",
			query: "map(.unknown)",
			want:  []string{}, // should not panic, just empty result
		},
		{
			name:    "empty query",
			query:   "",
			wantErr: true,
		},
		{
			name:  "no matches",
			query: "map(.children) |> filter(.type == \"notfound\")",
			want:  []string{},
		},
		{
			name:  "deeply nested",
			query: "map(.children) |> map(.children) |> filter(.type == \"param\")",
			want:  []string{"a", "b", "x", "y"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := tree.FindDSL(tt.query)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			var gotTokens []string
			for _, n := range got {
				gotTokens = append(gotTokens, n.Token)
			}
			if len(gotTokens) != len(tt.want) {
				t.Fatalf("got %v nodes, want %v: %v", len(gotTokens), tt.want, gotTokens)
			}
			for i, wantTok := range tt.want {
				if gotTokens[i] != wantTok {
					t.Errorf("result[%d] = %q, want %q", i, gotTokens[i], wantTok)
				}
			}
		})
	}
}

func TestNode_FindDSL_ComplexRFilterMap(t *testing.T) {
	// Deeply nested tree:
	// root
	// ├── A (a1)
	// │   └── B (b1)
	// │       └── C (c1)
	// │           └── D (d1)
	// └── A (a2)
	//     └── B (b2)
	//         └── C (c2)
	//             └── D (d2)
	tree := &Node{
		Type: "root",
		Children: []*Node{
			{
				Type:  "A",
				Token: "a1",
				Children: []*Node{
					{
						Type:  "B",
						Token: "b1",
						Children: []*Node{
							{
								Type:  "C",
								Token: "c1",
								Children: []*Node{
									{
										Type:  "D",
										Token: "d1",
									},
								},
							},
						},
					},
				},
			},
			{
				Type:  "A",
				Token: "a2",
				Children: []*Node{
					{
						Type:  "B",
						Token: "b2",
						Children: []*Node{
							{
								Type:  "C",
								Token: "c2",
								Children: []*Node{
									{
										Type:  "D",
										Token: "d2",
									},
								},
							},
						},
					},
				},
			},
		},
	}

	tests := []struct {
		name  string
		query string
		want  []string
	}{
		{
			name:  "rfilter D nodes and map token",
			query: "rfilter(.type == \"D\") |> map(.token)",
			want:  []string{"d1", "d2"},
		},
		{
			name:  "rfilter C or D and map token",
			query: "rfilter(.type == \"C\" || .type == \"D\") |> map(.token)",
			want:  []string{"c1", "d1", "c2", "d2"},
		},
		{
			name:  "rfilter not A and map type",
			query: "rfilter(!(.type == \"A\")) |> map(.type)",
			want:  []string{"B", "C", "D", "B", "C", "D"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := tree.FindDSL(tt.query)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			var gotTokens []string
			for _, n := range got {
				gotTokens = append(gotTokens, n.Token)
			}
			if len(gotTokens) != len(tt.want) {
				t.Fatalf("got %d nodes, want %d: %v", len(gotTokens), len(tt.want), gotTokens)
			}
			for i, wantTok := range tt.want {
				if gotTokens[i] != wantTok {
					t.Errorf("result[%d] = %q, want %q", i, gotTokens[i], wantTok)
				}
			}
		})
	}
}

func TestHasRole_Comprehensive(t *testing.T) {
	t.Run("no roles", func(t *testing.T) {
		n := &Node{}
		if n.HasAnyRole("Exported") {
			t.Errorf("expected false for node with no roles")
		}
	})

	t.Run("one role, present", func(t *testing.T) {
		n := &Node{Roles: []Role{"Exported"}}
		if !n.HasAnyRole("Exported") {
			t.Errorf("expected true for present role")
		}
	})

	t.Run("one role, not present", func(t *testing.T) {
		n := &Node{Roles: []Role{"Test"}}
		if n.HasAnyRole("Exported") {
			t.Errorf("expected false for absent role")
		}
	})

	t.Run("multiple roles, present", func(t *testing.T) {
		n := &Node{Roles: []Role{"Exported", "Test"}}
		if !n.HasAnyRole("Test") {
			t.Errorf("expected true for present role")
		}
	})

	t.Run("multiple roles, not present", func(t *testing.T) {
		n := &Node{Roles: []Role{"Exported", "Test"}}
		if n.HasAnyRole("Private") {
			t.Errorf("expected false for absent role")
		}
	})

	t.Run("empty role string", func(t *testing.T) {
		n := &Node{Roles: []Role{"Exported"}}
		if n.HasAnyRole("") {
			t.Errorf("expected false for empty role string")
		}
	})

	t.Run("mutation during check (should not panic)", func(t *testing.T) {
		n := &Node{Roles: []Role{"Exported", "Test"}}
		defer func() {
			if r := recover(); r != nil {
				t.Errorf("panicked during mutation: %v", r)
			}
		}()
		count := 0
		for i := range n.Roles {
			if n.HasAnyRole(n.Roles[i]) {
				count++
				n.Roles = n.Roles[:1] // mutate during check
				break                 // avoid out-of-bounds after mutation
			}
		}
		if count == 0 {
			t.Errorf("expected at least one true result before mutation")
		}
	})
}

func TestDSLMapFilterPipeline(t *testing.T) {
	root := &Node{
		Type: "File",
		Children: []*Node{
			{
				Type:  "Function",
				Token: "Hello",
				Roles: []Role{"Function", "Declaration"},
			},
			{
				Type:  "Function",
				Token: "World",
				Roles: []Role{"Function", "Declaration"},
			},
		},
	}
	query := `filter(.type == "Function") |> map(.token)`
	results, err := root.FindDSL(query)
	if err != nil {
		t.Fatalf("Query failed: %v", err)
	}
	if len(results) != 2 {
		t.Errorf("Expected 2 results, got %d", len(results))
	}
	if results[0].Token != "Hello" || results[1].Token != "World" {
		t.Errorf("Unexpected tokens: %v, %v", results[0].Token, results[1].Token)
	}
}

func TestDSLMapChildren(t *testing.T) {
	root := &Node{
		Type: "File",
		Children: []*Node{
			{
				Type:  "Function",
				Token: "Hello",
				Roles: []Role{"Function", "Declaration"},
			},
			{
				Type:  "Function",
				Token: "World",
				Roles: []Role{"Function", "Declaration"},
			},
		},
	}
	query := `map(.children)`
	results, err := root.FindDSL(query)
	if err != nil {
		t.Fatalf("Query failed: %v", err)
	}
	if len(results) != 2 {
		t.Errorf("Expected 2 results, got %d", len(results))
	}
	if results[0].Token != "Hello" || results[1].Token != "World" {
		t.Errorf("Unexpected tokens: %v, %v", results[0].Token, results[1].Token)
	}
}
