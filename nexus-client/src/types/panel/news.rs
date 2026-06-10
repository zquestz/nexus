//! News management panel state

use nexus_common::names::fold_name;
use nexus_common::protocol::NewsItem;

use crate::image::{CachedImage, decode_data_uri_max_width};
use crate::style::NEWS_IMAGE_MAX_CACHE_WIDTH;

// =============================================================================
// News Management State
// =============================================================================

/// News management panel mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NewsManagementMode {
    /// Showing list of all news items
    #[default]
    List,
    /// Creating a new news item
    Create,
    /// Editing an existing news item
    Edit {
        /// News item ID being edited
        id: i64,
        /// Original body at time of edit.
        original_body: String,
        /// Original image at time of edit (empty = no image).
        original_image: String,
    },
    /// Confirming deletion of a news item
    ConfirmDelete {
        /// News item ID to delete
        id: i64,
    },
}

impl NewsManagementMode {
    /// Returns true when the edit form contains at least one field the
    /// client would send in a `NewsUpdate`.
    pub fn has_effective_news_update_changes(&self, body: &str, image: &str) -> bool {
        let Self::Edit {
            original_body,
            original_image,
            ..
        } = self
        else {
            return false;
        };

        body != original_body || image != original_image
    }

    /// Return the sparse `NewsUpdate` body/image fields for an edit.
    ///
    /// Unchanged fields are omitted. Changed fields are sent explicitly,
    /// including `Some("")` when the user cleared a field.
    pub fn news_update_fields(
        &self,
        body: String,
        image: String,
    ) -> Option<(Option<String>, Option<String>)> {
        let Self::Edit {
            original_body,
            original_image,
            ..
        } = self
        else {
            return None;
        };

        let body = (body != *original_body).then_some(body);
        let image = (image != *original_image).then_some(image);

        if body.is_none() && image.is_none() {
            None
        } else {
            Some((body, image))
        }
    }
}

/// News management panel state (per-connection)
///
/// Note: The body text is stored in `NexusApp.news_body_content` as a `text_editor::Content`
/// because it's not Clone. Only the image and error state are stored here.
#[derive(Clone)]
pub struct NewsManagementState {
    /// Current mode (list, create, edit, confirm delete)
    pub mode: NewsManagementMode,
    /// All news items (None = not loaded, Some(Ok) = loaded, Some(Err) = error)
    pub news_items: Option<Result<Vec<NewsItem>, String>>,
    /// Image data URI for form (used in both create and edit modes)
    pub form_image: String,
    /// Cached image for form preview
    pub cached_form_image: Option<CachedImage>,
    /// Error message for form (create or edit)
    pub form_error: Option<String>,
    /// Error message for list view
    pub list_error: Option<String>,
    /// Error message for delete confirmation dialog
    pub delete_error: Option<String>,
    /// Whether a create/edit submission is in progress (double-submit prevention)
    pub is_submitting: bool,
    /// Whether a delete submission is in progress (double-submit prevention)
    pub is_delete_submitting: bool,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for NewsManagementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewsManagementState")
            .field("mode", &self.mode)
            .field("news_items", &self.news_items)
            .field("form_image", &format!("<{} bytes>", self.form_image.len()))
            .field(
                "cached_form_image",
                &self.cached_form_image.as_ref().map(|_| "<cached>"),
            )
            .field("form_error", &self.form_error)
            .field("list_error", &self.list_error)
            .finish()
    }
}

impl Default for NewsManagementState {
    fn default() -> Self {
        Self {
            mode: NewsManagementMode::List,
            news_items: None,
            form_image: String::new(),
            cached_form_image: None,
            form_error: None,
            list_error: None,
            delete_error: None,
            is_submitting: false,
            is_delete_submitting: false,
        }
    }
}

impl NewsManagementState {
    /// Sort news items the same way the server returns them: newest first, with
    /// higher ids first when multiple posts share the same timestamp.
    pub fn sort_items_newest_first(items: &mut [NewsItem]) {
        items.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
    }

    /// Reset to list mode and clear all form state
    pub fn reset_to_list(&mut self) {
        self.mode = NewsManagementMode::List;
        self.clear_form();
        self.list_error = None;
        self.is_delete_submitting = false;
    }

    /// Clear the form fields (used for both create and edit)
    pub fn clear_form(&mut self) {
        self.form_image.clear();
        self.cached_form_image = None;
        self.form_error = None;
        self.is_submitting = false;
    }

    /// Enter create mode
    pub fn enter_create_mode(&mut self) {
        self.clear_form();
        self.mode = NewsManagementMode::Create;
    }

