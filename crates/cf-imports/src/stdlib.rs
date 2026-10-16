//! Per-language standard-library classification.
//!
//! [`is_stdlib`] answers "does this import path name the language's own
//! standard library?" using the importing file's language, so the
//! `stdlib` / `external` split in the metrics is meaningful per ecosystem:
//!
//! | Language | Rule |
//! |---|---|
//! | Go | the Go toolchain's `IsStandardImportPath`: the first path element has no `.` (`errors`, `net/http`; not `github.com/x/y`) |
//! | Python | top-level module in CPython's `sys.stdlib_module_names` (`os.path` -> `os`) |
//! | JavaScript / TypeScript | a `node:`-prefixed specifier, or a Node builtin (`fs`, `fs/promises`, `node:test`) |
//! | Rust | first `::` segment is `std`, `core`, `alloc`, `proc_macro` or `test` |
//! | Java | `java.`, `javax.`, `jdk.` prefixes |
//! | Kotlin | `kotlin.` prefix, or the Java rule |
//! | C# | `System` or `System.…` |
//! | C | a C standard / POSIX header name (`stdio.h`, `sys/types.h`) |
//! | C++ | the C rule, or a C++ standard header (`vector`, `cstdio`) |
//! | Ruby | a default or bundled library (`json`, `net/http`, `fileutils`) |
//! | PHP | none — PHP has no importable standard namespace |
//! | other / unknown | any of the tables above (the union) |

use crate::language::Language;

/// Node.js builtin-module specifier prefix (`node:fs`).
const NODE_PREFIX: &str = "node:";
/// Rust path separator.
const RUST_SEP: &str = "::";
/// The C# base-class-library root namespace.
const CSHARP_SYSTEM: &str = "System";

/// Go standard-library packages (`go list std`, Go 1.26, without
/// `internal`/`vendor` paths). Go files use the toolchain's first-element
/// rule ([`go_is_std`]); this table serves the language-less union, where
/// the rule would claim every bare name.
const GO_STDLIB: &[&str] = &[
    "archive/tar",
    "archive/zip",
    "bufio",
    "bytes",
    "cmp",
    "compress/bzip2",
    "compress/flate",
    "compress/gzip",
    "compress/lzw",
    "compress/zlib",
    "container/heap",
    "container/list",
    "container/ring",
    "context",
    "crypto",
    "crypto/aes",
    "crypto/cipher",
    "crypto/des",
    "crypto/dsa",
    "crypto/ecdh",
    "crypto/ecdsa",
    "crypto/ed25519",
    "crypto/elliptic",
    "crypto/fips140",
    "crypto/hkdf",
    "crypto/hmac",
    "crypto/hpke",
    "crypto/md5",
    "crypto/mlkem",
    "crypto/mlkem/mlkemtest",
    "crypto/pbkdf2",
    "crypto/rand",
    "crypto/rc4",
    "crypto/rsa",
    "crypto/sha1",
    "crypto/sha256",
    "crypto/sha3",
    "crypto/sha512",
    "crypto/subtle",
    "crypto/tls",
    "crypto/x509",
    "crypto/x509/pkix",
    "database/sql",
    "database/sql/driver",
    "debug/buildinfo",
    "debug/dwarf",
    "debug/elf",
    "debug/gosym",
    "debug/macho",
    "debug/pe",
    "debug/plan9obj",
    "embed",
    "encoding",
    "encoding/ascii85",
    "encoding/asn1",
    "encoding/base32",
    "encoding/base64",
    "encoding/binary",
    "encoding/csv",
    "encoding/gob",
    "encoding/hex",
    "encoding/json",
    "encoding/pem",
    "encoding/xml",
    "errors",
    "expvar",
    "flag",
    "fmt",
    "go/ast",
    "go/build",
    "go/build/constraint",
    "go/constant",
    "go/doc",
    "go/doc/comment",
    "go/format",
    "go/importer",
    "go/parser",
    "go/printer",
    "go/scanner",
    "go/token",
    "go/types",
    "go/version",
    "hash",
    "hash/adler32",
    "hash/crc32",
    "hash/crc64",
    "hash/fnv",
    "hash/maphash",
    "html",
    "html/template",
    "image",
    "image/color",
    "image/color/palette",
    "image/draw",
    "image/gif",
    "image/jpeg",
    "image/png",
    "index/suffixarray",
    "io",
    "io/fs",
    "io/ioutil",
    "iter",
    "log",
    "log/slog",
    "log/syslog",
    "maps",
    "math",
    "math/big",
    "math/bits",
    "math/cmplx",
    "math/rand",
    "math/rand/v2",
    "mime",
    "mime/multipart",
    "mime/quotedprintable",
    "net",
    "net/http",
    "net/http/cgi",
    "net/http/cookiejar",
    "net/http/fcgi",
    "net/http/httptest",
    "net/http/httptrace",
    "net/http/httputil",
    "net/http/pprof",
    "net/mail",
    "net/netip",
    "net/rpc",
    "net/rpc/jsonrpc",
    "net/smtp",
    "net/textproto",
    "net/url",
    "os",
    "os/exec",
    "os/signal",
    "os/user",
    "path",
    "path/filepath",
    "plugin",
    "reflect",
    "regexp",
    "regexp/syntax",
    "runtime",
    "runtime/cgo",
    "runtime/coverage",
    "runtime/debug",
    "runtime/metrics",
    "runtime/pprof",
    "runtime/race",
    "runtime/trace",
    "slices",
    "sort",
    "strconv",
    "strings",
    "structs",
    "sync",
    "sync/atomic",
    "syscall",
    "testing",
    "testing/cryptotest",
    "testing/fstest",
    "testing/iotest",
    "testing/quick",
    "testing/slogtest",
    "testing/synctest",
    "text/scanner",
    "text/tabwriter",
    "text/template",
    "text/template/parse",
    "time",
    "time/tzdata",
    "unicode",
    "unicode/utf16",
    "unicode/utf8",
    "unique",
    "unsafe",
    "weak",
];

