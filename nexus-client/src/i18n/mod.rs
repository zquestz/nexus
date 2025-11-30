//! Internationalization support using Fluent
//!
//! This module provides translation functions for the client UI.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::i18n::{t, t_args};
//!
//! // Simple translation
//! let cancel = t("button-cancel"); // "Cancel"
//!
//! // Translation with parameters
//! let msg = t_args("msg-user-connected", &[("username", "alice")]);
//! ```
//!
//! ## Permission Translation
//!
//! Permission names (like "user_list", "chat_send") are translated using the
//! `translate_permission()` function, which looks up the corresponding
//! "permission-{name}" key in the translation files.

mod bundle;
mod constants;
mod locale;
mod permissions;
mod translate;

pub use constants::DEFAULT_LOCALE;
pub use locale::get_locale;
pub use permissions::translate_permission;
pub use translate::{t, t_args};
