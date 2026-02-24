package codestyle_test

import (
	"bufio"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode"
)

// projectRoot returns the repository root by walking up from the current file.
func projectRoot(t *testing.T) string {
	t.Helper()

	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}

	for {
		_, statErr := os.Stat(filepath.Join(dir, "go.mod"))
		if statErr == nil {
			return dir
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("could not find project root (no go.mod found)")
		}

		dir = parent
	}
}

// skipDir returns true for directories that should be excluded from scanning.
func skipDir(name string) bool {
	switch name {
	case "vendor", "third_party", "testdata", ".git", "node_modules":
		return true
	default:
		return false
	}
}

// isGenerated reports whether a Go file contains the standard generated-code marker.
func isGenerated(path string) bool {
	f, err := os.Open(path)
	if err != nil {
		return false
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.Contains(line, "Code generated") && strings.Contains(line, "DO NOT EDIT") {
			return true
		}
		// Only scan first 20 lines for the marker.
		if scanner.Err() != nil {
			break
		}
	}

	return false
}

// isGoSource returns true for non-test, non-generated Go source files.
func isGoSource(path string) bool {
	return strings.HasSuffix(path, ".go") &&
		!strings.HasSuffix(path, "_test.go") &&
		!isGenerated(path)
}

// walkGoFiles walks root and calls fn for every non-test, non-generated Go source file.
func walkGoFiles(t *testing.T, root string, fn func(rel string, f *ast.File)) {
	t.Helper()

	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if info.IsDir() {
			if skipDir(filepath.Base(path)) {
				return filepath.SkipDir
			}

			return nil
		}

		if !isGoSource(path) {
			return nil
		}

		fset := token.NewFileSet()

		parsed, parseErr := parser.ParseFile(fset, path, nil, parser.ParseComments)
		if parseErr != nil {
			return fmt.Errorf("parsing Go file %s: %w", path, parseErr)
		}

		rel, relErr := filepath.Rel(root, path)
		if relErr != nil {
			return fmt.Errorf("computing relative path for %s: %w", path, relErr)
		}

		fn(rel, parsed)

		return nil
	})
	if err != nil {
		t.Fatalf("walk: %v", err)
	}
}

// ---------- Banned filenames ----------.

// bannedFilename describes a filename pattern that violates Go best practices.
type bannedFilename struct {
	Name   string // exact basename to match.
	Reason string // why it is a code smell.
	Fix    string // agent-friendly fix instruction (accepts one %s for the relative path).
}

// getBannedFilenames returns the authoritative list of filenames that must not exist.
func getBannedFilenames() []bannedFilename {
	return []bannedFilename{
		{
			Name: "types.go",
			Reason: "Grouping types by kind ('types.go') instead of by domain violates Go best practices. " +
				"Types belong next to the code that uses them.",
			Fix: "Move each type from %s into the file that uses it most. " +
				"For example, if 'Analyzer' struct is used primarily in 'analyzer.go', move it there. " +
				"If 'Function' struct is used in 'visitor.go', move it there. " +
				"Delete types.go once empty.",
		},
		{
			Name: "utils.go",
			Reason: "'utils.go' is a grab-bag file with no clear domain responsibility. " +
				"Each function should live in the file whose domain it belongs to.",
			Fix: "Move each function from %s to the file that owns that domain. " +
				"If a utility is used across packages, extract it into a focused package " +
				"(e.g., 'pkg/stringutil/', 'pkg/mathutil/'). Delete utils.go once empty.",
		},
		{
			Name: "helpers.go",
			Reason: "'helpers.go' is a grab-bag file with no clear domain responsibility, " +
				"same problem as 'utils.go'.",
			Fix: "Move each function from %s to the file that owns that domain. " +
				"For example, 'LoadRepository' belongs in 'repository.go', " +
				"'ParseTime' belongs in 'time.go' or a dedicated 'pkg/timeutil/' package, " +
				"'LoadCommits' belongs in 'commit.go'. Delete helpers.go once empty.",
		},
		{
			Name:   "common.go",
			Reason: "'common.go' signals unclear ownership. If everything is 'common', nothing is.",
			Fix: "Move each symbol from %s to the file that owns its domain concept. " +
				"Delete common.go once empty.",
		},
		{
			Name:   "constants.go",
			Reason: "Constants should live next to the code that uses them, not in a separate 'constants.go'.",
			Fix: "Move each constant from %s to the file where it is primarily used. " +
				"Delete constants.go once empty.",
		},
		{
			Name:   "errors.go",
			Reason: "Sentinel errors should live next to the functions that return them, not in a separate 'errors.go'.",
			Fix: "Move each error variable from %s to the file containing the function that returns it. " +
				"Delete errors.go once empty.",
		},
	}
}

