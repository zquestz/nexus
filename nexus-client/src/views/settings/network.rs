//! Network settings tab (proxy configuration)

use iced::widget::{Column, Id, Space, checkbox, row, text_input};
use iced::{Center, Element, Fill};
use iced_aw::NumberInput;

use crate::config::settings::ProxySettings;
use crate::i18n::t;
use crate::style::{ELEMENT_SPACING, INPUT_PADDING, SPACER_SIZE_MEDIUM, TEXT_SIZE, shaped_text};
use crate::types::{InputId, Message};

/// Build the Network tab content (proxy configuration)
pub(super) fn network_tab_content(proxy: &ProxySettings) -> Element<'_, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Proxy enabled checkbox
    let proxy_enabled_checkbox = checkbox(proxy.enabled)
        .label(t("label-use-socks5-proxy"))
        .on_toggle(Message::ProxyEnabledToggled)
        .text_size(TEXT_SIZE);
    items.push(proxy_enabled_checkbox.into());

    // Proxy address input (disabled when proxy is disabled)
    let proxy_address_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-address"), &proxy.address)
            .on_input(Message::ProxyAddressChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-proxy-address"), &proxy.address)
            .id(Id::from(InputId::ProxyAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };
    items.push(proxy_address_input.into());

    // Proxy port input with label (disabled when proxy is disabled)
    let proxy_port_label = shaped_text(t("label-proxy-port")).size(TEXT_SIZE);
    let proxy_port_input: Element<'_, Message> = if proxy.enabled {
        NumberInput::new(&proxy.port, 1..=65535, Message::ProxyPortChanged)
            .id(Id::from(InputId::ProxyPort))
            .padding(INPUT_PADDING)
            .into()
    } else {
        NumberInput::new(&proxy.port, 1..=65535, Message::ProxyPortChanged)
            .on_input_maybe(None::<fn(u16) -> Message>)
            .id(Id::from(InputId::ProxyPort))
            .padding(INPUT_PADDING)
            .into()
    };
    let proxy_port_row = row![proxy_port_label, proxy_port_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(proxy_port_row.into());

    // Proxy username input (optional, disabled when proxy is disabled)
    let proxy_username_value = proxy.username.as_deref().unwrap_or("");
    let proxy_username_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-username"), proxy_username_value)
            .on_input(Message::ProxyUsernameChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-proxy-username"), proxy_username_value)
            .id(Id::from(InputId::ProxyUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };
    items.push(proxy_username_input.into());

    // Proxy password input (optional, disabled when proxy is disabled)
    let proxy_password_value = proxy.password.as_deref().unwrap_or("");
    let proxy_password_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-password"), proxy_password_value)
            .on_input(Message::ProxyPasswordChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyPassword))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .secure(true)
    } else {
        text_input(&t("placeholder-proxy-password"), proxy_password_value)
            .id(Id::from(InputId::ProxyPassword))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .secure(true)
    };
    items.push(proxy_password_input.into());

    // Allow voice bypass (non-interactive when proxy is disabled). Voice uses
    // direct UDP, so enabling this exposes the user's real IP to the server.
    let allow_voice_bypass_checkbox = if proxy.enabled {
        checkbox(proxy.allow_voice_bypass)
            .label(t("label-allow-voice-bypass"))
            .on_toggle(Message::ProxyAllowVoiceBypassToggled)
            .text_size(TEXT_SIZE)
    } else {
        checkbox(proxy.allow_voice_bypass)
            .label(t("label-allow-voice-bypass"))
            .text_size(TEXT_SIZE)
    };
    items.push(allow_voice_bypass_checkbox.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}
