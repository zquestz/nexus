//! Password validation
//!
//! Validates password strings for different contexts:
//! - `validate_password_input` - For login flow (empty allowed, auth decides)
//! - `validate_password` - For setting/changing passwords (must not be empty, strength enforced)
//!
//! Password strength is evaluated using zxcvbn (Dropbox algorithm) and expressed
//! as a `PasswordStrength` level (0-4). The server configures a minimum strength
//! requirement; the client displays a visual strength bar.

/// Maximum length for passwords in bytes
///
/// This limit prevents DoS attacks via Argon2 hashing of extremely long passwords.
pub const MAX_PASSWORD_LENGTH: usize = 256;

/// Password strength levels based on zxcvbn scoring
///
/// Variants are ordered from weakest to strongest. The `#[repr(u8)]` ensures
/// `score()` can use a simple cast, and the discriminant values match zxcvbn's
/// 0-4 scoring range exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PasswordStrength {
    /// Score 0: Trivially guessable
    Weak = 0,
    /// Score 1: Still easy to guess
    Fair = 1,
    /// Score 2: Reasonable password
    Good = 2,
    /// Score 3: Hard to guess
    Strong = 3,
    /// Score 4: Very hard to guess
    Excellent = 4,
}

impl PasswordStrength {
    /// All variants in order, useful for pick lists
    pub const ALL: &[PasswordStrength] = &[
        PasswordStrength::Weak,
        PasswordStrength::Fair,
        PasswordStrength::Good,
        PasswordStrength::Strong,
        PasswordStrength::Excellent,
    ];

    /// Get the numeric score (0-4)
    pub fn score(self) -> u8 {
        self as u8
    }
}

impl From<u8> for PasswordStrength {
    /// Convert a numeric score to a strength level.
    ///
    /// Values above 4 are clamped to `Excellent` (the highest level).
    fn from(score: u8) -> Self {
        match score {
            0 => Self::Weak,
            1 => Self::Fair,
            2 => Self::Good,
            3 => Self::Strong,
            _ => Self::Excellent,
        }
    }
}

/// Validation error for passwords
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    /// Password is empty
    Empty,
    /// Password exceeds maximum length
    TooLong,
    /// Password does not meet minimum strength requirement
    TooWeak {
        /// The strength level the server requires
        required: PasswordStrength,
        /// The actual strength of the password
        actual: PasswordStrength,
    },
}

/// Evaluate password strength using zxcvbn
///
/// Returns a `PasswordStrength` level based on the zxcvbn score (0-4).
/// Pass the username in `user_inputs` so that passwords based on the username
/// are penalized appropriately.
pub fn password_strength(password: &str, user_inputs: &[&str]) -> PasswordStrength {
    let entropy = zxcvbn::zxcvbn(password, user_inputs);
    match entropy.score() {
        zxcvbn::Score::Zero => PasswordStrength::Weak,
        zxcvbn::Score::One => PasswordStrength::Fair,
        zxcvbn::Score::Two => PasswordStrength::Good,
        zxcvbn::Score::Three => PasswordStrength::Strong,
        // Future-proof: any unknown score variant maps to the highest level
        _ => PasswordStrength::Excellent,
    }
}

/// Validate a password for login
///
/// Checks:
/// - Does not exceed maximum length (256 bytes)
///
/// Empty passwords are allowed - the authentication logic determines whether
/// an empty password is valid for a given account (e.g., guest accounts).
///
/// Strength is intentionally NOT checked here — existing users with weak
/// passwords must still be able to log in. Strength is only enforced when
/// setting or changing a password.
///
/// Note: We don't check for control characters in passwords since they
/// may be part of a passphrase or generated password.
///
/// # Errors
///
/// Returns a `PasswordError` variant describing the validation failure.
pub fn validate_password_input(password: &str) -> Result<(), PasswordError> {
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(PasswordError::TooLong);
    }
    Ok(())
}

