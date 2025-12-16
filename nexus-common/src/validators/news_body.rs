//! News body validation
//!
//! Validates markdown content for news posts. Unlike chat messages,
//! news body allows newlines and tabs for markdown formatting.

/// Maximum length for news body in characters
pub const MAX_NEWS_BODY_LENGTH: usize = 4096;

/// Validation error for news body
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsBodyError {
    /// Body exceeds maximum length
    TooLong,
    /// Body contains invalid control characters (not newline/tab)
    InvalidCharacters,
}

/// Validate a news body (markdown content)
///
/// Checks:
/// - Does not exceed maximum length (4096 characters)
/// - No control characters except newlines (\n, \r) and tabs (\t)
///
/// Note: Empty body is allowed (news can be image-only).
/// The requirement for at least body OR image is enforced at the handler level.
///
/// # Errors
///
/// Returns a `NewsBodyError` variant describing the validation failure.
pub fn validate_news_body(body: &str) -> Result<(), NewsBodyError> {
    if body.len() > MAX_NEWS_BODY_LENGTH {
        return Err(NewsBodyError::TooLong);
    }

    for ch in body.chars() {
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            return Err(NewsBodyError::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_body() {
        assert!(validate_news_body("Hello, world!").is_ok());
        assert!(validate_news_body("a").is_ok());
        assert!(validate_news_body(&"a".repeat(MAX_NEWS_BODY_LENGTH)).is_ok());
    }

    #[test]
    fn test_empty_body() {
        // Empty is allowed (image-only news)
        assert!(validate_news_body("").is_ok());
        assert!(validate_news_body("   ").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_news_body(&"a".repeat(MAX_NEWS_BODY_LENGTH + 1)),
            Err(NewsBodyError::TooLong)
        );
    }

    #[test]
    fn test_newlines_allowed() {
        // Newlines should be allowed for markdown
        assert!(validate_news_body("Hello\nWorld").is_ok());
        assert!(validate_news_body("Hello\r\nWorld").is_ok());
        assert!(validate_news_body("Line1\n\nLine2").is_ok());
        assert!(validate_news_body("# Header\n\nParagraph").is_ok());
    }

    #[test]
    fn test_tabs_allowed() {
        // Tabs should be allowed for markdown code blocks
        assert!(validate_news_body("Hello\tWorld").is_ok());
        assert!(validate_news_body("\tindented").is_ok());
        assert!(validate_news_body("```\n\tcode\n```").is_ok());
    }

    #[test]
    fn test_markdown_content() {
        let markdown = r#"# News Title

This is a **bold** statement and _italic_ text.

## Features

- Item 1
- Item 2
- Item 3

```rust
fn main() {
	println!("Hello!");
}
```

> A blockquote

[Link](https://example.com)
"#;
        assert!(validate_news_body(markdown).is_ok());
    }

    #[test]
    fn test_unicode_content() {
        // Unicode should be allowed
        assert!(validate_news_body("日本語").is_ok());
        assert!(validate_news_body("Привет").is_ok());
        assert!(validate_news_body("مرحبا").is_ok());
        assert!(validate_news_body("Hello 👋 World").is_ok());
        assert!(validate_news_body("Math: ∑∏∫").is_ok());
    }

    #[test]
    fn test_invalid_control_characters() {
        // Null byte
        assert_eq!(
            validate_news_body("Hello\0World"),
            Err(NewsBodyError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_news_body("Hello\x01World"),
            Err(NewsBodyError::InvalidCharacters)
        );
        assert_eq!(
            validate_news_body("Test\x7FDelete"),
            Err(NewsBodyError::InvalidCharacters)
        );
        // Escape character
        assert_eq!(
            validate_news_body("Test\x1BEscape"),
            Err(NewsBodyError::InvalidCharacters)
        );
    }
}
