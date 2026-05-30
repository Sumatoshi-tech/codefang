//! Overflow-checked numeric and type conversions.
//!
//! Rust port of the Go package `pkg/safeconv`. The Go package documents three
//! families of conversion helpers:
//!
//! - `Must*` variants **panic** on overflow / out-of-range / sign loss.
//! - `Safe*` variants **clamp** to the target type's representable range.
//! - `To*` / `Extract` variants **extract** a typed value from a
//!   dynamically-typed value (Go `any`) via numeric coercion, returning
//!   `(value, ok)`.
//!
//! # Mapping Go generics to Rust
//!
//! Go exposes generic helpers `MustConvert[From, To Integer]`,
//! `SafeConvert[From, To Integer]` and `Extract[T any]` that drive the concrete
//! wrappers. Go's integer type parameters are constrained by the `Integer`
//! interface (`~int | ~int8 | … | ~uint64 | ~uintptr`); Rust models the same
//! surface with the [`ConvInteger`] trait, implemented for every fixed-width
//! and word-sized integer type. [`must_convert`] and [`safe_convert`] reproduce
//! Go's round-trip / sign-comparison checks exactly.
//!
//! `Extract[T any]` does a direct type-assertion first, then a reflect-based
//! numeric coercion when both source and target are numeric kinds. Rust has no
//! runtime `any` type switch, so the dynamic side is modelled by the [`Value`]
//! enum (an explicit tagged union of the kinds the Go code observes); the
//! direct-assertion + numeric-coercion behavior is reproduced by [`extract`].
//!
//! # Platform note
//!
//! Go's `int`/`uint`/`uintptr` are platform-word-sized. Rust's [`isize`] /
//! [`usize`] are the faithful equivalents and are used wherever Go used
//! `int`/`uint`, so [`MAX_INT`] tracks the host word size exactly as Go's
//! `int(^uint(0) >> 1)` does.

#![forbid(unsafe_code)]

/// Panic payload used by all `Must*` conversions on overflow / sign loss.
///
/// In Go this is the unexported `panicOverflow` constant; the Go tests assert
/// the `Must*` helpers panic with exactly this value
/// (`assert.PanicsWithValue(t, panicOverflow, ...)`). Keeping the message
/// stable preserves that observable contract.
pub const PANIC_OVERFLOW: &str = "safeconv: integer conversion overflow";

/// Maximum value of Go's platform-dependent `int` type.
///
/// Mirrors Go's `const MaxInt = int(^uint(0) >> 1)`. On a 64-bit target this is
/// [`i64::MAX`]; on a 32-bit target it is [`i32::MAX`].
pub const MAX_INT: isize = isize::MAX;

/// Maximum value of Go's `int64` type ([`i64::MAX`]).
pub const MAX_INT64: i64 = i64::MAX;

/// Maximum value of Go's `uint32` type ([`u32::MAX`]).
pub const MAX_UINT32: u32 = u32::MAX;

// ---------------------------------------------------------------------------
// Generic integer conversion (Go's `Integer` constraint + Must/SafeConvert).
// ---------------------------------------------------------------------------

/// Marker + bridge trait for the integer types Go's `Integer` constraint
/// admits (`~int | ~int8 | ~int16 | ~int32 | ~int64 | ~uint | ~uint8 |
/// ~uint16 | ~uint32 | ~uint64 | ~uintptr`).
///
/// Conversions are validated by widening every value to [`i128`], which holds
/// every Go integer value losslessly (including [`u64`] / [`usize`] maxima),
/// then range-checking against the target's `[MIN, MAX]` widened the same way.
/// This reproduces Go's `From(To(v)) == v && (v < 0) == (to < 0)` round-trip
/// test without any lossy intermediate cast.
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
/// Equivalent to Go's `MustConvert[From, To Integer]`. The Go implementation
/// panics when `From(To(v)) != v || (v < 0) != (to < 0)`; here we equivalently
/// require the widened value to lie within the target's `[MIN, MAX]` range,
/// which captures both overflow and signed/unsigned boundary crossings.
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
/// Equivalent to Go's `SafeConvert[From, To Integer]`: values above the target
/// maximum clamp to its maximum, values below the target minimum clamp to its
/// minimum.
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

/// Converts a [`usize`] (Go `uint`) to [`isize`] (Go `int`); panics on overflow.
///
/// Equivalent to Go's `MustUintToInt` / `MustConvert[uint, int]`.
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

/// Converts an [`isize`] (Go `int`) to [`usize`] (Go `uint`); panics if negative.
///
/// Equivalent to Go's `MustIntToUint` / `MustConvert[int, uint]`.
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

/// Converts an [`isize`] (Go `int`) to [`u32`]; panics on bounds violation.
///
/// Equivalent to Go's `MustIntToUint32` / `MustConvert[int, uint32]`. Panics
/// when `v` is negative or greater than [`MAX_UINT32`].
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

/// Converts a [`u64`] (Go `uint64`) to [`i64`], clamping on overflow.
///
/// Equivalent to Go's `SafeInt64` / `SafeConvert[uint64, int64]`. Values larger
/// than [`MAX_INT64`] clamp to [`MAX_INT64`].
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