/// Validate a password for setting or changing
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (256 bytes)
/// - Meets minimum strength requirement (via zxcvbn)
///
/// Use this when a user is creating an account or changing their password,
/// where a password must be provided.
///
/// Note: We don't check for control characters in passwords since they
/// may be part of a passphrase or generated password.
///
/// # Errors
///
/// Returns a `PasswordError` variant describing the validation failure.
pub fn validate_password(
    password: &str,
    min_strength: PasswordStrength,
    user_inputs: &[&str],
) -> Result<(), PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(PasswordError::TooLong);
    }

    // Skip zxcvbn when min_strength is Weak — any non-empty password passes,
    // and this avoids the computational cost in tests and permissive servers
    if min_strength > PasswordStrength::Weak {
        let strength = password_strength(password, user_inputs);
        if strength < min_strength {
            return Err(PasswordError::TooWeak {
                required: min_strength,
                actual: strength,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // validate_password_input tests (login flow)
    // ========================================================================

    #[test]
    fn test_input_valid_passwords() {
        assert!(validate_password_input("password123").is_ok());
        assert!(validate_password_input("a").is_ok());
        assert!(validate_password_input(&"a".repeat(MAX_PASSWORD_LENGTH)).is_ok());
        // Passwords can contain special characters
        assert!(validate_password_input("p@$$w0rd!#$%").is_ok());
        // Passwords can contain spaces
        assert!(validate_password_input("correct horse battery staple").is_ok());
        // Passwords can contain unicode
        assert!(validate_password_input("密码🔐").is_ok());
        // Passwords can contain control characters (passphrases, generated)
        assert!(validate_password_input("pass\tword").is_ok());
        assert!(validate_password_input("pass\nword").is_ok());
    }

    #[test]
    fn test_input_empty_allowed() {
        // Empty passwords are allowed for login (guest accounts)
        assert!(validate_password_input("").is_ok());
    }

    #[test]
    fn test_input_too_long() {
        assert_eq!(
            validate_password_input(&"a".repeat(MAX_PASSWORD_LENGTH + 1)),
            Err(PasswordError::TooLong)
        );
    }

    #[test]
    fn test_input_does_not_check_strength() {
        // Login flow must not reject weak passwords — existing users need to log in
        assert!(validate_password_input("a").is_ok());
        assert!(validate_password_input("password").is_ok());
        assert!(validate_password_input("123").is_ok());
    }

    // ========================================================================
    // validate_password tests (create/change flow)
    // ========================================================================

    #[test]
    fn test_valid_passwords() {
        assert!(validate_password("password123", PasswordStrength::Weak, &[]).is_ok());
        assert!(validate_password("a", PasswordStrength::Weak, &[]).is_ok());
        assert!(
            validate_password(
                &"a".repeat(MAX_PASSWORD_LENGTH),
                PasswordStrength::Weak,
                &[]
            )
            .is_ok()
        );
        // Passwords can contain special characters
        assert!(validate_password("p@$$w0rd!#$%", PasswordStrength::Weak, &[]).is_ok());
        // Passwords can contain spaces
        assert!(
            validate_password("correct horse battery staple", PasswordStrength::Weak, &[]).is_ok()
        );
        // Passwords can contain unicode
        assert!(validate_password("密码🔐", PasswordStrength::Weak, &[]).is_ok());
        // Passwords can contain control characters (passphrases, generated)
        assert!(validate_password("pass\tword", PasswordStrength::Weak, &[]).is_ok());
        assert!(validate_password("pass\nword", PasswordStrength::Weak, &[]).is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(
            validate_password("", PasswordStrength::Weak, &[]),
            Err(PasswordError::Empty)
        );
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_password(
                &"a".repeat(MAX_PASSWORD_LENGTH + 1),
                PasswordStrength::Weak,
                &[]
            ),
            Err(PasswordError::TooLong)
        );
    }

    #[test]
    fn test_too_weak_returns_correct_fields() {
        let result = validate_password("a", PasswordStrength::Good, &[]);
        assert_eq!(
            result,
            Err(PasswordError::TooWeak {
                required: PasswordStrength::Good,
                actual: PasswordStrength::Weak,
            })
        );
    }

    #[test]
    fn test_strength_boundary_exact_match_passes() {
        // A password that scores exactly at the minimum should pass
        let passphrase = "correct horse battery staple xkcd";
        let strength = password_strength(passphrase, &[]);
        assert!(validate_password(passphrase, strength, &[]).is_ok());
    }

    #[test]
    fn test_strength_boundary_one_below_fails() {
        // A password that scores one level below the minimum should fail
        let passphrase = "correct horse battery staple xkcd";
        let strength = password_strength(passphrase, &[]);
        if strength < PasswordStrength::Excellent {
            let one_above = PasswordStrength::from(strength.score() + 1);
            let result = validate_password(passphrase, one_above, &[]);
            assert!(matches!(result, Err(PasswordError::TooWeak { .. })));
        }
    }

    #[test]
    fn test_user_inputs_can_cause_rejection() {
        // A password that passes without user inputs may fail with them
        let password = "josh2024!!";
        let without = validate_password(password, PasswordStrength::Weak, &[]);
        assert!(without.is_ok());

        let strength_without = password_strength(password, &[]);
        let strength_with = password_strength(password, &["josh"]);

        // If user input lowered the score below the threshold, validation should fail
        if strength_with < strength_without {
            let result = validate_password(password, strength_without, &["josh"]);
            assert!(matches!(result, Err(PasswordError::TooWeak { .. })));
        }
    }

    #[test]
    fn test_validate_each_strength_level() {
        // With min Weak, even trivial passwords pass
        assert!(validate_password("a", PasswordStrength::Weak, &[]).is_ok());

        // With min Excellent, only very strong passwords pass
        assert!(
            validate_password(
                "7h!s_1s-a.V3ry$Str0ng&P@ssphr4se#2025",
                PasswordStrength::Excellent,
                &[]
            )
            .is_ok()
        );

        // With min Excellent, a common password fails
        let result = validate_password("password123", PasswordStrength::Excellent, &[]);
        assert!(matches!(result, Err(PasswordError::TooWeak { .. })));
    }

    // ========================================================================
    // PasswordStrength tests
    // ========================================================================

    #[test]
    fn test_password_strength_from_score() {
        assert_eq!(PasswordStrength::from(0), PasswordStrength::Weak);
        assert_eq!(PasswordStrength::from(1), PasswordStrength::Fair);
        assert_eq!(PasswordStrength::from(2), PasswordStrength::Good);
        assert_eq!(PasswordStrength::from(3), PasswordStrength::Strong);
        assert_eq!(PasswordStrength::from(4), PasswordStrength::Excellent);
    }

    #[test]
    fn test_password_strength_from_score_clamps_above_max() {
        assert_eq!(PasswordStrength::from(5), PasswordStrength::Excellent);
        assert_eq!(PasswordStrength::from(u8::MAX), PasswordStrength::Excellent);
    }

    #[test]
    fn test_password_strength_score_roundtrip() {
        for variant in PasswordStrength::ALL {
            assert_eq!(PasswordStrength::from(variant.score()), *variant);
        }
    }

    #[test]
    fn test_password_strength_ordering() {
        assert!(PasswordStrength::Weak < PasswordStrength::Fair);
        assert!(PasswordStrength::Fair < PasswordStrength::Good);
        assert!(PasswordStrength::Good < PasswordStrength::Strong);
        assert!(PasswordStrength::Strong < PasswordStrength::Excellent);
    }

    #[test]
    fn test_password_strength_score() {
        assert_eq!(PasswordStrength::Weak.score(), 0);
        assert_eq!(PasswordStrength::Fair.score(), 1);
        assert_eq!(PasswordStrength::Good.score(), 2);
        assert_eq!(PasswordStrength::Strong.score(), 3);
        assert_eq!(PasswordStrength::Excellent.score(), 4);
    }

    #[test]
    fn test_password_strength_all_is_complete_and_ordered() {
        assert_eq!(PasswordStrength::ALL.len(), 5);
        for i in 1..PasswordStrength::ALL.len() {
            assert!(PasswordStrength::ALL[i - 1] < PasswordStrength::ALL[i]);
        }
    }

    // ========================================================================
    // zxcvbn integration tests
    // ========================================================================

    #[test]
    fn test_password_strength_weak() {
        assert_eq!(password_strength("a", &[]), PasswordStrength::Weak);
        assert_eq!(password_strength("password", &[]), PasswordStrength::Weak);
        assert_eq!(password_strength("123456", &[]), PasswordStrength::Weak);
    }

    #[test]
    fn test_password_strength_excellent() {
        assert_eq!(
            password_strength("7h!s_1s-a.V3ry$Str0ng&P@ssphr4se#2025", &[]),
            PasswordStrength::Excellent
        );
    }

    #[test]
    fn test_password_strength_user_inputs_penalize() {
        let without = password_strength("josh2024!!", &[]);
        let with = password_strength("josh2024!!", &["josh"]);
        assert!(with <= without);
    }
}
