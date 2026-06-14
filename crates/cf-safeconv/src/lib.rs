//! Overflow-checked numeric and type conversions.
//!
//! Three families of conversion helpers:
//!
//! - `must_*` variants **panic** on overflow / out-of-range / sign loss.
//! - `safe_*` variants **clamp** to the target type's representable range.
//! - `to_*` / [`extract`] variants **extract** a typed value from a
//!   dynamically-typed [`Value`] via numeric coercion, returning `(value, ok)`.
//!
//! # Generic conversions
//!
//! [`must_convert`] and [`safe_convert`] are generic over the [`ConvInteger`]
//! trait, implemented for every fixed-width and word-sized integer type.
//! Validation widens both the value and the target bounds to [`i128`], which
//! holds every supported integer value losslessly, so overflow and
//! signed/unsigned boundary crossings are caught without lossy intermediate
//! casts.
//!
//! # Dynamic extraction
//!
//! Report values are dynamically typed; [`Value`] is the tagged union of the
//! kinds the extractors observe. [`extract`] coerces any numeric kind
//! ([`Number`]) into the requested numeric target type, and [`extract_string`]
//! covers the string case; every non-coercible value yields `(zero, false)`.
//! These coercion rules are part of the report compatibility contract (pinned
//! by `tests/compat`): a float extracted as an integer truncates toward
//! zero, and strings/bools never coerce to numbers.
//!
//! # Platform note
//!
//! Word-sized integers are [`isize`] / [`usize`], so [`MAX_INT`] tracks the
//! host word size ([`i64::MAX`] on the supported 64-bit targets).

#![forbid(unsafe_code)]

/// Panic payload used by all `must_*` conversions on overflow / sign loss.
///
/// The message text is an observable contract (tests assert the exact wording);
/// keep it stable.
pub const PANIC_OVERFLOW: &str = "safeconv: integer conversion overflow";

/// Maximum value of the platform word-sized signed integer ([`isize::MAX`]).
///
/// [`i64::MAX`] on a 64-bit target; [`i32::MAX`] on a 32-bit target.
pub const MAX_INT: isize = isize::MAX;

/// Maximum [`i64`] value ([`i64::MAX`]).
pub const MAX_INT64: i64 = i64::MAX;

/// Maximum [`u32`] value ([`u32::MAX`]).
pub const MAX_UINT32: u32 = u32::MAX;

// ---------------------------------------------------------------------------
// Generic integer conversion (must_convert / safe_convert).
// ---------------------------------------------------------------------------

/// Bridge trait for the integer types the generic conversions support: every
/// fixed-width and word-sized signed/unsigned integer.
///
/// Conversions are validated by widening every value to [`i128`], which holds
/// every supported integer value losslessly (including [`u64`] / [`usize`]
/// maxima), then range-checking against the target's `[MIN, MAX]` widened the
/// same way — equivalent to a round-trip-and-sign check, with no lossy
/// intermediate cast.
pub trait ConvInteger: Copy {
    /// Widen `self` to [`i128`] losslessly.
    fn to_i128(self) -> i128;
    /// Narrow an [`i128`] back to `Self` by truncation (the value is known to be
    /// in range when this is called).
    fn from_i128(v: i128) -> Self;
    /// This type's minimum value, widened to [`i128`].
    fn min_i128() -> i128;
    /// This type's maximum value, widened to [`i128`].
    fn max_i128() -> i128;
}

macro_rules! impl_conv_integer {
    ($($t:ty),+ $(,)?) => {
        $(
            impl ConvInteger for $t {
                #[inline]
                fn to_i128(self) -> i128 { self as i128 }
                #[inline]
                fn from_i128(v: i128) -> Self { v as $t }
                #[inline]
                fn min_i128() -> i128 { <$t>::MIN as i128 }
                #[inline]
                fn max_i128() -> i128 { <$t>::MAX as i128 }
            }
        )+
    };
}

