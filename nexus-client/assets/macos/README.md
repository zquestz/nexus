# macOS Assets

This directory contains macOS-specific application assets for Nexus BBS.

## Files

- `nexus.png` - 1024×1024px PNG icon
- `nexus.icns` - macOS icon bundle (generated from PNG)
- `generate_assets.sh` - Script to generate macOS assets from SVG source

## Generating Assets

```bash
./generate_assets.sh
```

This will generate macOS icons automatically from the SVG source.

## Icon Format

- **PNG**: 1024×1024px, used as intermediate for ICNS generation
- **ICNS**: macOS icon bundle containing multiple sizes for Retina and non-Retina displays

## Requirements

- ImageMagick (`magick` or `convert` command)
- libicns (`png2icns` command) - optional, for ICNS generation

**Install on macOS:**
```bash
brew install imagemagick libicns
```

**Install on Linux:**
- Arch: `pacman -S imagemagick libicns`
- Debian/Ubuntu: `apt install imagemagick icnsutils`
- Fedora: `dnf install imagemagick libicns`

## Building App Bundle

```bash
# Install cargo-bundle (one-time)
cargo install cargo-bundle

# Build from project root
cargo bundle --release

# Result: target/release/bundle/osx/Nexus BBS.app
```
