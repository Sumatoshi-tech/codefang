// Package version provides the build version information for the Codefang binary.
package version

import (
	"reflect"
	"strconv"
	"strings"
)

// Version is the release version, injected via ldflags at build time.
var Version = "dev"

// Commit is the git commit hash, injected via ldflags at build time.
var Commit = "none"

// Date is the build date, injected via ldflags at build time.
var Date = "unknown"

// BinaryGitHash is the Git hash of the Codefang binary file which is executing.
var BinaryGitHash = "<unknown>"

// Binary is Codefang's API version. It matches the package name.
var Binary = 0

type versionProbe struct{}

// InitBinaryVersion extracts the API version from the package path and sets Binary.
func InitBinaryVersion() {
	parts := strings.Split(reflect.TypeFor[versionProbe]().PkgPath(), ".")

	parsed, parseErr := strconv.Atoi(parts[len(parts)-1][1:])
	if parseErr == nil {
		Binary = parsed
	}
}
