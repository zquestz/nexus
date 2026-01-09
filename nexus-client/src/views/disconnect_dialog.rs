//! Disconnect user dialog view (kick or ban)
//!
//! Modal dialog shown when clicking the disconnect icon on a user in the user list.
//! Shows different options based on permissions:
//! - user_kick only: Just kick option
//! - ban_create only: Just ban option
//! - Both: Radio buttons to choose kick or ban

use iced::widget::{Column, Space, button, column, pick_list, radio, row, text_input};
use iced::{Center, Element, Fill};

use super::constants::{PERMISSION_BAN_CREATE, PERMISSION_USER_KICK};
use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, error_text_style, muted_text_style,
    panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{
    BanDuration, DisconnectAction, DisconnectDialogState, Message, ServerConnection,
};

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the disconnect user dialog
///
/// Shows kick and/or ban options based on user permissions.
pub fn disconnect_dialog_view<'a>(
    conn: &'a ServerConnection,
    state: &'a DisconnectDialogState,
) -> Element<'a, Message> {
    let has_kick = conn.has_permission(PERMISSION_USER_KICK);
    let has_ban = conn.has_permission(PERMISSION_BAN_CREATE);

    // Title with nickname
    let title = panel_title(t_args(
        "title-disconnect-user-name",
        &[("nickname", &state.nickname)],
    ));

    let mut form_items: Vec<Element<'_, Message>> =
        vec![title.into(), Space::new().height(SPACER_SIZE_MEDIUM).into()];

    // Action selection (only show if user has both permissions)
    if has_kick && has_ban {
        let kick_radio = radio(
            t("disconnect-action-kick"),
            DisconnectAction::Kick,
            Some(state.action),
            Message::DisconnectDialogActionChanged,
        )
        .size(TEXT_SIZE)
        .text_size(TEXT_SIZE);

        let ban_radio = radio(
            t("disconnect-action-ban"),
            DisconnectAction::Ban,
            Some(state.action),
            Message::DisconnectDialogActionChanged,
        )
        .size(TEXT_SIZE)
        .text_size(TEXT_SIZE);

        let action_row = row![
            kick_radio,
            Space::new().width(SPACER_SIZE_MEDIUM),
            ban_radio
        ]
        .align_y(Center);

        form_items.push(action_row.into());
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    // Duration picker - only show for ban
    let show_duration = state.action == DisconnectAction::Ban || (!has_kick && has_ban);

    if show_duration {
        let duration_label = shaped_text(t("disconnect-duration-label"))
            .size(TEXT_SIZE)
            .style(muted_text_style);

        let duration_options: Vec<BanDuration> = BanDuration::all().to_vec();
        let duration_picker = pick_list(
            duration_options,
            Some(state.duration),
            Message::DisconnectDialogDurationChanged,
        )
        .text_size(TEXT_SIZE)
        .padding(INPUT_PADDING)
        .width(Fill);

        let duration_row = column![duration_label, duration_picker].spacing(SPACER_SIZE_SMALL);

        form_items.push(duration_row.into());
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    }

    // Reason input - always show (works for both kick and ban)
    let reason_label = shaped_text(t("disconnect-reason-label"))
        .size(TEXT_SIZE)
        .style(muted_text_style);

    let reason_input = text_input(&t("disconnect-reason-placeholder"), &state.reason)
        .on_input(Message::DisconnectDialogReasonChanged)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let reason_row = column![reason_label, reason_input].spacing(SPACER_SIZE_SMALL);

    form_items.push(reason_row.into());
    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Error message (if any)
    if let Some(ref error) = state.error {
        form_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    }

    // Buttons
    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::DisconnectDialogCancel)
        .padding(BUTTON_PADDING)
        .style(button::secondary);

    // Submit button text depends on action
    let submit_text = if show_duration {
        t("button-ban")
    } else {
        t("button-kick")
    };

    let submit_button = button(shaped_text(submit_text).size(TEXT_SIZE))
        .on_press(Message::DisconnectDialogSubmit)
        .padding(BUTTON_PADDING)
        .style(button::danger);

    let button_row =
        row![Space::new().width(Fill), cancel_button, submit_button].spacing(ELEMENT_SPACING);

    form_items.push(button_row.into());

    let form = Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Helper Functions
// ============================================================================

fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

// ============================================================================
// Display Implementation for BanDuration
// ============================================================================

impl std::fmt::Display for BanDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", t(self.translation_key()))
    }
}
