package uast

import (
	"context"
	"strings"
	"testing"

	sitter "github.com/alexaandru/go-tree-sitter-bare"
)

// Sample Go DSL mapping rules for benchmarking.
const benchmarkDSL = `[language "go", extensions: ".go"]

source_file <- (source_file) => uast(
    type: "File",
    roles: "Module"
)

package_clause <- (package_clause) => uast(
    type: "Package",
    roles: "Declaration"
)

import_declaration <- (import_declaration) => uast(
    type: "Import",
    roles: "Declaration", "Import"
)

import_spec <- (import_spec) => uast(
    type: "ImportPath",
    roles: "Import"
)

function_declaration <- (function_declaration name: (identifier) @name) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function"
)

method_declaration <- (method_declaration name: (identifier) @name) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method"
)

type_declaration <- (type_declaration) => uast(
    type: "TypeDecl",
    roles: "Declaration", "Type"
)

type_spec <- (type_spec name: (identifier) @name) => uast(
    type: "TypeSpec",
    token: "@name",
    roles: "Declaration", "Type"
)

struct_type <- (struct_type) => uast(
    type: "Struct",
    roles: "Type", "Struct"
)

interface_type <- (interface_type) => uast(
    type: "Interface",
    roles: "Type", "Interface"
)

field_declaration <- (field_declaration) => uast(
    type: "Field",
    roles: "Declaration"
)

var_declaration <- (var_declaration) => uast(
    type: "VarDecl",
    roles: "Declaration", "Variable"
)

var_spec <- (var_spec name: (identifier) @name) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable"
)

const_declaration <- (const_declaration) => uast(
    type: "ConstDecl",
    roles: "Declaration"
)

const_spec <- (const_spec name: (identifier) @name) => uast(
    type: "Constant",
    token: "@name",
    roles: "Declaration"
)

identifier <- (identifier) => uast(
    type: "Identifier",
    roles: "Name"
)

block <- (block) => uast(
    type: "Block",
    roles: "Block"
)

if_statement <- (if_statement) => uast(
    type: "If",
    roles: "Statement", "Conditional"
)

for_statement <- (for_statement) => uast(
    type: "For",
    roles: "Statement", "Loop"
)

range_clause <- (range_clause) => uast(
    type: "Range",
    roles: "Iterator"
)

return_statement <- (return_statement) => uast(
    type: "Return",
    roles: "Statement", "Return"
)

call_expression <- (call_expression) => uast(
    type: "Call",
    roles: "Expression", "Call"
)

selector_expression <- (selector_expression) => uast(
    type: "Selector",
    roles: "Expression"
)

binary_expression <- (binary_expression) => uast(
    type: "Binary",
    roles: "Expression", "Binary"
)

unary_expression <- (unary_expression) => uast(
    type: "Unary",
    roles: "Expression", "Unary"
)

composite_literal <- (composite_literal) => uast(
    type: "CompositeLiteral",
    roles: "Literal"
)

interpreted_string_literal <- (interpreted_string_literal) => uast(
    type: "StringLiteral",
    roles: "Literal", "String"
)

raw_string_literal <- (raw_string_literal) => uast(
    type: "StringLiteral",
    roles: "Literal", "String"
)

int_literal <- (int_literal) => uast(
    type: "IntLiteral",
    roles: "Literal", "Number"
)

float_literal <- (float_literal) => uast(
    type: "FloatLiteral",
    roles: "Literal", "Number"
)

parameter_list <- (parameter_list) => uast(
    type: "Parameters",
    roles: "List"
)

parameter_declaration <- (parameter_declaration) => uast(
    type: "Parameter",
    roles: "Declaration", "Argument"
)

argument_list <- (argument_list) => uast(
    type: "Arguments",
    roles: "List"
)

assignment_statement <- (assignment_statement) => uast(
    type: "Assignment",
    roles: "Statement", "Assignment"
)

short_var_declaration <- (short_var_declaration) => uast(
    type: "ShortVarDecl",
    roles: "Declaration", "Variable"
)

expression_statement <- (expression_statement) => uast(
    type: "ExpressionStmt",
    roles: "Statement"
)

defer_statement <- (defer_statement) => uast(
    type: "Defer",
    roles: "Statement"
)

go_statement <- (go_statement) => uast(
    type: "Go",
    roles: "Statement"
)

switch_statement <- (switch_statement) => uast(
    type: "Switch",
    roles: "Statement", "Switch"
)

expression_case <- (expression_case) => uast(
    type: "Case",
    roles: "Statement", "Case"
)

default_case <- (default_case) => uast(
    type: "Default",
    roles: "Statement", "Default"
)
`

