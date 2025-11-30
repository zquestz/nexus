//! Text helper functions for consistent text rendering
//!
//! This module provides text widgets with advanced shaping enabled for proper
//! CJK (Chinese, Japanese, Korean) character support and font fallback.

use iced::widget::text::{self, Text};

/// Create a text widget with advanced shaping enabled for CJK character support.
///
/// Advanced shaping enables font fallback, allowing the system to find fonts
/// that contain Chinese, Japanese, and Korean characters even if the default
/// font doesn't support them.
///
/// This should be used for ALL text in the application to ensure proper
/// display of internationalized content.
///
/// # Optimization
/// Empty strings (used as spacers) skip advanced shaping since there's no
/// text to render. This is a minor optimization with no functional impact.
///
/// # Example
/// ```ignore
/// use crate::style::shaped_text;
///
/// shaped_text("你好世界")  // Chinese characters will display correctly
///     .size(14)
/// ```
pub fn shaped_text<'a>(content: impl Into<String>) -> Text<'a> {
    let s = content.into();
    if s.is_empty() {
        text::Text::new(s)
    } else {
        text::Text::new(s).shaping(text::Shaping::Advanced)
    }
}

/// Create a text widget with advanced shaping and word wrapping enabled.
///
/// Same as [`shaped_text`] but also enables word wrapping for long text
/// that should flow across multiple lines. Use this for:
/// - Chat messages
/// - Error messages
/// - Any multi-line or potentially long content
///
/// # Example
/// ```ignore
/// use crate::style::shaped_text_wrapped;
///
/// shaped_text_wrapped("This is a long message that will wrap to the next line...")
///     .size(14)
/// ```
pub fn shaped_text_wrapped<'a>(content: impl Into<String>) -> Text<'a> {
    shaped_text(content).wrapping(text::Wrapping::Word)
}
