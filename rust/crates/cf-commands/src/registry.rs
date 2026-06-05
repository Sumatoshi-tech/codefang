//! Minimal analyzer-registration interfaces that the command tree builds on.
//!
//! `cf-commands` is the analyzer-registration aggregation point (DESIGN §1 tier
//! 8). In Go, `registerAnalyzerFlags` (`run.go`) iterates every analyzer's
//! `ListConfigurationOptions()` and registers one CLI flag per option. The
//! analyzers themselves live in crates that are not yet building in this tree,
//! so this module defines the **minimal contract** the flag builder needs —
//! [`ConfigOptionProvider`] — expressed purely in terms of the already-stable
//! [`cf_pipeline::ConfigurationOption`].
//!
//! When the analyzer crates compile, each analyzer's Rust type implements
//! [`ConfigOptionProvider`] (delegating to its existing
//! `list_configuration_options`), and the runtime pipeline builder collects the
//! Core + Leaf analyzers and hands their providers to
//! [`crate::flags::register_analyzer_flags`]. Until then, the flag builder is
//! exercised with the explicit option lists in
//! [`crate::flags::default_analyzer_options`].

use cf_pipeline::ConfigurationOption;

/// Anything that can describe its configuration options as
/// [`cf_pipeline::ConfigurationOption`]s — the Rust analogue of the relevant
/// slice of Go's analyzer interface (`ListConfigurationOptions() []pipeline.ConfigurationOption`).
///
/// Used by [`crate::flags::register_analyzer_flags`] to build one clap flag per
/// option, deduplicating by [`ConfigurationOption::flag`] exactly as Go's
/// `registerAnalyzerFlags` does with its `registeredFlags` set.
pub trait ConfigOptionProvider {
    /// Returns this provider's configuration options, in declaration order.
    fn list_configuration_options(&self) -> Vec<ConfigurationOption>;
}

impl ConfigOptionProvider for Vec<ConfigurationOption> {
    fn list_configuration_options(&self) -> Vec<ConfigurationOption> {
        self.clone()
    }
}

impl ConfigOptionProvider for &[ConfigurationOption] {
    fn list_configuration_options(&self) -> Vec<ConfigurationOption> {
        self.to_vec()
    }
}

/// Errors raised while wiring analyzer flags into the command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// A configuration option's declared kind did not match the type of its
    /// default value (Go's `registerConfigFlag` silently skips this case via a
    /// failed type assertion; we surface it so misconfiguration is visible).
    DefaultTypeMismatch {
        /// The offending flag name.
        flag: String,
    },
}

impl core::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RegistrationError::DefaultTypeMismatch { flag } => {
                write!(f, "configuration option {flag}: default value type mismatch")
            }
        }
    }
}

impl std::error::Error for RegistrationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_pipeline::{ConfigurationOptionType, DefaultValue};

    fn opt(flag: &str) -> ConfigurationOption {
        ConfigurationOption {
            name: "Name".into(),
            flag: flag.into(),
            description: "desc".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(0),
        }
    }

    #[test]
    fn vec_provider_returns_its_options() {
        let opts = vec![opt("a"), opt("b")];
        let listed = opts.list_configuration_options();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].flag, "a");
    }

    #[test]
    fn slice_provider_returns_its_options() {
        let opts = vec![opt("x")];
        let provider: &[ConfigurationOption] = &opts;
        assert_eq!(provider.list_configuration_options().len(), 1);
    }

    #[test]
    fn registration_error_display() {
        let e = RegistrationError::DefaultTypeMismatch { flag: "f".into() };
        assert_eq!(
            e.to_string(),
            "configuration option f: default value type mismatch"
        );
    }
}
