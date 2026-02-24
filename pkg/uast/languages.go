package uast

import (
	"sync"
	"unsafe"

	sitter "github.com/alexaandru/go-tree-sitter-bare"

	"github.com/alexaandru/go-sitter-forest/ansible"
	"github.com/alexaandru/go-sitter-forest/bash"
	"github.com/alexaandru/go-sitter-forest/c"
	"github.com/alexaandru/go-sitter-forest/c_sharp"
	"github.com/alexaandru/go-sitter-forest/clojure"
	"github.com/alexaandru/go-sitter-forest/cmake"
	"github.com/alexaandru/go-sitter-forest/commonlisp"
	"github.com/alexaandru/go-sitter-forest/cpp"
	"github.com/alexaandru/go-sitter-forest/crystal"
	"github.com/alexaandru/go-sitter-forest/css"
	"github.com/alexaandru/go-sitter-forest/csv"
	"github.com/alexaandru/go-sitter-forest/dart"
	"github.com/alexaandru/go-sitter-forest/dockerfile"
	"github.com/alexaandru/go-sitter-forest/dotenv"
	"github.com/alexaandru/go-sitter-forest/elixir"
	"github.com/alexaandru/go-sitter-forest/elm"
	"github.com/alexaandru/go-sitter-forest/fish"
	"github.com/alexaandru/go-sitter-forest/fortran"
	"github.com/alexaandru/go-sitter-forest/git_config"
	"github.com/alexaandru/go-sitter-forest/gitattributes"
	"github.com/alexaandru/go-sitter-forest/gitignore"
	golang "github.com/alexaandru/go-sitter-forest/go"
	"github.com/alexaandru/go-sitter-forest/gosum"
	"github.com/alexaandru/go-sitter-forest/gotmpl"
	"github.com/alexaandru/go-sitter-forest/gowork"
	"github.com/alexaandru/go-sitter-forest/graphql"
	"github.com/alexaandru/go-sitter-forest/groovy"
	"github.com/alexaandru/go-sitter-forest/haskell"
	"github.com/alexaandru/go-sitter-forest/hcl"
	"github.com/alexaandru/go-sitter-forest/helm"
	"github.com/alexaandru/go-sitter-forest/html"
	"github.com/alexaandru/go-sitter-forest/ini"
	"github.com/alexaandru/go-sitter-forest/java"
	"github.com/alexaandru/go-sitter-forest/javascript"
	"github.com/alexaandru/go-sitter-forest/json"
	"github.com/alexaandru/go-sitter-forest/kotlin"
	"github.com/alexaandru/go-sitter-forest/latex"
	"github.com/alexaandru/go-sitter-forest/lua"
	"github.com/alexaandru/go-sitter-forest/make"
	"github.com/alexaandru/go-sitter-forest/markdown"
	"github.com/alexaandru/go-sitter-forest/markdown_inline"
	"github.com/alexaandru/go-sitter-forest/nim"
	"github.com/alexaandru/go-sitter-forest/nim_format_string"
	"github.com/alexaandru/go-sitter-forest/perl"
	"github.com/alexaandru/go-sitter-forest/php"
	"github.com/alexaandru/go-sitter-forest/powershell"
	"github.com/alexaandru/go-sitter-forest/properties"
	"github.com/alexaandru/go-sitter-forest/proto"
	"github.com/alexaandru/go-sitter-forest/proxima"
	"github.com/alexaandru/go-sitter-forest/prql"
	"github.com/alexaandru/go-sitter-forest/psv"
	"github.com/alexaandru/go-sitter-forest/python"
	"github.com/alexaandru/go-sitter-forest/r"
	"github.com/alexaandru/go-sitter-forest/rego"
	"github.com/alexaandru/go-sitter-forest/ruby"
	"github.com/alexaandru/go-sitter-forest/rust"
	"github.com/alexaandru/go-sitter-forest/rust_with_rstml"
	"github.com/alexaandru/go-sitter-forest/scala"
	"github.com/alexaandru/go-sitter-forest/sql"
	"github.com/alexaandru/go-sitter-forest/ssh_config"
	"github.com/alexaandru/go-sitter-forest/swift"
	"github.com/alexaandru/go-sitter-forest/tcl"
	"github.com/alexaandru/go-sitter-forest/toml"
	"github.com/alexaandru/go-sitter-forest/tsx"
	"github.com/alexaandru/go-sitter-forest/typescript"
	"github.com/alexaandru/go-sitter-forest/xml"
	"github.com/alexaandru/go-sitter-forest/yaml"
	"github.com/alexaandru/go-sitter-forest/zig"
)

