// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icons.toml
// 04a7bfe860d59b634f46091931706aede1cf49cd9f8f7184da242a33499ff090
use iced::Font;
use iced::widget::{Text, text};

pub const FONT: &[u8] = include_bytes!("../fonts/icons.ttf");

pub fn bookmark<'a>() -> Text<'a> {
    icon("\u{1F516}")
}

pub fn chat<'a>() -> Text<'a> {
    icon("\u{E720}")
}

pub fn close<'a>() -> Text<'a> {
    icon("\u{2715}")
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

pub fn info<'a>() -> Text<'a> {
    icon("\u{F129}")
}

pub fn kick<'a>() -> Text<'a> {
    icon("\u{E741}")
}

pub fn logout<'a>() -> Text<'a> {
    icon("\u{E741}")
}

pub fn megaphone<'a>() -> Text<'a> {
    icon("\u{1F4E3}")
}

pub fn message<'a>() -> Text<'a> {
    icon("\u{E720}")
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
