//! Features validation
//!
//! Validates the features list sent during login.

/// Maximum number of features allowed
pub const MAX_FEATURES_COUNT: usize = 16;

/// Maximum length for each feature name in bytes
pub const MAX_FEATURE_LENGTH: usize = 32;

/// Validation error for features
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeaturesError {
    /// Too many features in the list
    TooMany,
    /// A feature name is empty
    EmptyFeature,
    /// A feature name exceeds maximum length
    FeatureTooLong,
    /// A feature name contains invalid characters
    InvalidCharacters,
}

/// Validate a features list
///
/// Checks:
/// - Does not exceed maximum count (16 features)
/// - Each feature is not empty
/// - Each feature does not exceed maximum length (32 characters)
/// - No control characters in feature names
///
/// # Errors
///
/// Returns a `FeaturesError` variant describing the validation failure.
pub fn validate_features(features: &[String]) -> Result<(), FeaturesError> {
    if features.len() > MAX_FEATURES_COUNT {
        return Err(FeaturesError::TooMany);
    }
    for feature in features {
        if feature.is_empty() {
            return Err(FeaturesError::EmptyFeature);
        }
        if feature.len() > MAX_FEATURE_LENGTH {
            return Err(FeaturesError::FeatureTooLong);
        }
        for ch in feature.chars() {
            if ch.is_control() {
                return Err(FeaturesError::InvalidCharacters);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_features() {
        assert!(validate_features(&[]).is_ok());
        assert!(validate_features(&["chat".to_string()]).is_ok());
        assert!(validate_features(&["chat".to_string(), "file_transfer".to_string()]).is_ok());
        assert!(validate_features(&["a".repeat(MAX_FEATURE_LENGTH)]).is_ok());
        // At the limit
        let max_features: Vec<String> = (0..MAX_FEATURES_COUNT)
            .map(|i| format!("feature{}", i))
            .collect();
        assert!(validate_features(&max_features).is_ok());
    }

    #[test]
    fn test_too_many() {
        let too_many: Vec<String> = (0..MAX_FEATURES_COUNT + 1)
            .map(|i| format!("feature{}", i))
            .collect();
        assert_eq!(validate_features(&too_many), Err(FeaturesError::TooMany));
    }

    #[test]
    fn test_empty_feature() {
        assert_eq!(
            validate_features(&["".to_string()]),
            Err(FeaturesError::EmptyFeature)
        );
        assert_eq!(
            validate_features(&["chat".to_string(), "".to_string()]),
            Err(FeaturesError::EmptyFeature)
        );
    }

    #[test]
    fn test_feature_too_long() {
        assert_eq!(
            validate_features(&["a".repeat(MAX_FEATURE_LENGTH + 1)]),
            Err(FeaturesError::FeatureTooLong)
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            validate_features(&["chat\0".to_string()]),
            Err(FeaturesError::InvalidCharacters)
        );
        assert_eq!(
            validate_features(&["chat\n".to_string()]),
            Err(FeaturesError::InvalidCharacters)
        );
        assert_eq!(
            validate_features(&["chat\t".to_string()]),
            Err(FeaturesError::InvalidCharacters)
        );
    }
}
