use clap::Parser;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

use args::PasswordKind;

mod args;
mod auth;
mod constants;
mod logging;
mod tls;

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

    if let Err(e) = logging::init(
        &data_dir,
        cli.log_level,
        cli.log_retention,
        cli.no_log_timestamps,
    ) {
        // `init` installs a stderr-only fallback subscriber before
        // returning Err, so this warning still reaches the user.
        warn!(err = %e, "{}", constants::LOG_LOGGING_INIT_FAILED);
    }

    if let Err(e) = ensure_data_dir(&data_dir) {
        error!("{}", e);
        std::process::exit(1);
    }

    let log_path = logging::log_dir(&data_dir);
    logging::purge_old_logs(&log_path, cli.log_retention);

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
            if let Err(e) = run_daemon_startup(&data_dir, &cli) {
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

/// Daemon-mode startup: ensure TLS material is on disk, log auth gating
/// status, and (eventually) start listening. The listener itself is the
/// next implementation step; for now this just confirms the daemon's
/// preconditions are healthy.
fn run_daemon_startup(data_dir: &Path, cli: &args::Cli) -> Result<(), String> {
    // Ensure cert + key exist. `ensure_cert` logs the fingerprint and
    // generation messages internally.
    tls::ensure_cert(data_dir)?;
    info!("{}{}", constants::MSG_CERTIFICATES, data_dir.display());

    log_auth_status(data_dir, PasswordKind::Registration)?;
    log_auth_status(data_dir, PasswordKind::Listing)?;

    info!(
        bind = %cli.bind,
        port = cli.port,
        "nexus-trackerd starting (not yet implemented)"
    );
    Ok(())
}

/// Log whether the given auth flow is open (no hash file) or gated
/// (hash file present).
fn log_auth_status(data_dir: &Path, kind: PasswordKind) -> Result<(), String> {
    let present = auth::load_password_hash(data_dir, kind)?.is_some();
    let label = match kind {
        PasswordKind::Registration => constants::LABEL_REGISTRATION,
        PasswordKind::Listing => constants::LABEL_LISTING,
    };
    let status = if present {
        constants::STATUS_GATED
    } else {
        constants::STATUS_OPEN
    };
    info!("{}: {}", label, status);
    Ok(())
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
            builder.mode(constants::DATA_DIR_MODE);
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
            fs::Permissions::from_mode(constants::DATA_DIR_MODE),
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
            constants::DATA_DIR_MODE,
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
            constants::DATA_DIR_MODE,
            "pre-existing loose data dir should be corrected to 0o700"
        );
    }
}
