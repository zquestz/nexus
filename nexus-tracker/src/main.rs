use clap::Parser;
use std::fs;
use std::io::{self, IsTerminal};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use nexus_common::logging::{self, LogInitParams};
use nexus_tracker::{
    args::{self, PasswordKind},
    auth, connection, constants,
    registry::Registry,
    state::TrackerState,
    tls, upnp, websocket,
};

#[tokio::main]
async fn main() {
    // Install rustls crypto provider before any TLS operation. Required
    // because both tokio-rustls and our own TLS-using paths talk to
    // rustls 0.23 which needs an explicit provider selection.
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .expect(constants::ERR_RUSTLS_PROVIDER);

    let mut cli = args::Cli::parse();

    let data_dir = resolve_data_dir(cli.data_dir.take());

    if let Err(e) = logging::init(LogInitParams {
        data_dir: &data_dir,
        level: cli.log_level,
        retention: cli.log_retention,
        no_timestamps: cli.no_log_timestamps,
        log_file_prefix: constants::LOG_FILE_PREFIX,
    }) {
        // `init` installs a stderr-only fallback subscriber before
        // returning Err, so this warning still reaches the user.
        warn!(err = %e, "{}", constants::LOG_LOGGING_INIT_FAILED);
    }

    if let Err(e) = ensure_data_dir(&data_dir) {
        error!("{}", e);
        std::process::exit(1);
    }

    let log_path = logging::log_dir(&data_dir);
    logging::purge_old_logs(&log_path, cli.log_retention, constants::LOG_FILE_PREFIX);

    info!("{}{}", constants::MSG_BANNER, env!("CARGO_PKG_VERSION"));
    info!("{}{}", constants::MSG_LOG_LEVEL, cli.log_level);
    if cli.log_level != logging::LogLevel::None && cli.log_retention > std::time::Duration::ZERO {
        info!("{}{}", constants::MSG_LOG_DIR, log_path.display());
    }

    match cli.command {
        Some(args::Command::SetPassword { kind }) => {
            if let Err(e) = run_set_password(&data_dir, kind) {
                error!("{}", e);
                std::process::exit(1);
            }
        }
        Some(args::Command::ClearPassword { kind }) => {
            if let Err(e) = run_clear_password(&data_dir, kind) {
                error!("{}", e);
                std::process::exit(1);
            }
        }
        None => {
            if let Err(e) = run_daemon_startup(&data_dir, &cli).await {
                error!("{}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Prompt for (or read piped) and store a new password under the data
/// directory.
///
/// When stdin is a TTY: prompts twice with `rpassword` (entry +
/// confirmation, no echo). When stdin is piped: reads a single line of
/// stdin verbatim (no confirmation — the operator is scripted). Hashes
/// with Argon2id and writes the resulting PHC string to
/// `<data-dir>/<kind>.hash` atomically with mode `0o600` on Unix.
fn run_set_password(data_dir: &Path, kind: PasswordKind) -> Result<(), String> {
    info!(kind = %kind, "{}", constants::LOG_PASSWORD_SETTING);
    let plain = read_new_password_input()?;
    auth::set_password(data_dir, kind, &plain)?;
    info!(kind = %kind, "{}", constants::LOG_PASSWORD_SET);
    Ok(())
}

/// Read a fresh password either from a TTY (with confirmation) or from
/// piped stdin (single line). The TTY path is the operator-friendly
/// interactive form; the pipe path lets ops scripts do
/// `printf %s "$pw" | nexus-trackerd set-password registration`.
fn read_new_password_input() -> Result<String, String> {
    if std::io::stdin().is_terminal() {
        let plain = rpassword::prompt_password(constants::PROMPT_NEW_PASSWORD)
            .map_err(|e| format!("{}{}", constants::ERR_PROMPT_PASSWORD, e))?;
        let confirm = rpassword::prompt_password(constants::PROMPT_CONFIRM_PASSWORD)
            .map_err(|e| format!("{}{}", constants::ERR_PROMPT_PASSWORD, e))?;
        if plain != confirm {
            return Err(constants::ERR_PASSWORD_MISMATCH.to_string());
        }
        Ok(plain)
    } else {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("{}{}", constants::ERR_READ_STDIN, e))?;
        // Strip a single trailing CR / LF / CRLF added by `echo`/`printf`.
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

/// Remove the stored password hash file for `kind`, if any.
fn run_clear_password(data_dir: &Path, kind: PasswordKind) -> Result<(), String> {
    if auth::clear_password(data_dir, kind)? {
        info!(kind = %kind, "{}", constants::LOG_PASSWORD_CLEARED);
    } else {
        info!(kind = %kind, "{}", constants::LOG_PASSWORD_NOT_PRESENT);
    }
    Ok(())
}

/// Daemon-mode startup: ensure TLS material is on disk, build the TLS
/// acceptor, load auth state, bind the listener, and run the accept
/// loop until shutdown.
async fn run_daemon_startup(data_dir: &Path, cli: &args::Cli) -> Result<(), String> {
    // Ensure cert + key exist. `ensure_cert` logs the fingerprint and
    // generation messages internally.
    let fingerprint = tls::ensure_cert(data_dir)?;
    let tls_acceptor = tls::build_acceptor(data_dir)?;
    info!("{}{}", constants::MSG_CERTIFICATES, data_dir.display());

    // Load password hashes at startup. The daemon refuses to start if a
    // hash file exists but is unparseable. After startup, SIGHUP (Unix)
    // reloads both files via `TrackerState::reload_passwords`; on
    // Windows, password changes require a daemon restart.
    let registration_password_hash =
        auth::load_password_hash(data_dir, PasswordKind::Registration)?;
    let listing_password_hash = auth::load_password_hash(data_dir, PasswordKind::Listing)?;
    log_auth_status_value(
        PasswordKind::Registration,
        registration_password_hash.is_some(),
    );
    log_auth_status_value(PasswordKind::Listing, listing_password_hash.is_some());

    let registry = Registry::new(cli.max_entries, cli.max_entries_per_ip);
    let state = Arc::new(TrackerState::new(
        registry,
        registration_password_hash,
        listing_password_hash,
        cli.refresh_interval,
        cli.rate_connections,
        cli.rate_auth_failures,
        constants::REFRESH_FLOOR_INTERVAL,
    ));

    let bind_addr = SocketAddr::new(cli.bind, cli.port);
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|e| format!("{}{}: {}", constants::ERR_BIND_FAILED, bind_addr, e))?;
    let local_addr = listener.local_addr().unwrap_or(bind_addr);
    info!("{}{}", constants::MSG_LISTENING, local_addr);

    // Optional second listener for WebSocket clients. The same TLS
    // acceptor + state are shared with the TCP listener; the WS
    // entry point upgrades to WebSocket after TLS and then delegates
    // to the same connection task.
    let ws_listener = if cli.websocket {
        let ws_bind_addr = SocketAddr::new(cli.bind, cli.websocket_port);
        let listener = TcpListener::bind(ws_bind_addr)
            .await
            .map_err(|e| format!("{}{}: {}", constants::ERR_BIND_FAILED, ws_bind_addr, e))?;
        let local = listener.local_addr().unwrap_or(ws_bind_addr);
        info!("{}{}", constants::MSG_WS_LISTENING, local);
        Some(listener)
    } else {
        None
    };

    // Setup UPnP port forwarding if requested (forwards WS port only if enabled).
    let upnp = setup_upnp(
        cli.upnp,
        cli.bind,
        cli.port,
        if cli.websocket {
            Some(cli.websocket_port)
        } else {
            None
        },
    )
    .await;

    // Background sweep that drops idle, fully-refilled rate-limit buckets.
    // No-op when both limiters are disabled (capacity 0). Aborted on shutdown.
    let rate_limiter_gc = spawn_rate_limiter_gc(Arc::clone(&state));

    // Daily log-retention purge task. Returns `None` (no task) when file
    // logging is disabled — purge is a no-op in that case.
    let log_purge = logging::spawn_purge_task(
        data_dir.to_path_buf(),
        cli.log_level,
        cli.log_retention,
        constants::LOG_FILE_PREFIX.to_string(),
    );

    // SIGHUP password-reload task (Unix only). On Windows, password
    // changes require a daemon restart. Signal stream is installed
    // synchronously here so a setup failure surfaces as a startup
    // error to the operator instead of a runtime panic.
    #[cfg(unix)]
    let sighup = spawn_sighup_handler(Arc::clone(&state), data_dir.to_path_buf())?;

    let handles = DaemonHandles {
        rate_limiter_gc,
        log_purge,
        #[cfg(unix)]
        sighup,
        upnp,
    };

    accept_loop(
        listener,
        ws_listener,
        tls_acceptor,
        fingerprint,
        state,
        handles,
    )
    .await;
    Ok(())
}

/// Install the SIGHUP listener synchronously, then spawn a long-lived
/// task that recv()s on it and re-reads the password hash files on
/// each receipt. Returns a `JoinHandle` so graceful shutdown can
/// abort it. Returns an `Err` (with the operator-facing message) if
/// the signal stream itself can't be installed — failure surfaces at
/// startup rather than as a runtime panic on first SIGHUP.
#[cfg(unix)]
fn spawn_sighup_handler(
    state: Arc<TrackerState>,
    data_dir: PathBuf,
) -> Result<tokio::task::JoinHandle<()>, String> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sighup = signal(SignalKind::hangup())
        .map_err(|e| format!("{}: {}", constants::ERR_SIGNAL_SIGHUP, e))?;
    Ok(tokio::spawn(async move {
        loop {
            sighup.recv().await;
            info!("{}", constants::LOG_SIGHUP_RECEIVED);
            // `reload_passwords` does its own per-flow logging
            // (success / failure with previous-state-preserved
            // semantics).
            state.reload_passwords(&data_dir);
        }
    }))
}

/// Spawn the background rate-limiter GC task.
fn spawn_rate_limiter_gc(state: Arc<TrackerState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(constants::RATE_LIMITER_GC_INTERVAL);
        // Skip the initial immediate tick.
        interval.tick().await;
        loop {
            interval.tick().await;
            state
                .connection_rate_limiter
                .gc(constants::RATE_LIMITER_IDLE_TTL);
            state
                .auth_failure_rate_limiter
                .gc(constants::RATE_LIMITER_IDLE_TTL);
        }
    })
}

/// Set up UPnP port forwarding for the tracker if `enabled`. On failure
/// the daemon continues without forwarding (the operator log carries
/// the diagnostic), mirroring the BBS server's non-fatal posture.
async fn setup_upnp(
    enabled: bool,
    bind: std::net::IpAddr,
    main_port: u16,
    websocket_port: Option<u16>,
) -> Option<(Arc<upnp::Gateway>, tokio::task::JoinHandle<()>)> {
    if !enabled {
        return None;
    }

    match upnp::setup(bind, main_port, websocket_port).await {
        Ok(gateway) => {
            let gateway_arc = Arc::new(gateway);
            let renewal_task = upnp::spawn_lease_renewal_task(Arc::clone(&gateway_arc));
            Some((gateway_arc, renewal_task))
        }
        Err(e) => {
            warn!(err = %e, "{}", constants::LOG_UPNP_SETUP_FAILED);
            warn!("{}", constants::MSG_UPNP_CONTINUE);
            warn!("{}", constants::MSG_UPNP_MANUAL);
            None
        }
    }
}

/// Background tasks and resources owned by the daemon for the lifetime
/// of `accept_loop`. Bundled so `accept_loop`'s signature stays narrow
/// and its shutdown branch has a single `handles.shutdown().await`
/// call instead of inlined teardown for each task.
struct DaemonHandles {
    /// Periodic rate-limiter bucket sweep.
    rate_limiter_gc: tokio::task::JoinHandle<()>,
    /// Daily log-retention purge task. `None` when file logging is
    /// disabled (level == None or retention == 0).
    log_purge: Option<tokio::task::JoinHandle<()>>,
    /// SIGHUP password-reload listener. Unix only.
    #[cfg(unix)]
    sighup: tokio::task::JoinHandle<()>,
    /// UPnP gateway plus its lease-renewal task. `None` when `--upnp`
    /// is not set or setup failed. Listed last because shutdown's
    /// only async / fallible step is the port-mapping removal here.
    upnp: Option<(Arc<upnp::Gateway>, tokio::task::JoinHandle<()>)>,
}

impl DaemonHandles {
    /// Stop every background task and release every external resource
    /// (UPnP port mapping). Failures during teardown are best-effort —
    /// the daemon is going down anyway.
    async fn shutdown(self) {
        self.rate_limiter_gc.abort();
        if let Some(handle) = self.log_purge {
            handle.abort();
        }
        #[cfg(unix)]
        self.sighup.abort();
        if let Some((gateway, renewal_task)) = self.upnp {
            renewal_task.abort();
            if let Err(e) = gateway.remove_port_mapping().await {
                warn!(err = %e, "{}", constants::LOG_UPNP_REMOVE_FAILED);
            }
        }
    }
}

/// Accept loop, terminated by SIGINT / SIGTERM (or Ctrl+C on Windows).
/// Runs the TCP accept loop and (when `--websocket`) the WS accept
/// loop concurrently; either exiting on shutdown drains both.
async fn accept_loop(
    listener: TcpListener,
    ws_listener: Option<TcpListener>,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
    handles: DaemonHandles,
) {
    let tcp = run_tcp_accepts(
        &listener,
        tls_acceptor.clone(),
        fingerprint.clone(),
        Arc::clone(&state),
    );
    let ws = run_optional_ws_accepts(ws_listener, tls_acceptor, fingerprint, state);

    tokio::select! {
        () = tcp => {}
        () = ws => {}
        () = setup_shutdown_signal() => {
            info!("{}", constants::MSG_SHUTDOWN_RECEIVED);
            handles.shutdown().await;
        }
    }
}

/// Run the TCP accept loop. Each accepted connection is spawned as its
/// own task; an `accept()` failure logs and brief-sleeps to avoid a
/// busy loop on persistent errors (e.g., file-descriptor exhaustion).
async fn run_tcp_accepts(
    listener: &TcpListener,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let acceptor = tls_acceptor.clone();
                let fingerprint = fingerprint.clone();
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = connection::handle_connection(
                        stream,
                        peer_addr,
                        acceptor,
                        fingerprint,
                        state,
                    )
                    .await
                    {
                        log_connection_error(&e, peer_addr);
                    }
                });
            }
            Err(e) => {
                warn!(err = %e, "{}", constants::LOG_ACCEPT_ERROR);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Run the WebSocket accept loop when `--websocket` is enabled. When
/// the listener is `None` (flag not set) this future never resolves —
/// it sits in the `tokio::select!` and lets the TCP loop run.
async fn run_optional_ws_accepts(
    listener: Option<TcpListener>,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) {
    let Some(listener) = listener else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let acceptor = tls_acceptor.clone();
                let fingerprint = fingerprint.clone();
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = websocket::handle_tracker_websocket_connection(
                        stream,
                        peer_addr,
                        acceptor,
                        fingerprint,
                        state,
                    )
                    .await
                    {
                        log_connection_error(&e, peer_addr);
                    }
                });
            }
            Err(e) => {
                warn!(err = %e, "{}", constants::LOG_ACCEPT_ERROR);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Categorize and log a connection error from `handle_connection`.
///
/// - `close_notify` warnings (clients disconnecting abruptly) → silent.
/// - `TLS handshake failed:` prefix (scanners, incompatible clients) → debug.
/// - everything else (frame errors, write failures, etc.) → error.
///
/// Mirrors `nexus-server`'s shared logger so the two daemons categorize
/// connection noise the same way. When WebSocket support lands the same
/// function will route those errors too.
fn log_connection_error(error: &io::Error, peer_addr: SocketAddr) {
    let msg = error.to_string();
    if msg.contains(constants::TLS_CLOSE_NOTIFY_MSG) {
        return;
    }
    if msg.contains(constants::TLS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", constants::LOG_CONNECTION_ERROR_TLS);
        return;
    }
    error!(ip = %peer_addr, err = %error, "{}", constants::LOG_CONNECTION_ERROR);
}

/// Resolve when SIGINT/SIGTERM (Ctrl+C on Windows) is received.
async fn setup_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect(constants::ERR_SIGNAL_SIGTERM);
        let mut sigint = signal(SignalKind::interrupt()).expect(constants::ERR_SIGNAL_SIGINT);
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect(constants::ERR_SIGNAL_CTRLC);
    }
}