// allowedBannedFiles lists files that are known to use banned names but are not yet migrated.
// Each entry is a relative path from the project root.
var allowedBannedFiles = map[string]bool{
	"pkg/analyzers/cohesion/types.go": true,
	"pkg/analyzers/comments/types.go": true,
	"pkg/gitlib/helpers.go":           true,
	"pkg/plumbing/types.go":           true,
	"pkg/uast/pkg/node/types.go":      true,
	"pkg/uast/types.go":               true,
}

// TestNoBannedFilenames verifies that no Go source files use grab-bag naming patterns.
func TestNoBannedFilenames(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)

	var violations []string

	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if info.IsDir() {
			if skipDir(filepath.Base(path)) {
				return filepath.SkipDir
			}

			return nil
		}

		if !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}

		base := filepath.Base(path)
		for _, banned := range getBannedFilenames() {
			if base == banned.Name {
				rel, relErr := filepath.Rel(root, path)
				if relErr != nil {
					return fmt.Errorf("computing relative path for %s: %w", path, relErr)
				}

				if allowedBannedFiles[rel] {
					continue
				}

				violations = append(violations, fmt.Sprintf(
					"VIOLATION: %s\n  Reason: %s\n  Fix: %s",
					rel, banned.Reason, fmt.Sprintf(banned.Fix, rel),
				))
			}
		}

		return nil
	})
	if err != nil {
		t.Fatalf("walk: %v", err)
	}

	if len(violations) > 0 {
		t.Errorf("found %d banned filename(s):\n\n%s",
			len(violations), strings.Join(violations, "\n\n"))
	}
}

// ---------- Interface checks ----------.

// interfaceDecl captures a declared interface and its location.
type interfaceDecl struct {
	Name    string
	File    string // relative path.
	Pkg     string // package directory (relative).
	Methods int    // number of methods.
}

// collectInterfaces parses all Go files under root and returns declared interfaces.
func collectInterfaces(t *testing.T, root string) []interfaceDecl {
	t.Helper()

	var decls []interfaceDecl

	walkGoFiles(t, root, func(rel string, f *ast.File) {
		pkgDir := filepath.Dir(rel)

		for _, decl := range f.Decls {
			genDecl, isGenDecl := decl.(*ast.GenDecl)
			if !isGenDecl || genDecl.Tok != token.TYPE {
				continue
			}

			for _, spec := range genDecl.Specs {
				typeSpec, isTypeSpec := spec.(*ast.TypeSpec)
				if !isTypeSpec {
					continue
				}

				ifaceType, isIface := typeSpec.Type.(*ast.InterfaceType)
				if !isIface {
					continue
				}

				decls = append(decls, interfaceDecl{
					Name:    typeSpec.Name.Name,
					File:    rel,
					Pkg:     pkgDir,
					Methods: countMethods(ifaceType),
				})
			}
		}
	})

	return decls
}

// countMethods returns the number of methods (not embedded interfaces) in an interface type.
func countMethods(iface *ast.InterfaceType) int {
	count := 0

	for _, method := range iface.Methods.List {
		if _, ok := method.Type.(*ast.FuncType); ok {
			count++
		}
	}

	return count
}

// allowedInterfacesInTypesFiles lists interfaces in types.go that are co-located with
// their implementing structs for cohesion (strategy/registry patterns).
var allowedInterfacesInTypesFiles = map[string]bool{
	"DSLNodeLowerer":      true, // strategy interface co-located with DSL node types.
	"FieldAccessStrategy": true, // strategy interface co-located with field access types.
	"LanguageParser":      true, // parser interface co-located with UAST types.
}

