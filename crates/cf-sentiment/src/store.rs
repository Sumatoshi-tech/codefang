//! The per-tick sentiment series as the cross-analyzer anomaly path reads it
//! (`history/anomaly` enriches its report with the other selected analyzers'
//! time series).

use crate::metrics::DIM_SENTIMENT;
use crate::model::TimeSeriesData;

/// Tick axis plus named per-tick value dimensions.
pub type StoreTimeSeries = (Vec<i64>, Vec<(String, Vec<f64>)>);

/// Extracts the per-tick sentiment dimension from the `time_series` records:
/// the tick axis and a single `"sentiment"` dimension of per-tick scores (as
/// `f64`). Returns `None` when no time-series records are present.
#[must_use]
pub fn extract_store_time_series(time_series: &[TimeSeriesData]) -> Option<StoreTimeSeries> {
    if time_series.is_empty() {
        return None;
    }

    let ticks: Vec<i64> = time_series.iter().map(|ts| ts.tick).collect();
    let sentiments: Vec<f64> = time_series
        .iter()
        .map(|ts| f64::from(ts.sentiment))
        .collect();

    Some((ticks, vec![(DIM_SENTIMENT.to_string(), sentiments)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_store_time_series_empty() {
        assert!(extract_store_time_series(&[]).is_none());
    }

    #[test]
    fn extract_store_time_series_dimension() {
        let ts = vec![
            TimeSeriesData {
                tick: 0,
                sentiment: 0.8,
                ..Default::default()
            },
            TimeSeriesData {
                tick: 1,
                sentiment: 0.3,
                ..Default::default()
            },
        ];
        let (ticks, dims) = extract_store_time_series(&ts).expect("non-empty");
        assert_eq!(ticks, vec![0, 1]);
        assert_eq!(dims.len(), 1);
        assert_eq!(dims[0].0, "sentiment");
        assert!((dims[0].1[0] - 0.8).abs() < 1e-6);
        assert!((dims[0].1[1] - 0.3).abs() < 1e-6);
    }
}
