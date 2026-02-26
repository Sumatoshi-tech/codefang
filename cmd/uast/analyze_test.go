package main

import (
	"bytes"
	"encoding/json"
	"os"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestAnalyzeNode_BasicStructure(t *testing.T) {
	t.Parallel()

	// Build a small tree:
	//   File
	//   ├── Package
	//   ├── Function (with If inside)
	//   │   └── If
	//   │       └── Call
	//   └── Method
	//       └── Loop
	root := node.NewBuilder().WithType(node.UASTFile).Build()
	pkg := node.NewBuilder().WithType(node.UASTPackage).WithRoles([]node.Role{node.RoleModule}).Build()
	fn := node.NewBuilder().WithType(node.UASTFunction).WithRoles([]node.Role{node.RoleFunction}).Build()
	ifNode := node.NewBuilder().WithType(node.UASTIf).WithRoles([]node.Role{node.RoleBranch}).Build()
	call := node.NewBuilder().WithType(node.UASTCall).WithRoles([]node.Role{node.RoleCall}).Build()
	method := node.NewBuilder().WithType(node.UASTMethod).WithRoles([]node.Role{node.RoleFunction}).Build()
	loop := node.NewBuilder().WithType(node.UASTLoop).WithRoles([]node.Role{node.RoleLoop}).Build()

	ifNode.AddChild(call)
	fn.AddChild(ifNode)
	method.AddChild(loop)
	root.AddChild(pkg)
	root.AddChild(fn)
	root.AddChild(method)

	result := analyzeNode(root, "test.go")

	// Total: root + pkg + fn + if + call + method + loop = 7 nodes.
	got, ok := result["total_nodes"].(int)
	if !ok || got != 7 {
		t.Errorf("total_nodes = %v, want 7", result["total_nodes"])
	}

	// Leaf count: pkg, call, loop = 3 leaf nodes.
	gotLeaf, ok := result["leaf_nodes"].(int)
	if !ok || gotLeaf != 3 {
		t.Errorf("leaf_nodes = %v, want 3", result["leaf_nodes"])
	}

	// Max depth: root(0) -> fn(1) -> if(2) -> call(3) = depth 3.
	gotDepth, ok := result["max_depth"].(int)
	if !ok || gotDepth != 3 {
		t.Errorf("max_depth = %v, want 3", result["max_depth"])
	}

	// Max children: root has 3 children.
	gotChildren, ok := result["max_children"].(int)
	if !ok || gotChildren != 3 {
		t.Errorf("max_children = %v, want 3", result["max_children"])
	}

	gotFile, ok := result["file"].(string)
	if !ok || gotFile != "test.go" {
		t.Errorf("file = %v, want %q", result["file"], "test.go")
	}

	types, ok := result["types"].(map[string]int)
	if !ok {
		t.Fatal("types is not map[string]int")
	}

	if types["Function"] != 1 {
		t.Errorf("types[Function] = %d, want 1", types["Function"])
	}

	if types["Method"] != 1 {
		t.Errorf("types[Method] = %d, want 1", types["Method"])
	}

	if types["If"] != 1 {
		t.Errorf("types[If] = %d, want 1", types["If"])
	}

	roles, ok := result["roles"].(map[string]int)
	if !ok {
		t.Fatal("roles is not map[string]int")
	}

	if roles["Function"] != 2 {
		t.Errorf("roles[Function] = %d, want 2", roles["Function"])
	}

	// Role coverage: 6 out of 7 have roles (root File has none).
	roleCov, ok := result["role_coverage"].(float64)
	if !ok {
		t.Fatal("role_coverage is not float64")
	}

	if roleCov < 0.85 || roleCov > 0.87 {
		t.Errorf("role_coverage = %.3f, want ~0.857", roleCov)
	}
}

func TestAnalyzeNode_EmptyTree(t *testing.T) {
	t.Parallel()

	root := node.NewBuilder().WithType(node.UASTFile).Build()
	result := analyzeNode(root, "empty.go")

	gotTotal, ok := result["total_nodes"].(int)
	if !ok || gotTotal != 1 {
		t.Errorf("total_nodes = %v, want 1", result["total_nodes"])
	}

	gotLeaf, ok := result["leaf_nodes"].(int)
	if !ok || gotLeaf != 1 {
		t.Errorf("leaf_nodes = %v, want 1", result["leaf_nodes"])
	}

	gotDepth, ok := result["max_depth"].(int)
	if !ok || gotDepth != 0 {
		t.Errorf("max_depth = %v, want 0", result["max_depth"])
	}
}

func TestRunAnalyze_TextOutput(t *testing.T) {
	t.Parallel()

	source := `package main

func hello() {
    if true {
        println("hi")
    }
}`

	tmpFile, err := os.CreateTemp(t.TempDir(), "*.go")
	if err != nil {
		t.Fatalf("create temp: %v", err)
	}

	_, writeErr := tmpFile.WriteString(source)
	if writeErr != nil {
		t.Fatalf("write: %v", writeErr)
	}

	tmpFile.Close()

	outFile := t.TempDir() + "/out.txt"

	runErr := runAnalyze([]string{tmpFile.Name()}, outFile, "text")
	if runErr != nil {
		t.Fatalf("runAnalyze: %v", runErr)
	}

	data, readErr := os.ReadFile(outFile)
	if readErr != nil {
		t.Fatalf("read output: %v", readErr)
	}

	out := string(data)

	for _, want := range []string{"Total nodes:", "Max depth:", "Role coverage:", "Node types:"} {
		if !bytes.Contains(data, []byte(want)) {
			t.Errorf("output missing %q:\n%s", want, out)
		}
	}
}

func TestRunAnalyze_JSONOutput(t *testing.T) {
	t.Parallel()

	source := `package main

func main() {}
`

	tmpFile, err := os.CreateTemp(t.TempDir(), "*.go")
	if err != nil {
		t.Fatalf("create temp: %v", err)
	}

	_, writeErr := tmpFile.WriteString(source)
	if writeErr != nil {
		t.Fatalf("write: %v", writeErr)
	}

	tmpFile.Close()

	outFile := t.TempDir() + "/out.json"

	runErr := runAnalyze([]string{tmpFile.Name()}, outFile, "json")
	if runErr != nil {
		t.Fatalf("runAnalyze: %v", runErr)
	}

	data, readErr := os.ReadFile(outFile)
	if readErr != nil {
		t.Fatalf("read output: %v", readErr)
	}

	var results []map[string]any

	unmarshalErr := json.Unmarshal(data, &results)
	if unmarshalErr != nil {
		t.Fatalf("invalid JSON: %v\n%s", unmarshalErr, data)
	}

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]

	if _, ok := r["total_nodes"]; !ok {
		t.Error("missing total_nodes in JSON output")
	}

	if _, ok := r["max_depth"]; !ok {
		t.Error("missing max_depth in JSON output")
	}

	if _, ok := r["types"]; !ok {
		t.Error("missing types in JSON output")
	}

	if _, ok := r["roles"]; !ok {
		t.Error("missing roles in JSON output")
	}

	// JSON numbers are float64.
	totalNodes, ok := r["total_nodes"].(float64)
	if !ok || totalNodes < 1 {
		t.Errorf("total_nodes = %v, want >= 1", r["total_nodes"])
	}
}

func TestRunAnalyze_NoFiles(t *testing.T) {
	t.Parallel()

	err := runAnalyze(nil, "", "text")
	if err == nil {
		t.Fatal("expected error for no files")
	}
}

func TestRunAnalyze_UnsupportedFormat(t *testing.T) {
	t.Parallel()

	source := "package main\n"

	tmpFile, err := os.CreateTemp(t.TempDir(), "*.go")
	if err != nil {
		t.Fatalf("create temp: %v", err)
	}

	_, writeErr := tmpFile.WriteString(source)
	if writeErr != nil {
		t.Fatalf("write: %v", writeErr)
	}

	tmpFile.Close()

	err = runAnalyze([]string{tmpFile.Name()}, "", "xml")
	if err == nil {
		t.Fatal("expected error for unsupported format")
	}
}
