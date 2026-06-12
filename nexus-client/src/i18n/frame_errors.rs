//! Frame error translation for client-visible protocol failures.

use std::io;

use nexus_common::framing::FrameError;

use super::{t, t_args};

/// Translate low-level frame errors before they are interpolated into
/// user-facing connection and tracker error messages.
pub fn translate_frame_error(error: &FrameError) -> String {
    match error {
        FrameError::InvalidMagic
        | FrameError::InvalidMessageId
        | FrameError::InvalidTypeLength
        | FrameError::TypeLengthOutOfRange
        | FrameError::TypeLengthTooManyDigits
        | FrameError::InvalidPayloadLength
        | FrameError::PayloadLengthTooManyDigits
        | FrameError::MissingDelimiter
        | FrameError::MissingTerminator => t("err-frame-invalid"),
        FrameError::PayloadLengthExceedsTypeMax {
            message_type,
            length,
            max,
        } => t_args(
            "err-frame-payload-too-large",
            &[
                ("message_type", message_type),
                ("length", &length.to_string()),
                ("max", &max.to_string()),
            ],
        ),
        FrameError::PayloadAllocationFailed {
            message_type,
            length,
        } => t_args(
            "err-frame-payload-allocation-failed",
            &[
                ("message_type", message_type),
                ("length", &length.to_string()),
            ],
        ),
        FrameError::UnknownMessageType(message_type) => t_args(
            "err-frame-unknown-message-type",
            &[("message_type", message_type)],
        ),
        FrameError::UnexpectedMessageType(message_type) => t_args(
            "err-frame-unexpected-message-type",
            &[("message_type", message_type)],
        ),
        FrameError::InvalidJson(error) => t_args("err-frame-invalid-json", &[("error", error)]),
        FrameError::Io(error) => t_args("err-frame-io", &[("error", error)]),
        FrameError::ConnectionClosed => t("err-connection-closed"),
        FrameError::FrameTimeout => t("err-frame-timeout"),
        FrameError::IdleTimeout => t("err-frame-idle-timeout"),
    }
}

/// Translate an I/O error that may carry a structured [`FrameError`] source.
pub fn translate_protocol_io_error(error: &io::Error) -> String {
    if let Some(frame_error) = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<FrameError>())
    {
        translate_frame_error(frame_error)
    } else if error.kind() == io::ErrorKind::ConnectionReset {
        t("err-connection-closed")
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_protocol_io_error_uses_structured_frame_error() {
        let error = io::Error::other(FrameError::UnexpectedMessageType("Login".to_string()));

        assert_eq!(
            translate_protocol_io_error(&error),
            t_args(
                "err-frame-unexpected-message-type",
                &[("message_type", "Login")]
            )
        );
    }

    #[test]
    fn translate_frame_error_localizes_payload_allocation_failure() {
        let error = FrameError::PayloadAllocationFailed {
            message_type: "UserListResponse".to_string(),
            length: 42,
        };

        let translated = translate_frame_error(&error);
        assert!(translated.contains("42"));
        assert!(translated.contains("UserListResponse"));
    }

    #[test]
    fn translate_protocol_io_error_localizes_connection_reset() {
        let error = io::Error::new(io::ErrorKind::ConnectionReset, "connection closed");

        assert_eq!(
            translate_protocol_io_error(&error),
            t("err-connection-closed")
        );
    }
}
