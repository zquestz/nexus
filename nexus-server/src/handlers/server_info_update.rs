//! Handler for ServerInfoUpdate command

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, BandwidthChunkSizeError, MAX_BANDWIDTH_CHUNK_SIZE, MIN_BANDWIDTH_CHUNK_SIZE,
    PublicAddressError, ServerDescriptionError, ServerImageError, ServerNameError,
    validate_bandwidth_chunk_size, validate_channel, validate_public_address,
    validate_server_description, validate_server_image, validate_server_name,
};

use super::{
    HandlerContext, Outcome, ServerInfoValues, channel_error_to_message, dispatch_outcome,
    err_admin_required, err_bandwidth_chunk_size_too_large, err_bandwidth_chunk_size_too_small,
    err_channel_list_invalid, err_database, err_invalid_password_strength, err_no_fields_to_update,
    err_not_logged_in, err_public_address_contains_brackets, err_public_address_contains_path,
    err_public_address_contains_port, err_public_address_contains_scheme,
    err_public_address_contains_userinfo, err_public_address_contains_whitespace,
    err_public_address_contains_zone_id, err_public_address_invalid_format,
    err_public_address_too_long, err_server_description_contains_newlines,
    err_server_description_invalid_characters, err_server_description_too_long,
    err_server_image_invalid_format, err_server_image_too_large, err_server_image_unsupported_type,
    err_server_name_contains_newlines, err_server_name_empty, err_server_name_invalid_characters,
    err_server_name_too_long,
};
use crate::constants::{
    HANDLER_SERVER_INFO_UPDATE, LOG_SERVER_INFO_ADMIN_REQUIRED,
    LOG_SERVER_INFO_CHANNEL_CREATE_FAILED, LOG_SERVER_INFO_CHANNEL_DELETE_FAILED,
    LOG_SERVER_INFO_CHANNEL_READ_FAILED, LOG_SERVER_INFO_DB_AUTO_JOIN, LOG_SERVER_INFO_DB_BEGIN,
    LOG_SERVER_INFO_DB_CHAT_BURST, LOG_SERVER_INFO_DB_CHAT_RATE, LOG_SERVER_INFO_DB_COMMIT,
    LOG_SERVER_INFO_DB_CONNECTIONS, LOG_SERVER_INFO_DB_DESC, LOG_SERVER_INFO_DB_IMAGE,
    LOG_SERVER_INFO_DB_MAX_OUTBOUND_RATE, LOG_SERVER_INFO_DB_NAME, LOG_SERVER_INFO_DB_PASSWORD,
    LOG_SERVER_INFO_DB_PERSISTENT, LOG_SERVER_INFO_DB_PUBLIC_ADDRESS, LOG_SERVER_INFO_DB_REINDEX,
    LOG_SERVER_INFO_DB_SCHEDULER_CHUNK_SIZE, LOG_SERVER_INFO_DB_TRANSFERS,
    LOG_SERVER_INFO_NOT_LOGGED_IN, LOG_SERVER_INFO_SUCCESS,
};
use crate::db::{ChannelDb, ConfigDb, channels::ChannelSettings};

/// Send a failure-shaped `ServerInfoUpdateResponse` and return — used
/// for every validation and DB-error path in this handler. Keeps the
/// handler on the typed-response convention even though this response
/// shape only carries `success` and `error`.
async fn send_failure<W>(ctx: &mut HandlerContext<'_, W>, error: String) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = ServerMessage::ServerInfoUpdateResponse {
        success: false,
        error: Some(error),
    };
    ctx.send_message(&response).await
}