impl_conv_integer!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// Converts `v` from one integer type to another, **panicking** on overflow or
/// sign loss.
///
/// The widened value must lie within the target's `[MIN, MAX]` range, which
/// captures both overflow and signed/unsigned boundary crossings.
///
/// # Panics
///
/// Panics with [`PANIC_OVERFLOW`] when `v` is not representable in `To`.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::must_convert;
/// let x: i32 = must_convert::<i64, i32>(42);
/// assert_eq!(x, 42);
/// ```
#[must_use]
pub fn must_convert<From: ConvInteger, To: ConvInteger>(v: From) -> To {
    let w = v.to_i128();
    if w < To::min_i128() || w > To::max_i128() {
        panic!("{PANIC_OVERFLOW}");
    }
    To::from_i128(w)
}

/// Converts `v` from one integer type to another, **clamping** to the target
/// type's range on overflow.
///
/// Values above the target maximum clamp to its maximum; values below the
/// target minimum clamp to its minimum.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::safe_convert;
/// let x: i8 = safe_convert::<i64, i8>(1000);
/// assert_eq!(x, i8::MAX);
/// let y: u32 = safe_convert::<i64, u32>(-5);
/// assert_eq!(y, 0);
/// ```
#[must_use]
pub fn safe_convert<From: ConvInteger, To: ConvInteger>(v: From) -> To {
    let w = v.to_i128();
    if w > To::max_i128() {
        To::from_i128(To::max_i128())
    } else if w < To::min_i128() {
        To::from_i128(To::min_i128())
    } else {
        To::from_i128(w)
    }
}

// ---------------------------------------------------------------------------
// Concrete Must* conversions (panic on overflow / out-of-range).
// ---------------------------------------------------------------------------

/// Converts a [`usize`] to [`isize`]; panics on overflow.
///
/// # Panics
///
/// Panics with [`PANIC_OVERFLOW`] when `v` exceeds [`MAX_INT`].
///
/// # Examples
///
/// ```
/// # use cf_safeconv::must_uint_to_int;
/// assert_eq!(must_uint_to_int(42), 42);
/// ```
#[must_use]
pub fn must_uint_to_int(v: usize) -> isize {
    must_convert::<usize, isize>(v)
}

/// Converts an [`isize`] to [`usize`]; panics if negative.
///
/// # Panics
///
/// Panics with [`PANIC_OVERFLOW`] when `v` is negative.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::must_int_to_uint;
/// assert_eq!(must_int_to_uint(42), 42);
/// ```
#[must_use]
pub fn must_int_to_uint(v: isize) -> usize {
    must_convert::<isize, usize>(v)
}

/// Converts an [`isize`] to [`u32`]; panics when `v` is negative or greater
/// than [`MAX_UINT32`].
///
/// # Panics
///
/// Panics with [`PANIC_OVERFLOW`] when `v < 0` or `v > u32::MAX`.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::must_int_to_uint32;
/// assert_eq!(must_int_to_uint32(42), 42);
/// ```
#[must_use]
pub fn must_int_to_uint32(v: isize) -> u32 {
    must_convert::<isize, u32>(v)
}

// ---------------------------------------------------------------------------
// Concrete Safe* conversions (clamp on overflow).
// ---------------------------------------------------------------------------

/// Converts a [`u64`] to [`i64`], clamping on overflow.
///
/// Values larger than [`MAX_INT64`] clamp to [`MAX_INT64`].
///
/// # Examples
///
/// ```
/// # use cf_safeconv::safe_int64;
/// assert_eq!(safe_int64(42), 42);
/// assert_eq!(safe_int64(u64::MAX), i64::MAX);
/// ```
#[must_use]
pub fn safe_int64(v: u64) -> i64 {
    safe_convert::<u64, i64>(v)
}

/// Converts a [`u64`] to the word-sized [`isize`], clamping on overflow.
///
/// Values larger than [`MAX_INT`] clamp to [`MAX_INT`].
///
/// # Examples
///
/// ```
/// # use cf_safeconv::safe_int;
/// assert_eq!(safe_int(42), 42);
/// assert_eq!(safe_int(u64::MAX), cf_safeconv::MAX_INT);
/// ```
#[must_use]
pub fn safe_int(v: u64) -> isize {
    safe_convert::<u64, isize>(v)
}

