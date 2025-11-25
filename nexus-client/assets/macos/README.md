# macOS Application Assets

Icon assets for macOS app bundle.

## Files

- `nexus.png` - 1024x1024 PNG icon
- `nexus.icns` - macOS icon bundle
- `generate_assets.sh` - Script to regenerate icons from SVG

## Generating Assets

```bash
./generate_assets.sh
```

**Requirements:**

- ImageMagick: `brew install imagemagick` (macOS) or `pacman -S imagemagick` (Linux)
- libicns: `brew install libicns` (macOS) or `pacman -S libicns` (Linux)

## Building App Bundle

```bash
# Install cargo-bundle
cargo install cargo-bundle

# Build from project root
cd ../../../..
cargo bundle --release

# Result: target/release/bundle/osx/Nexus BBS.app
```
