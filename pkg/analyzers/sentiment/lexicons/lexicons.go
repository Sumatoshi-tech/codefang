// Package lexicons provides multilingual sentiment dictionaries for code comment analysis.
//
// Lexicon data is sourced from Chen & Skiena (2014) "Building Sentiment Lexicons
// for All Major Languages" (ACL 2014, https://aclanthology.org/P14-2063/).
// The dataset covers 136 languages; we embed the 32 most common in software projects.
//
// Each entry maps a word to a valence score on the VADER scale:
// +1.5 for positive words, -1.5 for negative words.
//
// To regenerate the lexicon data after updating source files:
//
//	go run tools/lexgen/lexgen.go -pos pos_clean.txt -neg neg_clean.txt \
//	  -o pkg/analyzers/sentiment/lexicons/lexicon_data.gen.go
package lexicons

// Entry holds a single lexicon entry with valence on the VADER scale [-4, +4].
type Entry struct {
	Word    string
	Valence float64
}

// Language identifies a supported lexicon language.
type Language string

// Supported languages with ISO 639-1 codes.
const (
	LangArabic     Language = "ar"
	LangBulgarian  Language = "bg"
	LangChinese    Language = "zh"
	LangCroatian   Language = "hr"
	LangCzech      Language = "cs"
	LangDanish     Language = "da"
	LangDutch      Language = "nl"
	LangFinnish    Language = "fi"
	LangFrench     Language = "fr"
	LangGerman     Language = "de"
	LangGreek      Language = "el"
	LangHebrew     Language = "he"
	LangHindi      Language = "hi"
	LangHungarian  Language = "hu"
	LangIndonesian Language = "id"
	LangItalian    Language = "it"
	LangJapanese   Language = "ja"
	LangKorean     Language = "ko"
	LangMalay      Language = "ms"
	LangNorwegian  Language = "no"
	LangPersian    Language = "fa"
	LangPolish     Language = "pl"
	LangPortuguese Language = "pt"
	LangRomanian   Language = "ro"
	LangRussian    Language = "ru"
	LangSlovak     Language = "sk"
	LangSpanish    Language = "es"
	LangSwedish    Language = "sv"
	LangThai       Language = "th"
	LangTurkish    Language = "tr"
	LangUkrainian  Language = "uk"
	LangVietnamese Language = "vi"
)

// languageRegistry maps language codes to their lexicon loader functions and display names.
var languageRegistry = map[Language]struct {
	name   string
	loader func() []Entry
}{
	LangArabic:     {"Arabic", arabicLexicon},
	LangBulgarian:  {"Bulgarian", bulgarianLexicon},
	LangChinese:    {"Chinese", chineseLexicon},
	LangCroatian:   {"Croatian", croatianLexicon},
	LangCzech:      {"Czech", czechLexicon},
	LangDanish:     {"Danish", danishLexicon},
	LangDutch:      {"Dutch", dutchLexicon},
	LangFinnish:    {"Finnish", finnishLexicon},
	LangFrench:     {"French", frenchLexicon},
	LangGerman:     {"German", germanLexicon},
	LangGreek:      {"Greek", greekLexicon},
	LangHebrew:     {"Hebrew", hebrewLexicon},
	LangHindi:      {"Hindi", hindiLexicon},
	LangHungarian:  {"Hungarian", hungarianLexicon},
	LangIndonesian: {"Indonesian", indonesianLexicon},
	LangItalian:    {"Italian", italianLexicon},
	LangJapanese:   {"Japanese", japaneseLexicon},
	LangKorean:     {"Korean", koreanLexicon},
	LangMalay:      {"Malay", malayLexicon},
	LangNorwegian:  {"Norwegian", norwegianLexicon},
	LangPersian:    {"Persian", persianLexicon},
	LangPolish:     {"Polish", polishLexicon},
	LangPortuguese: {"Portuguese", portugueseLexicon},
	LangRomanian:   {"Romanian", romanianLexicon},
	LangRussian:    {"Russian", russianLexicon},
	LangSlovak:     {"Slovak", slovakLexicon},
	LangSpanish:    {"Spanish", spanishLexicon},
	LangSwedish:    {"Swedish", swedishLexicon},
	LangThai:       {"Thai", thaiLexicon},
	LangTurkish:    {"Turkish", turkishLexicon},
	LangUkrainian:  {"Ukrainian", ukrainianLexicon},
	LangVietnamese: {"Vietnamese", vietnameseLexicon},
}

// AllLanguages returns all supported lexicon languages.
func AllLanguages() []Language {
	langs := make([]Language, 0, len(languageRegistry))
	for lang := range languageRegistry {
		langs = append(langs, lang)
	}

	return langs
}

// LanguageName returns the display name for a language code.
func LanguageName(lang Language) string {
	if info, ok := languageRegistry[lang]; ok {
		return info.name
	}

	return string(lang)
}

// ForLanguage returns the lexicon entries for the given language.
// Returns nil if the language is not supported.
func ForLanguage(lang Language) []Entry {
	if info, ok := languageRegistry[lang]; ok {
		return info.loader()
	}

	return nil
}

// All returns combined lexicon entries from all supported languages.
func All() []Entry {
	all := make([]Entry, 0, EntryCount())

	for _, info := range languageRegistry {
		all = append(all, info.loader()...)
	}

	return all
}

// LanguageCount returns the number of supported languages.
func LanguageCount() int {
	return len(languageRegistry)
}

// EntryCount returns the total number of lexicon entries across all languages.
func EntryCount() int {
	total := 0

	for _, info := range languageRegistry {
		total += len(info.loader())
	}

	return total
}
