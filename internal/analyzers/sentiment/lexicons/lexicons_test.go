package lexicons

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func AllLanguages() []Language {
	langs := make([]Language, 0, len(languageRegistry))
	for lang := range languageRegistry {
		langs = append(langs, lang)
	}

	return langs
}

func LanguageName(lang Language) string {
	if info, ok := languageRegistry[lang]; ok {
		return info.name
	}

	return string(lang)
}

func ForLanguage(lang Language) []Entry {
	if info, ok := languageRegistry[lang]; ok {
		return info.loader()
	}

	return nil
}

func LanguageCount() int {
	return len(languageRegistry)
}

func TestAllLanguages(t *testing.T) {
	t.Parallel()

	langs := AllLanguages()

	assert.GreaterOrEqual(t, len(langs), 30)
}

func TestLanguageCount(t *testing.T) {
	t.Parallel()

	assert.GreaterOrEqual(t, LanguageCount(), 30)
}

func TestEntryCount(t *testing.T) {
	t.Parallel()

	assert.Greater(t, EntryCount(), 50000)
}

func TestForLanguage_Supported(t *testing.T) {
	t.Parallel()

	tests := []struct {
		lang    Language
		minSize int
	}{
		{LangRussian, 2000},
		{LangChinese, 1000},
		{LangJapanese, 500},
		{LangKorean, 1500},
		{LangSpanish, 3000},
		{LangFrench, 3000},
		{LangGerman, 3000},
		{LangPortuguese, 3000},
	}

	for _, tt := range tests {
		t.Run(string(tt.lang), func(t *testing.T) {
			t.Parallel()

			entries := ForLanguage(tt.lang)
			require.NotNil(t, entries)
			assert.GreaterOrEqual(t, len(entries), tt.minSize,
				"%s should have at least %d entries", tt.lang, tt.minSize)
		})
	}
}

func TestForLanguage_Unsupported(t *testing.T) {
	t.Parallel()

	entries := ForLanguage("xx")
	assert.Nil(t, entries)
}

func TestLanguageName(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "Russian", LanguageName(LangRussian))
	assert.Equal(t, "Chinese", LanguageName(LangChinese))
	assert.Equal(t, "xx", LanguageName("xx"))
}

func TestAll(t *testing.T) {
	t.Parallel()

	all := All()

	assert.Greater(t, len(all), 50000)
}

func TestEntryValence(t *testing.T) {
	t.Parallel()

	all := All()

	for _, entry := range all {
		assert.NotEmpty(t, entry.Word)

		assert.True(t, entry.Valence == 1.5 || entry.Valence == -1.5,
			"entry %q has unexpected valence %.1f", entry.Word, entry.Valence)
	}
}

func TestLanguagesHavePositiveAndNegative(t *testing.T) {
	t.Parallel()

	for _, lang := range AllLanguages() {
		t.Run(string(lang), func(t *testing.T) {
			t.Parallel()

			entries := ForLanguage(lang)
			require.NotNil(t, entries)

			posCount := 0
			negCount := 0

			for _, e := range entries {
				if e.Valence > 0 {
					posCount++
				} else {
					negCount++
				}
			}

			assert.Positive(t, posCount, "%s has no positive entries", lang)
			assert.Positive(t, negCount, "%s has no negative entries", lang)
		})
	}
}
