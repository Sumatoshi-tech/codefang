package uast

import (
	"testing"
)

func TestNewLoader(t *testing.T) {
	loader := NewLoader(nil)
	if loader == nil {
		t.Errorf("expected non-nil loader")
	}
}

func TestLoader_LoadProvider(t *testing.T) {
	loader := NewLoader(nil)

	// With embedded mappings, LanguageParser uses extension-based lookup.
	// "go" is a language name, not an extension — so lookup by extension ".go" should succeed.
	_, exists := loader.LanguageParser(".go")
	if !exists {
		t.Errorf("expected parser to exist for .go extension")
	}

	// Non-existent extension should not exist
	_, exists = loader.LanguageParser(".nonexistent")
	if exists {
		t.Errorf("expected parser to not exist for .nonexistent extension")
	}
}

func TestLoader_LoadAllProviders(t *testing.T) {
	loader := NewLoader(nil)

	// Test loading all providers (this will fail since we don't have actual embed.FS)
	parsers := loader.GetParsers()
	if len(parsers) == 0 {
		t.Errorf("expected providers when loading providers without embed.FS")
	}
}

func TestLoader_loadDSLMapping(t *testing.T) {
	loader := NewLoader(nil)

	// With embedded mappings, parsers are loaded from pre-compiled cache.
	// Extension-based lookup should work for known languages.
	_, exists := loader.LanguageParser(".go")
	if !exists {
		t.Errorf("expected parser to exist for .go via embedded mappings")
	}
}