/// Converts a [`u64`] (Go `uint64`) to [`isize`] (Go `int`), clamping on overflow.
///
/// Equivalent to Go's `SafeInt` / `SafeConvert[uint64, int]`. Values larger than
/// [`MAX_INT`] clamp to [`MAX_INT`].
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
// Dynamic numeric value (Go `any`) and extraction (Extract / To* helpers).
// ---------------------------------------------------------------------------

/// A dynamically-typed numeric value, modelling the numeric subset of Go's
/// `any` that `safeconv.Extract` coerces from.
///
/// Go's `Extract[T]` first attempts a direct type assertion (`v.(T)`) and, on
/// failure, a reflect-based numeric conversion that succeeds **only** when both
/// the source and target are numeric kinds. This enum captures every numeric
/// kind Go recognizes; the variant tag is the analogue of `reflect.Kind`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// Go `int` (platform word size).
    Int(isize),
    /// Go `int8`.
    Int8(i8),
    /// Go `int16`.
    Int16(i16),
    /// Go `int32` (also Go `rune`).
    Int32(i32),
    /// Go `int64`.
    Int64(i64),
    /// Go `uint` (platform word size).
    Uint(usize),
    /// Go `uint8` (also Go `byte`).
    Uint8(u8),
    /// Go `uint16`.
    Uint16(u16),
    /// Go `uint32`.
    Uint32(u32),
    /// Go `uint64`.
    Uint64(u64),
    /// Go `float32`.
    Float32(f32),
    /// Go `float64`.
    Float64(f64),
}

impl Number {
    /// Coerce to [`f64`] (Go's `float64(...)` conversion in the numeric switch).
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

    /// Coerce to [`f32`] (Go's `float32(...)` conversion).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        self.as_f64() as f32
    }

    /// Coerce to [`isize`] (Go's `int(...)` conversion). Float kinds truncate
    /// toward zero, matching Go's `int(f)`.
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

    /// Coerce to [`usize`] (Go's `uint(...)` conversion).
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

/// A dynamically-typed value, modelling Go's `any` for extraction.
///
/// Only the [`Value::Number`] variant participates in numeric coercion; every
/// other variant is "non-numeric" and yields `(zero, false)` from the numeric
/// extractors, exactly as Go's `Extract` returns `ok == false` for non-numeric
/// types (string, bool, nil, …). The [`Value::String`] variant additionally
/// supports the *direct type assertion* path of `Extract[string]`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A supported numeric value.
    Number(Number),
    /// Go `string`.
    String(String),
    /// Go `bool`.
    Bool(bool),
    /// Go `nil`.
    Nil,
}

impl Value {
    /// Whether this value holds a numeric kind (the `isNumericKind` predicate
    /// applied to the source side).
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Number(_))
    }
}

/// Generic numeric extraction target, mirroring the target side of Go's
/// `Extract[T]` numeric coercion.
///
/// Implemented for the numeric output types the package extracts to.
pub trait Extractable: Sized {
    /// Coerce a [`Number`] into this type (the body of Go's `Extract` switch).
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

/// Generic dynamic numeric extraction, mirroring Go's `Extract[T]` for numeric
/// target types.
///
/// Returns `(coerced, true)` when `value` holds a [`Number`], otherwise
/// `(T::zero(), false)`. This is the numeric-coercion arm of Go's `Extract`;
/// the direct-string-assertion arm is exposed via [`extract_string`].
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

/// Extracts a [`String`] via the direct type-assertion arm of Go's
/// `Extract[string]`.
///
/// Returns `(s.clone(), true)` for [`Value::String`], otherwise
/// `(String::new(), false)`. (A `string` target never reaches the numeric
/// coercion arm because `string` is not a numeric kind.)
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

/// Extracts an [`isize`] (Go `int`) from a dynamic value via numeric coercion.
///
/// Equivalent to Go's `ToInt` / `Extract[int]`. Returns `(0, false)` for
/// non-numeric values; float values truncate toward zero.
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

/// Extracts an [`f64`] (Go `float64`) from a dynamic value via numeric coercion.
///
/// Equivalent to Go's `ToFloat64` / `Extract[float64]`. Returns `(0.0, false)`
/// for non-numeric values.
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
    // The Go tests deliberately use the literal 3.14 as a conversion fixture
    // (it is not meant to be pi); silence the approx_constant lint for them.
    #![allow(clippy::approx_constant)]

    use super::*;

    // --- MustUintToInt (ported from safeconv_test.go TestMustUintToInt) ---

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

    // --- MustIntToUint (ported from TestMustIntToUint) ---

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

    // --- MustIntToUint32 (ported from TestMustIntToUint32) ---

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

    // --- MustConvert (ported from generic_test.go TestMustConvert_*) ---

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

    // --- SafeConvert (ported from generic_test.go TestSafeConvert_*) ---

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

    // --- ToInt (ported from safeconv_test.go TestToInt) ---

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

    // --- ToFloat64 (ported from TestToFloat64) ---

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

    // --- SafeInt (ported from TestSafeInt) ---

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

    // --- SafeInt64 (ported from TestSafeInt64) ---

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

    // --- Extract (ported from generic_test.go TestExtract_*) ---

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
        assert_eq!(v, 3); // truncation, same as Go conversion
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
