// Build script to generate icon fonts at compile time

use std::process::Command;

fn main() {
    // Rebuild if the icon font definition changes
    println!("cargo::rerun-if-changed=fonts/icons.toml");

    // Generate the icon font and module
    iced_fontello::build("fonts/icons.toml").expect("Failed to build icon font");

    // Format the entire codebase after generating icon.rs
    // Try cargo fmt, but don't fail the build if it's not available
    if let Ok(status) = Command::new("cargo").arg("fmt").arg("--all").status() {
        if !status.success() {
            eprintln!("Warning: cargo fmt failed");
        }
    } else {
        eprintln!("Warning: cargo not found, skipping formatting");
    }
}
