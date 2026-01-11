//! Trust reason validation
//!
//! Re-exports IP rule reason validation for trust entries.

pub use super::ip_rule_reason::{
    IpRuleReasonError as TrustReasonError, MAX_IP_RULE_REASON_LENGTH as MAX_TRUST_REASON_LENGTH,
    validate_ip_rule_reason as validate_trust_reason,
};
