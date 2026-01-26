// Package spec provides embedded UAST schema specifications.
package spec

import "embed"

// UASTSchemaFS contains the embedded UAST JSON schema.
//
//go:embed uast-schema.json
var UASTSchemaFS embed.FS