/// Rust crates that ship with the toolchain.
const RUST_STD_CRATES: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];

/// Java platform package prefixes.
const JAVA_STD_PREFIXES: &[&str] = &["java.", "javax.", "jdk."];

/// Kotlin standard-library package prefix.
const KOTLIN_STD_PREFIXES: &[&str] = &["kotlin."];

/// Python standard-library top-level module names
/// (`sys.stdlib_module_names`, CPython 3.12).
const PYTHON_STDLIB: &[&str] = &[
    "__future__",
    "_abc",
    "_aix_support",
    "_ast",
    "_asyncio",
    "_bisect",
    "_blake2",
    "_bz2",
    "_codecs",
    "_codecs_cn",
    "_codecs_hk",
    "_codecs_iso2022",
    "_codecs_jp",
    "_codecs_kr",
    "_codecs_tw",
    "_collections",
    "_collections_abc",
    "_compat_pickle",
    "_compression",
    "_contextvars",
    "_crypt",
    "_csv",
    "_ctypes",
    "_curses",
    "_curses_panel",
    "_datetime",
    "_dbm",
    "_decimal",
    "_elementtree",
    "_frozen_importlib",
    "_frozen_importlib_external",
    "_functools",
    "_gdbm",
    "_hashlib",
    "_heapq",
    "_imp",
    "_io",
    "_json",
    "_locale",
    "_lsprof",
    "_lzma",
    "_markupbase",
    "_md5",
    "_msi",
    "_multibytecodec",
    "_multiprocessing",
    "_opcode",
    "_operator",
    "_osx_support",
    "_overlapped",
    "_pickle",
    "_posixshmem",
    "_posixsubprocess",
    "_py_abc",
    "_pydatetime",
    "_pydecimal",
    "_pyio",
    "_pylong",
    "_queue",
    "_random",
    "_scproxy",
    "_sha1",
    "_sha2",
    "_sha3",
    "_signal",
    "_sitebuiltins",
    "_socket",
    "_sqlite3",
    "_sre",
    "_ssl",
    "_stat",
    "_statistics",
    "_string",
    "_strptime",
    "_struct",
    "_symtable",
    "_thread",
    "_threading_local",
    "_tkinter",
    "_tokenize",
    "_tracemalloc",
    "_typing",
    "_uuid",
    "_warnings",
    "_weakref",
    "_weakrefset",
    "_winapi",
    "_wmi",
    "_zoneinfo",
    "abc",
    "aifc",
    "antigravity",
    "argparse",
    "array",
    "ast",
    "asyncio",
    "atexit",
    "audioop",
    "base64",
    "bdb",
    "binascii",
    "bisect",
    "builtins",
    "bz2",
    "cProfile",
    "calendar",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codecs",
    "codeop",
    "collections",
    "colorsys",
    "compileall",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "copyreg",
    "crypt",
    "csv",
    "ctypes",
    "curses",
    "dataclasses",
    "datetime",
    "dbm",
    "decimal",
    "difflib",
    "dis",
    "doctest",
    "email",
    "encodings",
    "ensurepip",
    "enum",
    "errno",
    "faulthandler",
    "fcntl",
    "filecmp",
    "fileinput",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "genericpath",
    "getopt",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "grp",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "idlelib",
    "imaplib",
    "imghdr",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "lib2to3",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "mailbox",
    "mailcap",
    "marshal",
    "math",
    "mimetypes",
    "mmap",
    "modulefinder",
    "msilib",
    "msvcrt",
    "multiprocessing",
    "netrc",
    "nis",
    "nntplib",
    "nt",
    "ntpath",
    "nturl2path",
    "numbers",
    "opcode",
    "operator",
    "optparse",
    "os",
    "ossaudiodev",
    "pathlib",
    "pdb",
    "pickle",
    "pickletools",
    "pipes",
    "pkgutil",
    "platform",
    "plistlib",
    "poplib",
    "posix",
    "posixpath",
    "pprint",
    "profile",
    "pstats",
    "pty",
    "pwd",
    "py_compile",
    "pyclbr",
    "pydoc",
    "pydoc_data",
    "pyexpat",
    "queue",
    "quopri",
    "random",
    "re",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtplib",
    "sndhdr",
    "socket",
    "socketserver",
    "spwd",
    "sqlite3",
    "sre_compile",
    "sre_constants",
    "sre_parse",
    "ssl",
    "stat",
    "statistics",
    "string",
    "stringprep",
    "struct",
    "subprocess",
    "sunau",
    "symtable",
    "sys",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "tempfile",
    "termios",
    "textwrap",
    "this",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "trace",
    "traceback",
    "tracemalloc",
    "tty",
    "turtle",
    "turtledemo",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uu",
    "uuid",
    "venv",
    "warnings",
    "wave",
    "weakref",
    "webbrowser",
    "winreg",
    "winsound",
    "wsgiref",
    "xdrlib",
    "xml",
    "xmlrpc",
    "zipapp",
    "zipfile",
    "zipimport",
    "zlib",
    "zoneinfo",
];

