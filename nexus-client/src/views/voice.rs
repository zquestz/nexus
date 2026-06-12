//! Voice chat UI components
//!
//! This module provides UI elements for voice chat:
//! - Voice bar: Shows above the input when in a voice session
//! - Voice button: Join/leave toggle in the input row

use iced::widget::{Id, Row, Space, button, container, row, text_input, tooltip};
use iced::{Background, Border, Element, Fill, Theme};

use crate::i18n::{t, t_args};
use crate::icon;
use crate::style::{
    INPUT_PADDING, MONOSPACE_FONT, SMALL_SPACING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, shaped_text, speaking_indicator_style,
    tooltip_container_style, voice_bar_style, voice_deafen_button_style,
};
use crate::types::{InputId, Message, ServerConnection, VoiceState};

// =============================================================================
// Constants
// =============================================================================

/// Font size for the voice bar text
const VOICE_BAR_FONT_SIZE: f32 = 13.0;

/// Spacing within the voice bar
const VOICE_BAR_SPACING: f32 = 8.0;

/// Padding for the voice bar container
const VOICE_BAR_PADDING: f32 = 6.0;

/// Icon size for voice bar
const VOICE_BAR_ICON_SIZE: f32 = 14.0;

/// Padding for buttons inside the voice bar (tighter than bar padding)
const VOICE_BAR_BUTTON_PADDING: f32 = 4.0;

/// Fixed width for the deafen button area to prevent layout shifting
const DEAFEN_BUTTON_WIDTH: f32 = 24.0;

/// Maximum number of speaking users to show in voice bar
const MAX_SPEAKING_DISPLAY: usize = 3;

/// Number of segments in the VU meter
pub const VU_METER_SEGMENTS: usize = 8;

/// Width of each VU meter segment (small, for voice bar)
const VU_METER_SEGMENT_WIDTH_SMALL: f32 = 4.0;

/// Height of each VU meter segment (small, for voice bar)
const VU_METER_SEGMENT_HEIGHT_SMALL: f32 = 10.0;

/// Gap between VU meter segments
pub const VU_METER_SEGMENT_GAP: f32 = 2.0;

/// Threshold for yellow segments (60%)
pub const VU_METER_YELLOW_THRESHOLD: f32 = 0.6;

/// Threshold for red segments (80%)
pub const VU_METER_RED_THRESHOLD: f32 = 0.8;

// =============================================================================
// Voice Bar
// =============================================================================

/// Build the voice bar that appears above the chat input when in a voice session
///
/// Shows:
/// - Headphones icon
/// - Target name (channel or other user)
/// - Participant count
/// - Speaking indicators (who's currently talking)
/// - Local speaking indicator (if transmitting)
/// - Deafen toggle button
pub fn build_voice_bar(
    session: &VoiceState,
    is_local_speaking: bool,
    is_deafened: bool,
    mic_level: f32,
    theme: &Theme,
) -> Element<'static, Message> {
    // Icon
    let headphones_icon = icon::headphones().size(VOICE_BAR_ICON_SIZE);

    // Target name (channel like "#general" or other user's nickname)
    let target_text = shaped_text(&session.target).size(VOICE_BAR_FONT_SIZE);

    // Participant count (server includes self in list)
    let count = session.participant_count();

    // Build the bar content
    let mut bar_row = Row::new().spacing(VOICE_BAR_SPACING).align_y(iced::Center);

    bar_row = bar_row.push(headphones_icon);
    bar_row = bar_row.push(target_text);

    let count_text = shaped_text(t_args(
        "voice-bar-participants",
        &[("count", &count.to_string())],
    ))
    .size(VOICE_BAR_FONT_SIZE);
    bar_row = bar_row.push(count_text);

    // Add speaking indicators
    let speaking_users: Vec<_> = session
        .speaking_users
        .iter()
        .take(MAX_SPEAKING_DISPLAY)
        .collect();
    if !speaking_users.is_empty() || is_local_speaking {
        // Separator
        bar_row = bar_row.push(shaped_text("│").size(VOICE_BAR_FONT_SIZE));

        // Show local speaking indicator with VU meter
        if is_local_speaking {
            let local_indicator =
                container(icon::mic().size(VOICE_BAR_ICON_SIZE)).style(speaking_indicator_style);
            bar_row = bar_row.push(local_indicator);

            // Add VU meter (small size for voice bar)
            let vu_meter = build_vu_meter(
                mic_level,
                theme,
                VU_METER_SEGMENT_WIDTH_SMALL,
                VU_METER_SEGMENT_HEIGHT_SMALL,
            );
            bar_row = bar_row.push(vu_meter);
        }

        // Show who's speaking (up to MAX_SPEAKING_DISPLAY)
        for nickname in &speaking_users {
            let speaker_text = shaped_text(nickname.as_str()).size(VOICE_BAR_FONT_SIZE);
            let speaker_indicator = container(speaker_text).style(speaking_indicator_style);
            bar_row = bar_row.push(speaker_indicator);
        }

        // Show overflow count if more people are speaking
        let total_speaking = session.speaking_count() + if is_local_speaking { 1 } else { 0 };
        let shown = speaking_users.len() + if is_local_speaking { 1 } else { 0 };
        if total_speaking > shown {
            let overflow = total_speaking - shown;
            let overflow_text = shaped_text(format!("+{}", overflow)).size(VOICE_BAR_FONT_SIZE);
            bar_row = bar_row.push(overflow_text);
        }
    }

    // Push spacer to right-align the deafen button
    bar_row = bar_row.push(Space::new().width(Fill));

    // Deafen toggle button
    let deafen_icon = if is_deafened {
        icon::volume_off().size(VOICE_BAR_ICON_SIZE)
    } else {
        icon::volume_up().size(VOICE_BAR_ICON_SIZE)
    };

    let deafen_tooltip_text = if is_deafened {
        t("voice-unmute-all-tooltip")
    } else {
        t("voice-mute-all-tooltip")
    };

    let deafen_btn = button(deafen_icon)
        .on_press(Message::VoiceDeafenToggle)
        .padding(VOICE_BAR_BUTTON_PADDING)
        .style(voice_deafen_button_style);

    let deafen_button = tooltip(
        deafen_btn,
        container(shaped_text(deafen_tooltip_text).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Wrap in fixed-width container to prevent layout shifting when icon changes
    let deafen_container = container(deafen_button)
        .width(DEAFEN_BUTTON_WIDTH)
        .align_left(DEAFEN_BUTTON_WIDTH);

    bar_row = bar_row.push(deafen_container);

    container(bar_row)
        .width(Fill)
        .padding(VOICE_BAR_PADDING)
        .style(voice_bar_style)
        .into()
}

// =============================================================================
// VU Meter
// =============================================================================

/// Build a segmented VU meter showing microphone input level
///
/// Displays 8 segments with theme-appropriate colors:
/// - Green (0-60%): Normal speaking level
/// - Yellow (60-80%): Getting loud
/// - Red (80-100%): Too hot / clipping
///
/// # Arguments
/// * `level` - Audio level from 0.0 to 1.0
/// * `theme` - Current theme for colors
/// * `segment_width` - Width of each segment in pixels
/// * `segment_height` - Height of each segment in pixels
pub fn build_vu_meter(
    level: f32,
    theme: &Theme,
    segment_width: f32,
    segment_height: f32,
) -> Row<'static, Message> {
    let palette = theme.extended_palette();

    // Calculate how many segments should be lit
    let lit_segments = (level * VU_METER_SEGMENTS as f32).ceil() as usize;

    let mut meter_row = Row::new().spacing(VU_METER_SEGMENT_GAP);

    for i in 0..VU_METER_SEGMENTS {
        let segment_threshold = (i as f32 + 1.0) / VU_METER_SEGMENTS as f32;
        let is_lit = i < lit_segments;

        // Determine segment color based on its position
        let color = if segment_threshold <= VU_METER_YELLOW_THRESHOLD {
            palette.success.base.color
        } else if segment_threshold <= VU_METER_RED_THRESHOLD {
            palette.warning.base.color
        } else {
            palette.danger.base.color
        };

        // Dim color for unlit segments
        let segment_color = if is_lit {
            color
        } else {
            // Use a very dim version of the background
            iced::Color {
                a: 0.2,
                ..palette.background.strong.color
            }
        };

        let segment = container(Space::new())
            .width(segment_width)
            .height(segment_height)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(segment_color)),
                border: Border::default(),
                ..Default::default()
            });

        meter_row = meter_row.push(segment);
    }

    meter_row
}

