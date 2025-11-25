// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icons.toml
// 651a7e86b55f5ea1e980fadef16eef55ae00d681460445b90ba81385c20f38b9
use iced::widget::{text, Text};
use iced::Font;

pub const FONT: &[u8] = include_bytes!("../fonts/icons.ttf");

pub fn chat<'a>() -> Text<'a> {
    icon("\u{E720}")
}

pub fn cog<'a>() -> Text<'a> {
    icon("\u{2699}")
}

pub fn collapse_left<'a>() -> Text<'a> {
    icon("\u{F191}")
}

pub fn expand_right<'a>() -> Text<'a> {
    icon("\u{F152}")
}

pub fn logout<'a>() -> Text<'a> {
    icon("\u{E741}")
}

pub fn megaphone<'a>() -> Text<'a> {
    icon("\u{1F4E3}")
}

pub fn sun<'a>() -> Text<'a> {
    icon("\u{F185}")
}

pub fn user_plus<'a>() -> Text<'a> {
    icon("\u{F234}")
}

pub fn users<'a>() -> Text<'a> {
    icon("\u{1F465}")
}

fn icon(codepoint: &str) -> Text<'_> {
    text(codepoint).font(Font::with_name("icons"))
}
