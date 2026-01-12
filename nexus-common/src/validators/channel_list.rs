//! Channel list configuration validation
//!
//! Validates space-separated lists of channel names used for:
//! - `persistent_channels`: Channels that survive server restart
//! - `auto_join_channels`: Channels users automatically join on login

/// Maximum length for channel list configuration in bytes
///
/// This allows for multiple channel names separated by spaces.
/// With channel names up to 32 chars each, this supports approximately 15+ channels.
pub const MAX_CHANNEL_LIST_LENGTH: usize = 512;

/// Validation error for channel list configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelListError {
    /// Configuration string exceeds maximum length
    TooLong,
    /// Configuration contains invalid characters (control characters)
    InvalidCharacters,
    /// Configuration contains newlines
    ContainsNewlines,
}

/// Validate a channel list configuration string
///
/// This is the core validation function used by both `validate_persistent_channels`
/// and `validate_auto_join_channels`.
///
/// # Arguments
/// * `config` - The channel list configuration string to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(ChannelListError)` if invalid
///
/// # Validation Rules
/// - Must not exceed `MAX_CHANNEL_LIST_LENGTH` bytes
/// - Must not contain control characters
/// - Must not contain newlines
///
/// Note: Individual channel name validation should be done separately
/// after parsing the space-separated values.
pub fn validate_channel_list(config: &str) -> Result<(), ChannelListError> {
    if config.len() > MAX_CHANNEL_LIST_LENGTH {
        return Err(ChannelListError::TooLong);
    }

    for ch in config.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(ChannelListError::ContainsNewlines);
            }
            return Err(ChannelListError::InvalidCharacters);
        }
    }

    Ok(())
}

/// Validation error for persistent channels configuration
pub type PersistentChannelsError = ChannelListError;

/// Maximum length for persistent channels configuration in bytes
pub const MAX_PERSISTENT_CHANNELS_LENGTH: usize = MAX_CHANNEL_LIST_LENGTH;

/// Validate a persistent channels configuration string
///
/// Persistent channels survive server restart and cannot be deleted when empty.
///
/// # Arguments
/// * `config` - Space-separated list of channel names (e.g., "#nexus #support")
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(PersistentChannelsError)` if invalid
///
/// Note: Individual channel name validation should be done separately
/// after parsing the space-separated values.
pub fn validate_persistent_channels(config: &str) -> Result<(), PersistentChannelsError> {
    validate_channel_list(config)
}

/// Validation error for auto-join channels configuration
pub type AutoJoinChannelsError = ChannelListError;

/// Maximum length for auto-join channels configuration in bytes
pub const MAX_AUTO_JOIN_CHANNELS_LENGTH: usize = MAX_CHANNEL_LIST_LENGTH;

/// Validate an auto-join channels configuration string
///
/// Auto-join channels are automatically joined by users on login.
///
/// # Arguments
/// * `config` - Space-separated list of channel names (e.g., "#nexus #welcome")
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(AutoJoinChannelsError)` if invalid
///
/// Note: Individual channel name validation should be done separately
/// after parsing the space-separated values.
pub fn validate_auto_join_channels(config: &str) -> Result<(), AutoJoinChannelsError> {
    validate_channel_list(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Core channel list validation tests
    // ==========================================================================

    #[test]
    fn test_valid_channel_lists() {
        // Single channel
        assert!(validate_channel_list("#nexus").is_ok());

        // Multiple channels
        assert!(validate_channel_list("#nexus #support").is_ok());
        assert!(validate_channel_list("#nexus #support #general").is_ok());

        // Empty is valid (no channels)
        assert!(validate_channel_list("").is_ok());

        // Max length
        assert!(validate_channel_list(&"#".repeat(MAX_CHANNEL_LIST_LENGTH)).is_ok());

        // Unicode channel names
        assert!(validate_channel_list("#日本語 #Россия").is_ok());
    }

    #[test]
    fn test_too_long() {
        let long = "x".repeat(MAX_CHANNEL_LIST_LENGTH + 1);
        assert_eq!(validate_channel_list(&long), Err(ChannelListError::TooLong));
    }

    #[test]
    fn test_newlines_rejected() {
        assert_eq!(
            validate_channel_list("#nexus\n#support"),
            Err(ChannelListError::ContainsNewlines)
        );
        assert_eq!(
            validate_channel_list("#nexus\r#support"),
            Err(ChannelListError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_chars_rejected() {
        assert_eq!(
            validate_channel_list("#nexus\x00#support"),
            Err(ChannelListError::InvalidCharacters)
        );
        assert_eq!(
            validate_channel_list("#nexus\x07#support"),
            Err(ChannelListError::InvalidCharacters)
        );
    }

    // ==========================================================================
    // Persistent channels wrapper tests
    // ==========================================================================

    #[test]
    fn test_persistent_channels_valid() {
        assert!(validate_persistent_channels("#nexus #support").is_ok());
        assert!(validate_persistent_channels("").is_ok());
    }

    #[test]
    fn test_persistent_channels_too_long() {
        let long = "x".repeat(MAX_PERSISTENT_CHANNELS_LENGTH + 1);
        assert_eq!(
            validate_persistent_channels(&long),
            Err(PersistentChannelsError::TooLong)
        );
    }

    // ==========================================================================
    // Auto-join channels wrapper tests
    // ==========================================================================

    #[test]
    fn test_auto_join_channels_valid() {
        assert!(validate_auto_join_channels("#nexus #welcome").is_ok());
        assert!(validate_auto_join_channels("").is_ok());
    }

    #[test]
    fn test_auto_join_channels_too_long() {
        let long = "x".repeat(MAX_AUTO_JOIN_CHANNELS_LENGTH + 1);
        assert_eq!(
            validate_auto_join_channels(&long),
            Err(AutoJoinChannelsError::TooLong)
        );
    }

    // ==========================================================================
    // Constants tests
    // ==========================================================================

    #[test]
    fn test_constants_match() {
        // Ensure all length constants are the same (for now)
        assert_eq!(MAX_CHANNEL_LIST_LENGTH, MAX_PERSISTENT_CHANNELS_LENGTH);
        assert_eq!(MAX_CHANNEL_LIST_LENGTH, MAX_AUTO_JOIN_CHANNELS_LENGTH);
    }
}