// ---------------------------------------------------------------------------
// Dynamic numeric value and extraction (to_* / extract helpers).
// ---------------------------------------------------------------------------

/// A dynamically-typed numeric value: every numeric kind a dynamic report
/// value can carry, each remembering its concrete source type.
///
/// Numeric extraction succeeds **only** from a [`Number`]; the variant tag
/// records the source kind so the coercions below stay width- and
/// sign-faithful.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// Word-sized signed integer.
    Int(isize),
    /// 8-bit signed integer.
    Int8(i8),
    /// 16-bit signed integer.
    Int16(i16),
    /// 32-bit signed integer.
    Int32(i32),
    /// 64-bit signed integer.
    Int64(i64),
    /// Word-sized unsigned integer.
    Uint(usize),
    /// 8-bit unsigned integer.
    Uint8(u8),
    /// 16-bit unsigned integer.
    Uint16(u16),
    /// 32-bit unsigned integer.
    Uint32(u32),
    /// 64-bit unsigned integer.
    Uint64(u64),
    /// 32-bit float.
    Float32(f32),
    /// 64-bit float.
    Float64(f64),
}

impl Number {
    /// Coerce to [`f64`].
    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(v) => v as f64,
            Number::Int8(v) => v as f64,
            Number::Int16(v) => v as f64,
            Number::Int32(v) => v as f64,
            Number::Int64(v) => v as f64,
            Number::Uint(v) => v as f64,
            Number::Uint8(v) => v as f64,
            Number::Uint16(v) => v as f64,
            Number::Uint32(v) => v as f64,
            Number::Uint64(v) => v as f64,
            Number::Float32(v) => v as f64,
            Number::Float64(v) => v,
        }
    }

    /// Coerce to [`f32`].
    #[must_use]
    pub fn as_f32(self) -> f32 {
        self.as_f64() as f32
    }

    /// Coerce to [`isize`]. Float kinds truncate toward zero (report
    /// compatibility contract).
    #[must_use]
    pub fn as_isize(self) -> isize {
        match self {
            Number::Int(v) => v,
            Number::Int8(v) => v as isize,
            Number::Int16(v) => v as isize,
            Number::Int32(v) => v as isize,
            Number::Int64(v) => v as isize,
            Number::Uint(v) => v as isize,
            Number::Uint8(v) => v as isize,
            Number::Uint16(v) => v as isize,
            Number::Uint32(v) => v as isize,
            Number::Uint64(v) => v as isize,
            Number::Float32(v) => v as isize,
            Number::Float64(v) => v as isize,
        }
    }

    /// Coerce to [`usize`]. Float kinds truncate toward zero.
    #[must_use]
    pub fn as_usize(self) -> usize {
        match self {
            Number::Int(v) => v as usize,
            Number::Int8(v) => v as usize,
            Number::Int16(v) => v as usize,
            Number::Int32(v) => v as usize,
            Number::Int64(v) => v as usize,
            Number::Uint(v) => v,
            Number::Uint8(v) => v as usize,
            Number::Uint16(v) => v as usize,
            Number::Uint32(v) => v as usize,
            Number::Uint64(v) => v as usize,
            Number::Float32(v) => v as usize,
            Number::Float64(v) => v as usize,
        }
    }
}

/// A dynamically-typed value, as observed by the extractors.
///
/// Only the [`Value::Number`] variant participates in numeric coercion; every
/// other variant is "non-numeric" and yields `(zero, false)` from the numeric
/// extractors. The [`Value::String`] variant additionally supports the direct
/// string extraction in [`extract_string`].
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A supported numeric value.
    Number(Number),
    /// A string.
    String(String),
    /// A boolean.
    Bool(bool),
    /// The absent/null value.
    Nil,
}

impl Value {
    /// Whether this value holds a numeric kind.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Number(_))
    }
}

