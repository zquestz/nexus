//! Channel name validation
//!
//! Validates channel names for chat rooms.
//! Channel names must start with `#` and follow similar rules to usernames.

/// Prefix required for all channel names
pub const CHANNEL_PREFIX: char = '#';

/// The default/main channel name
pub const DEFAULT_CHANNEL: &str = "#nexus";

/// Maximum length for channel names in characters (including the `#` prefix)
pub const MAX_CHANNEL_LENGTH: usize = 32;

/// Minimum length for channel names in characters (including the `#` prefix)
pub const MIN_CHANNEL_LENGTH: usize = 2;

/// Maximum number of channels a user can be a member of simultaneously
///
/// This limit prevents resource exhaustion attacks where a malicious user
/// joins thousands of ephemeral channels to consume server memory.
pub const MAX_CHANNELS_PER_USER: usize = 100;

/// Characters that are not allowed in channel names (after the `#` prefix)
///
/// - Space: Command parsing ambiguity (`/join #my channel` - where does name end?)
/// - `#`: Parsing ambiguity (could be confused with channel prefix)
const FORBIDDEN_CHARS: &[char] = &[' ', '#'];

/// Validation error for channel names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelError {
    /// Channel name is empty
    Empty,
    /// Channel name doesn't start with `#`
    MissingPrefix,
    /// Channel name is just `#` with no name
    TooShort,
    /// Channel name exceeds maximum length
    TooLong,
    /// Channel name contains invalid characters
    InvalidCharacters,
}

/// Validate a channel name
///
/// Checks:
/// - Not empty
/// - Starts with `#`
/// - Has at least one character after `#`
/// - Does not exceed maximum length (32 characters including `#`)
/// - Contains only valid characters after `#`:
///   - Unicode letters (any language)
///   - ASCII graphic characters (printable non-space: `!` through `~`)
///   - No whitespace or control characters
///   - No additional `#` characters
///
/// # Errors
///
/// Returns a `ChannelError` variant describing the validation failure.
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_channel, ChannelError, CHANNEL_PREFIX};
///
/// // Valid channel names
/// assert!(validate_channel("#general").is_ok());
/// assert!(validate_channel("#support").is_ok());
/// assert!(validate_channel("#dev-team").is_ok());
/// assert!(validate_channel("#日本語").is_ok());
///
/// // Invalid channel names
/// assert_eq!(validate_channel(""), Err(ChannelError::Empty));
/// assert_eq!(validate_channel("general"), Err(ChannelError::MissingPrefix));
/// assert_eq!(validate_channel("#"), Err(ChannelError::TooShort));
/// assert_eq!(validate_channel("#has space"), Err(ChannelError::InvalidCharacters));
/// ```
pub fn validate_channel(channel: &str) -> Result<(), ChannelError> {
    if channel.is_empty() {
        return Err(ChannelError::Empty);
    }

    if !channel.starts_with(CHANNEL_PREFIX) {
        return Err(ChannelError::MissingPrefix);
    }

    let char_count = channel.chars().count();

    if char_count < MIN_CHANNEL_LENGTH {
        return Err(ChannelError::TooShort);
    }

    if char_count > MAX_CHANNEL_LENGTH {
        return Err(ChannelError::TooLong);
    }

    // Validate characters after the prefix
    for ch in channel.chars().skip(1) {
        if FORBIDDEN_CHARS.contains(&ch) {
            return Err(ChannelError::InvalidCharacters);
        }
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(ChannelError::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_channel_names() {
        assert!(validate_channel("#general").is_ok());
        assert!(validate_channel("#support").is_ok());
        assert!(validate_channel("#dev-team").is_ok());
        assert!(validate_channel("#dev.team").is_ok());
        assert!(validate_channel("#channel_name").is_ok());
        assert!(validate_channel("#123").is_ok());
        assert!(validate_channel("#a").is_ok());
        assert!(validate_channel("#server").is_ok());
        // Special characters (allowed in channels, unlike usernames)
        assert!(validate_channel("#dev/ops").is_ok());
        assert!(validate_channel("#c:\\temp").is_ok());
        assert!(validate_channel("#what?").is_ok());
        assert!(validate_channel("#star*power").is_ok());
        // Unicode letters
        assert!(validate_channel("#日本語").is_ok());
        assert!(validate_channel("#Россия").is_ok());
        assert!(validate_channel("#チャット").is_ok());
        // Mixed
        assert!(validate_channel("#dev日本").is_ok());
        // At max length
        let max_name = format!("#{}", "a".repeat(MAX_CHANNEL_LENGTH - 1));
        assert!(validate_channel(&max_name).is_ok());
    }

    #[test]
    fn test_default_channel_is_valid() {
        assert!(validate_channel(DEFAULT_CHANNEL).is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_channel(""), Err(ChannelError::Empty));
    }

    #[test]
    fn test_missing_prefix() {
        assert_eq!(
            validate_channel("general"),
            Err(ChannelError::MissingPrefix)
        );
        assert_eq!(validate_channel("server"), Err(ChannelError::MissingPrefix));
        assert_eq!(validate_channel("a"), Err(ChannelError::MissingPrefix));
    }

    #[test]
    fn test_too_short() {
        assert_eq!(validate_channel("#"), Err(ChannelError::TooShort));
    }

    #[test]
    fn test_too_long() {
        let too_long = format!("#{}", "a".repeat(MAX_CHANNEL_LENGTH));
        assert_eq!(validate_channel(&too_long), Err(ChannelError::TooLong));
    }

    #[test]
    fn test_invalid_characters() {
        // Spaces not allowed
        assert_eq!(
            validate_channel("#channel name"),
            Err(ChannelError::InvalidCharacters)
        );
        // Control characters not allowed
        assert_eq!(
            validate_channel("#channel\0name"),
            Err(ChannelError::InvalidCharacters)
        );
        assert_eq!(
            validate_channel("#channel\tname"),
            Err(ChannelError::InvalidCharacters)
        );
        assert_eq!(
            validate_channel("#channel\nname"),
            Err(ChannelError::InvalidCharacters)
        );
        // Additional # not allowed
        assert_eq!(
            validate_channel("##channel"),
            Err(ChannelError::InvalidCharacters)
        );
        assert_eq!(
            validate_channel("#chan#nel"),
            Err(ChannelError::InvalidCharacters)
        );
    }

    #[test]
    fn test_constants() {
        assert_eq!(CHANNEL_PREFIX, '#');
        assert_eq!(DEFAULT_CHANNEL, "#nexus");
        assert_eq!(MAX_CHANNEL_LENGTH, 32);
        assert_eq!(MIN_CHANNEL_LENGTH, 2);
    }
}
