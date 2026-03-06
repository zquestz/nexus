//! Audio settings tab (devices, voice quality, PTT, processing)

use iced::Element;
use iced::Fill;
use iced::widget::button as btn;
use iced::widget::{Column, Space, button, checkbox, pick_list, row};

use super::AudioTabData;
use crate::config::audio::{
    LocalizedVoiceQuality, MicBoost, NoiseSuppressionLevel, PttMode, PttReleaseDelay,
};
use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, INPUT_PADDING, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TEXT_SIZE, error_text_style, shaped_text, shaped_text_wrapped,
};
use crate::types::Message;
use crate::views::voice::build_vu_meter;
use crate::voice::ptt::{hotkey_to_string, parse_hotkey};

/// Build the Audio tab content
pub(super) fn audio_tab_content(data: AudioTabData<'_>) -> Element<'_, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Output device picker
    let output_label = shaped_text(t("audio-output-device")).size(TEXT_SIZE);
    let output_picker = pick_list(
        data.output_devices,
        Some(data.selected_output_device),
        Message::AudioOutputDeviceSelected,
    )
    .text_size(TEXT_SIZE);

    let output_row = row![
        output_label,
        Space::new().width(ELEMENT_SPACING),
        output_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(output_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Input device picker
    let input_label = shaped_text(t("audio-input-device")).size(TEXT_SIZE);
    let input_picker = pick_list(
        data.input_devices,
        Some(data.selected_input_device),
        Message::AudioInputDeviceSelected,
    )
    .text_size(TEXT_SIZE);

    let input_row = row![
        input_label,
        Space::new().width(ELEMENT_SPACING),
        input_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(input_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Refresh devices button
    let refresh_button = button(shaped_text(t("audio-refresh-devices")).size(TEXT_SIZE))
        .on_press(Message::AudioRefreshDevices)
        .padding(BUTTON_PADDING);
    items.push(refresh_button.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Voice quality picker
    let quality_label = shaped_text(t("audio-voice-quality")).size(TEXT_SIZE);
    let quality_options = LocalizedVoiceQuality::all();
    let quality_picker = pick_list(
        quality_options,
        Some(LocalizedVoiceQuality::from(data.voice_quality)),
        |lq| Message::AudioQualitySelected(lq.into()),
    )
    .text_size(TEXT_SIZE);

    let quality_row = row![
        quality_label,
        Space::new().width(ELEMENT_SPACING),
        quality_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(quality_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // PTT key capture
    let ptt_key_label = shaped_text(t("audio-ptt-key")).size(TEXT_SIZE);
    let ptt_key_display = if data.ptt_capturing {
        t("audio-ptt-key-hint")
    } else {
        // Parse and re-format for platform-aware display (e.g., "Cmd" on macOS, "Super" on Linux)
        match parse_hotkey(data.ptt_key) {
            Ok((modifiers, code)) => hotkey_to_string(modifiers, code),
            Err(_) => data.ptt_key.to_string(), // Fallback to raw string if parse fails
        }
    };
    let ptt_key_button = button(shaped_text(ptt_key_display).size(TEXT_SIZE))
        .on_press(Message::AudioPttKeyCapture)
        .padding(INPUT_PADDING)
        .style(btn::secondary);

    let ptt_key_row = row![
        ptt_key_label,
        Space::new().width(ELEMENT_SPACING),
        ptt_key_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(ptt_key_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // PTT mode picker
    let ptt_mode_label = shaped_text(t("audio-ptt-mode")).size(TEXT_SIZE);
    let ptt_modes: Vec<PttMode> = PttMode::ALL.to_vec();
    let ptt_mode_picker = pick_list(
        ptt_modes,
        Some(data.ptt_mode),
        Message::AudioPttModeSelected,
    )
    .text_size(TEXT_SIZE);

    let ptt_mode_row = row![
        ptt_mode_label,
        Space::new().width(ELEMENT_SPACING),
        ptt_mode_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(ptt_mode_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // PTT release delay picker
    let ptt_delay_label = shaped_text(t("audio-ptt-release-delay")).size(TEXT_SIZE);
    let ptt_delays: Vec<PttReleaseDelay> = PttReleaseDelay::ALL.to_vec();
    let ptt_delay_picker = pick_list(
        ptt_delays,
        Some(data.ptt_release_delay),
        Message::AudioPttReleaseDelaySelected,
    )
    .text_size(TEXT_SIZE);

    let ptt_delay_row = row![
        ptt_delay_label,
        Space::new().width(ELEMENT_SPACING),
        ptt_delay_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(ptt_delay_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Microphone test section
    let mic_test_label = shaped_text(t("audio-input-level")).size(TEXT_SIZE);

    // VU meter for mic level (larger size for settings)
    let mic_meter = build_vu_meter(data.settings_mic_level, &data.theme, 8.0, 16.0);

    let mic_test_button = if data.mic_testing {
        button(shaped_text(t("audio-stop-test")).size(TEXT_SIZE))
            .on_press(Message::AudioTestMicStop)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("audio-test-mic")).size(TEXT_SIZE))
            .on_press(Message::AudioTestMicStart)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let mic_test_row = row![
        mic_test_label,
        Space::new().width(ELEMENT_SPACING),
        mic_meter,
        Space::new().width(ELEMENT_SPACING),
        mic_test_button,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(ELEMENT_SPACING);
    items.push(mic_test_row.into());

    // Show mic error if present
    if let Some(error) = data.mic_error {
        items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .style(error_text_style)
                .into(),
        );
    }

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Microphone boost picker
    let mic_boost_label = shaped_text(t("audio-mic-boost")).size(TEXT_SIZE);
    let mic_boosts: Vec<MicBoost> = MicBoost::ALL.to_vec();
    let mic_boost_picker =
        pick_list(mic_boosts, Some(data.mic_boost), Message::AudioMicBoost).text_size(TEXT_SIZE);

    let mic_boost_row = row![
        mic_boost_label,
        Space::new().width(ELEMENT_SPACING),
        mic_boost_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(mic_boost_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Noise suppression picker (Off / Low / Moderate / High / Very High)
    let ns_label = shaped_text(t("audio-noise-suppression")).size(TEXT_SIZE);
    let ns_levels: Vec<NoiseSuppressionLevel> = NoiseSuppressionLevel::ALL.to_vec();
    let ns_picker = pick_list(
        ns_levels,
        Some(data.noise_suppression_level),
        Message::AudioNoiseSuppressionLevel,
    )
    .text_size(TEXT_SIZE);

    let ns_row = row![ns_label, Space::new().width(ELEMENT_SPACING), ns_picker,]
        .align_y(iced::Alignment::Center);
    items.push(ns_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Echo cancellation toggle
    let echo_cancellation_checkbox = checkbox(data.echo_cancellation)
        .label(t("audio-echo-cancellation"))
        .on_toggle(Message::AudioEchoCancellation)
        .text_size(TEXT_SIZE);
    items.push(echo_cancellation_checkbox.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Automatic gain control toggle
    let agc_checkbox = checkbox(data.agc)
        .label(t("audio-agc"))
        .on_toggle(Message::AudioAgc)
        .text_size(TEXT_SIZE);
    items.push(agc_checkbox.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Transient suppression toggle (keyboard/click noise reduction)
    let transient_suppression_checkbox = checkbox(data.transient_suppression)
        .label(t("audio-transient-suppression"))
        .on_toggle(Message::AudioTransientSuppression)
        .text_size(TEXT_SIZE);
    items.push(transient_suppression_checkbox.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}