/// Carried out of the `'stage` block so the failure is logged after `tx`
/// has rolled back, not while the SQLite write lock is held.
enum StageFail {
    /// (log constant, error detail)
    Db(&'static str, String),
    /// (log constant, error detail, channel name → `target` field)
    Channel(&'static str, String, String),
}

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
    pub max_outbound_rate: Option<u64>,
    pub scheduler_chunk_size: Option<u32>,
    pub session_id: Option<u32>,
}

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
        max_outbound_rate,
        scheduler_chunk_size,
        session_id,
    } = request;

    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_SERVER_INFO_UPDATE),
            )
            .await;
    };

    // Admin gate before validation so non-admins never receive validation-specific
    // errors (information leak). This early check has no lock — the 'locked block
    // re-checks admin under read_user_state to close the demotion race at commit time.
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => return send_failure(ctx, err_not_logged_in(ctx.locale)).await,
    };
    if !user.is_admin {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_ADMIN_REQUIRED);
        return send_failure(ctx, err_admin_required(ctx.locale)).await;
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
        && max_outbound_rate.is_none()
        && scheduler_chunk_size.is_none()
    {
        return send_failure(ctx, err_no_fields_to_update(ctx.locale)).await;
    }

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
        return send_failure(ctx, error_msg).await;
    }

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
        return send_failure(ctx, error_msg).await;
    }

    // Empty string clears the advertised value.
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
        return send_failure(ctx, error_msg).await;
    }

    // max_connections_per_ip / max_transfers_per_ip: 0 means unlimited, so no range check.

    // Empty string clears the image (skip validation in that case).
    if let Some(ref img) = image
        && !img.is_empty()
        && let Err(e) = validate_server_image(img)
    {
        let error_msg = match e {
            ServerImageError::TooLarge => err_server_image_too_large(ctx.locale),
            ServerImageError::InvalidFormat => err_server_image_invalid_format(ctx.locale),
            ServerImageError::UnsupportedType => err_server_image_unsupported_type(ctx.locale),
        };
        return send_failure(ctx, error_msg).await;
    }

    if let Some(ref channels_str) = persistent_channels {
        let channel_names = crate::db::ConfigDb::parse_channel_list(channels_str);
        for name in &channel_names {
            if let Err(e) = validate_channel(name) {
                let reason = channel_error_to_message(e, ctx.locale);
                let error_msg = err_channel_list_invalid(ctx.locale, name, &reason);
                return send_failure(ctx, error_msg).await;
            }
        }
    }

    if let Some(ref channels_str) = auto_join_channels {
        let channel_names = crate::db::ConfigDb::parse_channel_list(channels_str);
        for name in &channel_names {
            if let Err(e) = validate_channel(name) {
                let reason = channel_error_to_message(e, ctx.locale);
                let error_msg = err_channel_list_invalid(ctx.locale, name, &reason);
                return send_failure(ctx, error_msg).await;
            }
        }
    }

    // Strength must be 0-4 (zxcvbn score range).
    if let Some(strength) = min_password_strength
        && strength > validators::PasswordStrength::Excellent.score()
    {
        return send_failure(ctx, err_invalid_password_strength(ctx.locale)).await;
    }

    // Validate scheduler chunk size if provided (1024..=65536 bytes).
    // max_outbound_rate has no semantic bound — the u64 type itself is the limit.
    if let Some(size) = scheduler_chunk_size
        && let Err(e) = validate_bandwidth_chunk_size(size)
    {
        let error_msg = match e {
            BandwidthChunkSizeError::TooSmall => {
                err_bandwidth_chunk_size_too_small(ctx.locale, MIN_BANDWIDTH_CHUNK_SIZE)
            }
            BandwidthChunkSizeError::TooLarge => {
                err_bandwidth_chunk_size_too_large(ctx.locale, MAX_BANDWIDTH_CHUNK_SIZE)
            }
        };
        return send_failure(ctx, error_msg).await;
    }

    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        // Re-read the session under the user_state read lock so a concurrent
        // demotion either completes before this check or waits until after the
        // commit — preventing a non-admin from slipping through the admin gate.
        let user = match ctx.user_manager.get_user_by_session_id(id).await {
            Some(u) => u,
            None => break 'locked Outcome::Disconnect,
        };

        if !user.is_admin {
            warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_ADMIN_REQUIRED);
            break 'locked Outcome::Send(Box::new(ServerMessage::ServerInfoUpdateResponse {
                success: false,
                error: Some(err_admin_required(ctx.locale)),
            }));
        }

        let _server_info_guard = ctx.user_manager.lock_server_info_state().await;

        // The block scopes `tx` so it drops (rolling back on early `break`)
        // before the failure response is written — otherwise the SQLite write
        // lock would be held across that write to a possibly-slow client.
        let staged: Result<Option<Vec<crate::channels::Channel>>, StageFail> = 'stage: {
            let mut tx = match ctx.db.begin().await {
                Ok(t) => t,
                Err(e) => break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_BEGIN, e.to_string())),
            };

            if let Some(ref n) = name
                && let Err(e) = ConfigDb::set_server_name_in_tx(&mut tx, n).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_NAME, e.to_string()));
            }

            if let Some(ref d) = description
                && let Err(e) = ConfigDb::set_server_description_in_tx(&mut tx, d).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_DESC, e.to_string()));
            }

            if let Some(ref addr) = public_address
                && let Err(e) = ConfigDb::set_public_address_in_tx(&mut tx, addr).await
            {
                break 'stage Err(StageFail::Db(
                    LOG_SERVER_INFO_DB_PUBLIC_ADDRESS,
                    e.to_string(),
                ));
            }

            if let Some(max_conn) = max_connections_per_ip
                && let Err(e) = ConfigDb::set_max_connections_per_ip_in_tx(&mut tx, max_conn).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_CONNECTIONS, e.to_string()));
            }

            if let Some(max_xfer) = max_transfers_per_ip
                && let Err(e) = ConfigDb::set_max_transfers_per_ip_in_tx(&mut tx, max_xfer).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_TRANSFERS, e.to_string()));
            }

            if let Some(ref img) = image
                && let Err(e) = ConfigDb::set_server_image_in_tx(&mut tx, img).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_IMAGE, e.to_string()));
            }

            if let Some(interval) = file_reindex_interval
                && let Err(e) = ConfigDb::set_file_reindex_interval_in_tx(&mut tx, interval).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_REINDEX, e.to_string()));
            }
            // No runtime update needed: the timer task re-reads config each cycle.

            // Reconcile per-channel settings rows in-tx so the materialized
            // `channels_to_init` reflects the staged (not yet committed) state.
            let channels_to_init = if let Some(ref channels_str) = persistent_channels {
                if let Err(e) = ConfigDb::set_persistent_channels_in_tx(&mut tx, channels_str).await
                {
                    break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_PERSISTENT, e.to_string()));
                }

                let new_channel_names = ConfigDb::parse_channel_list(channels_str);

                let current_settings =
                    match ChannelDb::get_all_channel_settings_in_tx(&mut tx).await {
                        Ok(v) => v,
                        Err(e) => {
                            break 'stage Err(StageFail::Db(
                                LOG_SERVER_INFO_CHANNEL_READ_FAILED,
                                e.to_string(),
                            ));
                        }
                    };

                for channel_name in &new_channel_names {
                    let name_lower = fold_name(channel_name);
                    if !current_settings
                        .iter()
                        .any(|s| fold_name(&s.name) == name_lower)
                        && let Err(e) = ChannelDb::upsert_channel_settings_in_tx(
                            &mut tx,
                            &ChannelSettings {
                                name: channel_name.clone(),
                                topic: String::new(),
                                topic_set_by: String::new(),
                                secret: false,
                            },
                        )
                        .await
                    {
                        break 'stage Err(StageFail::Channel(
                            LOG_SERVER_INFO_CHANNEL_CREATE_FAILED,
                            e.to_string(),
                            channel_name.clone(),
                        ));
                    }
                }

                for settings in &current_settings {
                    let name_lower = fold_name(&settings.name);
                    if !new_channel_names.iter().any(|n| fold_name(n) == name_lower)
                        && let Err(e) =
                            ChannelDb::delete_channel_settings_in_tx(&mut tx, &settings.name).await
                    {
                        break 'stage Err(StageFail::Channel(
                            LOG_SERVER_INFO_CHANNEL_DELETE_FAILED,
                            e.to_string(),
                            settings.name.clone(),
                        ));
                    }
                }

                let mut init = Vec::with_capacity(new_channel_names.len());
                for channel_name in &new_channel_names {
                    match ChannelDb::get_channel_settings_in_tx(&mut tx, channel_name).await {
                        Ok(Some(settings)) => {
                            let (topic, topic_set_by) = if settings.topic.is_empty() {
                                (None, None)
                            } else {
                                (Some(settings.topic), Some(settings.topic_set_by))
                            };
                            init.push(crate::channels::Channel::with_settings(
                                channel_name.clone(),
                                topic,
                                topic_set_by,
                                settings.secret,
                            ));
                        }
                        Ok(None) => {
                            init.push(crate::channels::Channel::new(channel_name.clone()));
                        }
                        Err(e) => {
                            break 'stage Err(StageFail::Channel(
                                LOG_SERVER_INFO_CHANNEL_READ_FAILED,
                                e.to_string(),
                                channel_name.clone(),
                            ));
                        }
                    }
                }
                Some(init)
            } else {
                None
            };

            if let Some(ref channels_str) = auto_join_channels
                && let Err(e) = ConfigDb::set_auto_join_channels_in_tx(&mut tx, channels_str).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_AUTO_JOIN, e.to_string()));
            }

            if let Some(burst) = chat_burst_limit
                && let Err(e) = ConfigDb::set_chat_burst_limit_in_tx(&mut tx, burst).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_CHAT_BURST, e.to_string()));
            }

            if let Some(rate) = chat_rate_limit
                && let Err(e) = ConfigDb::set_chat_rate_limit_in_tx(&mut tx, rate).await
            {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_CHAT_RATE, e.to_string()));
            }

            if let Some(score) = min_password_strength {
                let strength = validators::PasswordStrength::from(score);
                if let Err(e) = ConfigDb::set_min_password_strength_in_tx(&mut tx, strength).await {
                    break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_PASSWORD, e.to_string()));
                }
            }

            if let Some(rate) = max_outbound_rate
                && let Err(e) = ConfigDb::set_max_outbound_rate_in_tx(&mut tx, rate).await
            {
                break 'stage Err(StageFail::Db(
                    LOG_SERVER_INFO_DB_MAX_OUTBOUND_RATE,
                    e.to_string(),
                ));
            }

            if let Some(size) = scheduler_chunk_size
                && let Err(e) = ConfigDb::set_scheduler_chunk_size_in_tx(&mut tx, size).await
            {
                break 'stage Err(StageFail::Db(
                    LOG_SERVER_INFO_DB_SCHEDULER_CHUNK_SIZE,
                    e.to_string(),
                ));
            }

            if let Err(e) = tx.commit().await {
                break 'stage Err(StageFail::Db(LOG_SERVER_INFO_DB_COMMIT, e.to_string()));
            }

            Ok(channels_to_init)
        };

        let channels_to_init = match staged {
            Ok(c) => c,
            Err(StageFail::Db(log_const, detail)) => {
                error!(user = %user.username, ip = %ctx.peer_addr, err = %detail, "{}", log_const);
                break 'locked Outcome::Send(Box::new(ServerMessage::ServerInfoUpdateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                }));
            }
            Err(StageFail::Channel(log_const, detail, target)) => {
                error!(user = %user.username, ip = %ctx.peer_addr, target = %target, err = %detail, "{}", log_const);
                break 'locked Outcome::Send(Box::new(ServerMessage::ServerInfoUpdateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                }));
            }
        };

        // Runtime side-effects apply only after the commit succeeds.
        if let Some(max_conn) = max_connections_per_ip {
            ctx.connection_tracker
                .set_max_connections_per_ip(max_conn as usize);
        }
        if let Some(max_xfer) = max_transfers_per_ip {
            ctx.connection_tracker
                .set_max_transfers_per_ip(max_xfer as usize);
        }
        if let Some(burst) = chat_burst_limit {
            ctx.flood_config.set_burst(burst);
        }
        if let Some(rate) = chat_rate_limit {
            ctx.flood_config.set_rate(rate);
        }
        if let Some(init) = channels_to_init {
            ctx.channel_manager
                .reinitialize_persistent_channels(init)
                .await;
        }

        let config = ctx.db.config.get_all().await;

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
                max_outbound_rate: config.max_outbound_rate,
                scheduler_chunk_size: config.scheduler_chunk_size,
            })
            .await;

        info!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_SERVER_INFO_SUCCESS);
        Outcome::Send(Box::new(ServerMessage::ServerInfoUpdateResponse {
            success: true,
            error: None,
        }))
    };

    dispatch_outcome(outcome, ctx, HANDLER_SERVER_INFO_UPDATE).await
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: None,
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "ServerInfoUpdate should require login");
    }

    #[tokio::test]
    async fn test_server_info_update_requires_admin() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_admin_required(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_no_fields_fails() {
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
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_no_fields_to_update(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_name_empty_fails() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_server_name_empty(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_name_too_long_fails() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error
                        .unwrap_or_default()
                        .contains(&validators::MAX_SERVER_NAME_LENGTH.to_string())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_description_too_long_fails() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error
                        .unwrap_or_default()
                        .contains(&validators::MAX_SERVER_DESCRIPTION_LENGTH.to_string())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_max_connections_zero_means_unlimited() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 0 means unlimited.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 0);
    }

    #[tokio::test]
    async fn test_server_info_update_name_success() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_name = test_ctx.db.config.get_all().await.server_name;
        assert_eq!(saved_name, "My Custom Server");
    }

    #[tokio::test]
    async fn test_server_info_update_name_at_unicode_cap_passes() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Char-counted validator: a multi-byte string at exactly MAX_SERVER_NAME_LENGTH
        // chars must pass even though its UTF-8 byte length is 4× larger.
        let unicode_name = "🚀".repeat(validators::MAX_SERVER_NAME_LENGTH);
        let request = ServerInfoUpdateRequest {
            name: Some(unicode_name.clone()),
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success, "Unicode name at char cap should pass: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }

        let saved_name = test_ctx.db.config.get_all().await.server_name;
        assert_eq!(saved_name, unicode_name);
    }

    #[tokio::test]
    async fn test_server_info_update_name_unicode_over_cap_rejected() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Char-counted validator: MAX+1 chars must reject regardless of UTF-8
        // byte length. Pins that the validator counts chars, not bytes.
        let unicode_name = "🚀".repeat(validators::MAX_SERVER_NAME_LENGTH + 1);
        let request = ServerInfoUpdateRequest {
            name: Some(unicode_name),
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error
                        .unwrap_or_default()
                        .contains(&validators::MAX_SERVER_NAME_LENGTH.to_string())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_description_success() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_desc = test_ctx.db.config.get_all().await.server_description;
        assert_eq!(saved_desc, "Welcome to my server!");
    }

    #[tokio::test]
    async fn test_server_info_update_max_connections_success() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 10);
    }

    #[tokio::test]
    async fn test_server_info_update_all_fields_success() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_name = test_ctx.db.config.get_all().await.server_name;
        assert_eq!(saved_name, "Full Update Server");

        let saved_desc = test_ctx.db.config.get_all().await.server_description;
        assert_eq!(saved_desc, "All fields updated");

        let saved_max = test_ctx.db.config.get_max_connections_per_ip().await;
        assert_eq!(saved_max, 15);
    }

    #[tokio::test]
    async fn test_server_info_update_empty_description_allowed() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Set a description, then clear it with an empty string.
        test_ctx
            .db
            .config
            .set_server_description("Initial description")
            .await
            .unwrap();

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_desc = test_ctx.db.config.get_all().await.server_description;
        assert_eq!(saved_desc, "");
    }

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Broadcast travels through the mpsc channel (test_ctx.rx), not the socket.
        let (broadcast, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("broadcast delivered")
            .expect_message();
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

        let saved = test_ctx.db.config.get_all().await.public_address;
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Empty stored value maps to None on the wire.
        let (broadcast, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("broadcast delivered")
            .expect_message();
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

        let saved = test_ctx.db.config.get_all().await.public_address;
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
        assert_eq!(test_ctx.db.config.get_all().await.public_address, "");

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Broadcast still fires; empty stored value maps to None on the wire.
        let (broadcast, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("broadcast delivered")
            .expect_message();
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

        assert_eq!(test_ctx.db.config.get_all().await.public_address, "");
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error
                        .unwrap_or_default()
                        .contains(&validators::MAX_PUBLIC_ADDRESS_LENGTH.to_string())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_public_address_contains_port(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_image_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Minimal valid PNG data URI.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_image = test_ctx.db.config.get_all().await.server_image;
        assert_eq!(saved_image, test_image);
    }

    #[tokio::test]
    async fn test_server_info_update_image_empty_allowed() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Set an image, then clear it with an empty string.
        test_ctx
            .db
            .config
            .set_server_image("data:image/png;base64,iVBORw0KGgo=")
            .await
            .unwrap();

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_image = test_ctx.db.config.get_all().await.server_image;
        assert_eq!(saved_image, "");
    }

    #[tokio::test]
    async fn test_server_info_update_image_too_large_fails() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_server_image_too_large(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_image_invalid_format_fails() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_server_image_invalid_format(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_image_unsupported_type_fails() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(
                    error.as_deref(),
                    Some(err_server_image_unsupported_type(DEFAULT_TEST_LOCALE).as_str())
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_file_reindex_interval_success() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_interval = test_ctx.db.config.get_file_reindex_interval().await;
        assert_eq!(saved_interval, 10);
    }

    #[tokio::test]
    async fn test_server_info_update_file_reindex_interval_zero_disables() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 0 disables reindexing.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_interval = test_ctx.db.config.get_file_reindex_interval().await;
        assert_eq!(saved_interval, 0);
    }

    #[tokio::test]
    async fn test_server_info_update_persistent_channels_valid() {
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
            persistent_channels: Some("#general #support".to_string()),
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved = test_ctx.db.config.get_persistent_channels().await;
        assert_eq!(saved, "#general #support");
    }

    #[tokio::test]
    async fn test_server_info_update_persistent_channels_missing_prefix() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // "general" lacks the # prefix → the whole list is rejected.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error.unwrap_or_default().contains("general"),
                    "Error should mention the invalid channel"
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_auto_join_channels_valid() {
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
            auto_join_channels: Some("#nexus #welcome".to_string()),
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved = test_ctx.db.config.get_all().await.auto_join_channels;
        assert_eq!(saved, "#nexus #welcome");
    }

    #[tokio::test]
    async fn test_server_info_update_auto_join_channels_invalid() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // "#" alone is too short (needs a char after #).
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                assert!(
                    error.unwrap_or_default().contains("#"),
                    "Error should mention the invalid channel"
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_channels_with_spaces_invalid() {
        let mut test_ctx = create_test_context().await;

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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success);
                // "channel" will fail because it doesn't start with #
                assert!(
                    error.unwrap_or_default().contains("channel"),
                    "Error should mention the invalid channel"
                );
            }
            _ => panic!("Expected ServerInfoUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_empty_channel_list_valid() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Empty string is valid — no channels.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved_persistent = test_ctx.db.config.get_persistent_channels().await;
        let saved_auto_join = test_ctx.db.config.get_all().await.auto_join_channels;
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved = test_ctx.db.config.get_chat_burst_limit().await;
        assert_eq!(saved, 10);

        // Runtime FloodConfig atomic is updated alongside the DB write.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved = test_ctx.db.config.get_chat_rate_limit().await;
        assert_eq!(saved, 60);

        // Runtime FloodConfig atomic is updated alongside the DB write.
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
            max_outbound_rate: None,
            scheduler_chunk_size: None,
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

        let saved = test_ctx.db.config.get_chat_rate_limit().await;
        assert_eq!(saved, 0);

        // Runtime FloodConfig atomic is updated alongside the DB write.
        assert_eq!(test_ctx.flood_config.rate(), 0);
    }

    #[tokio::test]
    async fn test_server_info_update_max_outbound_rate_persists_and_broadcasts() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 100 Mbps = 12_500_000 bytes/sec.
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
            max_outbound_rate: Some(12_500_000),
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Broadcast travels through the mpsc channel (test_ctx.rx).
        let (broadcast, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("broadcast delivered")
            .expect_message();
        match broadcast {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.max_outbound_rate, Some(12_500_000));
            }
            _ => panic!("Expected ServerInfoUpdated, got {:?}", broadcast),
        }

        // Response comes back over the socket.
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(success, "Expected success, got error: {:?}", error);
            }
            _ => panic!("Expected ServerInfoUpdateResponse"),
        }

        let stored = test_ctx.db.config.get_all().await.max_outbound_rate;
        assert_eq!(stored, 12_500_000);
    }

    #[tokio::test]
    async fn test_server_info_update_scheduler_chunk_size_below_min_rejected() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 512 < MIN_BANDWIDTH_CHUNK_SIZE (1024).
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
            max_outbound_rate: None,
            scheduler_chunk_size: Some(512),
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
            _ => panic!("Expected ServerInfoUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_server_info_update_scheduler_chunk_size_above_max_rejected() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 100_000 > MAX_BANDWIDTH_CHUNK_SIZE (65536).
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
            max_outbound_rate: None,
            scheduler_chunk_size: Some(100_000),
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
            _ => panic!("Expected ServerInfoUpdateResponse"),
        }
    }

    /// A mid-transaction failure must roll back earlier writes in the same
    /// request. The `persistent_channels` value below passes the handler's
    /// per-name `validate_channel` check but exceeds `MAX_CHANNEL_LIST_LENGTH`,
    /// so it's rejected by `set_persistent_channels_in_tx` — after
    /// `set_server_name_in_tx` and `set_max_connections_per_ip_in_tx` have
    /// already run in the same `tx`.
    #[tokio::test]
    async fn test_server_info_update_rolls_back_on_mid_tx_failure() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Capture pre-request state for the fields we're about to "set".
        let initial_name = test_ctx.db.config.get_all().await.server_name;
        let initial_max_conn = test_ctx.db.config.get_max_connections_per_ip().await;
        let initial_burst = test_ctx.db.config.get_chat_burst_limit().await;
        let initial_runtime_burst = test_ctx.flood_config.burst();

        // Build a channel list where each individual name validates
        // but the joined string exceeds MAX_CHANNEL_LIST_LENGTH (512).
        // "#aa " is 4 bytes; 130 entries → 520 bytes, just over the cap.
        let oversized_channels = (0..130).map(|_| "#aa").collect::<Vec<_>>().join(" ");
        assert!(oversized_channels.len() > validators::MAX_CHANNEL_LIST_LENGTH);

        let request = ServerInfoUpdateRequest {
            name: Some("Rolled Back Name".to_string()),
            description: None,
            public_address: None,
            max_connections_per_ip: Some(99),
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: Some(oversized_channels),
            auto_join_channels: None,
            chat_burst_limit: Some(42),
            chat_rate_limit: None,
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            session_id: Some(session_id),
        };
        let result = handle_server_info_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Response is the typed failure shape.
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                assert!(!success, "update must report failure");
                assert!(error.is_some(), "failure must carry an error message");
            }
            other => panic!("Expected ServerInfoUpdateResponse, got {:?}", other),
        }

        // Writes staged before the failure point must have rolled back.
        assert_eq!(
            test_ctx.db.config.get_all().await.server_name,
            initial_name,
            "server_name must roll back",
        );
        assert_eq!(
            test_ctx.db.config.get_max_connections_per_ip().await,
            initial_max_conn,
            "max_connections_per_ip must roll back",
        );
        assert_eq!(
            test_ctx.db.config.get_chat_burst_limit().await,
            initial_burst,
            "chat_burst_limit must not be written when the tx aborts",
        );

        // Runtime side effects and fanouts are post-commit only, so a mid-tx
        // failure must not update in-memory limits or queue ServerInfoUpdated.
        assert_eq!(
            test_ctx.flood_config.burst(),
            initial_runtime_burst,
            "runtime chat burst limit must not update when the tx aborts",
        );

        let mut guards = Vec::with_capacity(100);
        for slot in 0..100 {
            let guard = test_ctx
                .connection_tracker
                .try_acquire(test_ctx.peer_addr.ip())
                .unwrap_or_else(|| {
                    panic!("runtime connection limit changed after {slot} acquisitions")
                });
            guards.push(guard);
        }
        drop(guards);

        assert!(
            matches!(
                test_ctx.rx.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ),
            "failed update must not broadcast ServerInfoUpdated",
        );
    }
}
