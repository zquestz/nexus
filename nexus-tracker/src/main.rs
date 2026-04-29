use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info, warn};

mod args;
mod constants;
mod logging;

fn main() {
    let cli = args::Cli::parse();

    let data_dir = match resolve_data_dir(cli.data_dir.clone()) {
        Ok(d) => d,
        Err(e) => {
            // Logging not yet initialized — direct stderr is the only option.
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = logging::init(
        &data_dir,
        cli.log_level,
        cli.log_retention,
        cli.no_log_timestamps,
    ) {
        // `init` falls back to stderr-only on failure, so this still reaches
        // the user via the subscriber that was just set up.
        error!("{}", e);
    }

    let log_path = logging::log_dir(&data_dir);
    logging::purge_old_logs(&log_path, cli.log_retention);

    match cli.command {
        Some(args::Command::SetPassword { kind }) => {
            warn!(?kind, "set-password not yet implemented");
        }
        Some(args::Command::ClearPassword { kind }) => {
            warn!(?kind, "clear-password not yet implemented");
        }
        None => {
            info!(
                version = env!("CARGO_PKG_VERSION"),
                bind = %cli.bind,
                port = cli.port,
                "nexus-trackerd starting (not yet implemented)"
            );
        }
    }
}

/// Resolve the tracker data directory, preferring the CLI override when
/// set and otherwise falling back to the platform default.
fn resolve_data_dir(override_path: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    dirs::data_dir()
        .map(|d| d.join(constants::DATA_DIR_NAME))
        .ok_or_else(|| "could not determine platform data directory".to_string())
}