// Sample Go source code (small).
const smallGoSource = `package main

import "fmt"

func main() {
	fmt.Println("Hello, World!")
}
`

// Sample Go source code (medium - with structs and methods).
const mediumGoSource = `package main

import (
	"fmt"
	"strings"
)

type Person struct {
	Name string
	Age  int
}

func (p *Person) Greet() string {
	return fmt.Sprintf("Hello, I'm %s", p.Name)
}

func (p *Person) Birthday() {
	p.Age++
}

func NewPerson(name string, age int) *Person {
	return &Person{
		Name: name,
		Age:  age,
	}
}

func main() {
	p := NewPerson("Alice", 30)
	fmt.Println(p.Greet())
	p.Birthday()
	fmt.Println(p.Age)

	s := strings.ToUpper("hello")
	fmt.Println(s)
}
`

// Sample Go source code (large - many functions and statements).
const largeGoSource = `package main

import (
	"fmt"
	"strings"
	"strconv"
	"errors"
)

type Calculator struct {
	Value float64
}

func NewCalculator() *Calculator {
	return &Calculator{Value: 0}
}

func (c *Calculator) Add(x float64) *Calculator {
	c.Value += x
	return c
}

func (c *Calculator) Subtract(x float64) *Calculator {
	c.Value -= x
	return c
}

func (c *Calculator) Multiply(x float64) *Calculator {
	c.Value *= x
	return c
}

func (c *Calculator) Divide(x float64) (*Calculator, error) {
	if x == 0 {
		return nil, errors.New("division by zero")
	}
	c.Value /= x
	return c, nil
}

func (c *Calculator) Reset() {
	c.Value = 0
}

func (c *Calculator) Result() float64 {
	return c.Value
}

type StringProcessor struct {
	Data string
}

func NewStringProcessor(s string) *StringProcessor {
	return &StringProcessor{Data: s}
}

func (sp *StringProcessor) ToUpper() *StringProcessor {
	sp.Data = strings.ToUpper(sp.Data)
	return sp
}

func (sp *StringProcessor) ToLower() *StringProcessor {
	sp.Data = strings.ToLower(sp.Data)
	return sp
}

func (sp *StringProcessor) Trim() *StringProcessor {
	sp.Data = strings.TrimSpace(sp.Data)
	return sp
}

func (sp *StringProcessor) Append(s string) *StringProcessor {
	sp.Data += s
	return sp
}

func (sp *StringProcessor) Prepend(s string) *StringProcessor {
	sp.Data = s + sp.Data
	return sp
}

func (sp *StringProcessor) Result() string {
	return sp.Data
}

func processNumbers(numbers []int) (int, int, int) {
	sum := 0
	min := numbers[0]
	max := numbers[0]

	for _, n := range numbers {
		sum += n
		if n < min {
			min = n
		}
		if n > max {
			max = n
		}
	}

	return sum, min, max
}

func fibonacci(n int) int {
	if n <= 1 {
		return n
	}
	return fibonacci(n-1) + fibonacci(n-2)
}

func factorial(n int) int {
	if n <= 1 {
		return 1
	}
	return n * factorial(n-1)
}

func isPrime(n int) bool {
	if n <= 1 {
		return false
	}
	for i := 2; i*i <= n; i++ {
		if n%i == 0 {
			return false
		}
	}
	return true
}

func gcd(a, b int) int {
	for b != 0 {
		a, b = b, a%b
	}
	return a
}

func lcm(a, b int) int {
	return a * b / gcd(a, b)
}

func reverseString(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

func isPalindrome(s string) bool {
	s = strings.ToLower(s)
	return s == reverseString(s)
}

func countWords(s string) int {
	words := strings.Fields(s)
	return len(words)
}

func parseInt(s string) (int, error) {
	return strconv.Atoi(s)
}

func main() {
	calc := NewCalculator()
	calc.Add(10).Multiply(2).Subtract(5)
	result, _ := calc.Divide(5)
	fmt.Println(result.Result())

	sp := NewStringProcessor("  hello world  ")
	sp.Trim().ToUpper().Append("!")
	fmt.Println(sp.Result())

	numbers := []int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	sum, min, max := processNumbers(numbers)
	fmt.Printf("Sum: %d, Min: %d, Max: %d\n", sum, min, max)

	for i := 0; i < 10; i++ {
		fmt.Printf("fib(%d) = %d\n", i, fibonacci(i))
	}

	for i := 1; i <= 10; i++ {
		fmt.Printf("%d! = %d\n", i, factorial(i))
	}

	for i := 2; i <= 20; i++ {
		if isPrime(i) {
			fmt.Printf("%d is prime\n", i)
		}
	}

	fmt.Printf("GCD(48, 18) = %d\n", gcd(48, 18))
	fmt.Printf("LCM(4, 6) = %d\n", lcm(4, 6))

	testStr := "Hello World"
	fmt.Printf("Reverse of '%s' is '%s'\n", testStr, reverseString(testStr))
	fmt.Printf("'%s' is palindrome: %v\n", "radar", isPalindrome("radar"))
	fmt.Printf("Word count in '%s': %d\n", testStr, countWords(testStr))
}
`

