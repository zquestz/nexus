//! Menu button widget with tree-stored hover state
//!
//! Unlike iced's `Button`, this widget stores its hover/pressed state in the
//! widget tree rather than on the struct. This makes it work correctly in
//! overlays like context menus, where the content is rebuilt each frame.

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Background, Border, Color, Element, Event, Length, Padding, Rectangle, Size};

/// State stored in the widget tree
#[derive(Debug, Clone, Copy, Default)]
struct State {
    is_hovered: bool,
    is_pressed: bool,
}

/// The status of a [`MenuButton`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Normal state
    Active,
    /// Cursor is over the button
    Hovered,
    /// Button is being pressed
    Pressed,
}

/// The style of a [`MenuButton`]
#[derive(Debug, Clone, Copy, Default)]
pub struct Style {
    /// Background color
    pub background: Option<Background>,
    /// Text color
    pub text_color: Color,
    /// Border
    pub border: Border,
}

/// Style function type
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

/// A button designed for use in context menus and overlays.
///
/// Unlike iced's standard `Button`, this widget stores its hover/pressed state
/// in the widget tree, making it work correctly when the parent widget is
/// rebuilt each frame (as happens in overlay content).
pub struct MenuButton<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: renderer::Renderer,
{
    content: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    width: Length,
    height: Length,
    padding: Padding,
    style: StyleFn<'a, Theme>,
}

impl<'a, Message, Theme, Renderer> MenuButton<'a, Message, Theme, Renderer>
where
    Theme: 'a,
    Renderer: renderer::Renderer,
{
    /// Creates a new [`MenuButton`] with the given content.
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_press: None,
            width: Length::Shrink,
            height: Length::Shrink,
            padding: Padding::new(0.0),
            style: Box::new(|_theme, _status| Style::default()),
        }
    }

    /// Sets the message to emit when the button is pressed.
    #[must_use]
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    /// Sets the message to emit when the button is pressed, if Some.
    #[must_use]
    pub fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.on_press = message;
        self
    }

    /// Sets the width of the button.
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the button.
    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the padding of the button.
    #[must_use]
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the style of the button.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme: 'a,
    {
        self.style = Box::new(style);
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for MenuButton<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::padded(limits, self.width, self.height, self.padding, |limits| {
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits)
        })
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Some(content_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            content_layout,
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Forward event to content first
        if let Some(content_layout) = layout.children().next() {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                content_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }

        if shell.is_event_captured() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let is_over = cursor.is_over(bounds);

        // Update hover state
        let was_hovered = state.is_hovered;
        state.is_hovered = is_over;

        // Request redraw if hover state changed
        if was_hovered != is_over {
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(iced::touch::Event::FingerPressed { .. })
                if is_over && self.on_press.is_some() =>
            {
                state.is_pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(iced::touch::Event::FingerLifted { .. })
                if state.is_pressed =>
            {
                state.is_pressed = false;
                shell.capture_event();
                if is_over && let Some(on_press) = self.on_press.clone() {
                    shell.publish(on_press);
                }
            }
            Event::Touch(iced::touch::Event::FingerLost { .. }) => {
                state.is_pressed = false;
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        // Determine status from tree state
        let status = if state.is_pressed {
            Status::Pressed
        } else if state.is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };

        let styling = (self.style)(theme, status);

        // Draw background if present
        if styling.background.is_some() || styling.border.width > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: styling.border,
                    ..renderer::Quad::default()
                },
                styling
                    .background
                    .unwrap_or(Background::Color(Color::TRANSPARENT)),
            );
        }

        // Draw content
        if let Some(content_layout) = layout.children().next() {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                &renderer::Style {
                    text_color: styling.text_color,
                },
                content_layout,
                cursor,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if let Some(content_layout) = layout.children().next() {
            let content_interaction = self.content.as_widget().mouse_interaction(
                &tree.children[0],
                content_layout,
                cursor,
                viewport,
                renderer,
            );

            if content_interaction != mouse::Interaction::None {
                return content_interaction;
            }
        }

        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let content_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            content_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<MenuButton<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(button: MenuButton<'a, Message, Theme, Renderer>) -> Self {
        Self::new(button)
    }
}
