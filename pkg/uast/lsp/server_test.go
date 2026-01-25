package lsp

import (
	"testing"

	protocol "github.com/tliron/glsp/protocol_3_16"
)

func TestNewDocumentStore(t *testing.T) {
	store := NewDocumentStore()

	if store == nil {
		t.Fatal("Expected non-nil DocumentStore")
	}

	if store.documents == nil {
		t.Error("Expected documents map to be initialized")
	}
}

func TestDocumentStore_SetAndGet(t *testing.T) {
	store := NewDocumentStore()

	uri := "file:///test.uastmap"
	content := "test content"

	// Set document
	store.Set(uri, content)

	// Get document
	got, ok := store.Get(uri)
	if !ok {
		t.Errorf("Expected document to exist for URI %s", uri)
	}

	if got != content {
		t.Errorf("Expected content %q, got %q", content, got)
	}
}

func TestDocumentStore_Get_NotFound(t *testing.T) {
	store := NewDocumentStore()

	_, ok := store.Get("file:///nonexistent.uastmap")
	if ok {
		t.Error("Expected document to not exist")
	}
}

func TestDocumentStore_Delete(t *testing.T) {
	store := NewDocumentStore()

	uri := "file:///test.uastmap"
	content := "test content"

	// Set and then delete
	store.Set(uri, content)
	store.Delete(uri)

	// Verify it's gone
	_, ok := store.Get(uri)
	if ok {
		t.Error("Expected document to be deleted")
	}
}

func TestDocumentStore_Update(t *testing.T) {
	store := NewDocumentStore()

	uri := "file:///test.uastmap"
	content1 := "initial content"
	content2 := "updated content"

	// Set initial content
	store.Set(uri, content1)

	// Update content
	store.Set(uri, content2)

	// Verify update
	got, ok := store.Get(uri)
	if !ok {
		t.Errorf("Expected document to exist for URI %s", uri)
	}

	if got != content2 {
		t.Errorf("Expected content %q, got %q", content2, got)
	}
}

func TestNewServer(t *testing.T) {
	server := NewServer()

	if server == nil {
		t.Fatal("Expected non-nil Server")
	}

	if server.store == nil {
		t.Error("Expected store to be initialized")
	}
}

func TestExtractWordAtPosition(t *testing.T) {
	tests := []struct {
		name      string
		text      string
		line      int
		character int
		expected  string
	}{
		{
			name:      "simple word",
			text:      "hello world",
			line:      0,
			character: 2,
			expected:  "hello",
		},
		{
			name:      "second word",
			text:      "hello world",
			line:      0,
			character: 8,
			expected:  "world",
		},
		{
			name:      "keyword arrow",
			text:      "rule <- pattern",
			line:      0,
			character: 6,
			expected:  "<-",
		},
		{
			name:      "keyword mapping",
			text:      "pattern => uast",
			line:      0,
			character: 9,
			expected:  "=>",
		},
		{
			name:      "multiline first line",
			text:      "first\nsecond\nthird",
			line:      0,
			character: 2,
			expected:  "first",
		},
		{
			name:      "multiline second line",
			text:      "first\nsecond\nthird",
			line:      1,
			character: 3,
			expected:  "second",
		},
		{
			name:      "multiline third line",
			text:      "first\nsecond\nthird",
			line:      2,
			character: 2,
			expected:  "third",
		},
		{
			name:      "line out of bounds",
			text:      "single line",
			line:      5,
			character: 0,
			expected:  "",
		},
		{
			name:      "character past end of line",
			text:      "short",
			line:      0,
			character: 100,
			expected:  "short", // clamps to line length and returns last word
		},
		{
			name:      "underscore in word",
			text:      "my_variable = 1",
			line:      0,
			character: 5,
			expected:  "my_variable",
		},
		{
			name:      "empty text",
			text:      "",
			line:      0,
			character: 0,
			expected:  "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := extractWordAtPosition(tt.text, tt.line, tt.character)
			if got != tt.expected {
				t.Errorf("extractWordAtPosition(%q, %d, %d) = %q, expected %q",
					tt.text, tt.line, tt.character, got, tt.expected)
			}
		})
	}
}

func TestIsWordChar(t *testing.T) {
	tests := []struct {
		char     byte
		expected bool
	}{
		{'a', true},
		{'z', true},
		{'A', true},
		{'Z', true},
		{'_', true},
		{'<', true},
		{'>', true},
		{'-', true},
		{'=', true},
		{'0', false},
		{'9', false},
		{' ', false},
		{'\t', false},
		{'\n', false},
		{'(', false},
		{')', false},
		{'{', false},
		{'}', false},
		{':', false},
	}

	for _, tt := range tests {
		t.Run(string(tt.char), func(t *testing.T) {
			got := isWordChar(tt.char)
			if got != tt.expected {
				t.Errorf("isWordChar(%q) = %v, expected %v", tt.char, got, tt.expected)
			}
		})
	}
}

