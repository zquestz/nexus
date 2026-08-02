//! Events settings tab (notifications, toasts, sounds per event type)

use iced::Element;
use iced::Fill;
use iced::widget::button as btn;
use iced::widget::{Column, Space, button, checkbox, pick_list, row, slider};

use crate::config::events::{EventSettings, EventType, NotificationContent, SoundChoice};
use crate::config::settings::{SOUND_VOLUME_MAX, SOUND_VOLUME_MIN, SOUND_VOLUME_STEP};
use crate::i18n::t;
use crate::style::{
    ELEMENT_SPACING, INPUT_PADDING, SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TEXT_SIZE, shaped_text,
};
use crate::types::Message;

/// Build the Events tab content
pub(super) fn events_tab_content<'a>(
    event_settings: &'a EventSettings,
    selected_event_type: EventType,
    notifications_enabled: bool,
    sound_enabled: bool,
    sound_volume: f32,
) -> Element<'a, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Global toggles for notifications and sound
    let notifications_checkbox = checkbox(notifications_enabled)
        .label(t("settings-notifications-enabled"))
        .on_toggle(Message::ToggleNotificationsEnabled)
        .text_size(TEXT_SIZE)
        .spacing(ELEMENT_SPACING);

    let sound_checkbox = checkbox(sound_enabled)
        .label(t("settings-sound-enabled"))
        .on_toggle(Message::ToggleSoundEnabled)
        .text_size(TEXT_SIZE)
        .spacing(ELEMENT_SPACING);

    let global_toggles_row = row![
        notifications_checkbox,
        Space::new().width(SPACER_SIZE_LARGE),
        sound_checkbox,
    ]
    .align_y(iced::Alignment::Center);
    items.push(global_toggles_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Volume slider with label and percentage
    let volume_percent = (sound_volume * 100.0).round() as u8;
    let volume_label = shaped_text(t("settings-sound-volume")).size(TEXT_SIZE);
    let volume_value = shaped_text(format!("{}%", volume_percent)).size(TEXT_SIZE);

    // Create the slider (always interactive - disable is handled at handler level)
    let volume_slider = slider(
        SOUND_VOLUME_MIN..=SOUND_VOLUME_MAX,
        sound_volume,
        Message::SoundVolumeChanged,
    )
    .step(SOUND_VOLUME_STEP);

    let volume_row = row![
        volume_label,
        Space::new().width(ELEMENT_SPACING),
        volume_slider,
        Space::new().width(ELEMENT_SPACING),
        volume_value,
    ]
    .spacing(ELEMENT_SPACING)
    .align_y(iced::Alignment::Center);

    items.push(volume_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Event type picker with label on same row
    let event_label = shaped_text(t("event-settings-event")).size(TEXT_SIZE);
    let event_types: Vec<EventType> = EventType::all().to_vec();
    let event_picker = pick_list(
        event_types,
        Some(selected_event_type),
        Message::EventTypeSelected,
    )
    .text_size(TEXT_SIZE);

    let event_row = row![
        event_label,
        Space::new().width(ELEMENT_SPACING),
        event_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(event_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Get config for selected event
    let event_config = event_settings.get(selected_event_type);

    // Show notification checkbox - disabled when global notifications are off
    let show_notification_checkbox = if notifications_enabled {
        checkbox(event_config.show_notification)
            .label(t("event-settings-show-notification"))
            .on_toggle(Message::EventShowNotificationToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.show_notification)
            .label(t("event-settings-show-notification"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };
    items.push(show_notification_checkbox.into());

    // Content level picker and test button on same row
    let content_levels: Vec<NotificationContent> = NotificationContent::all().to_vec();
    let content_picker = pick_list(
        content_levels,
        Some(event_config.notification_content),
        Message::EventNotificationContentSelected,
    )
    .text_size(TEXT_SIZE);

    let test_notification_button = if notifications_enabled && event_config.show_notification {
        button(shaped_text(t("settings-notification-test")).size(TEXT_SIZE))
            .on_press(Message::TestNotification)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("settings-notification-test")).size(TEXT_SIZE))
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let notification_row = row![
        content_picker,
        Space::new().width(ELEMENT_SPACING),
        test_notification_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(notification_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Show toast checkbox (always available - no global toggle)
    let show_toast_checkbox = checkbox(event_config.show_toast)
        .label(t("event-settings-show-toast"))
        .on_toggle(Message::EventShowToastToggled)
        .text_size(TEXT_SIZE)
        .spacing(ELEMENT_SPACING);
    items.push(show_toast_checkbox.into());

    // Toast content level picker and test button on same row
    let toast_content_levels: Vec<NotificationContent> = NotificationContent::all().to_vec();
    let toast_content_picker = pick_list(
        toast_content_levels,
        Some(event_config.toast_content),
        Message::EventToastContentSelected,
    )
    .text_size(TEXT_SIZE);

    let test_toast_button = if event_config.show_toast {
        button(shaped_text(t("settings-toast-test")).size(TEXT_SIZE))
            .on_press(Message::TestToast)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("settings-toast-test")).size(TEXT_SIZE))
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let toast_row = row![
        toast_content_picker,
        Space::new().width(ELEMENT_SPACING),
        test_toast_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(toast_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Play sound checkbox with Always Play inline
    let always_play_enabled = sound_enabled && event_config.play_sound;

    let play_sound_checkbox = if sound_enabled {
        checkbox(event_config.play_sound)
            .label(t("settings-sound-play"))
            .on_toggle(Message::EventPlaySoundToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.play_sound)
            .label(t("settings-sound-play"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };

    let always_play_checkbox = if always_play_enabled {
        checkbox(event_config.always_play_sound)
            .label(t("settings-sound-always-play"))
            .on_toggle(Message::EventAlwaysPlaySoundToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.always_play_sound)
            .label(t("settings-sound-always-play"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };

    let sound_checkboxes_row = row![
        play_sound_checkbox,
        Space::new().width(SPACER_SIZE_LARGE),
        always_play_checkbox,
    ]
    .align_y(iced::Alignment::Center);
    items.push(sound_checkboxes_row.into());

    // Sound picker and test button on same row
    let sound_choices: Vec<SoundChoice> = SoundChoice::all().to_vec();
    let sound_picker = pick_list(
        sound_choices,
        Some(event_config.sound.clone()),
        Message::EventSoundSelected,
    )
    .text_size(TEXT_SIZE);

    let test_sound_button = if sound_enabled && event_config.play_sound {
        button(shaped_text(t("settings-sound-test")).size(TEXT_SIZE))
            .on_press(Message::TestSound)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("settings-sound-test")).size(TEXT_SIZE))
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let sound_row = row![
        sound_picker,
        Space::new().width(ELEMENT_SPACING),
        test_sound_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(sound_row.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}