/// Log whether the given auth flow is open (no hash file) or gated
/// (hash file present).
fn log_auth_status_value(kind: PasswordKind, gated: bool) {
    let label = match kind {
        PasswordKind::Registration => constants::LABEL_REGISTRATION,
        PasswordKind::Listing => constants::LABEL_LISTING,
    };
    let status = if gated {
        constants::STATUS_GATED
    } else {
        constants::STATUS_OPEN
    };
    info!("{}: {}", label, status);
}

/// Resolve the tracker data directory, preferring the CLI override when
/// set and otherwise falling back to the platform default.
///
/// Panics only if the platform itself cannot supply a data directory
/// (`dirs::data_dir()` returns `None` — e.g., Windows without `%APPDATA%`,
/// Linux without `HOME`). This is platform-broken territory, not an
/// operator-actionable error.
fn resolve_data_dir(override_path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    dirs::data_dir()
        .map(|d| d.join(constants::DATA_DIR_NAME))
        .expect(constants::ERR_NO_DATA_DIR)
}

/// Create the data directory if it doesn't already exist and lock it to
/// owner-only permissions (`DATA_DIR_MODE`) on Unix. The directory hosts
/// the TLS private key, password hashes, and (by default) log files, so a
/// permissive parent directory undercuts the per-file protections inside.
///
/// On Unix, the mode is set atomically at creation via `DirBuilder::mode`
/// — there is no window where a fresh data directory is world-readable.
/// `set_permissions` is then applied to handle the case where the
/// directory pre-existed with the wrong mode.
fn ensure_data_dir(data_dir: &Path) -> Result<(), String> {
    let create_result = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(nexus_common::DATA_DIR_MODE);
            builder.create(data_dir)
        }
        #[cfg(not(unix))]
        {
            fs::create_dir_all(data_dir)
        }
    };
    create_result.map_err(|e| format!("{}{}", constants::ERR_CREATE_DATA_DIR, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            data_dir,
            fs::Permissions::from_mode(nexus_common::DATA_DIR_MODE),
        )
        .map_err(|e| format!("{}{}", constants::ERR_SET_DATA_DIR_PERMS, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_data_dir_override_returned_verbatim() {
        let override_path = PathBuf::from("/var/lib/nexus-trackerd-custom");
        assert_eq!(resolve_data_dir(Some(override_path.clone())), override_path);
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_data_dir_creates_fresh_with_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let data_dir = tmp.path().join("data");

        ensure_data_dir(&data_dir).expect("ensure_data_dir");

        let mode = std::fs::metadata(&data_dir)
            .expect("read metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            nexus_common::DATA_DIR_MODE,
            "fresh data dir should be created with 0o700"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_data_dir_corrects_pre_existing_loose_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let data_dir = tmp.path().join("data");

        // Pre-create with world-readable perms to simulate a wrongly-
        // permissioned data directory left over from a previous run.
        std::fs::create_dir(&data_dir).expect("pre-create dir");
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755))
            .expect("set initial perms");

        ensure_data_dir(&data_dir).expect("ensure_data_dir");

        let mode = std::fs::metadata(&data_dir)
            .expect("read metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            nexus_common::DATA_DIR_MODE,
            "pre-existing loose data dir should be corrected to 0o700"
        );
    }
}