// TestNoInterfacesInTypesFiles verifies that interfaces are not defined inside types.go files.
// Interfaces should be defined where they are consumed (Go proverb: "accept interfaces, return structs").
func TestNoInterfacesInTypesFiles(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)
	decls := collectInterfaces(t, root)

	var violations []string

	for _, item := range decls {
		base := filepath.Base(item.File)
		if base == "types.go" && !allowedInterfacesInTypesFiles[item.Name] {
			violations = append(violations, fmt.Sprintf(
				"VIOLATION: interface %q defined in %s\n"+
					"  Reason: Interfaces must not live in types.go. In idiomatic Go, interfaces belong "+
					"in the file (or package) that CONSUMES them, not alongside struct definitions.\n"+
					"  Fix: Move interface %q from %s to the file that accepts it as a parameter "+
					"or stores it in a field. If multiple consumers exist, define it in the consumer "+
					"package closest to the call site.",
				item.Name, item.File, item.Name, item.File,
			))
		}
	}

	if len(violations) > 0 {
		t.Errorf("found %d interface(s) in types.go files:\n\n%s",
			len(violations), strings.Join(violations, "\n\n"))
	}
}

// maxInterfaceMethods is the maximum number of methods an interface should have.
// Interfaces with more than this many methods are likely too broad.
const maxInterfaceMethods = 5

// allowedFatInterfaces lists interfaces that exceed maxInterfaceMethods but are accepted
// because splitting them would hurt cohesion or break existing contracts.
var allowedFatInterfaces = map[string]bool{
	"StaticAnalyzer":  true, // core analyzer contract; all 8 methods are consumed together.
	"HistoryAnalyzer": true, // core analyzer contract; all 10 methods are consumed together.
	"ReportSection":   true, // rendering contract; all 8 methods are consumed together.
	"Timeline":        true, // burndown data structure; 11 methods form a cohesive mutation/query API.
	"Aggregator":      true, // streaming pipeline contract; 6 methods form a cohesive lifecycle (feed/flush/spill/collect/size/close).
}

// TestNoFatInterfaces verifies that interfaces stay small and focused.
// The Go proverb: "The bigger the interface, the weaker the abstraction.".
func TestNoFatInterfaces(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)
	decls := collectInterfaces(t, root)

	var violations []string

	for _, item := range decls {
		if item.Methods > maxInterfaceMethods && !allowedFatInterfaces[item.Name] {
			violations = append(violations, fmt.Sprintf(
				"VIOLATION: interface %q in %s has %d methods (max %d)\n"+
					"  Reason: Large interfaces create tight coupling and are hard to implement/mock. "+
					"Go proverb: 'The bigger the interface, the weaker the abstraction.'\n"+
					"  Fix: Split %q into smaller, composable interfaces with 1-3 methods each. "+
					"Compose them via embedding where a larger surface is needed.",
				item.Name, item.File, item.Methods, maxInterfaceMethods,
				item.Name,
			))
		}
	}

	if len(violations) > 0 {
		t.Errorf("found %d fat interface(s):\n\n%s",
			len(violations), strings.Join(violations, "\n\n"))
	}
}

// ---------- Package naming ----------.