// createBenchParser creates a parser for benchmarking.
func createBenchParser(b *testing.B) *DSLParser {
	b.Helper()

	parser := NewDSLParser(strings.NewReader(benchmarkDSL))

	err := parser.Load()
	if err != nil {
		b.Fatalf("Failed to load DSL: %v", err)
	}

	return parser
}

// BenchmarkToCanonicalNode_Small benchmarks parsing a small Go file.
func BenchmarkToCanonicalNode_Small(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(smallGoSource)

	b.ResetTimer()

	for b.Loop() {
		_, err := parser.Parse("test.go", source)
		if err != nil {
			b.Fatalf("Parse failed: %v", err)
		}
	}
}

// BenchmarkToCanonicalNode_Large_WarmCache benchmarks with pre-warmed query cache.
func BenchmarkToCanonicalNode_Large_WarmCache(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(largeGoSource)

	// Warm up the pattern cache by parsing once before timing.
	_, err := parser.Parse("warmup.go", source)
	if err != nil {
		b.Fatalf("Warmup parse failed: %v", err)
	}

	b.ResetTimer()

	for b.Loop() {
		_, parseErr := parser.Parse("test.go", source)
		if parseErr != nil {
			b.Fatalf("Parse failed: %v", parseErr)
		}
	}

	b.StopTimer()

	// Report cache stats.
	hits, misses := parser.patternMatcher.CacheStats()
	b.Logf("Cache stats: hits=%d, misses=%d, hit_rate=%.2f%%", hits, misses, float64(hits)/float64(hits+misses)*100)
}

// BenchmarkToCanonicalNode_Medium benchmarks parsing a medium Go file.
func BenchmarkToCanonicalNode_Medium(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(mediumGoSource)

	b.ResetTimer()

	for b.Loop() {
		_, err := parser.Parse("test.go", source)
		if err != nil {
			b.Fatalf("Parse failed: %v", err)
		}
	}
}

// BenchmarkToCanonicalNode_Large benchmarks parsing a large Go file.
func BenchmarkToCanonicalNode_Large(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(largeGoSource)

	b.ResetTimer()

	for b.Loop() {
		_, err := parser.Parse("test.go", source)
		if err != nil {
			b.Fatalf("Parse failed: %v", err)
		}
	}
}

// BenchmarkFindMappingRule benchmarks the findMappingRule function directly.
func BenchmarkFindMappingRule(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(smallGoSource)

	// Parse once to create a DSLNode.
	tsParser := sitter.NewParser()
	tsParser.SetLanguage(parser.language)

	tree, err := tsParser.ParseString(context.Background(), nil, source)
	if err != nil {
		b.Fatalf("Failed to parse: %v", err)
	}

	root := tree.RootNode()
	dslNode := parser.createDSLNode(root, tree, source)

	// Get some node types to search for.
	nodeTypes := []string{
		"source_file",
		"function_declaration",
		"identifier",
		"call_expression",
		"nonexistent_node_type",
	}

	b.ResetTimer()

	for b.Loop() {
		for _, nodeType := range nodeTypes {
			_ = dslNode.findMappingRule(nodeType)
		}
	}
}

// BenchmarkProcessChildren benchmarks child processing.
func BenchmarkProcessChildren(b *testing.B) {
	parser := createBenchParser(b)
	source := []byte(largeGoSource)

	// Parse once to create a DSLNode.
	tsParser := sitter.NewParser()
	tsParser.SetLanguage(parser.language)

	tree, err := tsParser.ParseString(context.Background(), nil, source)
	if err != nil {
		b.Fatalf("Failed to parse: %v", err)
	}

	root := tree.RootNode()
	dslNode := parser.createDSLNode(root, tree, source)
	mappingRule := dslNode.findMappingRule(root.Type())

	b.ResetTimer()

	for b.Loop() {
		_ = dslNode.processChildren(mappingRule)
	}
}
