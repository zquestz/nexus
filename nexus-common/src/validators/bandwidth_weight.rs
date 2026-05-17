//! Bandwidth weight validation
//!
//! Validates per-user / per-group bandwidth weight values used by the
//! outbound rate scheduler. Higher weight = larger share of the cap
//! when flows contend.
//!
//! Called at the protocol boundary by user / group create / update
//! handlers, and again at the DB layer as defense-in-depth.

/// Minimum allowed weight. Zero would starve the flow entirely.
pub const MIN_BANDWIDTH_WEIGHT: u16 = 1;

/// Server-wide fallback weight when neither user nor group provides one.
/// Matches the schema default on `groups.bandwidth_weight`.
pub const DEFAULT_BANDWIDTH_WEIGHT: u16 = 1;

/// Implicit weight floor for admin accounts with no per-user override.
/// Admins don't belong to groups, so this slots in where group resolution
/// would otherwise apply. Chosen to sit above typical group tiers (e.g.,
/// guest=1, shared=5, user=10, moderator=25) while leaving the 51..=65535
/// range open for service accounts that should outrank admins.
pub const DEFAULT_ADMIN_BANDWIDTH_WEIGHT: u16 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BandwidthWeightError {
    Zero,
}

/// Upper bound is implicit in the `u16` type (65535).
pub fn validate_bandwidth_weight(weight: u16) -> Result<(), BandwidthWeightError> {
    if weight < MIN_BANDWIDTH_WEIGHT {
        return Err(BandwidthWeightError::Zero);
    }
    Ok(())
}

/// Resolve a user's effective bandwidth weight given the precedence the
/// protocol promises: per-user override > admin default > group inherit >
/// system default.
///
/// Single source of truth for every reader in the codebase: the DB resolver,
/// the in-transaction resolver inside `update_user`, the offline-list path,
/// and the client baseline display all funnel through here. If the
/// precedence changes, change it here and every site moves together.
///
/// Inputs are already-validated u16 values — clamping of raw DB i64 values
/// happens at the DB boundary (`db::util::clamp_db_bandwidth_weight`), not
/// here.
pub fn resolve_bandwidth_weight(
    user_override: Option<u16>,
    group_weight: Option<u16>,
    is_admin: bool,
) -> u16 {
    if let Some(w) = user_override {
        return w;
    }
    if is_admin {
        return DEFAULT_ADMIN_BANDWIDTH_WEIGHT;
    }
    group_weight.unwrap_or(DEFAULT_BANDWIDTH_WEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_weights() {
        assert!(validate_bandwidth_weight(1).is_ok());
        assert!(validate_bandwidth_weight(100).is_ok());
        assert!(validate_bandwidth_weight(u16::MAX).is_ok());
    }

    #[test]
    fn test_zero_rejected() {
        assert_eq!(
            validate_bandwidth_weight(0),
            Err(BandwidthWeightError::Zero)
        );
    }

    // ========================================================================
    // Precedence tests for `resolve_bandwidth_weight`.
    //
    // The resolver is the single source of truth for the protocol's
    // bandwidth-weight precedence (override → admin default → group → system
    // default). Every reader funnels through it. These tests pin the
    // precedence so that any future drift becomes a test failure here
    // rather than divergent behavior across call sites.
    // ========================================================================

    #[test]
    fn test_resolve_override_wins_for_non_admin() {
        assert_eq!(resolve_bandwidth_weight(Some(99), Some(30), false), 99);
    }

    #[test]
    fn test_resolve_override_wins_for_admin() {
        assert_eq!(resolve_bandwidth_weight(Some(99), None, true), 99);
    }

    #[test]
    fn test_resolve_admin_default_when_no_override() {
        assert_eq!(
            resolve_bandwidth_weight(None, None, true),
            DEFAULT_ADMIN_BANDWIDTH_WEIGHT
        );
    }

    #[test]
    fn test_resolve_group_weight_when_inheriting() {
        assert_eq!(resolve_bandwidth_weight(None, Some(30), false), 30);
    }

    #[test]
    fn test_resolve_system_default_when_no_override_no_group() {
        assert_eq!(
            resolve_bandwidth_weight(None, None, false),
            DEFAULT_BANDWIDTH_WEIGHT
        );
    }

    /// The admin-default branch is reached only when the user has no
    /// override AND is an admin — the group inheritance branch is
    /// skipped. Admins can't be group members (schema CHECK), but pin
    /// the branch order independently in case that ever shifts.
    #[test]
    fn test_resolve_admin_default_skips_group_lookup() {
        assert_eq!(
            resolve_bandwidth_weight(None, Some(30), true),
            DEFAULT_ADMIN_BANDWIDTH_WEIGHT
        );
    }
}