func TestSplitLines(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected []string
	}{
		{
			name:     "single line",
			input:    "hello",
			expected: []string{"hello"},
		},
		{
			name:     "two lines",
			input:    "hello\nworld",
			expected: []string{"hello", "world"},
		},
		{
			name:     "three lines",
			input:    "one\ntwo\nthree",
			expected: []string{"one", "two", "three"},
		},
		{
			name:     "empty string",
			input:    "",
			expected: []string{""},
		},
		{
			name:     "trailing newline",
			input:    "hello\n",
			expected: []string{"hello", ""},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := splitLines(tt.input)
			if len(got) != len(tt.expected) {
				t.Errorf("splitLines(%q) returned %d lines, expected %d",
					tt.input, len(got), len(tt.expected))
				return
			}
			for i, line := range got {
				if line != tt.expected[i] {
					t.Errorf("splitLines(%q)[%d] = %q, expected %q",
						tt.input, i, line, tt.expected[i])
				}
			}
		})
	}
}

func TestPtrCompletionKind(t *testing.T) {
	kind := protocol.CompletionItemKindKeyword
	result := ptrCompletionKind(kind)

	if result == nil {
		t.Fatal("Expected non-nil pointer")
	}

	if *result != kind {
		t.Errorf("Expected %v, got %v", kind, *result)
	}
}

func TestPtrString(t *testing.T) {
	str := "test string"
	result := ptrString(str)

	if result == nil {
		t.Fatal("Expected non-nil pointer")
	}

	if *result != str {
		t.Errorf("Expected %q, got %q", str, *result)
	}
}

func TestMappingDSLKeywords(t *testing.T) {
	// Verify that the keywords are defined
	if len(mappingDSLKeywords) == 0 {
		t.Error("Expected mapping DSL keywords to be defined")
	}

	// Check for expected keywords
	expectedLabels := []string{"<-", "=>", "uast"}
	for _, expected := range expectedLabels {
		found := false
		for _, item := range mappingDSLKeywords {
			if item.Label == expected {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("Expected keyword %q not found in mappingDSLKeywords", expected)
		}
	}
}

func TestUastFields(t *testing.T) {
	// Verify that UAST fields are defined
	if len(uastFields) == 0 {
		t.Error("Expected UAST fields to be defined")
	}

	// Check for expected fields
	expectedLabels := []string{"type", "token", "roles", "props", "children"}
	for _, expected := range expectedLabels {
		found := false
		for _, item := range uastFields {
			if item.Label == expected {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("Expected field %q not found in uastFields", expected)
		}
	}
}

func TestHoverDocs(t *testing.T) {
	// Verify that hover docs are defined
	if len(hoverDocs) == 0 {
		t.Error("Expected hover docs to be defined")
	}

	// Check for expected documentation
	expectedKeys := []string{"<-", "=>", "uast", "type", "token", "roles", "props", "children"}
	for _, expected := range expectedKeys {
		if _, ok := hoverDocs[expected]; !ok {
			t.Errorf("Expected hover doc for %q not found", expected)
		}
	}

	// Verify docs are non-empty
	for key, doc := range hoverDocs {
		if doc == "" {
			t.Errorf("Hover doc for %q is empty", key)
		}
	}
}

func TestDocumentStore_ConcurrentAccess(t *testing.T) {
	store := NewDocumentStore()
	done := make(chan bool)

	// Concurrent writes
	go func() {
		for i := 0; i < 100; i++ {
			store.Set("file:///test1.uastmap", "content1")
		}
		done <- true
	}()

	go func() {
		for i := 0; i < 100; i++ {
			store.Set("file:///test2.uastmap", "content2")
		}
		done <- true
	}()

	// Concurrent reads
	go func() {
		for i := 0; i < 100; i++ {
			store.Get("file:///test1.uastmap")
		}
		done <- true
	}()

	go func() {
		for i := 0; i < 100; i++ {
			store.Get("file:///test2.uastmap")
		}
		done <- true
	}()

	// Wait for all goroutines
	for i := 0; i < 4; i++ {
		<-done
	}

	// Verify final state
	content1, ok1 := store.Get("file:///test1.uastmap")
	content2, ok2 := store.Get("file:///test2.uastmap")

	if !ok1 || content1 != "content1" {
		t.Error("Expected test1.uastmap to have content1")
	}
	if !ok2 || content2 != "content2" {
		t.Error("Expected test2.uastmap to have content2")
	}
}