// =============================================================================
// Voice Button
// =============================================================================

/// Build the voice join/leave button for the input row
///
/// - When not in voice: Shows mic icon, clicking joins voice for current tab
/// - When in voice: Shows mic icon, clicking leaves voice
pub fn build_voice_button<'a>(
    conn: &'a ServerConnection,
    has_voice_permission: bool,
    voice_target: Option<String>,
    font_size: f32,
) -> Element<'a, Message> {
    let is_in_voice = conn.voice_session.is_some();

    if is_in_voice {
        // In voice - click to leave
        let btn = button(icon::mic().size(font_size))
            .on_press(Message::VoiceLeavePressed)
            .padding(INPUT_PADDING);

        tooltip(
            btn,
            container(shaped_text(t("voice-leave-tooltip")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else if has_voice_permission && voice_target.is_some() {
        // Join voice button (enabled)
        let target = voice_target.unwrap_or_default();
        let btn = button(icon::mic().size(font_size))
            .on_press(Message::VoiceJoinPressed(target))
            .padding(INPUT_PADDING);

        tooltip(
            btn,
            container(shaped_text(t("voice-join-tooltip")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        // Join voice button (disabled - no permission or on console tab or no target)
        let btn: iced::widget::Button<'_, Message> =
            button(icon::mic().size(font_size)).padding(INPUT_PADDING);

        btn.into()
    }
}

// =============================================================================
// Voice Input Row
// =============================================================================

/// Build the input row with voice button
///
/// This extends the standard input row with a voice join/leave button.
pub fn build_input_row_with_voice<'a>(
    message_input: &'a str,
    font_size: f32,
    conn: &'a ServerConnection,
    has_voice_permission: bool,
    voice_target: Option<String>,
) -> Row<'a, Message> {
    let text_field = text_input(&t("placeholder-message"), message_input)
        .on_input(Message::ChatInputChanged)
        .on_submit(Message::SendMessagePressed)
        .id(Id::from(InputId::ChatInput))
        .padding(INPUT_PADDING)
        .size(font_size)
        .font(MONOSPACE_FONT)
        .width(Fill);

    let send_button = button(shaped_text(t("button-send")).size(font_size))
        .on_press(Message::SendMessagePressed)
        .padding(INPUT_PADDING);

    let voice_button = build_voice_button(conn, has_voice_permission, voice_target, font_size);

    row![voice_button, text_field, send_button]
        .spacing(SMALL_SPACING)
        .width(Fill)
}
