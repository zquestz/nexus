//! Reveal a search result using its laid-out row bounds.

use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, Scrollable};
use iced::advanced::widget::operation::{Operation, Outcome};
use iced::widget::Id;
use iced::{Rectangle, Task, Vector};
use iced_runtime::task as runtime_task;

use crate::NexusApp;
use crate::types::{ActivePanel, FileScrollOutcome, Message, TabId};

impl NexusApp {
    pub fn handle_file_scrolled(
        &mut self,
        tab_id: TabId,
        scroll_id: Id,
        offset: f32,
    ) -> Task<Message> {
        // Tab IDs are globally unique. Route queued events to their origin,
        // even if the user has since switched tabs or connections.
        for conn in self.connections.values_mut() {
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                if tab.scroll_id == scroll_id {
                    tab.scroll_offset = offset;
                }
                break;
            }
        }
        Task::none()
    }

    pub fn handle_file_scroll_to_target(&mut self, tab_id: TabId, target_id: Id) -> Task<Message> {
        if self.active_panel() != ActivePanel::Files {
            return Task::none();
        }
        let Some(connection_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };
        let tab = conn.files_management.active_tab_mut();
        if tab.id != tab_id || tab.is_searching() {
            return Task::none();
        }
        let Some(target) = tab
            .scroll_target
            .as_mut()
            .filter(|target| target.id == target_id)
        else {
            return Task::none();
        };
        if tab.entries.is_none() {
            // A reload may start after on_show queued this request. A fresh ID
            // makes the cached sensor notify again once the listing is ready.
            target.id = Id::unique();
            return Task::none();
        }

        runtime_task::widget(FindFileTarget::new(
            tab.scroll_id.clone(),
            target_id.clone(),
        ))
        .map(move |outcome| Message::FileScrollCompleted {
            connection_id,
            tab_id,
            target_id: target_id.clone(),
            outcome,
        })
    }

    pub fn handle_file_scroll_completed(
        &mut self,
        connection_id: usize,
        tab_id: TabId,
        target_id: Id,
        outcome: FileScrollOutcome,
    ) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id)
            && let Some(tab) = conn.files_management.tab_by_id_mut(tab_id)
            && let Some(target) = &mut tab.scroll_target
            && target.id == target_id
        {
            if tab.entries.is_none() || matches!(outcome, FileScrollOutcome::ViewUnavailable) {
                // A hidden or reloading view can retry with fresh widget state.
                target.id = Id::unique();
            } else {
                if let FileScrollOutcome::Scrolled(offset) = outcome {
                    // Save immediately: a tab switch may precede the next redraw's
                    // viewport notification after this programmatic scroll.
                    tab.scroll_offset = offset;
                }
                // A missing row in the correct viewport is terminal, not a
                // reason to retry on every redraw. Do not infer disk deletion.
                tab.scroll_target = None;
            }
        }
        Task::none()
    }
}

struct FindFileTarget {
    scroll_id: Id,
    target_id: Id,
    viewport: Option<Rectangle>,
    content: Option<Rectangle>,
    target: Option<Rectangle>,
}

impl FindFileTarget {
    fn new(scroll_id: Id, target_id: Id) -> Self {
        Self {
            scroll_id,
            target_id,
            viewport: None,
            content: None,
            target: None,
        }
    }
}

impl Operation<FileScrollOutcome> for FindFileTarget {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<FileScrollOutcome>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.target_id) {
            self.target = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.scroll_id) {
            self.viewport = Some(bounds);
            self.content = Some(content_bounds);
        }
    }

    fn finish(&self) -> Outcome<FileScrollOutcome> {
        let (Some(viewport), Some(content)) = (self.viewport, self.content) else {
            return Outcome::Some(FileScrollOutcome::ViewUnavailable);
        };
        let Some(target) = self.target else {
            return Outcome::Some(FileScrollOutcome::TargetMissing);
        };

        // Layout coordinates include all preceding wrapped rows. Center short
        // targets; align oversized rows at the top, clamping at either end.
        let offset = (target.y - content.y - (viewport.height - target.height).max(0.0) / 2.0)
            .clamp(0.0, (content.height - viewport.height).max(0.0));

        // Chain within one widget operation so a tab switch cannot occur
        // between measuring the target and scrolling its containing list.
        Outcome::Chain(Box::new(ScrollToFileTarget {
            scroll_id: self.scroll_id.clone(),
            offset,
            scrolled: false,
        }))
    }
}

struct ScrollToFileTarget {
    scroll_id: Id,
    offset: f32,
    scrolled: bool,
}

