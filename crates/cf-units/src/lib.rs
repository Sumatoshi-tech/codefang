//! Unit/quantity helpers — binary size unit multipliers (1024-based).
//!
//! `KIB`, `MIB`, and `GIB` are used by `cf-budget`, `cf-framework`, and
//! `cf-streaming` (and by the `codefang run` command) to convert byte counts
//! to mebibytes for budgets, limits, and observability attributes, e.g.
//! `bytes / cf_units::MIB`.
//!
//! Every consumer uses them in `i64` arithmetic, so they are exposed as `i64`
//! constants:
//!
//! - [`KIB`] = 1024
//! - [`MIB`] = 1024 × [`KIB`] = 1\u{a0}048\u{a0}576
//! - [`GIB`] = 1024 × [`MIB`] = 1\u{a0}073\u{a0}741\u{a0}824
//!
//! # Examples
//!
//! ```
//! use cf_units::{KIB, MIB, GIB};
//!
//! assert_eq!(KIB, 1024);
//! assert_eq!(MIB, 1024 * KIB);
//! assert_eq!(GIB, 1024 * MIB);
//!
//! // Typical consumer usage: convert a byte count to mebibytes.
//! let budget_bytes: i64 = 512 * MIB;
//! assert_eq!(budget_bytes / MIB, 512);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// One kibibyte: `1024` bytes.
pub const KIB: i64 = 1024;

/// One mebibyte: `1024 * KIB` = `1_048_576` bytes.
pub const MIB: i64 = 1024 * KIB;

/// One gibibyte: `1024 * MIB` = `1_073_741_824` bytes.
pub const GIB: i64 = 1024 * MIB;

#[cfg(test)]
mod tests {
    use super::*;

    // Expected binary size multiplier values.
    const EXPECTED_KIB: i64 = 1024;
    const EXPECTED_MIB: i64 = 1024 * 1024;
    const EXPECTED_GIB: i64 = 1024 * 1024 * 1024;

    #[test]
    fn binary_size_constants() {
        let cases: &[(&str, i64, i64)] = &[
            ("KiB equals 1024", KIB, EXPECTED_KIB),
            ("MiB equals 1024*KiB", MIB, EXPECTED_MIB),
            ("GiB equals 1024*MiB", GIB, EXPECTED_GIB),
        ];
        for (name, got, want) in cases {
            assert_eq!(got, want, "{name}: got {got}, want {want}");
        }
    }

    #[test]
    fn binary_size_relationships() {
        // "MiB is 1024 KiB"
        const KIB_PER_MIB: i64 = 1024;
        assert_eq!(MIB, KIB_PER_MIB * KIB, "MiB ({MIB}) != 1024*KiB");

        // "GiB is 1024 MiB"
        const MIB_PER_GIB: i64 = 1024;
        assert_eq!(GIB, MIB_PER_GIB * MIB, "GiB ({GIB}) != 1024*MiB");
    }

    /// Concrete value check independent of the arithmetic expressions above.
    #[test]
    fn concrete_values() {
        assert_eq!(KIB, 1_024);
        assert_eq!(MIB, 1_048_576);
        assert_eq!(GIB, 1_073_741_824);
    }
}