// TestNoGrabBagPackages verifies that package names don't use generic names
// that indicate unclear responsibility.
func TestNoGrabBagPackages(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)

	bannedPkgNames := map[string]string{
		"util":    "Use a domain-specific package name instead (e.g., 'stringutil', 'httputil').",
		"utils":   "Use a domain-specific package name instead (e.g., 'stringutil', 'httputil').",
		"misc":    "Every function belongs to a domain. Name the package after that domain.",
		"shared":  "If it's shared, it has a domain purpose. Name the package after that purpose.",
		"base":    "Use a concrete name describing what the package provides.",
		"generic": "Use a concrete name describing what the package provides.",
	}

	var violations []string

	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if !info.IsDir() {
			return nil
		}

		base := filepath.Base(path)
		if skipDir(base) {
			return filepath.SkipDir
		}

		if fix, banned := bannedPkgNames[base]; banned {
			// Verify it actually contains Go files.
			goFiles, globErr := filepath.Glob(filepath.Join(path, "*.go"))
			if globErr != nil {
				return fmt.Errorf("globbing Go files in %s: %w", path, globErr)
			}

			if len(goFiles) > 0 {
				rel, relErr := filepath.Rel(root, path)
				if relErr != nil {
					return fmt.Errorf("computing relative path for %s: %w", path, relErr)
				}

				violations = append(violations, fmt.Sprintf(
					"VIOLATION: package %q at %s\n"+
						"  Reason: Generic package names indicate unclear responsibility.\n"+
						"  Fix: %s",
					base, rel, fix,
				))
			}
		}

		return nil
	})
	if err != nil {
		t.Fatalf("walk: %v", err)
	}

	if len(violations) > 0 {
		t.Errorf("found %d grab-bag package(s):\n\n%s",
			len(violations), strings.Join(violations, "\n\n"))
	}
}

// ---------- Stuttering exports ----------.

// stutters reports whether an exported identifier repeats the package name.
// It requires the package name to appear as a proper CamelCase prefix with a word boundary:
//
//	checkpoint.CheckpointManager → ("Manager", true) — "Checkpoint" == pkg title-case, "M" is uppercase
//	mapping.MappingDSL           → ("DSL", true)     — "Mapping" == pkg title-case, "D" is uppercase
//	analyze.Analyzer             → ("", false)        — remaining "r" is lowercase, no word boundary
//	config.Config                → ("", false)        — exact match, not stuttering
func stutters(pkgName, exportedName string) (string, bool) {
	// Title-case the package name to match Go export casing.
	titled := strings.ToUpper(pkgName[:1]) + pkgName[1:]

	if !strings.HasPrefix(exportedName, titled) {
		return "", false
	}

	rest := exportedName[len(titled):]
	if rest == "" {
		return "", false // exact match is not stuttering.
	}

	// The character after the prefix must be uppercase or a digit (word boundary).
	// This prevents "analyze.Analyzer" from matching ("r" is lowercase, not a word boundary).
	firstRune := rune(rest[0])
	if !unicode.IsUpper(firstRune) && !unicode.IsDigit(firstRune) {
		return "", false
	}

	return rest, true
}

// TestNoStutteringExports detects exported type names that stutter with the package name.
// For example, package "config" should not export "ConfigLoader" (use "Loader" instead).
func TestNoStutteringExports(t *testing.T) {
	t.Parallel()

	root := projectRoot(t)

	var violations []string

	walkGoFiles(t, root, func(rel string, fval *ast.File) {
		pkgName := strings.ToLower(fval.Name.Name)

		for _, decl := range fval.Decls {
			genDecl, isGenDecl := decl.(*ast.GenDecl)
			if !isGenDecl || genDecl.Tok != token.TYPE {
				continue
			}

			for _, spec := range genDecl.Specs {
				typeSpec, isTypeSpec := spec.(*ast.TypeSpec)
				if !isTypeSpec {
					continue
				}

				name := typeSpec.Name.Name
				if !ast.IsExported(name) {
					continue
				}

				if trimmed, isStutter := stutters(pkgName, name); isStutter {
					violations = append(violations, fmt.Sprintf(
						"VIOLATION: type %s.%s in %s stutters with package name\n"+
							"  Reason: In Go, '%s.%s' is read redundantly. "+
							"The package name already provides context.\n"+
							"  Fix: Rename %q to %q so callers write '%s.%s' instead of '%s.%s'.",
						fval.Name.Name, name, rel,
						fval.Name.Name, name,
						name, trimmed,
						fval.Name.Name, trimmed, fval.Name.Name, name,
					))
				}
			}
		}
	})

	if len(violations) > 0 {
		t.Errorf("found %d stuttering export(s):\n\n%s",
			len(violations), strings.Join(violations, "\n\n"))
	}
}