/// Generic numeric extraction target.
///
/// Implemented for the numeric output types the crate extracts to.
pub trait Extractable: Sized {
    /// Coerce a [`Number`] into this type.
    fn from_number(n: Number) -> Self;
    /// The zero value returned alongside `ok == false`.
    fn zero() -> Self;
}

macro_rules! impl_extractable_int {
    ($($t:ty),+ $(,)?) => {
        $(impl Extractable for $t {
            fn from_number(n: Number) -> Self { n.as_isize() as $t }
            fn zero() -> Self { 0 }
        })+
    };
}
impl_extractable_int!(i8, i16, i32, i64, isize);

macro_rules! impl_extractable_uint {
    ($($t:ty),+ $(,)?) => {
        $(impl Extractable for $t {
            fn from_number(n: Number) -> Self { n.as_usize() as $t }
            fn zero() -> Self { 0 }
        })+
    };
}
impl_extractable_uint!(u8, u16, u32, u64, usize);

impl Extractable for f64 {
    fn from_number(n: Number) -> Self {
        n.as_f64()
    }
    fn zero() -> Self {
        0.0
    }
}

impl Extractable for f32 {
    fn from_number(n: Number) -> Self {
        n.as_f32()
    }
    fn zero() -> Self {
        0.0
    }
}

/// Generic dynamic numeric extraction.
///
/// Returns `(coerced, true)` when `value` holds a [`Number`], otherwise
/// `(T::zero(), false)`. The string case is exposed via [`extract_string`].
///
/// # Examples
///
/// ```
/// # use cf_safeconv::{extract, Value, Number};
/// let (v, ok): (isize, bool) = extract(&Value::Number(Number::Int(7)));
/// assert_eq!((v, ok), (7, true));
/// ```
#[must_use]
pub fn extract<T: Extractable>(value: &Value) -> (T, bool) {
    match value {
        Value::Number(n) => (T::from_number(*n), true),
        _ => (T::zero(), false),
    }
}

/// Extracts a [`String`] from a dynamic value.
///
/// Returns `(s.clone(), true)` for [`Value::String`], otherwise
/// `(String::new(), false)`. Numbers never stringify here: a string target is
/// satisfied only by an actual string.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::{extract_string, Value};
/// assert_eq!(extract_string(&Value::String("hi".into())), ("hi".to_string(), true));
/// assert_eq!(extract_string(&Value::Nil), (String::new(), false));
/// ```
#[must_use]
pub fn extract_string(value: &Value) -> (String, bool) {
    match value {
        Value::String(s) => (s.clone(), true),
        _ => (String::new(), false),
    }
}

/// Extracts an [`isize`] from a dynamic value via numeric coercion.
///
/// Returns `(0, false)` for non-numeric values; float values truncate toward
/// zero.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::{to_int, Value, Number};
/// assert_eq!(to_int(&Value::Number(Number::Float64(3.14))), (3, true));
/// assert_eq!(to_int(&Value::String("42".into())), (0, false));
/// ```
#[must_use]
pub fn to_int(value: &Value) -> (isize, bool) {
    extract::<isize>(value)
}

/// Extracts an [`f64`] from a dynamic value via numeric coercion.
///
/// Returns `(0.0, false)` for non-numeric values.
///
/// # Examples
///
/// ```
/// # use cf_safeconv::{to_float64, Value, Number};
/// assert_eq!(to_float64(&Value::Number(Number::Int(42))), (42.0, true));
/// assert_eq!(to_float64(&Value::Nil), (0.0, false));
/// ```
#[must_use]
pub fn to_float64(value: &Value) -> (f64, bool) {
    extract::<f64>(value)
}

#[cfg(test)]
mod tests {
    // The reference test fixtures deliberately use the literal 3.14 as a
    // conversion fixture
    // (it is not meant to be pi); silence the approx_constant lint for them.
    #![allow(clippy::approx_constant)]

    use super::*;

    // --- must_uint_to_int (reference suite: TestMustUintToInt) ---

    #[test]
    fn must_uint_to_int_normal_value() {
        assert_eq!(must_uint_to_int(42), 42);
    }

