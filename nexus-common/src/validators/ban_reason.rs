//! Ban reason validation
//!
//! Re-exports IP rule reason validation for backward compatibility.

pub use super::ip_rule_reason::{
    IpRuleReasonError as BanReasonError, MAX_IP_RULE_REASON_LENGTH as MAX_BAN_REASON_LENGTH,
    validate_ip_rule_reason as validate_ban_reason,
};
