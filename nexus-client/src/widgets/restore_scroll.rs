//! Restore a scrollable's saved offset before a reused widget handles events.

use iced::advanced::widget::{Operation, Tree, operation, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::Id;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// A scrollable ID is an operation target, not an Iced widget-state key.
/// This wrapper restores the offset on mount or ID change, before viewport
/// notifications can attribute the previous content's offset to the new one.
pub struct RestoreScroll<'a, Message> {
    content: Element<'a, Message>,
    id: Id,
    offset: f32,
}

impl<'a, Message> RestoreScroll<'a, Message> {
    pub fn new(content: impl Into<Element<'a, Message>>, id: Id, offset: f32) -> Self {
        Self {
            content: content.into(),
            id,
            offset,
        }
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for RestoreScroll<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Option<Id>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(None::<Id>)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let restored_id = tree.state.downcast_mut::<Option<Id>>();
        if restored_id.as_ref() != Some(&self.id) {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                Layout::new(&node),
                renderer,
                &mut operation::scrollable::scroll_to::<()>(
                    self.id.clone(),
                    operation::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(self.offset),
                    },
                ),
            );
            *restored_id = Some(self.id.clone());
        }
        node
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<RestoreScroll<'a, Message>> for Element<'a, Message> {
    fn from(widget: RestoreScroll<'a, Message>) -> Self {
        Element::new(widget)
    }
}
