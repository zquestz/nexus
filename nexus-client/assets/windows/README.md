# Windows Assets

This directory contains Windows-specific application assets for Nexus BBS.

## Files

- `nexus.ico` - Windows application icon (multi-resolution)
- `generate_assets.sh` - Script to generate Windows assets from SVG source

## Building for Windows

Simply build the executable - the icon is automatically embedded:

```bash
cargo build --release
```

The executable will be at `target/release/nexus.exe` with the icon embedded.

### MSI Installer (Optional)

You can generate an MSI installer using cargo-bundle with the WiX toolset:

```bash
cargo install --git https://github.com/zquestz/cargo-bundle --force
cargo bundle --target x86_64-pc-windows-msvc --format wxsmsi --release
```

The installer will be at `target/x86_64-pc-windows-msvc/release/bundle/wxsmsi/`.

## Generating Assets

```bash
./generate_assets.sh
```

Or on Windows PowerShell/CMD:
```powershell
sh generate_assets.sh
```

This will generate the `.ico` file from the SVG source.

## Icon Format

The `.ico` file contains multiple embedded sizes for Windows:
- 16×16px
- 32×32px
- 48×48px
- 64×64px
- 128×128px
- 256×256px

This ensures the icon looks good at all sizes in Windows Explorer, taskbar, and other contexts.

## Requirements

- ImageMagick (`magick` or `convert` command)

**Install on Windows:**
```powershell
winget install ImageMagick.ImageMagick
```

**Install on other platforms:**
- macOS: `brew install imagemagick`
- Arch: `pacman -S imagemagick`
- Debian/Ubuntu: `apt install imagemagick`
- Fedora: `dnf install imagemagick`
