package version

import (
	"reflect"
	"strconv"
	"strings"
)

// BinaryGitHash is the Git hash of the Codefang binary file which is executing.
var BinaryGitHash = "<unknown>"

// BinaryVersion is Codefang's API version. It matches the package name.
var Binary = 0

type versionProbe struct{}

func init() {
	parts := strings.Split(reflect.TypeOf(versionProbe{}).PkgPath(), ".")
	Binary, _ = strconv.Atoi(parts[len(parts)-1][1:])
}
