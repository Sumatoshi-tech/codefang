// Package pipeline defines configuration option types for analysis pipeline items.
package pipeline

import (
	"fmt"
	"log"
	"strings"
)

// ConfigurationOptionType represents the possible types of a ConfigurationOption's value.
type ConfigurationOptionType int

const (
	// BoolConfigurationOption reflects the boolean value type.
	BoolConfigurationOption ConfigurationOptionType = iota
	// IntConfigurationOption reflects the integer value type.
	IntConfigurationOption
	// StringConfigurationOption reflects the string value type.
	StringConfigurationOption
	// FloatConfigurationOption reflects a floating point value type.
	FloatConfigurationOption
	// StringsConfigurationOption reflects the array of strings value type.
	StringsConfigurationOption
	// PathConfigurationOption reflects the file system path value type.
	PathConfigurationOption
)

// String returns an empty string for the boolean type, "int" for integers and "string" for
// strings. It is used in the command line interface to show the argument's type.
func (opt ConfigurationOptionType) String() string {
	switch opt {
	case BoolConfigurationOption:
		return ""
	case IntConfigurationOption:
		return "int"
	case StringConfigurationOption:
		return "string"
	case FloatConfigurationOption:
		return "float"
	case StringsConfigurationOption:
		return "string"
	case PathConfigurationOption:
		return "path"
	}

	log.Panicf("Invalid ConfigurationOptionType value %d", opt)

	return ""
}

// ConfigurationOption allows for the unified, retrospective way to setup PipelineItem-s.
type ConfigurationOption struct {
	// Default is the initial value of the configuration option.
	Default any
	// Name identifies the configuration option in facts.
	Name string
	// Description represents the help text about the configuration option.
	Description string
	// Flag corresponds to the CLI token with "--" prepended.
	Flag string
	// Type specifies the kind of the configuration option's value.
	Type ConfigurationOptionType
}

// FormatDefault converts the default value of ConfigurationOption to string.
// Used in the command line interface to show the argument's default value.
func (opt ConfigurationOption) FormatDefault() string {
	if opt.Type == StringsConfigurationOption {
		strSlice, ok := opt.Default.([]string)
		if !ok {
			return fmt.Sprint(opt.Default)
		}

		return fmt.Sprintf("%q", strings.Join(strSlice, ","))
	}

	if opt.Type != StringConfigurationOption {
		return fmt.Sprint(opt.Default)
	}

	return fmt.Sprintf("%q", opt.Default)
}
