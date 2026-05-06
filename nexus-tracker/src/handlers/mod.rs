//! Tracker request handlers
//!
//! Each submodule handles one message type. Handlers are pure functions
//! over their inputs (decoded message + connection context) and write a
//! response via the supplied [`FrameWriter`].
//!
//! [`FrameWriter`]: nexus_common::framing::FrameWriter

pub mod handshake;
pub mod tracker_server_list;
pub mod tracker_server_register;

use nexus_common::validators::{self, LocaleError, MAX_LOCALE_LENGTH};

use crate::constants::{DEFAULT_LOCALE, REASON_LOCALE_INVALID, REASON_LOCALE_TOO_LONG};
use crate::errors::{err_tracker_locale_invalid, err_tracker_locale_too_long};

/// Validate the request `locale` field shared by every tracker
/// handler. Returns `None` on success; `Some((reason, message))` on
/// failure where `reason` is the operator-log `REASON_*` value and
/// `message` is a translated human-readable error rendered in
/// `DEFAULT_LOCALE` — we can't trust the request locale for
/// translation when the locale field itself is the suspect input.
///
/// Each handler wraps the result into its own failure shape (typed
/// response for the list flow, `reject` helper for the register flow)
/// since the delivery semantics differ between flows.
pub(crate) fn validate_locale(locale: &str) -> Option<(&'static str, String)> {
    match validators::validate_locale(locale) {
        Ok(()) => None,
        Err(LocaleError::TooLong) => Some((
            REASON_LOCALE_TOO_LONG,
            err_tracker_locale_too_long(DEFAULT_LOCALE, MAX_LOCALE_LENGTH),
        )),
        Err(LocaleError::InvalidCharacters) => Some((
            REASON_LOCALE_INVALID,
            err_tracker_locale_invalid(DEFAULT_LOCALE),
        )),
    }
}
