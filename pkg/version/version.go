// Package version provides the build version information for the Codefang binary.
package version

import (
	"reflect"
	"strconv"
	"strings"
)

// Version is the release version, injected via ldflags at build time.
var Version = "dev" //nolint:gochecknoglobals // package-level version state is intentional.

// Commit is the git commit hash, injected via ldflags at build time.
var Commit = "none" //nolint:gochecknoglobals // package-level version state is intentional.

// Date is the build date, injected via ldflags at build time.
var Date = "unknown" //nolint:gochecknoglobals // package-level version state is intentional.

// BinaryGitHash is the Git hash of the Codefang binary file which is executing.
var BinaryGitHash = "<unknown>" //nolint:gochecknoglobals // package-level version state is intentional.

// Binary is Codefang's API version. It matches the package name.
var Binary = 0 //nolint:gochecknoglobals // package-level version state is intentional.

type versionProbe struct{}

//nolint:gochecknoinits // init is required to extract the version from the package path at startup.
func init() {
	parts := strings.Split(reflect.TypeFor[versionProbe]().PkgPath(), ".")

	parsed, parseErr := strconv.Atoi(parts[len(parts)-1][1:])
	if parseErr == nil {
		Binary = parsed
	}
}
