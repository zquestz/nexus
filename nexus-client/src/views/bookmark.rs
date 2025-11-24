//! Bookmark add/edit form

use crate::types::{BookmarkEditMode, InputId, Message};
use iced::widget::{button, checkbox, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

/// Displays form for adding or editing a server bookmark
pub fn bookmark_edit_view<'a>(
    bookmark_edit_mode: &'a BookmarkEditMode,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
    bookmark_auto_connect: bool,
    bookmark_error: &'a Option<String>,
) -> Element<'a, Message> {
    let dialog_title = match bookmark_edit_mode {
        BookmarkEditMode::Add => "Add Server",
        BookmarkEditMode::Edit(_) => "Edit Server",
        BookmarkEditMode::None => "",
    };

    let can_save = !bookmark_name.trim().is_empty()
        && !bookmark_address.trim().is_empty()
        && !bookmark_port.trim().is_empty();

    let mut column_items = vec![
        text(dialog_title).size(20).width(Fill).align_x(Center).into(),
        text("").size(15).into(),
    ];

    // Show error if present
    if let Some(error) = bookmark_error {
        column_items.push(
            text(error)
                .size(14)
                .style(|_theme| text::Style {
                    color: Some(iced::Color::from_rgb(1.0, 0.3, 0.3)),
                })
                .into(),
        );
        column_items.push(text("").size(10).into());
    }

    column_items.extend(vec![
        text_input("Server Name", bookmark_name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(if can_save {
                Message::SaveBookmark
            } else {
                Message::BookmarkNameChanged(String::new())
            })
            .id(text_input::Id::from(InputId::BookmarkName))
            .padding(8)
            .size(14)
            .into(),
        text_input("IPv6 Address", bookmark_address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(if can_save {
                Message::SaveBookmark
            } else {
                Message::BookmarkAddressChanged(String::new())
            })
            .id(text_input::Id::from(InputId::BookmarkAddress))
            .padding(8)
            .size(14)
            .into(),
        text_input("Port", bookmark_port)
            .on_input(Message::BookmarkPortChanged)
            .on_submit(if can_save {
                Message::SaveBookmark
            } else {
                Message::BookmarkPortChanged(String::new())
            })
            .id(text_input::Id::from(InputId::BookmarkPort))
            .padding(8)
            .size(14)
            .into(),
        text_input("Username (optional)", bookmark_username)
            .on_input(Message::BookmarkUsernameChanged)
            .on_submit(if can_save {
                Message::SaveBookmark
            } else {
                Message::BookmarkUsernameChanged(String::new())
            })
            .id(text_input::Id::from(InputId::BookmarkUsername))
            .padding(8)
            .size(14)
            .into(),
        text_input("Password (optional)", bookmark_password)
            .on_input(Message::BookmarkPasswordChanged)
            .on_submit(if can_save {
                Message::SaveBookmark
            } else {
                Message::BookmarkPasswordChanged(String::new())
            })
            .id(text_input::Id::from(InputId::BookmarkPassword))
            .secure(true)
            .padding(8)
            .size(14)
            .into(),
        text("").size(5).into(),
        checkbox("Auto-connect at startup", bookmark_auto_connect)
            .on_toggle(Message::BookmarkAutoConnectToggled)
            .size(14)
            .into(),
        text("").size(10).into(),
        row![
            if can_save {
                button(text("Save").size(14))
                    .on_press(Message::SaveBookmark)
                    .padding(10)
            } else {
                button(text("Save").size(14)).padding(10)
            },
            button(text("Cancel").size(14))
                .on_press(Message::CancelBookmarkEdit)
                .padding(10),
        ]
        .spacing(10)
        .into(),
    ]);

    let content = column(column_items)
        .spacing(10)
        .padding(20)
        .max_width(400);

    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}