impl Operation<FileScrollOutcome> for ScrollToFileTarget {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<FileScrollOutcome>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.scroll_id) {
            state.scroll_to(AbsoluteOffset {
                x: None,
                y: Some(self.offset),
            });
            self.scrolled = true;
        }
    }

    fn finish(&self) -> Outcome<FileScrollOutcome> {
        Outcome::Some(if self.scrolled {
            FileScrollOutcome::Scrolled(self.offset)
        } else {
            FileScrollOutcome::ViewUnavailable
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::black_box;
    use iced::advanced::widget::operation::scrollable::RelativeOffset;
    use iced::advanced::{clipboard, mouse};
    use iced::{Event, Font, Pixels, Size, window};
    use iced_runtime::user_interface::{Cache, UserInterface};
    use nexus_common::protocol::FileEntry;

    use crate::testing::support::test_connection_with_receiver;
    use crate::types::{FileScrollTarget, FileTab, FilesManagementState};
    use crate::views::files::{FilePermissions, files_view};

    #[derive(Default)]
    struct ScrollState {
        offset: Option<AbsoluteOffset<Option<f32>>>,
    }

    impl Scrollable for ScrollState {
        fn snap_to(&mut self, _offset: RelativeOffset<Option<f32>>) {
            panic!("revealing a file must use its actual bounds");
        }

        fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>) {
            self.offset = Some(offset);
        }

        fn scroll_by(&mut self, _offset: AbsoluteOffset, _bounds: Rectangle, _content: Rectangle) {
            panic!("revealing a file must use an absolute offset");
        }
    }

    fn run_scroll(target_y: f32, target_height: f32, content_height: f32) -> f32 {
        let id = Id::unique();
        let scroll_id = Id::unique();
        let mut operation = FindFileTarget::new(scroll_id.clone(), id.clone());
        let mut state = ScrollState::default();
        let viewport = Rectangle {
            x: 20.0,
            y: 100.0,
            width: 600.0,
            height: 400.0,
        };
        let content = Rectangle {
            height: content_height,
            ..viewport
        };
        operation.scrollable(
            Some(&scroll_id),
            viewport,
            content,
            Vector::new(0.0, 75.0),
            &mut state,
        );
        operation.container(
            Some(&id),
            Rectangle {
                y: content.y + target_y,
                height: target_height,
                ..content
            },
        );
        let Outcome::Chain(mut scroll) = operation.finish() else {
            panic!("a matching target must produce a scroll operation");
        };
        assert!(state.offset.is_none());
        scroll.scrollable(
            Some(&scroll_id),
            viewport,
            content,
            Vector::new(0.0, 75.0),
            &mut state,
        );
        assert!(matches!(
            scroll.finish(),
            Outcome::Some(FileScrollOutcome::Scrolled(_))
        ));
        let offset = state.offset.expect("scroll offset was applied");
        assert_eq!(offset.x, None);
        offset.y.expect("vertical offset was applied")
    }

    #[test]
    fn scroll_uses_actual_wrapped_row_bounds() {
        assert_eq!(run_scroll(900.0, 80.0, 2000.0), 740.0);
        assert_eq!(run_scroll(1100.0, 80.0, 2000.0), 940.0);
    }

    #[test]
    fn scroll_clamps_at_list_edges_and_handles_tall_rows() {
        assert_eq!(run_scroll(0.0, 30.0, 2000.0), 0.0);
        assert_eq!(run_scroll(1970.0, 30.0, 2000.0), 1600.0);
        assert_eq!(run_scroll(30.0, 30.0, 200.0), 0.0);
        assert_eq!(run_scroll(600.0, 800.0, 2000.0), 600.0);
    }

    #[test]
    fn missing_target_does_not_scroll_another_listing() {
        let scroll_id = Id::unique();
        let mut operation = FindFileTarget::new(scroll_id.clone(), Id::unique());
        let mut state = ScrollState::default();
        operation.scrollable(
            Some(&scroll_id),
            Rectangle::default(),
            Rectangle::default(),
            Vector::ZERO,
            &mut state,
        );
        operation.container(Some(&Id::unique()), Rectangle::default());
        assert!(matches!(
            operation.finish(),
            Outcome::Some(FileScrollOutcome::TargetMissing)
        ));
        assert!(state.offset.is_none());
    }

    #[test]
    fn both_reveal_passes_are_scoped_to_the_original_scrollable() {
        let scroll_id = Id::unique();
        let other_id = Id::unique();
        let target_id = Id::unique();
        let mut find = FindFileTarget::new(scroll_id.clone(), target_id.clone());
        let mut state = ScrollState::default();
        find.container(Some(&target_id), Rectangle::default());
        find.scrollable(
            Some(&other_id),
            Rectangle::default(),
            Rectangle::default(),
            Vector::ZERO,
            &mut state,
        );
        assert!(matches!(
            find.finish(),
            Outcome::Some(FileScrollOutcome::ViewUnavailable)
        ));
        find.scrollable(
            Some(&scroll_id),
            Rectangle::default(),
            Rectangle::default(),
            Vector::ZERO,
            &mut state,
        );
        let Outcome::Chain(mut scroll) = find.finish() else {
            panic!("matching viewport");
        };
        scroll.scrollable(
            Some(&other_id),
            Rectangle::default(),
            Rectangle::default(),
            Vector::ZERO,
            &mut state,
        );
        assert!(matches!(
            scroll.finish(),
            Outcome::Some(FileScrollOutcome::ViewUnavailable)
        ));
        assert!(state.offset.is_none(), "another tab must not be scrolled");
    }

    #[test]
    fn queued_viewport_events_follow_their_tab_but_not_replaced_content() {
        let (mut conn, _rx) = test_connection_with_receiver(1);
        let tab_id = conn.files_management.active_tab().id;
        let scroll_id = conn.files_management.active_tab().scroll_id.clone();
        conn.files_management.tabs.push(FileTab::default());
        conn.files_management.active_tab = 1;
        let mut app = NexusApp::default();
        app.connections.insert(1, conn);
        // No active connection: the queued notification still belongs to tab 0.
        drop(app.handle_file_scrolled(tab_id, scroll_id.clone(), 240.0));
        let files = &mut app
            .connections
            .get_mut(&1)
            .expect("connection")
            .files_management;
        assert_eq!(files.tabs[0].scroll_offset, 240.0);
        assert_eq!(files.tabs[1].scroll_offset, 0.0);
        files.tabs[0].navigate_to("another-directory".to_string());
        drop(app.handle_file_scrolled(tab_id, scroll_id, 500.0));
        assert_eq!(
            app.connections[&1].files_management.tabs[0].scroll_offset,
            0.0
        );
    }

    #[tokio::test]
    async fn rendered_listing_reveals_target_at_different_sizes_and_sort_orders() {
        struct CaptureOffset(Id, Option<Vector>);

        impl Operation for CaptureOffset {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }

            fn scrollable(
                &mut self,
                id: Option<&Id>,
                _bounds: Rectangle,
                _content: Rectangle,
                translation: Vector,
                _state: &mut dyn Scrollable,
            ) {
                if id == Some(&self.0) {
                    self.1 = Some(translation);
                }
            }
        }

        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        for (size, ascending) in [
            (Size::new(640.0, 480.0), true),
            (Size::new(1280.0, 800.0), false),
        ] {
            let mut files = FilesManagementState::default();
            let tab = files.active_tab_mut();
            let entries: Vec<FileEntry> = (0..80)
                .map(|index| FileEntry {
                    name: format!("file-{index:02}-{}.txt", "long-name-".repeat(12)),
                    size: 100,
                    modified: 0,
                    dir_type: None,
                    can_upload: false,
                })
                .collect();
            let target_id = Id::unique();
            let tab_id = tab.id;
            let scroll_id = tab.scroll_id.clone();
            tab.scroll_target = Some(FileScrollTarget {
                name: entries[40].name.clone(),
                id: target_id.clone(),
            });
            tab.entries = Some(entries);
            tab.sort_ascending = ascending;
            tab.update_sorted_entries();
            let perms = FilePermissions {
                file_root: false,
                file_create_dir: false,
                file_info: false,
                file_delete: false,
                file_rename: false,
                file_move: false,
                file_copy: false,
                file_download: false,
                file_upload: false,
                file_upload_anywhere: false,
                file_search: true,
            };
            let mut ui = UserInterface::build(
                files_view(&files, perms, false, false, "me"),
                size,
                Cache::new(),
                &mut renderer,
            );
            let mut messages = Vec::new();
            let _ = ui.update(
                &[Event::Window(window::Event::RedrawRequested(
                    iced::time::Instant::now(),
                ))],
                mouse::Cursor::Unavailable,
                &mut renderer,
                &mut clipboard::Null,
                &mut messages,
            );
            assert!(messages.iter().any(|message| matches!(message, Message::FileScrollToTarget { tab_id: id, target_id: target } if *id == tab_id && *target == target_id)));

            let mut find = FindFileTarget::new(scroll_id.clone(), target_id);
            ui.operate(&renderer, &mut black_box(&mut find));
            let target = find
                .target
                .expect("target ID is exposed through the lazy table");
            let viewport = find.viewport.expect("files viewport");
            assert!(
                target.y > viewport.y + viewport.height,
                "target starts below the viewport"
            );
            let Outcome::Chain(mut scroll) = find.finish() else {
                panic!("rendered target must produce a scroll operation");
            };
            ui.operate(&renderer, &mut black_box(scroll.as_mut()));
            assert!(matches!(
                scroll.finish(),
                Outcome::Some(FileScrollOutcome::Scrolled(_))
            ));
            let mut offset = CaptureOffset(scroll_id, None);
            ui.operate(&renderer, &mut offset);
            let visible_y = target.y - offset.1.expect("scroll translation").y;
            assert!(visible_y >= viewport.y, "target top is visible");
            assert!(
                visible_y + target.height <= viewport.y + viewport.height,
                "target bottom is visible"
            );
        }
    }
}
