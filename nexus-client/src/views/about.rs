//! About panel view

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, SPACER_SIZE_MEDIUM, TEXT_SIZE,
    TITLE_SIZE, content_background_style, shaped_text,
};
use crate::types::Message;
use iced::widget::{Space, button, column, container, row, svg};
use iced::{Center, Element, Fill};

/// App icon SVG bytes (embedded at compile time)
const APP_ICON_SVG: &[u8] = include_bytes!("../../assets/linux/nexus.svg");

/// App icon size in pixels
const APP_ICON_SIZE: u16 = 128;

/// Render the about panel
///
/// Displays app icon, name, version, and copyright.
pub fn about_view() -> Element<'static, Message> {
    // App icon (SVG)
    let app_icon = svg(svg::Handle::from_memory(APP_ICON_SVG))
        .width(APP_ICON_SIZE)
        .height(APP_ICON_SIZE);
    let icon_row = row![Space::with_width(Fill), app_icon, Space::with_width(Fill)];

    // App name
    let app_name = shaped_text(t("about-app-name"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    // Version
    let version = shaped_text(format!("v{}", env!("CARGO_PKG_VERSION")))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    // Copyright
    let copyright = shaped_text(t("about-copyright"))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    // Close button (primary style since it's the default action)
    let buttons = row![
        Space::with_width(Fill),
        button(shaped_text(t("button-close")).size(TEXT_SIZE))
            .on_press(Message::CloseAbout)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    let content = column![
        icon_row,
        app_name,
        version,
        Space::with_height(SPACER_SIZE_MEDIUM),
        copyright,
        Space::with_height(SPACER_SIZE_MEDIUM),
        buttons,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}
