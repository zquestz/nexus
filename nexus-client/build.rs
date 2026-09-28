// Build script to generate icon fonts at compile time
//
// This is a separate compilation unit from the crate, so it can't
// import from `crate::constants`. The single panic message it needs
// lives here as a private const.

/// Panic message: `iced_fontello::build` failed to generate the icon
/// font module. This fires at build time only.
const ERR_BUILD_ICON_FONT: &str = "Failed to build icon font";

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

    // Linux (target OS, not the build host): search `$ORIGIN/../lib` for
    // shared libraries first. In the AppImage that's usr/lib, where
    // cargo-bundle copies `appimage_libs`. `--disable-new-dtags` emits
    // DT_RPATH instead of DT_RUNPATH so the path also covers those libraries'
    // own dependencies. In the .deb it resolves to /usr/lib, which is harmless.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bins=-Wl,--disable-new-dtags,-rpath,$ORIGIN/../lib");
    }
}
