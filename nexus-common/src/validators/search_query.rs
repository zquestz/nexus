//! File search query validation
//!
//! Validates search queries for the file search feature.
//!
//! ## Search Term Logic
//!
//! - Minimum 3 bytes total (after trimming)
//! - If ALL terms are < 3 bytes: treat entire query as literal phrase search
//! - Otherwise: AND logic with terms >= 2 bytes (single-byte terms filtered out)
//!
//! Note: Length is measured in bytes, not characters. ASCII chars are 1 byte,
//! but Unicode chars vary (e.g., CJK chars are typically 3 bytes each).
//!
//! ## Examples
//!
//! - `"mr dj"` → literal "mr dj" (all terms short)
//! - `"mr carter"` → AND("mr", "carter") (carter is 3+)
//! - `"a b c"` → literal "a b c" (all terms short)
//! - `"a test"` → AND("test") only ("a" filtered as single char)
//! - `"test file mp3"` → AND("test", "file", "mp3")

/// Maximum length for search queries in bytes
pub const MAX_SEARCH_QUERY_LENGTH: usize = 256;

/// Minimum length for the entire query in bytes (after trimming)
pub const MIN_QUERY_LENGTH: usize = 3;

/// Minimum length for a term in bytes to be considered "primary" (triggers AND mode)
pub const MIN_PRIMARY_TERM_LENGTH: usize = 3;

/// Minimum length for a term in bytes to be included in AND search
pub const MIN_TERM_LENGTH: usize = 2;

/// Validation error for search queries
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchQueryError {
    /// Query is empty or contains only whitespace
    Empty,
    /// Query is too short (less than 3 bytes)
    TooShort,
    /// Query exceeds maximum length
    TooLong,
    /// Query contains invalid characters (control characters)
    InvalidCharacters,
}

/// Validate a file search query
///
/// Checks:
/// - Not empty or whitespace-only
/// - At least 3 bytes total (after trimming)
/// - Does not exceed maximum length (256 bytes)
/// - No control characters
///
/// # Errors
///
/// Returns a `SearchQueryError` variant describing the validation failure.
pub fn validate_search_query(query: &str) -> Result<(), SearchQueryError> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return Err(SearchQueryError::Empty);
    }

    // Check for control characters
    for ch in query.chars() {
        if ch.is_control() {
            return Err(SearchQueryError::InvalidCharacters);
        }
    }

    // Check max length on raw input
    if query.len() > MAX_SEARCH_QUERY_LENGTH {
        return Err(SearchQueryError::TooLong);
    }

    // Must have at least 3 bytes total (after trimming)
    if trimmed.len() < MIN_QUERY_LENGTH {
        return Err(SearchQueryError::TooShort);
    }

    Ok(())
}

