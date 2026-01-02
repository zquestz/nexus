//! Handler for ServerInfoUpdate command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, ServerDescriptionError, ServerImageError, ServerNameError, validate_server_description,
    validate_server_image, validate_server_name,
};

use crate::users::manager::broadcasts::ServerInfoBroadcastParams;

use super::{
    HandlerContext, err_admin_required, err_authentication, err_database, err_no_fields_to_update,
    err_not_logged_in, err_server_description_contains_newlines,
    err_server_description_invalid_characters, err_server_description_too_long,
    err_server_image_invalid_format, err_server_image_too_large, err_server_image_unsupported_type,
    err_server_name_contains_newlines, err_server_name_empty, err_server_name_invalid_characters,
    err_server_name_too_long,
};

/// Handle ServerInfoUpdate command
pub async fn handle_server_info_update<W>(
    name: Option<String>,
    description: Option<String>,
    max_connections_per_ip: Option<u32>,
    max_transfers_per_ip: Option<u32>,
    image: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(id) = session_id else {
        eprintln!("ServerInfoUpdate from {} without login", ctx.peer_addr);
        return ctx
            .send_error(&err_not_logged_in(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    };

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error(&err_authentication(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
    };

    // Admin-only - check if user is admin (before validation to not reveal validation rules)
    if !user.is_admin {
        eprintln!(
            "ServerInfoUpdate from {} (user: {}) without admin",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(&err_admin_required(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    // Check that at least one field is being updated
    if name.is_none()
        && description.is_none()
        && max_connections_per_ip.is_none()
        && max_transfers_per_ip.is_none()
        && image.is_none()
    {
        return ctx
            .send_error(
                &err_no_fields_to_update(ctx.locale),
                Some("ServerInfoUpdate"),
            )
            .await;
    }

    // Validate name if provided
    if let Some(ref n) = name
        && let Err(e) = validate_server_name(n)
    {
        let error_msg = match e {
            ServerNameError::Empty => err_server_name_empty(ctx.locale),
            ServerNameError::TooLong => {
                err_server_name_too_long(ctx.locale, validators::MAX_SERVER_NAME_LENGTH)
            }
            ServerNameError::ContainsNewlines => err_server_name_contains_newlines(ctx.locale),
            ServerNameError::InvalidCharacters => err_server_name_invalid_characters(ctx.locale),
        };
        return ctx.send_error(&error_msg, Some("ServerInfoUpdate")).await;
    }

    // Validate description if provided
    if let Some(ref d) = description
        && let Err(e) = validate_server_description(d)
    {
        let error_msg = match e {
            ServerDescriptionError::TooLong => err_server_description_too_long(
                ctx.locale,
                validators::MAX_SERVER_DESCRIPTION_LENGTH,
            ),
            ServerDescriptionError::ContainsNewlines => {
                err_server_description_contains_newlines(ctx.locale)
            }
            ServerDescriptionError::InvalidCharacters => {
                err_server_description_invalid_characters(ctx.locale)
            }
        };
        return ctx.send_error(&error_msg, Some("ServerInfoUpdate")).await;
    }

    // Note: max_connections_per_ip and max_transfers_per_ip allow 0 (meaning unlimited)
    // No validation needed beyond Option<u32> type checking

    // Validate image if provided (empty string is allowed to clear image)
    if let Some(ref img) = image
        && !img.is_empty()
        && let Err(e) = validate_server_image(img)
    {
        let error_msg = match e {
            ServerImageError::TooLarge => err_server_image_too_large(ctx.locale),
            ServerImageError::InvalidFormat => err_server_image_invalid_format(ctx.locale),
            ServerImageError::UnsupportedType => err_server_image_unsupported_type(ctx.locale),
        };
        return ctx.send_error(&error_msg, Some("ServerInfoUpdate")).await;
    }

    // Apply updates to database
    if let Some(ref n) = name
        && let Err(e) = ctx.db.config.set_server_name(n).await
    {
        eprintln!("Database error setting server name: {}", e);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(ref d) = description
        && let Err(e) = ctx.db.config.set_server_description(d).await
    {
        eprintln!("Database error setting server description: {}", e);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(max_conn) = max_connections_per_ip {
        if let Err(e) = ctx.db.config.set_max_connections_per_ip(max_conn).await {
            eprintln!("Database error setting max_connections_per_ip: {}", e);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
        // Update the connection tracker limit dynamically
        ctx.connection_tracker
            .set_max_connections_per_ip(max_conn as usize);
    }

    if let Some(max_xfer) = max_transfers_per_ip {
        if let Err(e) = ctx.db.config.set_max_transfers_per_ip(max_xfer).await {
            eprintln!("Database error setting max_transfers_per_ip: {}", e);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
        // Update the connection tracker limit dynamically
        ctx.connection_tracker
            .set_max_transfers_per_ip(max_xfer as usize);
    }

    if let Some(ref img) = image
        && let Err(e) = ctx.db.config.set_server_image(img).await
    {
        eprintln!("Database error setting server image: {}", e);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    // Fetch current server info for broadcast
    let current_name = ctx.db.config.get_server_name().await;
    let current_description = ctx.db.config.get_server_description().await;
    let current_max_connections = ctx.db.config.get_max_connections_per_ip().await as u32;
    let current_max_transfers = ctx.db.config.get_max_transfers_per_ip().await as u32;
    let current_image = ctx.db.config.get_server_image().await;
    let server_version = env!("CARGO_PKG_VERSION").to_string();

    // Broadcast ServerInfoUpdated to all connected users
    // Admins get max_connections_per_ip and max_transfers_per_ip, non-admins don't
    ctx.user_manager
        .broadcast_server_info_updated(ServerInfoBroadcastParams {
            name: current_name,
            description: current_description,
            version: server_version,
            max_connections_per_ip: current_max_connections,
            max_transfers_per_ip: current_max_transfers,
            image: current_image,
            transfer_port: ctx.transfer_port,
        })
        .await;

    // Send success response to requester
    ctx.send_message(&ServerMessage::ServerInfoUpdateResponse {
        success: true,
        error: None,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
    };

    #[tokio::test]
    async fn test_server_info_update_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_server_info_update(
            Some("New Name".to_string()),
            None,
            None,
            None,
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_not_logged_in(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_requires_admin() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin user
        let session_id = login_user(&mut test_ctx, "testuser", "password", &[], false).await;

        let result = handle_server_info_update(
            Some("New Name".to_string()),
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_admin_required(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_no_fields_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_no_fields_to_update(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_name_empty_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            Some("".to_string()),
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_server_name_empty(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_name_too_long_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_name = "a".repeat(validators::MAX_SERVER_NAME_LENGTH + 1);
        let result = handle_server_info_update(
            Some(long_name),
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert!(message.contains(&validators::MAX_SERVER_NAME_LENGTH.to_string()));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_description_too_long_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_desc = "a".repeat(validators::MAX_SERVER_DESCRIPTION_LENGTH + 1);
        let result = handle_server_info_update(
            None,
            Some(long_desc),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert!(message.contains(&validators::MAX_SERVER_DESCRIPTION_LENGTH.to_string()));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_max_connections_zero_means_unlimited() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 0 means unlimited - should succeed
        let result = handle_server_info_update(
            None,
            None,
            Some(0),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify 0 was saved (means unlimited)
        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 0);
    }

    #[tokio::test]
    async fn test_server_info_update_name_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            Some("My New Server".to_string()),
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify name was saved
        let saved_name = test_ctx.db.config.get_server_name().await;
        assert_eq!(saved_name, "My New Server");
    }

    #[tokio::test]
    async fn test_server_info_update_description_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            None,
            Some("Welcome to my server!".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify description was saved
        let saved_desc = test_ctx.db.config.get_server_description().await;
        assert_eq!(saved_desc, "Welcome to my server!");
    }

    #[tokio::test]
    async fn test_server_info_update_max_connections_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            None,
            None,
            Some(10),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify max_connections was saved
        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 10);
    }

    #[tokio::test]
    async fn test_server_info_update_all_fields_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_server_info_update(
            Some("Full Update Server".to_string()),
            Some("All fields updated".to_string()),
            Some(15),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify all fields were saved
        let saved_name = test_ctx.db.config.get_server_name().await;
        assert_eq!(saved_name, "Full Update Server");

        let saved_desc = test_ctx.db.config.get_server_description().await;
        assert_eq!(saved_desc, "All fields updated");

        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 15);
    }

    #[tokio::test]
    async fn test_server_info_update_empty_description_allowed() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // First set a description
        test_ctx
            .db
            .config
            .set_server_description("Initial description")
            .await
            .unwrap();

        // Then clear it
        let result = handle_server_info_update(
            None,
            Some("".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify description was cleared
        let saved_desc = test_ctx.db.config.get_server_description().await;
        assert_eq!(saved_desc, "");
    }

    // =========================================================================
    // Server Image Tests
    // =========================================================================

    #[tokio::test]
    async fn test_server_info_update_image_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a test image (minimal valid PNG data URI)
        let test_image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            Some(test_image.to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify image was saved
        let saved_image = test_ctx.db.config.get_server_image().await;
        assert_eq!(saved_image, test_image);
    }

    #[tokio::test]
    async fn test_server_info_update_image_empty_allowed() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // First set an image
        test_ctx
            .db
            .config
            .set_server_image("data:image/png;base64,iVBORw0KGgo=")
            .await
            .unwrap();

        // Then clear it
        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            Some("".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify image was cleared
        let saved_image = test_ctx.db.config.get_server_image().await;
        assert_eq!(saved_image, "");
    }

    #[tokio::test]
    async fn test_server_info_update_image_too_large_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create an image that exceeds the limit
        let prefix = "data:image/png;base64,";
        let padding = "A".repeat(validators::MAX_SERVER_IMAGE_DATA_URI_LENGTH);
        let large_image = format!("{}{}", prefix, padding);

        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            Some(large_image),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_server_image_too_large(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_image_invalid_format_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Invalid format (not a data URI)
        let invalid_image = "not a data uri";

        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            Some(invalid_image.to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(
                    message,
                    err_server_image_invalid_format(DEFAULT_TEST_LOCALE)
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_image_unsupported_type_fails() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Unsupported image type (GIF)
        let unsupported_image = "data:image/gif;base64,R0lGODlh";

        let result = handle_server_info_update(
            None,
            None,
            None,
            None,
            Some(unsupported_image.to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(
                    message,
                    err_server_image_unsupported_type(DEFAULT_TEST_LOCALE)
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }
}
