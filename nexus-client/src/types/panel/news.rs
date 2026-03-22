//! News management panel state

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
    },
    /// Confirming deletion of a news item
    ConfirmDelete {
        /// News item ID to delete
        id: i64,
    },
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
    pub fn enter_edit_mode(&mut self, id: i64, image: Option<String>) {
        self.form_image = image.clone().unwrap_or_default();
        self.cached_form_image = if self.form_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&self.form_image, NEWS_IMAGE_MAX_CACHE_WIDTH)
        };
        self.form_error = None;

        self.mode = NewsManagementMode::Edit { id };
    }

    /// Enter confirm delete mode for a news item
    pub fn enter_confirm_delete_mode(&mut self, id: i64) {
        self.mode = NewsManagementMode::ConfirmDelete { id };
        self.delete_error = None;
        self.is_delete_submitting = false;
    }
}
