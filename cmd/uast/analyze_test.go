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
	if result.TotalNodes != 7 {
		t.Errorf("TotalNodes = %d, want 7", result.TotalNodes)
	}

	// Leaf count: pkg, call, loop = 3 leaf nodes.
	if result.LeafNodes != 3 {
		t.Errorf("LeafNodes = %d, want 3", result.LeafNodes)
	}

	// Max depth: root(0) -> fn(1) -> if(2) -> call(3) = depth 3.
	if result.MaxDepth != 3 {
		t.Errorf("MaxDepth = %d, want 3", result.MaxDepth)
	}

	// Max children: root has 3 children.
	if result.MaxChildren != 3 {
		t.Errorf("MaxChildren = %d, want 3", result.MaxChildren)
	}

	if result.File != "test.go" {
		t.Errorf("File = %q, want %q", result.File, "test.go")
	}

	if result.Types["Function"] != 1 {
		t.Errorf("Types[Function] = %d, want 1", result.Types["Function"])
	}

	if result.Types["Method"] != 1 {
		t.Errorf("Types[Method] = %d, want 1", result.Types["Method"])
	}

	if result.Types["If"] != 1 {
		t.Errorf("Types[If] = %d, want 1", result.Types["If"])
	}

	if result.Roles["Function"] != 2 {
		t.Errorf("Roles[Function] = %d, want 2", result.Roles["Function"])
	}

	// Role coverage: 6 out of 7 have roles (root File has none).
	if result.RoleCoverage < 0.85 || result.RoleCoverage > 0.87 {
		t.Errorf("RoleCoverage = %.3f, want ~0.857", result.RoleCoverage)
	}
}

func TestAnalyzeNode_EmptyTree(t *testing.T) {
	t.Parallel()

	root := node.NewBuilder().WithType(node.UASTFile).Build()
	result := analyzeNode(root, "empty.go")

	if result.TotalNodes != 1 {
		t.Errorf("TotalNodes = %d, want 1", result.TotalNodes)
	}

	if result.LeafNodes != 1 {
		t.Errorf("LeafNodes = %d, want 1", result.LeafNodes)
	}

	if result.MaxDepth != 0 {
		t.Errorf("MaxDepth = %d, want 0", result.MaxDepth)
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
