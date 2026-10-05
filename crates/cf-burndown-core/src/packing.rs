//! Packing of a `(person, tick)` pair into the 32-bit value a treap segment
//! stores ([`crate::TimeKey`]).
//!
//! The burndown walk keys every line by the tick it was written in and —
//! with per-developer tracking — by the author who wrote it. Both live in one
//! `u32` so a single `replace` report carries both facts for the deleted
//! lines. The split is chosen per walk from the window's last tick: the ticks
//! take exactly the bits they need and every remaining bit goes to the person
//! id, so no fixed sentinel tick can ever collide with real history (the fixed
//! 14-bit tick field of the original design overflowed at tick 16383, which a
//! 45-year daily history reaches).

/// Width of the packed value in bits.
const VALUE_BITS: u32 = 32;

/// Upper bound on the tick field width: at least one bit is always left for
/// the person field, so a packed value never reaches [`crate::TREE_END`].
pub const MAX_TICK_BITS: u32 = VALUE_BITS - 1;

/// The `(person, tick)` ⇄ value codec of one burndown walk.
///
/// ```
/// use cf_burndown_core::ValuePacking;
///
/// // A walk whose newest commit is at tick 310 keeps 9 bits for ticks.
/// let p = ValuePacking::for_people(310);
/// assert_eq!(p.tick_bits(), 9);
/// let v = p.pack(3, 310);
/// assert_eq!(p.unpack(v), (3, 310));
///
/// // Without people tracking the value IS the tick.
/// let t = ValuePacking::ticks_only();
/// assert_eq!(t.pack(7, 16383), 16383);
/// assert_eq!(t.unpack(16383), (0, 16383));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValuePacking {
    tick_bits: u32,
}

impl ValuePacking {
    /// The codec of a walk without per-developer tracking: the whole value is
    /// the tick (31 bits) and every person folds to the single slot 0.
    #[must_use]
    pub const fn ticks_only() -> Self {
        Self {
            tick_bits: MAX_TICK_BITS,
        }
    }

    /// The codec of a walk with per-developer tracking whose newest commit is
    /// at `max_tick`: the tick field is the narrowest one holding `max_tick`,
    /// the person field takes the rest.
    #[must_use]
    pub fn for_people(max_tick: i64) -> Self {
        let needed =
            VALUE_BITS.saturating_sub(max_tick.max(0).leading_zeros().saturating_sub(VALUE_BITS));
        Self {
            tick_bits: needed.clamp(1, MAX_TICK_BITS),
        }
    }

    /// Width of the tick field.
    #[must_use]
    pub const fn tick_bits(&self) -> u32 {
        self.tick_bits
    }

    /// Exclusive upper bound of the tick field.
    #[must_use]
    pub const fn tick_limit(&self) -> i64 {
        1_i64 << self.tick_bits
    }

    /// Exclusive upper bound of the person field. One value below the raw
    /// field capacity is kept free so a packed value never equals
    /// [`crate::TREE_END`].
    #[must_use]
    pub const fn person_limit(&self) -> i64 {
        (1_i64 << (VALUE_BITS - self.tick_bits)) - 1
    }

    /// The person slot of lines with no known author: lines that pre-date the
    /// window and ids beyond [`ValuePacking::person_limit`].
    #[must_use]
    pub const fn unknown_person(&self) -> i64 {
        self.person_limit() - 1
    }

    /// Packs `person` and `tick`; an out-of-range person folds to
    /// [`ValuePacking::unknown_person`], an out-of-range tick saturates.
    #[must_use]
    pub fn pack(&self, person: i64, tick: i64) -> i64 {
        let tick = tick.clamp(0, self.tick_limit() - 1);
        let person = if (0..self.unknown_person()).contains(&person) {
            person
        } else {
            self.unknown_person()
        };
        (person << self.tick_bits) | tick
    }

    /// Splits a packed value back into `(person, tick)`.
    #[must_use]
    pub const fn unpack(&self, value: i64) -> (i64, i64) {
        (value >> self.tick_bits, value & (self.tick_limit() - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TREE_END;

    #[test]
    fn ticks_only_is_the_identity_on_ticks() {
        let p = ValuePacking::ticks_only();
        for tick in [0, 1, 16_383, 16_384, 1 << 20, (1 << 31) - 1] {
            assert_eq!(p.pack(0, tick), tick);
            assert_eq!(p.pack(99, tick), tick, "every person folds to slot 0");
            assert_eq!(p.unpack(tick), (0, tick));
        }
    }

    #[test]
    fn people_codec_uses_the_narrowest_tick_field() {
        assert_eq!(ValuePacking::for_people(0).tick_bits(), 1);
        assert_eq!(ValuePacking::for_people(1).tick_bits(), 1);
        assert_eq!(ValuePacking::for_people(2).tick_bits(), 2);
        assert_eq!(ValuePacking::for_people(310).tick_bits(), 9);
        assert_eq!(ValuePacking::for_people(511).tick_bits(), 9);
        assert_eq!(ValuePacking::for_people(512).tick_bits(), 10);
        assert_eq!(ValuePacking::for_people(16_384).tick_bits(), 15);
        assert_eq!(
            ValuePacking::for_people(i64::MAX).tick_bits(),
            MAX_TICK_BITS
        );
    }

    #[test]
    fn people_codec_round_trips_every_person_and_tick_in_range() {
        let p = ValuePacking::for_people(16_384);
        assert_eq!(p.person_limit(), (1 << 17) - 1);
        for person in [0, 1, 2, p.unknown_person() - 1] {
            for tick in [0, 16_383, 16_384, p.tick_limit() - 1] {
                assert_eq!(p.unpack(p.pack(person, tick)), (person, tick));
            }
        }
    }

    #[test]
    fn out_of_range_persons_fold_to_the_unknown_slot() {
        let p = ValuePacking::for_people(310);
        let unknown = p.unknown_person();
        assert_eq!(p.unpack(p.pack(-1, 5)), (unknown, 5));
        assert_eq!(p.unpack(p.pack(p.person_limit(), 5)), (unknown, 5));
        assert_eq!(p.unpack(p.pack(unknown, 5)), (unknown, 5));
        assert_eq!(p.unpack(p.pack(i64::MAX, 5)), (unknown, 5));
    }

    #[test]
    fn out_of_range_ticks_saturate() {
        let p = ValuePacking::for_people(310);
        assert_eq!(p.unpack(p.pack(1, -4)), (1, 0));
        assert_eq!(p.unpack(p.pack(1, 1 << 40)), (1, p.tick_limit() - 1));
    }

    #[test]
    fn packed_values_never_reach_the_tree_end_sentinel() {
        for max_tick in [0, 1, 310, 16_384, (1 << 31) - 1, i64::MAX] {
            let p = ValuePacking::for_people(max_tick);
            let v = p.pack(p.unknown_person(), p.tick_limit() - 1);
            assert!(v < i64::from(TREE_END), "max_tick {max_tick}: {v}");
            assert!(v >= 0);
        }
        let t = ValuePacking::ticks_only();
        assert!(t.pack(0, i64::MAX) < i64::from(TREE_END));
    }
}
