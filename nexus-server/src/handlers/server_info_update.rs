//! Handler for ServerInfoUpdate command

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, PublicAddressError, ServerDescriptionError, ServerImageError, ServerNameError,
    validate_channel, validate_public_address, validate_server_description, validate_server_image,
    validate_server_name,
};

use super::{
    HandlerContext, ServerInfoValues, channel_error_to_message, err_admin_required,
    err_authentication, err_channel_list_invalid, err_database, err_invalid_password_strength,
    err_no_fields_to_update, err_not_logged_in, err_public_address_contains_brackets,
    err_public_address_contains_path, err_public_address_contains_port,
    err_public_address_contains_scheme, err_public_address_contains_userinfo,
    err_public_address_contains_whitespace, err_public_address_contains_zone_id,
    err_public_address_invalid_format, err_public_address_too_long,
    err_server_description_contains_newlines, err_server_description_invalid_characters,
    err_server_description_too_long, err_server_image_invalid_format, err_server_image_too_large,
    err_server_image_unsupported_type, err_server_name_contains_newlines, err_server_name_empty,
    err_server_name_invalid_characters, err_server_name_too_long,
};
use crate::constants::{
    LOG_SERVER_INFO_ADMIN_REQUIRED, LOG_SERVER_INFO_CHANNEL_CREATE_FAILED,
    LOG_SERVER_INFO_CHANNEL_DELETE_FAILED, LOG_SERVER_INFO_DB_AUTO_JOIN,
    LOG_SERVER_INFO_DB_CHAT_BURST, LOG_SERVER_INFO_DB_CHAT_RATE, LOG_SERVER_INFO_DB_CONNECTIONS,
    LOG_SERVER_INFO_DB_DESC, LOG_SERVER_INFO_DB_IMAGE, LOG_SERVER_INFO_DB_NAME,
    LOG_SERVER_INFO_DB_PASSWORD, LOG_SERVER_INFO_DB_PERSISTENT, LOG_SERVER_INFO_DB_PUBLIC_ADDRESS,
    LOG_SERVER_INFO_DB_REINDEX, LOG_SERVER_INFO_DB_TRANSFERS, LOG_SERVER_INFO_NOT_LOGGED_IN,
    LOG_SERVER_INFO_SUCCESS,
};

/// Request parameters for ServerInfoUpdate command
pub struct ServerInfoUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub public_address: Option<String>,
    pub max_connections_per_ip: Option<u32>,
    pub max_transfers_per_ip: Option<u32>,
    pub image: Option<String>,
    pub file_reindex_interval: Option<u32>,
    pub persistent_channels: Option<String>,
    pub auto_join_channels: Option<String>,
    pub chat_burst_limit: Option<u32>,
    pub chat_rate_limit: Option<u32>,
    pub min_password_strength: Option<u8>,
    pub session_id: Option<u32>,
}

