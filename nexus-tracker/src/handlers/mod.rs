//! Tracker request handlers — one submodule per message type. Handlers
//! are pure functions over (decoded message + connection context) that
//! write a response via the supplied `FrameWriter`.

pub mod handshake;
pub mod tracker_server_list;
pub mod tracker_server_register;

use nexus_common::validators::{self, LocaleError, MAX_LOCALE_LENGTH};

use crate::constants::{DEFAULT_LOCALE, REASON_LOCALE_INVALID, REASON_LOCALE_TOO_LONG};
use crate::errors::{err_tracker_locale_invalid, err_tracker_locale_too_long};

/// Validate the request `locale` field shared by every tracker handler.
/// `None` on success; `Some((REASON_* log value, message))` on failure.
/// Errors render in `DEFAULT_LOCALE` — the request locale can't be
/// trusted for translation when it's the suspect input.
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