/// Node.js builtin modules (`require('module').builtinModules`, Node 22).
/// `node:`-only modules (`node:test`, `node:sqlite`, `node:sea`) are covered
/// by the prefix rule.
const NODE_BUILTINS: &[&str] = &[
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_stream_duplex",
    "_stream_passthrough",
    "_stream_readable",
    "_stream_transform",
    "_stream_wrap",
    "_stream_writable",
    "_tls_common",
    "_tls_wrap",
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// C standard (C11) and common POSIX headers.
const C_STD_HEADERS: &[&str] = &[
    "assert.h",
    "complex.h",
    "ctype.h",
    "errno.h",
    "fenv.h",
    "float.h",
    "inttypes.h",
    "iso646.h",
    "limits.h",
    "locale.h",
    "math.h",
    "setjmp.h",
    "signal.h",
    "stdalign.h",
    "stdarg.h",
    "stdatomic.h",
    "stdbool.h",
    "stddef.h",
    "stdint.h",
    "stdio.h",
    "stdlib.h",
    "stdnoreturn.h",
    "string.h",
    "tgmath.h",
    "threads.h",
    "time.h",
    "uchar.h",
    "wchar.h",
    "wctype.h",
    // POSIX.
    "aio.h",
    "arpa/inet.h",
    "cpio.h",
    "dirent.h",
    "dlfcn.h",
    "fcntl.h",
    "fmtmsg.h",
    "fnmatch.h",
    "ftw.h",
    "getopt.h",
    "glob.h",
    "grp.h",
    "iconv.h",
    "langinfo.h",
    "libgen.h",
    "monetary.h",
    "mqueue.h",
    "ndbm.h",
    "net/if.h",
    "netdb.h",
    "netinet/in.h",
    "netinet/tcp.h",
    "nl_types.h",
    "poll.h",
    "pthread.h",
    "pwd.h",
    "regex.h",
    "sched.h",
    "search.h",
    "semaphore.h",
    "spawn.h",
    "strings.h",
    "stropts.h",
    "sys/ipc.h",
    "sys/mman.h",
    "sys/msg.h",
    "sys/resource.h",
    "sys/select.h",
    "sys/sem.h",
    "sys/shm.h",
    "sys/socket.h",
    "sys/stat.h",
    "sys/statvfs.h",
    "sys/time.h",
    "sys/times.h",
    "sys/types.h",
    "sys/uio.h",
    "sys/un.h",
    "sys/utsname.h",
    "sys/wait.h",
    "syslog.h",
    "tar.h",
    "termios.h",
    "trace.h",
    "ulimit.h",
    "unistd.h",
    "utime.h",
    "utmpx.h",
    "wordexp.h",
];

/// C++ standard headers (C++20, header names without `.h`).
const CPP_STD_HEADERS: &[&str] = &[
    "algorithm",
    "any",
    "array",
    "atomic",
    "barrier",
    "bit",
    "bitset",
    "cassert",
    "ccomplex",
    "cctype",
    "cerrno",
    "cfenv",
    "cfloat",
    "charconv",
    "chrono",
    "cinttypes",
    "ciso646",
    "climits",
    "clocale",
    "cmath",
    "codecvt",
    "compare",
    "complex",
    "concepts",
    "condition_variable",
    "coroutine",
    "csetjmp",
    "csignal",
    "cstdalign",
    "cstdarg",
    "cstdbool",
    "cstddef",
    "cstdint",
    "cstdio",
    "cstdlib",
    "cstring",
    "ctgmath",
    "ctime",
    "cuchar",
    "cwchar",
    "cwctype",
    "deque",
    "exception",
    "execution",
    "filesystem",
    "format",
    "forward_list",
    "fstream",
    "functional",
    "future",
    "initializer_list",
    "iomanip",
    "ios",
    "iosfwd",
    "iostream",
    "istream",
    "iterator",
    "latch",
    "limits",
    "list",
    "locale",
    "map",
    "memory",
    "memory_resource",
    "mutex",
    "new",
    "numbers",
    "numeric",
    "optional",
    "ostream",
    "queue",
    "random",
    "ranges",
    "ratio",
    "regex",
    "scoped_allocator",
    "semaphore",
    "set",
    "shared_mutex",
    "source_location",
    "span",
    "sstream",
    "stack",
    "stdexcept",
    "stop_token",
    "streambuf",
    "string",
    "string_view",
    "strstream",
    "syncstream",
    "system_error",
    "thread",
    "tuple",
    "type_traits",
    "typeindex",
    "typeinfo",
    "unordered_map",
    "unordered_set",
    "utility",
    "valarray",
    "variant",
    "vector",
    "version",
];

/// Ruby default and bundled libraries (`require` targets shipped with the
/// interpreter).
const RUBY_STDLIB: &[&str] = &[
    "English",
    "abbrev",
    "base64",
    "benchmark",
    "bigdecimal",
    "bundler",
    "cgi",
    "coverage",
    "csv",
    "date",
    "delegate",
    "did_you_mean",
    "digest",
    "drb",
    "erb",
    "error_highlight",
    "etc",
    "expect",
    "fcntl",
    "fiddle",
    "fileutils",
    "find",
    "forwardable",
    "getoptlong",
    "io/console",
    "io/nonblock",
    "io/wait",
    "ipaddr",
    "irb",
    "json",
    "logger",
    "matrix",
    "minitest",
    "monitor",
    "mutex_m",
    "net/ftp",
    "net/http",
    "net/imap",
    "net/pop",
    "net/protocol",
    "net/smtp",
    "nkf",
    "objspace",
    "observer",
    "open-uri",
    "open3",
    "openssl",
    "optparse",
    "ostruct",
    "pathname",
    "pp",
    "prettyprint",
    "prime",
    "pstore",
    "psych",
    "pty",
    "racc",
    "rake",
    "rbconfig",
    "rdoc",
    "readline",
    "reline",
    "resolv",
    "resolv-replace",
    "rexml",
    "rinda",
    "ripper",
    "rss",
    "rubygems",
    "securerandom",
    "set",
    "shellwords",
    "singleton",
    "socket",
    "stringio",
    "strscan",
    "syntax_suggest",
    "syslog",
    "tempfile",
    "test/unit",
    "thread",
    "time",
    "timeout",
    "tmpdir",
    "tsort",
    "typeprof",
    "un",
    "unicode_normalize",
    "uri",
    "weakref",
    "win32ole",
    "yaml",
    "zlib",
];

/// Every language with a dedicated table, used for the union rule (Go joins
/// through [`GO_STDLIB`], not its first-element rule).
const TABLED_LANGUAGES: &[Language] = &[
    Language::Python,
    Language::JavaScript,
    Language::Rust,
    Language::Java,
    Language::Kotlin,
    Language::CSharp,
    Language::C,
    Language::Cpp,
    Language::Ruby,
];

/// Reports whether `path` names the standard library of `language`.
///
/// `language` is a parser language name (`go`, `python`, …); an unknown or
/// empty name applies every table (the union), which is what the plot path
/// and the language-less history path rely on.
///
/// ```
/// use cf_imports::stdlib::is_stdlib;
///
/// assert!(is_stdlib("go", "strconv"));
/// assert!(is_stdlib("go", "net/http"));
/// assert!(!is_stdlib("go", "github.com/example/lib"));
/// assert!(is_stdlib("python", "os.path"));
/// assert!(is_stdlib("javascript", "node:fs/promises"));
/// assert!(is_stdlib("rust", "std::collections::HashMap"));
/// assert!(is_stdlib("", "subprocess"));
/// ```
#[must_use]
pub fn is_stdlib(language: &str, path: &str) -> bool {
    is_stdlib_in(Language::from_name(language), path)
}

/// [`is_stdlib`] over a resolved [`Language`].
fn is_stdlib_in(language: Language, path: &str) -> bool {
    match language {
        Language::Go => go_is_std(path),
        Language::Python => PYTHON_STDLIB.contains(&first_segment(path, ".")),
        Language::JavaScript => node_is_builtin(path),
        Language::Rust => RUST_STD_CRATES.contains(&first_segment(path, RUST_SEP)),
        Language::Java => has_any_prefix(path, JAVA_STD_PREFIXES),
        Language::Kotlin => {
            has_any_prefix(path, KOTLIN_STD_PREFIXES) || has_any_prefix(path, JAVA_STD_PREFIXES)
        }
        Language::CSharp => path == CSHARP_SYSTEM || path.starts_with("System."),
        Language::C => C_STD_HEADERS.contains(&path),
        Language::Cpp => C_STD_HEADERS.contains(&path) || CPP_STD_HEADERS.contains(&path),
        Language::Ruby => {
            RUBY_STDLIB.contains(&path) || RUBY_STDLIB.contains(&first_segment(path, "/"))
        }
        Language::Php => false,
        Language::Other => {
            GO_STDLIB.contains(&path) || TABLED_LANGUAGES.iter().any(|l| is_stdlib_in(*l, path))
        }
    }
}

/// The Go toolchain's `IsStandardImportPath`: a path whose first element
/// carries no dot is a standard-library (or toolchain-internal) package.
fn go_is_std(path: &str) -> bool {
    let head = first_segment(path, "/");
    !head.is_empty() && !head.contains('.')
}

/// Node builtin: `node:`-prefixed, or an exact / first-segment table match.
fn node_is_builtin(path: &str) -> bool {
    if path.starts_with(NODE_PREFIX) {
        return true;
    }
    NODE_BUILTINS.contains(&path) || NODE_BUILTINS.contains(&first_segment(path, "/"))
}

/// The substring before the first `sep` (or the whole path).
fn first_segment<'a>(path: &'a str, sep: &str) -> &'a str {
    path.split(sep).next().unwrap_or(path)
}