/// Handle ServerInfoUpdate command
pub async fn handle_server_info_update<W>(
    request: ServerInfoUpdateRequest,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let ServerInfoUpdateRequest {
        name,
        description,
        public_address,
        max_connections_per_ip,
        max_transfers_per_ip,
        image,
        file_reindex_interval,
        persistent_channels,
        auto_join_channels,
        chat_burst_limit,
        chat_rate_limit,
        min_password_strength,
        session_id,
    } = request;

    // Verify authentication
    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_NOT_LOGGED_IN);
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
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_ADMIN_REQUIRED);
        return ctx
            .send_error(&err_admin_required(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    // Check that at least one field is being updated
    if name.is_none()
        && description.is_none()
        && public_address.is_none()
        && max_connections_per_ip.is_none()
        && max_transfers_per_ip.is_none()
        && image.is_none()
        && file_reindex_interval.is_none()
        && persistent_channels.is_none()
        && auto_join_channels.is_none()
        && chat_burst_limit.is_none()
        && chat_rate_limit.is_none()
        && min_password_strength.is_none()
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

    // Validate public_address if provided (empty string clears the advertised value)
    if let Some(ref addr) = public_address
        && let Err(e) = validate_public_address(addr)
    {
        let error_msg = match e {
            PublicAddressError::TooLong => {
                err_public_address_too_long(ctx.locale, validators::MAX_PUBLIC_ADDRESS_LENGTH)
            }
            PublicAddressError::ContainsScheme => err_public_address_contains_scheme(ctx.locale),
            PublicAddressError::ContainsBrackets => {
                err_public_address_contains_brackets(ctx.locale)
            }
            PublicAddressError::ContainsPath => err_public_address_contains_path(ctx.locale),
            PublicAddressError::ContainsUserinfo => {
                err_public_address_contains_userinfo(ctx.locale)
            }
            PublicAddressError::ContainsWhitespace => {
                err_public_address_contains_whitespace(ctx.locale)
            }
            PublicAddressError::ContainsPort => err_public_address_contains_port(ctx.locale),
            PublicAddressError::ContainsZoneId => err_public_address_contains_zone_id(ctx.locale),
            PublicAddressError::InvalidFormat => err_public_address_invalid_format(ctx.locale),
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

    // Validate persistent_channels if provided
    if let Some(ref channels_str) = persistent_channels {
        let channel_names = crate::db::ConfigDb::parse_channel_list(channels_str);
        for name in &channel_names {
            if let Err(e) = validate_channel(name) {
                let reason = channel_error_to_message(e, ctx.locale);
                let error_msg = err_channel_list_invalid(ctx.locale, name, &reason);
                return ctx.send_error(&error_msg, Some("ServerInfoUpdate")).await;
            }
        }
    }

    // Validate auto_join_channels if provided
    if let Some(ref channels_str) = auto_join_channels {
        let channel_names = crate::db::ConfigDb::parse_channel_list(channels_str);
        for name in &channel_names {
            if let Err(e) = validate_channel(name) {
                let reason = channel_error_to_message(e, ctx.locale);
                let error_msg = err_channel_list_invalid(ctx.locale, name, &reason);
                return ctx.send_error(&error_msg, Some("ServerInfoUpdate")).await;
            }
        }
    }

    // Validate min password strength if provided (must be 0-4)
    if let Some(strength) = min_password_strength
        && strength > validators::PasswordStrength::Excellent.score()
    {
        let response = ServerMessage::ServerInfoUpdateResponse {
            success: false,
            error: Some(err_invalid_password_strength(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Apply updates to database
    if let Some(ref n) = name
        && let Err(e) = ctx.db.config.set_server_name(n).await
    {
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_NAME);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(ref d) = description
        && let Err(e) = ctx.db.config.set_server_description(d).await
    {
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_DESC);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(ref addr) = public_address
        && let Err(e) = ctx.db.config.set_public_address(addr).await
    {
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_PUBLIC_ADDRESS);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(max_conn) = max_connections_per_ip {
        if let Err(e) = ctx.db.config.set_max_connections_per_ip(max_conn).await {
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_CONNECTIONS);
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
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_TRANSFERS);
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
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_IMAGE);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    if let Some(interval) = file_reindex_interval
        && let Err(e) = ctx.db.config.set_file_reindex_interval(interval).await
    {
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_REINDEX);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }
    // Note: The timer task reads from config each cycle, so no runtime update needed

    // Handle persistent_channels update
    if let Some(ref channels_str) = persistent_channels {
        // Save to config
        if let Err(e) = ctx.db.config.set_persistent_channels(channels_str).await {
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_PERSISTENT);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }

        // Parse new channel names
        let new_channel_names = crate::db::ConfigDb::parse_channel_list(channels_str);

        // Get current channel settings from DB
        let current_settings = ctx
            .db
            .channels
            .get_all_channel_settings()
            .await
            .unwrap_or_default();

        // Create settings for new channels (those in new list but not in DB)
        for name in &new_channel_names {
            let name_lower = name.to_lowercase();
            if !current_settings
                .iter()
                .any(|s| s.name.to_lowercase() == name_lower)
                && let Err(e) = ctx
                    .db
                    .channels
                    .upsert_channel_settings(&crate::db::channels::ChannelSettings {
                        name: name.clone(),
                        topic: String::new(),
                        topic_set_by: String::new(),
                        secret: false,
                    })
                    .await
            {
                error!(user = %user.username, ip = %ctx.peer_addr, target = %name, err = %e, "{}", LOG_SERVER_INFO_CHANNEL_CREATE_FAILED);
            }
        }

        // Delete settings for removed channels (those in DB but not in new list)
        for settings in &current_settings {
            let name_lower = settings.name.to_lowercase();
            if !new_channel_names
                .iter()
                .any(|n| n.to_lowercase() == name_lower)
                && let Err(e) = ctx
                    .db
                    .channels
                    .delete_channel_settings(&settings.name)
                    .await
            {
                error!(user = %user.username, ip = %ctx.peer_addr, target = %settings.name, err = %e, "{}", LOG_SERVER_INFO_CHANNEL_DELETE_FAILED);
            }
        }

        // Reinitialize the channel manager with new persistent channels
        // First, build the list of channels with their settings
        let mut channels_to_init = Vec::new();
        for name in &new_channel_names {
            match ctx.db.channels.get_channel_settings(name).await {
                Ok(Some(settings)) => {
                    let (topic, topic_set_by) = if settings.topic.is_empty() {
                        (None, None)
                    } else {
                        (Some(settings.topic), Some(settings.topic_set_by))
                    };
                    channels_to_init.push(crate::channels::Channel::with_settings(
                        name.clone(),
                        topic,
                        topic_set_by,
                        settings.secret,
                    ));
                }
                _ => {
                    channels_to_init.push(crate::channels::Channel::new(name.clone()));
                }
            }
        }

        // Reinitialize (this clears old persistent set and adds new ones)
        ctx.channel_manager
            .reinitialize_persistent_channels(channels_to_init)
            .await;
    }

    // Handle auto_join_channels update (just save to DB, no channel manager changes)
    if let Some(ref channels_str) = auto_join_channels
        && let Err(e) = ctx.db.config.set_auto_join_channels(channels_str).await
    {
        error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_AUTO_JOIN);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
            .await;
    }

    // Handle chat_burst_limit update
    if let Some(burst) = chat_burst_limit {
        if let Err(e) = ctx.db.config.set_chat_burst_limit(burst).await {
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_CHAT_BURST);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
        // Update the shared flood config dynamically
        ctx.flood_config.set_burst(burst);
    }

    // Handle chat_rate_limit update
    if let Some(rate) = chat_rate_limit {
        if let Err(e) = ctx.db.config.set_chat_rate_limit(rate).await {
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_CHAT_RATE);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
        // Update the shared flood config dynamically
        ctx.flood_config.set_rate(rate);
    }

    // Handle min_password_strength update
    if let Some(score) = min_password_strength {
        let strength = validators::PasswordStrength::from(score);
        if let Err(e) = ctx.db.config.set_min_password_strength(strength).await {
            error!(user = %user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_SERVER_INFO_DB_PASSWORD);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ServerInfoUpdate"))
                .await;
        }
    }

    // Fetch current server info for broadcast (single query)
    let config = ctx.db.config.get_all().await;

    // Broadcast ServerInfoUpdated to all connected users
    ctx.user_manager
        .broadcast_server_info_updated(ServerInfoValues {
            name: config.server_name,
            description: config.server_description,
            public_address: config.public_address,
            version: env!("CARGO_PKG_VERSION").to_string(),
            max_connections_per_ip: config.max_connections_per_ip,
            max_transfers_per_ip: config.max_transfers_per_ip,
            image: config.server_image,
            transfer_port: ctx.transfer_port,
            transfer_websocket_port: ctx.transfer_websocket_port,
            file_reindex_interval: config.file_reindex_interval,
            persistent_channels: config.persistent_channels,
            auto_join_channels: config.auto_join_channels,
            min_password_strength: config.min_password_strength.score(),
            chat_burst_limit: config.chat_burst_limit,
            chat_rate_limit: config.chat_rate_limit,
            fingerprint: ctx.fingerprint.to_string(),
        })
        .await;

    // Log and send success response to requester
    info!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_SUCCESS);
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

        let request = ServerInfoUpdateRequest {
            name: Some("New Name".to_string()),
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: None,
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: Some("New Name".to_string()),
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: Some("".to_string()),
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
        let request = ServerInfoUpdateRequest {
            name: Some(long_name),
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
        let request = ServerInfoUpdateRequest {
            name: None,
            description: Some(long_desc),
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: Some(0),
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: Some("My Custom Server".to_string()),
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify name was saved
        let saved_name = test_ctx.db.config.get_server_name().await;
        assert_eq!(saved_name, "My Custom Server");
    }

    #[tokio::test]
    async fn test_server_info_update_description_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: Some("Welcome to my server!".to_string()),
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: Some(10),
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: Some("Full Update Server".to_string()),
            description: Some("All fields updated".to_string()),
            public_address: None,
            max_connections_per_ip: Some(15),
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
        let request = ServerInfoUpdateRequest {
            name: None,
            description: Some("".to_string()),
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
    // Public Address Tests
    // =========================================================================

    #[tokio::test]
    async fn test_server_info_update_public_address_success() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: Some("bbs.example.com".to_string()),
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Broadcast travels through the mpsc channel (test_ctx.rx), not the socket.
        let (broadcast, _) = test_ctx.rx.recv().await.expect("broadcast delivered");
        match broadcast {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(
                    server_info.public_address,
                    Some("bbs.example.com".to_string())
                );
            }
            _ => panic!("Expected ServerInfoUpdated, got {:?}", broadcast),
        }

        // Response comes back over the socket.
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        let saved = test_ctx.db.config.get_public_address().await;
        assert_eq!(saved, "bbs.example.com");
    }

    #[tokio::test]
    async fn test_server_info_update_public_address_empty_clears() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Pre-seed so we can verify the clear actually took effect.
        test_ctx
            .db
            .config
            .set_public_address("bbs.example.com")
            .await
            .unwrap();

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: Some(String::new()),
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Empty stored value maps to None on the wire.
        let (broadcast, _) = test_ctx.rx.recv().await.expect("broadcast delivered");
        match broadcast {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.public_address, None);
            }
            _ => panic!("Expected ServerInfoUpdated, got {:?}", broadcast),
        }

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        let saved = test_ctx.db.config.get_public_address().await;
        assert_eq!(saved, "");
    }

    #[tokio::test]
    async fn test_server_info_update_public_address_empty_when_already_unset() {
        // Server must handle `Some("")` on an already-unset field without error:
        // the DB write is a no-op at the value level, but the broadcast still
        // fires and still reports `None` on the wire. Guards against future
        // client diff-logic refactors that might send this payload.
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Baseline: public_address is unset (migration seeds empty).
        assert_eq!(test_ctx.db.config.get_public_address().await, "");

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: Some(String::new()),
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Broadcast still fires; empty stored value maps to None on the wire.
        let (broadcast, _) = test_ctx.rx.recv().await.expect("broadcast delivered");
        match broadcast {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.public_address, None);
            }
            _ => panic!("Expected ServerInfoUpdated, got {:?}", broadcast),
        }

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        assert_eq!(test_ctx.db.config.get_public_address().await, "");
    }

    #[tokio::test]
    async fn test_server_info_update_public_address_too_long_fails() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_addr = "a".repeat(validators::MAX_PUBLIC_ADDRESS_LENGTH + 1);
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: Some(long_addr),
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert!(message.contains(&validators::MAX_PUBLIC_ADDRESS_LENGTH.to_string()));
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_public_address_invalid_format_fails() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Embedded port should trip the ContainsPort branch.
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: Some("example.com:7500".to_string()),
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(
                    message,
                    err_public_address_contains_port(DEFAULT_TEST_LOCALE)
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: Some(test_image.to_string()),
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: Some("".to_string()),
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: Some(large_image),
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: Some(invalid_image.to_string()),
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: Some(unsupported_image.to_string()),
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

    // =========================================================================
    // File Reindex Interval Tests
    // =========================================================================

    #[tokio::test]
    async fn test_server_info_update_file_reindex_interval_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: Some(10),
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify interval was saved
        let saved_interval = test_ctx.db.config.get_file_reindex_interval().await;
        assert_eq!(saved_interval, 10);
    }

    #[tokio::test]
    async fn test_server_info_update_file_reindex_interval_zero_disables() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 0 means disabled - should succeed
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: Some(0),
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify 0 was saved (means disabled)
        let saved_interval = test_ctx.db.config.get_file_reindex_interval().await;
        assert_eq!(saved_interval, 0);
    }

    #[tokio::test]
    async fn test_server_info_update_persistent_channels_valid() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: Some("#general #support".to_string()),
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify channels were saved
        let saved = test_ctx.db.config.get_persistent_channels().await;
        assert_eq!(saved, "#general #support");
    }

    #[tokio::test]
    async fn test_server_info_update_persistent_channels_missing_prefix() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // "general" is missing the # prefix
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: Some("#valid general".to_string()),
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert!(
                    message.contains("general"),
                    "Error should mention the invalid channel"
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_auto_join_channels_valid() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: Some("#nexus #welcome".to_string()),
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify channels were saved
        let saved = test_ctx.db.config.get_auto_join_channels().await;
        assert_eq!(saved, "#nexus #welcome");
    }

    #[tokio::test]
    async fn test_server_info_update_auto_join_channels_invalid() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // "#" alone is too short (needs at least one character after #)
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: Some("#nexus #".to_string()),
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert!(
                    message.contains("#"),
                    "Error should mention the invalid channel"
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_channels_with_spaces_invalid() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Channel names can't contain spaces - but the parse_channel_list splits on whitespace,
        // so "#my channel" becomes "#my" and "channel" - the latter fails validation
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: Some("#my channel".to_string()),
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                // "channel" will fail because it doesn't start with #
                assert!(
                    message.contains("channel"),
                    "Error should mention the invalid channel"
                );
                assert_eq!(command, Some("ServerInfoUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_empty_channel_list_valid() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Empty string is valid - means no channels
        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: Some("".to_string()),
            auto_join_channels: Some("".to_string()),
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify empty was saved
        let saved_persistent = test_ctx.db.config.get_persistent_channels().await;
        let saved_auto_join = test_ctx.db.config.get_auto_join_channels().await;
        assert_eq!(saved_persistent, "");
        assert_eq!(saved_auto_join, "");
    }

    #[tokio::test]
    async fn test_server_info_update_min_password_strength_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: Some(3),
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify saved value
        let saved = test_ctx.db.config.get_min_password_strength().await;
        assert_eq!(saved, validators::PasswordStrength::Strong);
    }

    #[tokio::test]
    async fn test_server_info_update_min_password_strength_invalid() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: Some(5),
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_min_password_strength_weak_allowed() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: Some(0),
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        let saved = test_ctx.db.config.get_min_password_strength().await;
        assert_eq!(saved, validators::PasswordStrength::Weak);
    }

    #[tokio::test]
    async fn test_server_info_update_min_password_strength_excellent() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: Some(4),
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        let saved = test_ctx.db.config.get_min_password_strength().await;
        assert_eq!(saved, validators::PasswordStrength::Excellent);
    }

    #[tokio::test]
    async fn test_server_info_update_chat_burst_limit_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: Some(10),
            chat_rate_limit: None,
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify DB persistence
        let saved = test_ctx.db.config.get_chat_burst_limit().await;
        assert_eq!(saved, 10);

        // Verify FloodConfig atomic was updated
        assert_eq!(test_ctx.flood_config.burst(), 10);
    }

    #[tokio::test]
    async fn test_server_info_update_chat_rate_limit_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: Some(60),
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify DB persistence
        let saved = test_ctx.db.config.get_chat_rate_limit().await;
        assert_eq!(saved, 60);

        // Verify FloodConfig atomic was updated
        assert_eq!(test_ctx.flood_config.rate(), 60);
    }

    #[tokio::test]
    async fn test_server_info_update_chat_rate_limit_zero_disables() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = ServerInfoUpdateRequest {
            name: None,
            description: None,
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: Some(0),
            min_password_strength: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        // Verify 0 was saved (means disabled)
        let saved = test_ctx.db.config.get_chat_rate_limit().await;
        assert_eq!(saved, 0);

        // Verify FloodConfig atomic was updated
        assert_eq!(test_ctx.flood_config.rate(), 0);
    }
}