/// Extract valid search terms from a query
///
/// Returns terms that should be used for searching:
/// - If ALL terms are < 3 bytes: returns the entire trimmed query as a single literal term
/// - Otherwise: returns all terms with 2+ bytes (single-byte terms filtered out)
///
/// This should be called after `validate_search_query` succeeds.
pub fn extract_search_terms(query: &str) -> Vec<&str> {
    let trimmed = query.trim();

    // Empty query returns no terms
    if trimmed.is_empty() {
        return vec![];
    }

    let terms: Vec<&str> = trimmed.split_whitespace().collect();

    // Check if ANY term is 3+ bytes (primary term)
    let has_primary_term = terms
        .iter()
        .any(|term| term.len() >= MIN_PRIMARY_TERM_LENGTH);

    if has_primary_term {
        // AND mode: return all terms with 2+ chars
        terms
            .into_iter()
            .filter(|term| term.len() >= MIN_TERM_LENGTH)
            .collect()
    } else {
        // Literal mode: all terms are short, treat entire query as literal
        vec![trimmed]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // validate_search_query Tests
    // =========================================================================

    #[test]
    fn test_valid_queries() {
        assert!(validate_search_query("abc").is_ok());
        assert!(validate_search_query("test").is_ok());
        assert!(validate_search_query("Hello, world!").is_ok());
        assert!(validate_search_query(&"a".repeat(MAX_SEARCH_QUERY_LENGTH)).is_ok());
        // Unicode
        assert!(validate_search_query("日本語").is_ok());
        assert!(validate_search_query("файл").is_ok());
        // Multiple terms
        assert!(validate_search_query("alice bob mp3").is_ok());
        assert!(validate_search_query("mr alice").is_ok());
        assert!(validate_search_query("a test").is_ok());
        // All short terms (literal mode) - still valid if >= 3 bytes total
        assert!(validate_search_query("mr dj").is_ok());
        assert!(validate_search_query("ab cd").is_ok());
        assert!(validate_search_query("a b c").is_ok());
    }

    #[test]
    fn test_empty_queries() {
        assert_eq!(validate_search_query(""), Err(SearchQueryError::Empty));
        assert_eq!(validate_search_query("   "), Err(SearchQueryError::Empty));
        assert_eq!(validate_search_query("\t\t"), Err(SearchQueryError::Empty));
    }

    #[test]
    fn test_too_short() {
        assert_eq!(validate_search_query("a"), Err(SearchQueryError::TooShort));
        assert_eq!(validate_search_query("ab"), Err(SearchQueryError::TooShort));
        assert_eq!(
            validate_search_query("  ab  "),
            Err(SearchQueryError::TooShort)
        );
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_search_query(&"a".repeat(MAX_SEARCH_QUERY_LENGTH + 1)),
            Err(SearchQueryError::TooLong)
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            validate_search_query("test\0file"),
            Err(SearchQueryError::InvalidCharacters)
        );
        assert_eq!(
            validate_search_query("test\nfile"),
            Err(SearchQueryError::InvalidCharacters)
        );
        assert_eq!(
            validate_search_query("test\rfile"),
            Err(SearchQueryError::InvalidCharacters)
        );
        assert_eq!(
            validate_search_query("test\tfile"),
            Err(SearchQueryError::InvalidCharacters)
        );
    }

    #[test]
    fn test_special_characters_allowed() {
        assert!(validate_search_query("file.txt").is_ok());
        assert!(validate_search_query("my-file").is_ok());
        assert!(validate_search_query("my_file").is_ok());
        assert!(validate_search_query("file (1)").is_ok());
        assert!(validate_search_query("test@#$%").is_ok());
        assert!(validate_search_query("*.txt").is_ok());
    }

    #[test]
    fn test_boundary_length() {
        // Exactly at minimum (3 bytes)
        assert!(validate_search_query("abc").is_ok());
        // Exactly at maximum
        assert!(validate_search_query(&"x".repeat(MAX_SEARCH_QUERY_LENGTH)).is_ok());
    }

    // =========================================================================
    // extract_search_terms Tests
    // =========================================================================

    #[test]
    fn test_extract_terms_and_mode() {
        // Has 3+ byte term -> AND mode with 2+ byte terms
        let terms = extract_search_terms("alice bob");
        assert_eq!(terms, vec!["alice", "bob"]);

        let terms = extract_search_terms("mr carter");
        assert_eq!(terms, vec!["mr", "carter"]);

        let terms = extract_search_terms("test file mp3");
        assert_eq!(terms, vec!["test", "file", "mp3"]);
    }

    #[test]
    fn test_extract_terms_filters_single_char() {
        // Single char terms filtered in AND mode
        let terms = extract_search_terms("a test b");
        assert_eq!(terms, vec!["test"]);

        let terms = extract_search_terms("alice a bob b");
        assert_eq!(terms, vec!["alice", "bob"]);
    }

    #[test]
    fn test_extract_terms_literal_mode() {
        // All terms < 3 bytes -> literal mode (entire query)
        let terms = extract_search_terms("mr dj");
        assert_eq!(terms, vec!["mr dj"]);

        let terms = extract_search_terms("ab cd");
        assert_eq!(terms, vec!["ab cd"]);

        let terms = extract_search_terms("a b c");
        assert_eq!(terms, vec!["a b c"]);

        let terms = extract_search_terms("ab");
        assert_eq!(terms, vec!["ab"]);
    }

    #[test]
    fn test_extract_terms_literal_preserves_spacing() {
        // Trimmed but internal spacing preserved in literal mode
        let terms = extract_search_terms("  mr dj  ");
        assert_eq!(terms, vec!["mr dj"]);
    }

    #[test]
    fn test_extract_terms_empty() {
        let terms = extract_search_terms("");
        assert!(terms.is_empty());

        let terms = extract_search_terms("   ");
        assert!(terms.is_empty());
    }

    #[test]
    fn test_extract_terms_unicode() {
        // Note: len() is bytes, not chars. CJK chars are 3 bytes each.
        // 日本語 is 9 bytes, so it's a primary term
        let terms = extract_search_terms("日本語 ab test");
        assert_eq!(terms, vec!["日本語", "ab", "test"]);
    }

    #[test]
    fn test_extract_terms_boundary() {
        // Exactly 3 bytes triggers AND mode
        let terms = extract_search_terms("abc xy");
        assert_eq!(terms, vec!["abc", "xy"]);

        // Exactly 2 chars included in AND mode
        let terms = extract_search_terms("test ab");
        assert_eq!(terms, vec!["test", "ab"]);
    }
}
