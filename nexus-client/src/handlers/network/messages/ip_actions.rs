//! Shared success/error message builder for the ban/trust create/delete
//! response handlers. All four responses carry the same shape (`success`,
//! `error`, `ips`, `nickname`) and differ only in the i18n keys used.

use crate::i18n::{t, t_args};
use crate::types::ChatMessage;

/// i18n keys for one IP-rule action (ban / unban / trust / untrust).
pub(super) struct IpActionKeys {
    /// Single IP, no nickname (`{ $ip }`).
    pub single: &'static str,
    /// Single IP with nickname (`{ $ip }`, `{ $nickname }`).
    pub single_nickname: &'static str,
    /// Multiple IPs, no nickname (`{ $count }`).
    pub multi: &'static str,
    /// Multiple IPs with nickname (`{ $count }`, `{ $nickname }`).
    pub multi_nickname: &'static str,
    /// No IPs returned — generic success.
    pub fallback: &'static str,
}

pub(super) const BAN_CREATE_KEYS: IpActionKeys = IpActionKeys {
    single: "msg-banned-ip",
    single_nickname: "msg-banned-ip-nickname",
    multi: "msg-banned-ips",
    multi_nickname: "msg-banned-ips-nickname",
    fallback: "msg-ban-created",
};

pub(super) const BAN_DELETE_KEYS: IpActionKeys = IpActionKeys {
    single: "msg-unbanned-ip",
    single_nickname: "msg-unbanned-ip-nickname",
    multi: "msg-unbanned-ips",
    multi_nickname: "msg-unbanned-ips-nickname",
    fallback: "msg-unbanned-success",
};

pub(super) const TRUST_CREATE_KEYS: IpActionKeys = IpActionKeys {
    single: "msg-trusted-ip",
    single_nickname: "msg-trusted-ip-nickname",
    multi: "msg-trusted-ips",
    multi_nickname: "msg-trusted-ips-nickname",
    fallback: "msg-trust-created",
};

pub(super) const TRUST_DELETE_KEYS: IpActionKeys = IpActionKeys {
    single: "msg-untrusted-ip",
    single_nickname: "msg-untrusted-ip-nickname",
    multi: "msg-untrusted-ips",
    multi_nickname: "msg-untrusted-ips-nickname",
    fallback: "msg-untrusted-success",
};

/// Build the chat message for a ban/trust create/delete response: on success,
/// pick the key by IP count and nickname presence; on failure, surface the
/// server's error text directly.
pub(super) fn ip_action_message(
    keys: &IpActionKeys,
    success: bool,
    error: Option<String>,
    ips: Option<Vec<String>>,
    nickname: Option<String>,
) -> ChatMessage {
    if !success {
        // Show the server's error message directly
        return ChatMessage::error(error.unwrap_or_default());
    }

    let ips = ips.unwrap_or_default();
    if ips.len() == 1 {
        if let Some(ref nick) = nickname {
            ChatMessage::info(t_args(
                keys.single_nickname,
                &[("ip", &ips[0]), ("nickname", nick)],
            ))
        } else {
            ChatMessage::info(t_args(keys.single, &[("ip", &ips[0])]))
        }
    } else if !ips.is_empty() {
        if let Some(ref nick) = nickname {
            ChatMessage::info(t_args(
                keys.multi_nickname,
                &[("count", &ips.len().to_string()), ("nickname", nick)],
            ))
        } else {
            ChatMessage::info(t_args(keys.multi, &[("count", &ips.len().to_string())]))
        }
    } else {
        // No IPs returned — generic success
        ChatMessage::info(t(keys.fallback))
    }
}
