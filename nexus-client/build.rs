// Build script to generate icon fonts at compile time
//
// This is a separate compilation unit from the crate, so it can't
// import from `crate::constants`. The single panic message it needs
// lives here as a private const.

use std::process::Command;

/// Panic message: `iced_fontello::build` failed to generate the icon
/// font module. This fires at build time only.
const ERR_BUILD_ICON_FONT: &str = "Failed to build icon font";

/// Operator warning: `cargo fmt` ran but exited non-zero. The build
/// continues regardless; formatting is best-effort post-codegen.
const WARN_CARGO_FMT_FAILED: &str = "Warning: cargo fmt failed";

/// Operator warning: `cargo` binary was not found on PATH at build
/// time, so the post-codegen formatting step is skipped. The build
/// continues regardless.
const WARN_CARGO_NOT_FOUND: &str = "Warning: cargo not found, skipping formatting";

fn main() {
    // Rebuild if the icon font definition changes
    println!("cargo::rerun-if-changed=fonts/icons.toml");

    // Generate the icon font and module
    iced_fontello::build("fonts/icons.toml").expect(ERR_BUILD_ICON_FONT);

    // macOS icon configuration
    #[cfg(target_os = "macos")]
    {
        println!("cargo::rerun-if-changed=assets/macos/nexus.icns");
        // The icon will be included in the app bundle via Info.plist
        // For cargo-bundle, place nexus.icns in assets/macos/
    }

    // Format the entire codebase after generating icon.rs
    // Try cargo fmt, but don't fail the build if it's not available
    if let Ok(status) = Command::new("cargo").arg("fmt").arg("--all").status() {
        if !status.success() {
            eprintln!("{}", WARN_CARGO_FMT_FAILED);
        }
    } else {
        eprintln!("{}", WARN_CARGO_NOT_FOUND);
    }
}