// languageFuncs maps language names to their tree-sitter GetLanguage functions.
// Only the languages with .uastmap files are included.
var languageFuncs = map[string]func() unsafe.Pointer{
	"ansible":           ansible.GetLanguage,
	"bash":              bash.GetLanguage,
	"c":                 c.GetLanguage,
	"c_sharp":           c_sharp.GetLanguage,
	"clojure":           clojure.GetLanguage,
	"cmake":             cmake.GetLanguage,
	"commonlisp":        commonlisp.GetLanguage,
	"cpp":               cpp.GetLanguage,
	"crystal":           crystal.GetLanguage,
	"css":               css.GetLanguage,
	"csv":               csv.GetLanguage,
	"dart":              dart.GetLanguage,
	"dockerfile":        dockerfile.GetLanguage,
	"dotenv":            dotenv.GetLanguage,
	"elixir":            elixir.GetLanguage,
	"elm":               elm.GetLanguage,
	"fish":              fish.GetLanguage,
	"fortran":           fortran.GetLanguage,
	"git_config":        git_config.GetLanguage,
	"gitattributes":     gitattributes.GetLanguage,
	"gitignore":         gitignore.GetLanguage,
	"go":                golang.GetLanguage,
	"gosum":             gosum.GetLanguage,
	"gotmpl":            gotmpl.GetLanguage,
	"gowork":            gowork.GetLanguage,
	"graphql":           graphql.GetLanguage,
	"groovy":            groovy.GetLanguage,
	"haskell":           haskell.GetLanguage,
	"hcl":               hcl.GetLanguage,
	"helm":              helm.GetLanguage,
	"html":              html.GetLanguage,
	"ini":               ini.GetLanguage,
	"java":              java.GetLanguage,
	"javascript":        javascript.GetLanguage,
	"json":              json.GetLanguage,
	"kotlin":            kotlin.GetLanguage,
	"latex":             latex.GetLanguage,
	"lua":               lua.GetLanguage,
	"make":              make.GetLanguage,
	"markdown":          markdown.GetLanguage,
	"markdown_inline":   markdown_inline.GetLanguage,
	"nim":               nim.GetLanguage,
	"nim_format_string": nim_format_string.GetLanguage,
	"perl":              perl.GetLanguage,
	"php":               php.GetLanguage,
	"powershell":        powershell.GetLanguage,
	"properties":        properties.GetLanguage,
	"proto":             proto.GetLanguage,
	"proxima":           proxima.GetLanguage,
	"prql":              prql.GetLanguage,
	"psv":               psv.GetLanguage,
	"python":            python.GetLanguage,
	"r":                 r.GetLanguage,
	"rego":              rego.GetLanguage,
	"ruby":              ruby.GetLanguage,
	"rust":              rust.GetLanguage,
	"rust_with_rstml":   rust_with_rstml.GetLanguage,
	"scala":             scala.GetLanguage,
	"sql":               sql.GetLanguage,
	"ssh_config":        ssh_config.GetLanguage,
	"swift":             swift.GetLanguage,
	"tcl":               tcl.GetLanguage,
	"toml":              toml.GetLanguage,
	"tsx":               tsx.GetLanguage,
	"typescript":        typescript.GetLanguage,
	"xml":               xml.GetLanguage,
	"yaml":              yaml.GetLanguage,
	"zig":               zig.GetLanguage,
}

var languageCache sync.Map

// GetLanguage returns the tree-sitter Language for the given name, or nil if not supported.
func GetLanguage(name string) *sitter.Language {
	if cached, ok := languageCache.Load(name); ok {
		lang, castOK := cached.(*sitter.Language)
		if castOK {
			return lang
		}
	}

	fn, ok := languageFuncs[name]
	if !ok {
		return nil
	}

	lang := sitter.NewLanguage(fn())
	languageCache.Store(name, lang)

	return lang
}
