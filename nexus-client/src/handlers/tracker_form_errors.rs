//! Field-shape error translation shared by the BBS-admin tracker form
//! (`handlers/tracker_management.rs`) and the discovery-panel form
//! (`handlers/tracker_browser.rs`). Both forms accept the same field
//! set (name, address, fingerprint, password) and need to render
//! identical localized error text for failures, but differ in their
//! existing-entry shape (`TrackerInfo` vs `ClientTracker`) so dedup
//! stays per-caller. Sharing the field-shape translation prevents
//! silent drift if a new validator variant is added — adding a
//! variant to `PublicAddressError` here forces a compile error in
//! the single match arm rather than passing one form and missing the
//! other.

use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::validators::{
    self, MAX_PASSWORD_LENGTH, MAX_TRACKER_NAME_LENGTH, PublicAddressError, TrackerAddressError,
    TrackerNameError,
};

use crate::i18n::{t, t_args};

/// Run the field-shape checks (name, address, fingerprint, password)
/// in the canonical order both tracker forms use. On the first
/// failure, returns `Err(localized_message)` with the translated
/// error string the caller can drop straight into its own error
/// channel. On success, returns `Ok(())` — callers then proceed to
/// any caller-specific dedup against their own existing-entry list.
pub fn translate_tracker_field_errors(
    name: &str,
    address: &str,
    fingerprint: &str,
    password: &str,
) -> Result<(), String> {
    if let Err(e) = validators::validate_tracker_name(name) {
        return Err(match e {
            TrackerNameError::Empty => t("err-tracker-name-empty"),
            TrackerNameError::TooLong => t_args(
                "err-tracker-name-too-long",
                &[("max", &MAX_TRACKER_NAME_LENGTH.to_string())],
            ),
            TrackerNameError::ContainsNewlines => t("err-tracker-name-contains-newlines"),
            TrackerNameError::InvalidCharacters => t("err-tracker-name-invalid-characters"),
        });
    }

    if let Err(e) = validators::validate_tracker_address(address) {
        return Err(match e {
            TrackerAddressError::Empty => t("err-tracker-address-empty"),
            TrackerAddressError::Invalid(inner) => match inner {
                PublicAddressError::TooLong => t_args(
                    "err-tracker-address-too-long",
                    &[("max", &validators::MAX_PUBLIC_ADDRESS_LENGTH.to_string())],
                ),
                PublicAddressError::ContainsScheme => t("err-tracker-address-contains-scheme"),
                PublicAddressError::ContainsBrackets => t("err-tracker-address-contains-brackets"),
                PublicAddressError::ContainsPath => t("err-tracker-address-contains-path"),
                PublicAddressError::ContainsUserinfo => t("err-tracker-address-contains-userinfo"),
                PublicAddressError::ContainsWhitespace => {
                    t("err-tracker-address-contains-whitespace")
                }
                PublicAddressError::ContainsPort => t("err-tracker-address-contains-port"),
                PublicAddressError::ContainsZoneId => t("err-tracker-address-contains-zone-id"),
                PublicAddressError::InvalidFormat => t("err-tracker-address-invalid-format"),
            },
        });
    }

    if !fingerprint.is_empty() && !is_canonical_fingerprint(fingerprint) {
        return Err(t("err-tracker-fingerprint-invalid"));
    }

    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(t_args(
            "err-tracker-password-too-long",
            &[("max", &MAX_PASSWORD_LENGTH.to_string())],
        ));
    }

    Ok(())
}
