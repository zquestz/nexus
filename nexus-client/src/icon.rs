// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../fonts/icons.toml
// ebb64483600623f5595f8cab843e1fa36af1dd7d89a1e27519ddd9d58c1a77a8
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

pub fn down_dir<'a>() -> Text<'a> {
    icon("\u{25BE}")
}

pub fn download<'a>() -> Text<'a> {
    icon("\u{1F4E5}")
}

pub fn edit<'a>() -> Text<'a> {
    icon("\u{270E}")
}

pub fn exchange<'a>() -> Text<'a> {
    icon("\u{F0EC}")
}

pub fn expand_right<'a>() -> Text<'a> {
    icon("\u{F152}")
}

pub fn eye<'a>() -> Text<'a> {
    icon("\u{E70A}")
}

pub fn eye_off<'a>() -> Text<'a> {
    icon("\u{E70B}")
}

pub fn file<'a>() -> Text<'a> {
    icon("\u{1F4C4}")
}

pub fn file_archive<'a>() -> Text<'a> {
    icon("\u{F1C6}")
}

pub fn file_audio<'a>() -> Text<'a> {
    icon("\u{F1C7}")
}

pub fn file_code<'a>() -> Text<'a> {
    icon("\u{F1C9}")
}

pub fn file_excel<'a>() -> Text<'a> {
    icon("\u{F1C3}")
}

pub fn file_image<'a>() -> Text<'a> {
    icon("\u{F1C5}")
}

pub fn file_pdf<'a>() -> Text<'a> {
    icon("\u{F1C1}")
}

pub fn file_powerpoint<'a>() -> Text<'a> {
    icon("\u{F1C4}")
}

pub fn file_text<'a>() -> Text<'a> {
    icon("\u{F0F6}")
}

pub fn file_video<'a>() -> Text<'a> {
    icon("\u{F1C8}")
}

pub fn file_word<'a>() -> Text<'a> {
    icon("\u{F1C2}")
}

pub fn folder<'a>() -> Text<'a> {
    icon("\u{1F4C1}")
}

pub fn folder_empty<'a>() -> Text<'a> {
    icon("\u{F114}")
}

pub fn folder_root<'a>() -> Text<'a> {
    icon("\u{F0E8}")
}

pub fn home<'a>() -> Text<'a> {
    icon("\u{2302}")
}

pub fn info<'a>() -> Text<'a> {
    icon("\u{F129}")
}

pub fn info_circled<'a>() -> Text<'a> {
    icon("\u{E705}")
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

pub fn newspaper<'a>() -> Text<'a> {
    icon("\u{F1EA}")
}

pub fn paste<'a>() -> Text<'a> {
    icon("\u{F0EA}")
}

pub fn plus<'a>() -> Text<'a> {
    icon("\u{2B}")
}

pub fn refresh<'a>() -> Text<'a> {
    icon("\u{E760}")
}

pub fn server<'a>() -> Text<'a> {
    icon("\u{F233}")
}

pub fn trash<'a>() -> Text<'a> {
    icon("\u{E729}")
}

pub fn up_dir<'a>() -> Text<'a> {
    icon("\u{25B4}")
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