    #[test]
    fn must_uint_to_int_zero() {
        assert_eq!(must_uint_to_int(0), 0);
    }

    #[test]
    fn must_uint_to_int_max_int() {
        assert_eq!(must_uint_to_int(MAX_INT as usize), MAX_INT);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_uint_to_int_overflow_panics() {
        let _ = must_uint_to_int(MAX_INT as usize + 1);
    }

    // --- must_int_to_uint (reference suite: TestMustIntToUint) ---

    #[test]
    fn must_int_to_uint_normal_value() {
        assert_eq!(must_int_to_uint(42), 42);
    }

    #[test]
    fn must_int_to_uint_zero() {
        assert_eq!(must_int_to_uint(0), 0);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_int_to_uint_negative_panics() {
        let _ = must_int_to_uint(-1);
    }

    // --- must_int_to_uint32 (reference suite: TestMustIntToUint32) ---

    #[test]
    fn must_int_to_uint32_normal_value() {
        assert_eq!(must_int_to_uint32(42), 42);
    }

    #[test]
    fn must_int_to_uint32_zero() {
        assert_eq!(must_int_to_uint32(0), 0);
    }

    #[test]
    fn must_int_to_uint32_max_uint32() {
        assert_eq!(must_int_to_uint32(MAX_UINT32 as isize), MAX_UINT32);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_int_to_uint32_negative_panics() {
        let _ = must_int_to_uint32(-1);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_int_to_uint32_overflow_panics() {
        let _ = must_int_to_uint32(MAX_UINT32 as isize + 1);
    }

    // --- must_convert (reference suite: TestMustConvert_*) ---

    #[test]
    fn must_convert_uint_to_int() {
        assert_eq!(must_convert::<usize, isize>(42), 42);
        assert_eq!(must_convert::<usize, isize>(0), 0);
        assert_eq!(must_convert::<usize, isize>(MAX_INT as usize), MAX_INT);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_uint_to_int_overflow() {
        let _ = must_convert::<usize, isize>(MAX_INT as usize + 1);
    }

    #[test]
    fn must_convert_int_to_uint() {
        assert_eq!(must_convert::<isize, usize>(42), 42);
        assert_eq!(must_convert::<isize, usize>(0), 0);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_int_to_uint_negative() {
        let _ = must_convert::<isize, usize>(-1);
    }

    #[test]
    fn must_convert_int_to_uint32() {
        assert_eq!(must_convert::<isize, u32>(42), 42u32);
        assert_eq!(must_convert::<isize, u32>(0), 0u32);
        assert_eq!(must_convert::<isize, u32>(MAX_UINT32 as isize), MAX_UINT32);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_int_to_uint32_overflow() {
        let _ = must_convert::<isize, u32>(MAX_UINT32 as isize + 1);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_int_to_uint32_negative() {
        let _ = must_convert::<isize, u32>(-1);
    }

    #[test]
    fn must_convert_int64_to_int8() {
        assert_eq!(must_convert::<i64, i8>(42), 42i8);
        assert_eq!(must_convert::<i64, i8>(i8::MAX as i64), i8::MAX);
        assert_eq!(must_convert::<i64, i8>(i8::MIN as i64), i8::MIN);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_int64_to_int8_overflow_high() {
        let _ = must_convert::<i64, i8>(i8::MAX as i64 + 1);
    }

    #[test]
    #[should_panic(expected = "safeconv: integer conversion overflow")]
    fn must_convert_int64_to_int8_overflow_low() {
        let _ = must_convert::<i64, i8>(i8::MIN as i64 - 1);
    }

    #[test]
    fn must_convert_same_type() {
        assert_eq!(must_convert::<isize, isize>(42), 42);
        assert_eq!(must_convert::<usize, usize>(99), 99);
    }

    // --- safe_convert (reference suite: TestSafeConvert_*) ---

    #[test]
    fn safe_convert_uint64_to_int64() {
        let cases: Vec<(&str, u64, i64)> = vec![
            ("zero", 0, 0),
            ("normal", 42, 42),
            ("max_int64", i64::MAX as u64, i64::MAX),
            ("overflow_clamps", u64::MAX, i64::MAX),
            ("just_above_max", i64::MAX as u64 + 1, i64::MAX),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_convert::<u64, i64>(input), expected, "{name}");
        }
    }

    #[test]
    fn safe_convert_uint64_to_int() {
        let cases: Vec<(&str, u64, isize)> = vec![
            ("zero", 0, 0),
            ("normal", 42, 42),
            ("max_int", MAX_INT as u64, MAX_INT),
            ("overflow_clamps", u64::MAX, MAX_INT),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_convert::<u64, isize>(input), expected, "{name}");
        }
    }

    #[test]
    fn safe_convert_int_to_uint() {
        let cases: Vec<(&str, isize, usize)> = vec![
            ("zero", 0, 0),
            ("positive", 42, 42),
            ("negative_clamps_to_zero", -1, 0),
            ("min_int_clamps_to_zero", isize::MIN, 0),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_convert::<isize, usize>(input), expected, "{name}");
        }
    }

    #[test]
    fn safe_convert_int64_to_int8() {
        let cases: Vec<(&str, i64, i8)> = vec![
            ("zero", 0, 0),
            ("fits", 42, 42),
            ("max_int8", i8::MAX as i64, i8::MAX),
            ("min_int8", i8::MIN as i64, i8::MIN),
            ("above_max_clamps", i8::MAX as i64 + 1, i8::MAX),
            ("below_min_clamps", i8::MIN as i64 - 1, i8::MIN),
            ("large_positive", i64::MAX, i8::MAX),
            ("large_negative", i64::MIN, i8::MIN),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_convert::<i64, i8>(input), expected, "{name}");
        }
    }

    #[test]
    fn safe_convert_same_type() {
        assert_eq!(safe_convert::<isize, isize>(42), 42);
    }

    // --- to_int (reference suite: TestToInt) ---

    #[test]
    fn to_int_cases() {
        let cases: Vec<(&str, Value, isize, bool)> = vec![
            ("int", Value::Number(Number::Int(42)), 42, true),
            ("int32", Value::Number(Number::Int32(100)), 100, true),
            ("int64", Value::Number(Number::Int64(999)), 999, true),
            ("float64", Value::Number(Number::Float64(3.14)), 3, true),
            ("zero_int", Value::Number(Number::Int(0)), 0, true),
            ("negative_float", Value::Number(Number::Float64(-2.9)), -2, true),
            ("string_unsupported", Value::String("42".into()), 0, false),
            ("nil_unsupported", Value::Nil, 0, false),
            ("bool_unsupported", Value::Bool(true), 0, false),
            ("uint_coerced", Value::Number(Number::Uint(10)), 10, true),
        ];
        for (name, input, expected, ok) in cases {
            let (got, got_ok) = to_int(&input);
            assert_eq!(got_ok, ok, "ok mismatch in {name}");
            assert_eq!(got, expected, "value mismatch in {name}");
        }
    }

    // --- to_float64 (reference suite: TestToFloat64) ---

    #[test]
    fn to_float64_cases() {
        let cases: Vec<(&str, Value, f64, bool)> = vec![
            ("float64", Value::Number(Number::Float64(3.14)), 3.14, true),
            ("int", Value::Number(Number::Int(42)), 42.0, true),
            ("int32", Value::Number(Number::Int32(100)), 100.0, true),
            ("int64", Value::Number(Number::Int64(999)), 999.0, true),
            ("zero_float", Value::Number(Number::Float64(0.0)), 0.0, true),
            ("negative_int", Value::Number(Number::Int(-5)), -5.0, true),
            ("string_unsupported", Value::String("3.14".into()), 0.0, false),
            ("nil_unsupported", Value::Nil, 0.0, false),
            ("bool_unsupported", Value::Bool(true), 0.0, false),
            ("uint_coerced", Value::Number(Number::Uint(10)), 10.0, true),
        ];
        for (name, input, expected, ok) in cases {
            let (got, got_ok) = to_float64(&input);
            assert_eq!(got_ok, ok, "ok mismatch in {name}");
            assert!((got - expected).abs() < 0.001, "value mismatch in {name}");
        }
    }

    // --- safe_int (reference suite: TestSafeInt) ---

    #[test]
    fn safe_int_cases() {
        let cases: Vec<(&str, u64, isize)> = vec![
            ("zero", 0, 0),
            ("normal_value", 42, 42),
            ("max_int", MAX_INT as u64, MAX_INT),
            ("overflow_clamps", u64::MAX, MAX_INT),
            ("just_above_max_int", MAX_INT as u64 + 1, MAX_INT),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_int(input), expected, "mismatch in {name}");
        }
    }

    // --- safe_int64 (reference suite: TestSafeInt64) ---

    #[test]
    fn safe_int64_cases() {
        let cases: Vec<(&str, u64, i64)> = vec![
            ("zero", 0, 0),
            ("normal_value", 42, 42),
            ("max_int64", i64::MAX as u64, i64::MAX),
            ("overflow_clamps", u64::MAX, i64::MAX),
            ("just_above_max_int64", i64::MAX as u64 + 1, i64::MAX),
        ];
        for (name, input, expected) in cases {
            assert_eq!(safe_int64(input), expected, "mismatch in {name}");
        }
    }

    // --- extract (reference suite: TestExtract_*) ---

    #[test]
    fn extract_direct_type_match() {
        let (v, ok): (isize, bool) = extract(&Value::Number(Number::Int(42)));
        assert!(ok);
        assert_eq!(v, 42);
    }

    #[test]
    fn extract_string_direct() {
        let (s, ok) = extract_string(&Value::String("hello".into()));
        assert!(ok);
        assert_eq!(s, "hello");
    }

    #[test]
    fn extract_numeric_coercion_int_from_int64() {
        let (v, ok): (isize, bool) = extract(&Value::Number(Number::Int64(99)));
        assert!(ok);
        assert_eq!(v, 99);
    }

    #[test]
    fn extract_numeric_coercion_int_from_int32() {
        let (v, ok): (isize, bool) = extract(&Value::Number(Number::Int32(100)));
        assert!(ok);
        assert_eq!(v, 100);
    }

    #[test]
    fn extract_numeric_coercion_int_from_float64() {
        let (v, ok): (isize, bool) = extract(&Value::Number(Number::Float64(3.14)));
        assert!(ok);
        assert_eq!(v, 3); // truncates toward zero
    }

    #[test]
    fn extract_numeric_coercion_float64_from_int() {
        let (v, ok): (f64, bool) = extract(&Value::Number(Number::Int(42)));
        assert!(ok);
        assert!((v - 42.0).abs() < 0.001);
    }

    #[test]
    fn extract_numeric_coercion_float64_from_int64() {
        let (v, ok): (f64, bool) = extract(&Value::Number(Number::Int64(999)));
        assert!(ok);
        assert!((v - 999.0).abs() < 0.001);
    }

    #[test]
    fn extract_unsupported_type() {
        let (v, ok): (isize, bool) = extract(&Value::String("not a number".into()));
        assert!(!ok);
        assert_eq!(v, 0);
    }

    #[test]
    fn extract_nil() {
        let (v, ok): (isize, bool) = extract(&Value::Nil);
        assert!(!ok);
        assert_eq!(v, 0);
    }

    #[test]
    fn extract_bool_to_int_fails() {
        let (v, ok): (isize, bool) = extract(&Value::Bool(true));
        assert!(!ok);
        assert_eq!(v, 0);
    }

    #[test]
    fn extract_numeric_coercion_uint_from_int() {
        let (v, ok): (usize, bool) = extract(&Value::Number(Number::Int(42)));
        assert!(ok);
        assert_eq!(v, 42);
    }

    #[test]
    fn extract_float32_from_float64() {
        let (v, ok): (f32, bool) = extract(&Value::Number(Number::Float64(1.5)));
        assert!(ok);
        assert!((v - 1.5f32).abs() < 0.001);
    }
}
