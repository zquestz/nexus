//! Password strength bar widget and localized wrapper
//!
//! Displays a segmented colored bar indicating password strength.
//! Each of the five segments represents one strength level (Weak through Excellent).
//! Segments up to the current score are filled with the strength color; the rest are gray.
//! A label beneath the bar shows the strength level in the corresponding color.
//!
//! Also provides `LocalizedPasswordStrength`, a wrapper for use in pick lists
//! that implements `Display` via i18n (same pattern as `LocalizedVoiceQuality`).

use iced::widget::{Space, container, row};
use iced::{Background, Color, Element, Length, Theme};
use nexus_common::validators::{PasswordStrength, password_strength};

use crate::i18n::t;
use crate::style::ui::{
    adjust_brightness, password_strength_excellent, password_strength_fair, password_strength_good,
    password_strength_strong, password_strength_weak,
};
use crate::style::{TEXT_SIZE, shaped_text};
use crate::types::Message;

/// Height of each strength bar segment in pixels
const BAR_HEIGHT: f32 = 6.0;

/// Spacing between bar segments
const SEGMENT_SPACING: f32 = 2.0;

/// Spacing between bar and label
const BAR_LABEL_SPACING: f32 = 2.0;

/// Number of strength levels (segments)
const NUM_SEGMENTS: u8 = 5;

/// Brightness adjustment for unfilled bar segments (subtle contrast from background)
const UNFILLED_BRIGHTNESS: f32 = 0.15;

/// Brightness adjustment for threshold marker (stronger contrast from background)
const THRESHOLD_BRIGHTNESS: f32 = 0.35;

/// Localized wrapper for `PasswordStrength` enabling use in pick lists.
///
/// `PasswordStrength` lives in `nexus-common` which has no access to the
/// client's i18n system. This wrapper provides `Display` via `translation_key()`
/// so the pick list renders localized labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizedPasswordStrength(pub PasswordStrength);

impl LocalizedPasswordStrength {
    /// Get all strength options as localized wrappers
    pub fn all() -> Vec<LocalizedPasswordStrength> {
        PasswordStrength::ALL
            .iter()
            .map(|&s| LocalizedPasswordStrength(s))
            .collect()
    }
}

impl std::fmt::Display for LocalizedPasswordStrength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(strength_translation_key(self.0)))
    }
}

impl From<PasswordStrength> for LocalizedPasswordStrength {
    fn from(s: PasswordStrength) -> Self {
        LocalizedPasswordStrength(s)
    }
}

impl From<LocalizedPasswordStrength> for PasswordStrength {
    fn from(ls: LocalizedPasswordStrength) -> Self {
        ls.0
    }
}

/// Create a password strength bar with label.
///
/// Returns `None` if the password is empty (bar should not be shown).
/// The `min_strength` parameter controls where a threshold marker is drawn
/// on the bar, indicating the minimum acceptable strength level.
pub fn password_strength_bar(
    password: &str,
    username: &str,
    min_strength: PasswordStrength,
    theme: &Theme,
) -> Option<Element<'static, Message>> {
    if password.is_empty() {
        return None;
    }

    let user_inputs: Vec<&str> = if username.is_empty() {
        vec![]
    } else {
        vec![username]
    };
    let strength = password_strength(password, &user_inputs);
    let score = strength.score();
    let threshold_score = min_strength.score();

    let bg = theme.palette().background;
    let is_dark = theme.extended_palette().is_dark;
    let unfilled_color = adjust_brightness(bg, UNFILLED_BRIGHTNESS, is_dark);
    let threshold_color = adjust_brightness(bg, THRESHOLD_BRIGHTNESS, is_dark);

    let bar_color = strength_color(strength, theme);

    let label_key = strength_translation_key(strength);

    // Build bar as 5 segments, filled up to the current score
    let mut segments: Vec<Element<'static, Message>> = Vec::with_capacity(NUM_SEGMENTS as usize);
    for i in 0..NUM_SEGMENTS {
        let color = if i <= score {
            bar_color
        } else {
            unfilled_color
        };

        // Place a right-side threshold marker on the segment that corresponds
        // to min_strength, using a thin contrasting edge on the right side.
        let is_threshold_segment = i == threshold_score && threshold_score < NUM_SEGMENTS - 1;

        let segment: Element<'static, Message> = if is_threshold_segment {
            // Threshold segment: inner fill + thin white right edge
            let marker_width = 2.0;
            let inner = container(Space::new())
                .width(Length::Fill)
                .height(BAR_HEIGHT)
                .style(move |_: &Theme| container::Style {
                    background: Some(Background::Color(color)),
                    ..Default::default()
                });
            let marker = container(Space::new())
                .width(marker_width)
                .height(BAR_HEIGHT)
                .style(move |_: &Theme| container::Style {
                    background: Some(Background::Color(threshold_color)),
                    ..Default::default()
                });
            row![inner, marker].width(Length::Fill).into()
        } else {
            container(Space::new())
                .width(Length::Fill)
                .height(BAR_HEIGHT)
                .style(move |_: &Theme| container::Style {
                    background: Some(Background::Color(color)),
                    ..Default::default()
                })
                .into()
        };

        segments.push(segment);
    }

    let bar = row(segments).spacing(SEGMENT_SPACING).width(Length::Fill);
    let label = shaped_text(t(label_key)).size(TEXT_SIZE).color(bar_color);

    Some(
        iced::widget::column![bar, label]
            .spacing(BAR_LABEL_SPACING)
            .into(),
    )
}

/// Get the i18n translation key for a password strength level.
pub(crate) fn strength_translation_key(strength: PasswordStrength) -> &'static str {
    match strength {
        PasswordStrength::Weak => "password-strength-weak",
        PasswordStrength::Fair => "password-strength-fair",
        PasswordStrength::Good => "password-strength-good",
        PasswordStrength::Strong => "password-strength-strong",
        PasswordStrength::Excellent => "password-strength-excellent",
    }
}

/// Get the color for a given strength level.
fn strength_color(strength: PasswordStrength, theme: &Theme) -> Color {
    match strength {
        PasswordStrength::Weak => password_strength_weak(theme),
        PasswordStrength::Fair => password_strength_fair(theme),
        PasswordStrength::Good => password_strength_good(theme),
        PasswordStrength::Strong => password_strength_strong(theme),
        PasswordStrength::Excellent => password_strength_excellent(theme),
    }
}
