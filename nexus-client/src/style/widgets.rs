//! Widget style functions
//!
//! Provides consistent styling for Iced widgets across the application.
//! All styles use the primary blue accent color for visual consistency.

use super::colors::{
    action_button_text, chat_text_color, disabled_action_background, disabled_action_text,
    interactive_hover_color, primary_action_background, primary_action_background_hovered,
    primary_action_background_pressed, theme_color, tooltip_text_color,
};
use super::layout::{BORDER_WIDTH, BORDER_WIDTH_FOCUSED, WIDGET_BORDER_RADIUS};
use super::palette;
use iced::widget::{button, checkbox, container, scrollable, text_input};
use iced::{Background, Border, Theme};

// ============================================================================
// Button Styles
// ============================================================================

/// Primary button style (blue background, white text)
pub fn primary_button_style() -> fn(&Theme, button::Status) -> button::Style {
    |_theme, status| match status {
        button::Status::Active => button::Style {
            background: Some(Background::from(primary_action_background())),
            text_color: action_button_text(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::from(primary_action_background_hovered())),
            text_color: action_button_text(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::from(primary_action_background_pressed())),
            text_color: action_button_text(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::from(disabled_action_background())),
            text_color: disabled_action_text(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
    }
}

/// Active chat tab style (blue background)
pub fn chat_tab_active_style() -> fn(&Theme, button::Status) -> button::Style {
    |_theme, _status| button::Style {
        background: Some(Background::from(primary_action_background())),
        text_color: action_button_text(),
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// Inactive chat tab style (transparent with hover)
pub fn chat_tab_inactive_style() -> fn(&Theme, button::Status) -> button::Style {
    |theme, status| match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::from(interactive_hover_color())),
            text_color: action_button_text(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
        _ => button::Style {
            background: None,
            text_color: chat_text_color(theme),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        },
    }
}

// ============================================================================
// Checkbox Style
// ============================================================================

/// Primary checkbox style (blue accent when checked)
pub fn primary_checkbox_style() -> fn(&Theme, checkbox::Status) -> checkbox::Style {
    |theme, status| {
        let unchecked_bg = theme_color(theme, palette::PEWTER, palette::DIM_GRAY);
        let unchecked_border = theme_color(theme, palette::LIGHT_SLATE, palette::SLATE);

        match status {
            checkbox::Status::Active { is_checked } => checkbox::Style {
                background: if is_checked {
                    Background::from(primary_action_background())
                } else {
                    Background::from(unchecked_bg)
                },
                icon_color: action_button_text(),
                border: Border {
                    color: if is_checked {
                        primary_action_background()
                    } else {
                        unchecked_border
                    },
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                text_color: None,
            },
            checkbox::Status::Hovered { is_checked } => checkbox::Style {
                background: if is_checked {
                    Background::from(primary_action_background())
                } else {
                    Background::from(theme_color(theme, palette::SMOKE, palette::DARK_SLATE))
                },
                icon_color: action_button_text(),
                border: Border {
                    color: primary_action_background(),
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                text_color: None,
            },
            checkbox::Status::Disabled { is_checked } => checkbox::Style {
                background: if is_checked {
                    Background::from(disabled_action_background())
                } else {
                    Background::from(unchecked_bg)
                },
                icon_color: theme_color(theme, palette::GAINSBORO, palette::GRANITE),
                border: Border {
                    color: theme_color(theme, palette::SILVER, palette::DARK_SLATE),
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                text_color: Some(theme_color(theme, palette::LIGHT_SLATE, palette::GRANITE)),
            },
        }
    }
}

// ============================================================================
// Text Input Style
// ============================================================================

/// Primary text input style (blue border when focused)
pub fn primary_text_input_style() -> fn(&Theme, text_input::Status) -> text_input::Style {
    |theme, status| {
        let bg = theme_color(theme, palette::WHITE, palette::CHARCOAL);
        let icon = palette::SLATE;
        let placeholder = theme_color(theme, palette::SILVER, palette::SLATE);
        let value = theme_color(theme, palette::BLACK, palette::WHITE);

        match status {
            text_input::Status::Active => text_input::Style {
                background: Background::from(bg),
                border: Border {
                    color: theme_color(theme, palette::LIGHT_SLATE, palette::GRANITE),
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                icon,
                placeholder,
                value,
                selection: primary_action_background(),
            },
            text_input::Status::Focused => text_input::Style {
                background: Background::from(bg),
                border: Border {
                    color: primary_action_background(),
                    width: BORDER_WIDTH_FOCUSED,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                icon,
                placeholder,
                value,
                selection: primary_action_background(),
            },
            text_input::Status::Hovered => text_input::Style {
                background: Background::from(bg),
                border: Border {
                    color: primary_action_background(),
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                icon,
                placeholder,
                value,
                selection: primary_action_background(),
            },
            text_input::Status::Disabled => text_input::Style {
                background: Background::from(theme_color(theme, palette::SMOKE, palette::GUNMETAL)),
                border: Border {
                    color: theme_color(theme, palette::GAINSBORO, palette::DARK_SLATE),
                    width: BORDER_WIDTH,
                    radius: WIDGET_BORDER_RADIUS.into(),
                },
                icon: theme_color(theme, palette::SILVER, palette::DARK_SLATE),
                placeholder: theme_color(theme, palette::GAINSBORO, palette::GRANITE),
                value: theme_color(theme, palette::LIGHT_SLATE, palette::SLATE),
                selection: theme_color(theme, palette::SILVER, palette::GRANITE),
            },
        }
    }
}

// ============================================================================
// Scrollbar Style
// ============================================================================

/// Primary scrollbar style (gray with blue hover)
pub fn primary_scrollbar_style() -> fn(&Theme, scrollable::Status) -> scrollable::Style {
    |theme, status| {
        let default_color = theme_color(theme, palette::GAINSBORO, palette::JET);
        let scroller_color = match status {
            scrollable::Status::Active => default_color,
            scrollable::Status::Hovered {
                is_horizontal_scrollbar_hovered,
                is_vertical_scrollbar_hovered,
            } => {
                if is_horizontal_scrollbar_hovered || is_vertical_scrollbar_hovered {
                    interactive_hover_color()
                } else {
                    default_color
                }
            }
            scrollable::Status::Dragged {
                is_horizontal_scrollbar_dragged,
                is_vertical_scrollbar_dragged,
            } => {
                if is_horizontal_scrollbar_dragged || is_vertical_scrollbar_dragged {
                    interactive_hover_color()
                } else {
                    default_color
                }
            }
        };

        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: scrollable::Rail {
                background: None,
                border: Border::default(),
                scroller: scrollable::Scroller {
                    color: scroller_color,
                    border: Border {
                        radius: WIDGET_BORDER_RADIUS.into(),
                        ..Default::default()
                    },
                },
            },
            horizontal_rail: scrollable::Rail {
                background: None,
                border: Border::default(),
                scroller: scrollable::Scroller {
                    color: scroller_color,
                    border: Border {
                        radius: WIDGET_BORDER_RADIUS.into(),
                        ..Default::default()
                    },
                },
            },
            gap: None,
        }
    }
}

// ============================================================================
// Container Styles
// ============================================================================

/// Modal overlay style (semi-transparent dark background)
pub fn modal_overlay_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BLACK_80)),
        ..Default::default()
    }
}

// ============================================================================
// Tooltip Style
// ============================================================================

/// Tooltip container style
pub fn tooltip_container_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BLACK_80)),
        text_color: Some(tooltip_text_color(theme)),
        border: Border {
            radius: WIDGET_BORDER_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
