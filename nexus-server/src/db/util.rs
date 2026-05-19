//! Internal helpers shared across the `db` submodules.

use nexus_common::validators::MIN_BANDWIDTH_WEIGHT;
use tracing::warn;

use crate::constants::LOG_BANDWIDTH_WEIGHT_CLAMPED;

/// Clamp a raw `i64` bandwidth weight from a DB row into the valid `u16`
/// range. Defends against corrupt rows (operator hand-edits, restored
/// backup); all writer paths validate, so this is normally the identity.
pub(super) fn clamp_db_bandwidth_weight(value: i64) -> u16 {
    let clamped = value.clamp(MIN_BANDWIDTH_WEIGHT as i64, u16::MAX as i64) as u16;
    if i64::from(clamped) != value {
        warn!(
            raw = value,
            clamped = clamped,
            "{}",
            LOG_BANDWIDTH_WEIGHT_CLAMPED
        );
    }
    clamped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_range_values_round_trip() {
        assert_eq!(clamp_db_bandwidth_weight(1), 1);
        assert_eq!(clamp_db_bandwidth_weight(100), 100);
        assert_eq!(clamp_db_bandwidth_weight(65535), u16::MAX);
    }

    #[test]
    fn zero_clamps_up_to_min() {
        assert_eq!(clamp_db_bandwidth_weight(0), MIN_BANDWIDTH_WEIGHT);
    }

    #[test]
    fn negative_clamps_up_to_min() {
        assert_eq!(clamp_db_bandwidth_weight(-1), MIN_BANDWIDTH_WEIGHT);
        assert_eq!(clamp_db_bandwidth_weight(i64::MIN), MIN_BANDWIDTH_WEIGHT);
    }

    #[test]
    fn above_max_clamps_down_to_u16_max() {
        assert_eq!(clamp_db_bandwidth_weight(65536), u16::MAX);
        assert_eq!(clamp_db_bandwidth_weight(100_000), u16::MAX);
        assert_eq!(clamp_db_bandwidth_weight(i64::MAX), u16::MAX);
    }
}
