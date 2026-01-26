// Package importmodel defines the data model for source file import analysis.
package importmodel

// File represents a source file with its detected imports, language, and any parse error.
type File struct {
	Imports []string
	Lang    string
	Error   error
}
