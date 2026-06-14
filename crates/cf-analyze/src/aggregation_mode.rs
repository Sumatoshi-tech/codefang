//! Aggregation-mode flag controlling per-item data collection.
//!
//! /// Controls whether per-item data is collected during aggregation.
///
/// [`AggregationMode::Full`] is the
/// zero value (collect everything); [`AggregationMode::SummaryOnly`] skips
/// per-item data collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggregationMode {
    /// Collect all per-item data (default, zero value — `AggregationModeFull`).
    #[default]
    Full,
    /// Skip per-item data collection (`AggregationModeSummaryOnly`).
    SummaryOnly,
}

/// Implemented by aggregators that support runtime mode switching.
///
///
pub trait AggregationModeAware {
    /// Sets the aggregation mode.
    fn set_aggregation_mode(&mut self, mode: AggregationMode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_is_default_zero_value() {
        assert_eq!(AggregationMode::default(), AggregationMode::Full);
    }

    #[test]
    fn variants_are_distinct() {
        assert_ne!(AggregationMode::Full, AggregationMode::SummaryOnly);
    }
}
