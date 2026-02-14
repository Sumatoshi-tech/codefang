package lsp

import (
	"testing"

	protocol "github.com/tliron/glsp/protocol_3_16"
)

const testDocumentURI = "file:///test.uastmap"

func TestNewDocumentStore(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()

	if store == nil {
		t.Fatal("Expected non-nil DocumentStore")
	}

	if store.documents == nil {
		t.Error("Expected documents map to be initialized")
	}
}

func TestDocumentStore_SetAndGet(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()

	uri := testDocumentURI
	content := "test content"

	// Set document.
	store.Set(uri, content)

	// Get document.
	got, ok := store.Get(uri)
	if !ok {
		t.Errorf("Expected document to exist for URI %s", uri)
	}

	if got != content {
		t.Errorf("Expected content %q, got %q", content, got)
	}
}

func TestDocumentStore_Get_NotFound(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()

	_, ok := store.Get("file:///nonexistent.uastmap")
	if ok {
		t.Error("Expected document to not exist")
	}
}

func TestDocumentStore_Delete(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()

	uri := testDocumentURI
	content := "test content"

	// Set and then delete.
	store.Set(uri, content)
	store.Delete(uri)

	// Verify it's gone.
	_, ok := store.Get(uri)
	if ok {
		t.Error("Expected document to be deleted")
	}
}

func TestDocumentStore_Update(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()

	uri := testDocumentURI
	content1 := "initial content"
	content2 := "updated content"

	// Set initial content.
	store.Set(uri, content1)

	// Update content.
	store.Set(uri, content2)

	// Verify update.
	got, ok := store.Get(uri)
	if !ok {
		t.Errorf("Expected document to exist for URI %s", uri)
	}

	if got != content2 {
		t.Errorf("Expected content %q, got %q", content2, got)
	}
}

func TestNewServer(t *testing.T) {
	t.Parallel()

	server := NewServer()

	if server == nil {
		t.Fatal("Expected non-nil Server")
	}

	if server.store == nil {
		t.Error("Expected store to be initialized")
	}
}

func TestExtractWordAtPosition(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		text      string
		expected  string
		line      int
		character int
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
			// Clamps to line length and returns last word.
			expected: "short",
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
			t.Parallel()

			got := extractWordAtPosition(tt.text, tt.line, tt.character)
			if got != tt.expected {
				t.Errorf("extractWordAtPosition(%q, %d, %d) = %q, expected %q",
					tt.text, tt.line, tt.character, got, tt.expected)
			}
		})
	}
}

func TestIsWordChar(t *testing.T) {
	t.Parallel()

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
			t.Parallel()

			got := isWordChar(tt.char)
			if got != tt.expected {
				t.Errorf("isWordChar(%q) = %v, expected %v", tt.char, got, tt.expected)
			}
		})
	}
}

func TestSplitLines(t *testing.T) {
	t.Parallel()

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
			t.Parallel()

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

func TestCompletionItem(t *testing.T) {
	t.Parallel()

	item := completionItem("test", protocol.CompletionItemKindKeyword, "Test detail")

	if item.Label != "test" {
		t.Errorf("Expected label %q, got %q", "test", item.Label)
	}

	if item.Kind == nil || *item.Kind != protocol.CompletionItemKindKeyword {
		t.Error("Expected CompletionItemKindKeyword")
	}

	if item.Detail == nil || *item.Detail != "Test detail" {
		t.Errorf("Expected detail %q, got %v", "Test detail", item.Detail)
	}
}

func TestMappingDSLKeywords(t *testing.T) {
	t.Parallel()

	// Verify that the keywords are defined.
	if len(mappingDSLKeywords) == 0 {
		t.Error("Expected mapping DSL keywords to be defined")
	}

	// Check for expected keywords.
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
	t.Parallel()

	// Verify that UAST fields are defined.
	if len(uastFields) == 0 {
		t.Error("Expected UAST fields to be defined")
	}

	// Check for expected fields.
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
	t.Parallel()

	// Verify that hover docs are defined.
	if len(hoverDocs) == 0 {
		t.Error("Expected hover docs to be defined")
	}

	// Check for expected documentation.
	expectedKeys := []string{"<-", "=>", "uast", "type", "token", "roles", "props", "children"}

	for _, expected := range expectedKeys {
		if _, ok := hoverDocs[expected]; !ok {
			t.Errorf("Expected hover doc for %q not found", expected)
		}
	}

	// Verify docs are non-empty.
	for key, doc := range hoverDocs {
		if doc == "" {
			t.Errorf("Hover doc for %q is empty", key)
		}
	}
}

func TestDocumentStore_ConcurrentAccess(t *testing.T) {
	t.Parallel()

	store := NewDocumentStore()
	done := make(chan bool)

	// Concurrent writes.
	go func() {
		for range 100 {
			store.Set("file:///test1.uastmap", "content1")
		}

		done <- true
	}()

	go func() {
		for range 100 {
			store.Set("file:///test2.uastmap", "content2")
		}

		done <- true
	}()

	// Concurrent reads.
	go func() {
		for range 100 {
			store.Get("file:///test1.uastmap")
		}

		done <- true
	}()

	go func() {
		for range 100 {
			store.Get("file:///test2.uastmap")
		}

		done <- true
	}()

	// Wait for all goroutines.
	for range 4 {
		<-done
	}

	// Verify final state.
	content1, ok1 := store.Get("file:///test1.uastmap")
	content2, ok2 := store.Get("file:///test2.uastmap")

	if !ok1 || content1 != "content1" {
		t.Error("Expected test1.uastmap to have content1")
	}

	if !ok2 || content2 != "content2" {
		t.Error("Expected test2.uastmap to have content2")
	}
}