/// True when `path` starts with any of `prefixes`.
fn has_any_prefix(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| path.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample of the `go list std` output, one per top-level family.
    const GO_STD_SAMPLE: &[&str] = &[
        "archive/tar",
        "bufio",
        "bytes",
        "compress/gzip",
        "container/heap",
        "context",
        "crypto/sha256",
        "database/sql",
        "debug/elf",
        "embed",
        "encoding/json",
        "errors",
        "expvar",
        "flag",
        "fmt",
        "go/ast",
        "hash/fnv",
        "html/template",
        "image/png",
        "io/fs",
        "iter",
        "log/slog",
        "maps",
        "math/rand/v2",
        "mime/multipart",
        "net/http/httptest",
        "os/exec",
        "path/filepath",
        "plugin",
        "reflect",
        "regexp/syntax",
        "runtime/debug",
        "slices",
        "sort",
        "strconv",
        "strings",
        "sync/atomic",
        "syscall",
        "testing/fstest",
        "text/template",
        "time",
        "unicode/utf8",
        "unique",
        "unsafe",
        "C",
    ];

    #[test]
    fn go_std_is_the_no_dot_first_element_rule() {
        for p in GO_STD_SAMPLE {
            assert!(is_stdlib("go", p), "expected Go std: {p}");
        }
        // The rule covers the whole toolchain list.
        for p in GO_STDLIB {
            assert!(is_stdlib("go", p), "expected Go std: {p}");
        }
        for p in [
            "github.com/example/lib/v2",
            "golang.org/x/net/context",
            "k8s.io/client-go/kubernetes",
            "example.com",
            "",
        ] {
            assert!(!is_stdlib("go", p), "expected external: {p}");
        }
    }

    #[test]
    fn python_std_by_top_level_module() {
        for p in [
            "os",
            "os.path",
            "sys",
            "datetime",
            "subprocess",
            "typing",
            "json",
            "re",
            "__future__",
        ] {
            assert!(is_stdlib("python", p), "expected Python std: {p}");
        }
        for p in ["numpy", "requests", "django.db", "x"] {
            assert!(!is_stdlib("python", p), "expected external: {p}");
        }
    }

    #[test]
    fn node_builtins_including_prefix_and_subpaths() {
        for lang in ["javascript", "typescript", "tsx"] {
            for p in [
                "fs",
                "fs/promises",
                "node:fs",
                "node:test",
                "path",
                "stream/web",
                "events",
            ] {
                assert!(is_stdlib(lang, p), "expected Node builtin ({lang}): {p}");
            }
            for p in ["react", "ns-lib", "@scope/pkg", "lodash/fp", "test"] {
                assert!(!is_stdlib(lang, p), "expected external ({lang}): {p}");
            }
        }
    }

    #[test]
    fn rust_toolchain_crates() {
        for p in [
            "std",
            "std::collections::HashMap",
            "core::fmt",
            "alloc",
            "std::io::*",
            "proc_macro::TokenStream",
        ] {
            assert!(is_stdlib("rust", p), "expected Rust std: {p}");
        }
        for p in ["serde::Serialize", "crate::foo::bar", "super::baz", "tokio"] {
            assert!(!is_stdlib("rust", p), "expected non-std: {p}");
        }
    }

    #[test]
    fn jvm_languages_and_csharp() {
        assert!(is_stdlib("java", "java.util.List"));
        assert!(is_stdlib("java", "javax.swing.JFrame"));
        assert!(is_stdlib("java", "jdk.internal.misc.Unsafe"));
        assert!(!is_stdlib("java", "org.junit.Test"));
        assert!(is_stdlib("kotlin", "kotlin.collections.Map"));
        assert!(is_stdlib("kotlin", "java.util.List"));
        assert!(!is_stdlib("kotlin", "kotlinx.coroutines.*"));
        assert!(is_stdlib("csharp", "System"));
        assert!(is_stdlib("csharp", "System.Collections.Generic"));
        assert!(!is_stdlib("csharp", "SystemX.Y"));
        assert!(!is_stdlib("csharp", "MyLib.Utils"));
    }

    #[test]
    fn c_family_headers_and_ruby_libraries() {
        assert!(is_stdlib("c", "stdio.h"));
        assert!(is_stdlib("c", "sys/types.h"));
        assert!(!is_stdlib("c", "local.h"));
        assert!(!is_stdlib("c", "curl/curl.h"));
        assert!(is_stdlib("cpp", "vector"));
        assert!(is_stdlib("cpp", "cstdio"));
        assert!(is_stdlib("cpp", "stdio.h"));
        assert!(!is_stdlib("cpp", "boost/asio.hpp"));
        assert!(is_stdlib("ruby", "json"));
        assert!(is_stdlib("ruby", "net/http"));
        assert!(is_stdlib("ruby", "json/ext"));
        assert!(!is_stdlib("ruby", "nokogiri"));
        assert!(!is_stdlib("ruby", "lib/x"));
        assert!(!is_stdlib("php", "App\\Models\\User"));
    }

    #[test]
    fn unknown_language_applies_the_union() {
        for p in [
            "fmt",
            "strconv",
            "net/http",
            "subprocess",
            "fs",
            "std::io",
            "java.util.List",
            "stdio.h",
            "json",
        ] {
            assert!(is_stdlib("", p), "expected union hit: {p}");
            assert!(is_stdlib("swift", p), "expected union hit: {p}");
        }
        // The union goes by Go's package LIST, so bare third-party names
        // (which Go's first-element rule would claim) stay external.
        for p in ["github.com/example/lib", "react", "numpy", "nokogiri", "C"] {
            assert!(!is_stdlib("", p), "expected external: {p}");
        }
    }
}