    /// Enter edit mode for a news item (image pre-populated, body handled by text_editor)
    pub fn enter_edit_mode(&mut self, id: i64, body: Option<String>, image: Option<String>) {
        let original_body = body.unwrap_or_default();
        let original_image = image.unwrap_or_default();
        self.form_image = original_image.clone();
        self.cached_form_image = if self.form_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&self.form_image, NEWS_IMAGE_MAX_CACHE_WIDTH)
        };
        self.form_error = None;

        self.mode = NewsManagementMode::Edit {
            id,
            original_body,
            original_image,
        };
    }

    /// Enter confirm delete mode for a news item
    pub fn enter_confirm_delete_mode(&mut self, id: i64) {
        self.mode = NewsManagementMode::ConfirmDelete { id };
        self.delete_error = None;
        self.is_delete_submitting = false;
    }

    /// Re-label cached news authors after a user rename.
    pub fn rename_cached_author(&mut self, old: &str, new: &str, is_admin: bool) {
        let Some(Ok(items)) = &mut self.news_items else {
            return;
        };

        let old_folded = fold_name(old);
        for item in items {
            if item
                .author
                .as_deref()
                .is_some_and(|author| fold_name(author) == old_folded)
            {
                item.author = Some(new.to_string());
                item.author_is_admin = is_admin;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn news_item(id: i64, author: &str, author_is_admin: bool) -> NewsItem {
        NewsItem {
            id,
            body: Some("body".to_string()),
            image: None,
            author: Some(author.to_string()),
            author_is_admin,
            created_at: 1_767_225_600,
            updated_at: 1_767_225_600,
        }
    }

    #[test]
    fn sort_items_newest_first_uses_timestamp_then_id() {
        let mut items = vec![
            news_item(1, "alice", false),
            news_item(3, "alice", false),
            news_item(2, "alice", false),
        ];
        items[0].created_at = 100;
        items[0].updated_at = 100;
        items[1].created_at = 200;
        items[1].updated_at = 200;
        items[2].created_at = 200;
        items[2].updated_at = 200;

        NewsManagementState::sort_items_newest_first(&mut items);

        let ids: Vec<i64> = items.iter().map(|item| item.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn rename_cached_author_updates_author_and_admin_flag() {
        let mut state = NewsManagementState {
            news_items: Some(Ok(vec![
                news_item(1, "Alice", false),
                news_item(2, "bob", true),
            ])),
            ..Default::default()
        };

        state.rename_cached_author("alice", "alicia", true);

        let items = state.news_items.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(items[0].author.as_deref(), Some("alicia"));
        assert!(items[0].author_is_admin);
        assert_eq!(items[1].author.as_deref(), Some("bob"));
        assert!(items[1].author_is_admin);
    }

    #[test]
    fn edit_mode_has_no_effective_update_changes_initially() {
        let mut state = NewsManagementState::default();
        state.enter_edit_mode(7, Some("body".to_string()), Some("image".to_string()));

        assert!(
            !state
                .mode
                .has_effective_news_update_changes("body", "image")
        );
    }

    #[test]
    fn edit_mode_detects_effective_update_changes() {
        let mut state = NewsManagementState::default();
        state.enter_edit_mode(7, Some("body".to_string()), Some("image".to_string()));

        assert!(
            state
                .mode
                .has_effective_news_update_changes("updated", "image")
        );
    }

    #[test]
    fn edit_mode_toggle_back_is_not_an_effective_update() {
        let mut state = NewsManagementState::default();
        state.enter_edit_mode(7, Some("body".to_string()), None);

        assert!(!state.mode.has_effective_news_update_changes("body", ""));
    }

    #[test]
    fn edit_mode_news_update_fields_are_sparse() {
        let mut state = NewsManagementState::default();
        state.enter_edit_mode(7, Some("body".to_string()), Some("image".to_string()));

        assert_eq!(
            state
                .mode
                .news_update_fields("updated".to_string(), "image".to_string()),
            Some((Some("updated".to_string()), None))
        );
        assert_eq!(
            state
                .mode
                .news_update_fields("body".to_string(), "new-image".to_string()),
            Some((None, Some("new-image".to_string())))
        );
        assert_eq!(
            state
                .mode
                .news_update_fields("body".to_string(), "".to_string()),
            Some((None, Some(String::new())))
        );
        assert_eq!(
            state
                .mode
                .news_update_fields("body".to_string(), "image".to_string()),
            None
        );
    }
}
