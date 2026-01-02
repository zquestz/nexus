//! Frame parsing and writing errors

use std::fmt;
use std::io;

use super::{
    MAX_PAYLOAD_LENGTH, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH, MAX_TYPE_LENGTH_DIGITS,
};

/// Errors that can occur when parsing or writing frames
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Invalid magic bytes (expected "NX|")
    InvalidMagic,
    /// Invalid message ID (must be 12 hex characters)
    InvalidMessageId,
    /// Invalid type length field (not a valid number)
    InvalidTypeLength,
    /// Type length is zero or exceeds maximum (1-999)
    TypeLengthOutOfRange,
    /// Type length field has too many digits (max 3)
    TypeLengthTooManyDigits,
    /// Invalid payload length field (not a valid number)
    InvalidPayloadLength,
    /// Payload length field has too many digits (max 10)
    PayloadLengthTooManyDigits,
    /// Payload length exceeds sanity maximum
    PayloadLengthTooLarge,
    /// Payload length exceeds per-type maximum
    PayloadLengthExceedsTypeMax {
        message_type: String,
        length: u64,
        max: u64,
    },
    /// Missing delimiter where expected
    MissingDelimiter,
    /// Missing terminator (newline)
    MissingTerminator,
    /// Unknown message type
    UnknownMessageType(String),
    /// Invalid JSON payload
    InvalidJson(String),
    /// I/O error
    Io(String),
    /// Connection closed
    ConnectionClosed,
    /// Frame read timeout (frame started but not completed in time)
    FrameTimeout,
    /// Idle timeout (no data received within idle timeout period)
    IdleTimeout,
    /// Stream timeout (no progress during streaming transfer)
    StreamTimeout,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::InvalidMagic => write!(f, "invalid magic bytes, expected 'NX|'"),
            FrameError::InvalidMessageId => {
                write!(f, "invalid message ID, expected 12 hex characters")
            }
            FrameError::InvalidTypeLength => write!(f, "invalid type length field"),
            FrameError::TypeLengthOutOfRange => {
                write!(f, "type length must be between 1 and {MAX_TYPE_LENGTH}")
            }
            FrameError::TypeLengthTooManyDigits => {
                write!(
                    f,
                    "type length field exceeds {MAX_TYPE_LENGTH_DIGITS} digits"
                )
            }
            FrameError::InvalidPayloadLength => write!(f, "invalid payload length field"),
            FrameError::PayloadLengthTooManyDigits => {
                write!(
                    f,
                    "payload length field exceeds {MAX_PAYLOAD_LENGTH_DIGITS} digits"
                )
            }
            FrameError::PayloadLengthTooLarge => {
                write!(f, "payload length exceeds maximum of {MAX_PAYLOAD_LENGTH}")
            }
            FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length,
                max,
            } => {
                write!(
                    f,
                    "payload length {length} exceeds maximum {max} for message type '{message_type}'"
                )
            }
            FrameError::MissingDelimiter => write!(f, "missing delimiter '|'"),
            FrameError::MissingTerminator => write!(f, "missing terminator '\\n'"),
            FrameError::UnknownMessageType(t) => write!(f, "unknown message type: '{t}'"),
            FrameError::InvalidJson(e) => write!(f, "invalid JSON payload: {e}"),
            FrameError::Io(e) => write!(f, "I/O error: {e}"),
            FrameError::ConnectionClosed => write!(f, "connection closed"),
            FrameError::FrameTimeout => write!(f, "frame read timeout"),
            FrameError::IdleTimeout => write!(f, "idle timeout"),
            FrameError::StreamTimeout => write!(f, "stream timeout"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            FrameError::ConnectionClosed
        } else {
            FrameError::Io(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_error_display() {
        // Test all error variants have sensible display messages
        let cases: Vec<(FrameError, &str)> = vec![
            (
                FrameError::InvalidMagic,
                "invalid magic bytes, expected 'NX|'",
            ),
            (
                FrameError::InvalidMessageId,
                "invalid message ID, expected 12 hex characters",
            ),
            (FrameError::InvalidTypeLength, "invalid type length field"),
            (
                FrameError::TypeLengthOutOfRange,
                "type length must be between 1 and 999",
            ),
            (
                FrameError::TypeLengthTooManyDigits,
                "type length field exceeds 3 digits",
            ),
            (
                FrameError::InvalidPayloadLength,
                "invalid payload length field",
            ),
            (
                FrameError::PayloadLengthTooManyDigits,
                "payload length field exceeds 10 digits",
            ),
            (
                FrameError::PayloadLengthTooLarge,
                "payload length exceeds maximum of 9999999999",
            ),
            (
                FrameError::PayloadLengthExceedsTypeMax {
                    message_type: "ChatSend".to_string(),
                    length: 5000,
                    max: 2048,
                },
                "payload length 5000 exceeds maximum 2048 for message type 'ChatSend'",
            ),
            (FrameError::MissingDelimiter, "missing delimiter '|'"),
            (FrameError::MissingTerminator, "missing terminator '\\n'"),
            (
                FrameError::UnknownMessageType("FakeType".to_string()),
                "unknown message type: 'FakeType'",
            ),
            (
                FrameError::InvalidJson("expected value".to_string()),
                "invalid JSON payload: expected value",
            ),
            (
                FrameError::Io("connection reset".to_string()),
                "I/O error: connection reset",
            ),
            (FrameError::ConnectionClosed, "connection closed"),
            (FrameError::FrameTimeout, "frame read timeout"),
            (FrameError::IdleTimeout, "idle timeout"),
            (FrameError::StreamTimeout, "stream timeout"),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected, "Failed for {:?}", error);
        }
    }

    #[test]
    fn test_frame_error_from_io_error() {
        // UnexpectedEof maps to ConnectionClosed
        let eof_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof");
        assert_eq!(FrameError::from(eof_err), FrameError::ConnectionClosed);

        // Other errors map to Io variant
        let other_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe");
        let frame_err = FrameError::from(other_err);
        assert!(matches!(frame_err, FrameError::Io(_)));
    }
}
