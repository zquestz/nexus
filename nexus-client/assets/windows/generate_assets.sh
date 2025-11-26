#!/bin/sh
# Generate Windows application assets from SVG source
# Requires: ImageMagick (magick/convert)

set -e

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SVG_SOURCE="${SCRIPT_DIR}/../linux/nexus.svg"
WINDOWS_DIR="${SCRIPT_DIR}"

# Check if SVG source exists
if [ ! -f "$SVG_SOURCE" ]; then
    echo "Error: SVG source not found at $SVG_SOURCE" >&2
    exit 1
fi

# Check for required tools
if ! command -v magick >/dev/null 2>&1 && ! command -v convert >/dev/null 2>&1; then
    echo "Error: ImageMagick not found (need 'magick' or 'convert' command)" >&2
    echo "Install with: winget install ImageMagick.ImageMagick (Windows)" >&2
    echo "            or brew install imagemagick (macOS)" >&2
    echo "            or pacman -S imagemagick (Arch)" >&2
    echo "            or apt install imagemagick (Debian/Ubuntu)" >&2
    echo "            or dnf install imagemagick (Fedora)" >&2
    exit 1
fi

# Determine which ImageMagick command to use
if command -v magick >/dev/null 2>&1; then
    CONVERT_CMD="magick"
else
    CONVERT_CMD="convert"
fi

echo "Generating Windows assets from $SVG_SOURCE"
echo ""

# Generate Windows ICO with multiple sizes embedded
# Windows .ico files contain multiple resolutions: 16, 32, 48, 64, 128, 256
echo "Generating Windows ICO (multi-size)..."
"$CONVERT_CMD" -background none "$SVG_SOURCE" \
    \( -clone 0 -resize 16x16 \) \
    \( -clone 0 -resize 32x32 \) \
    \( -clone 0 -resize 48x48 \) \
    \( -clone 0 -resize 64x64 \) \
    \( -clone 0 -resize 128x128 \) \
    \( -clone 0 -resize 256x256 \) \
    -delete 0 "${WINDOWS_DIR}/nexus.ico"
echo "✓ nexus.ico"

echo ""
echo "✓ Windows asset generation complete!"
