#!/bin/sh
# Generate macOS application assets from SVG source
# Requires: ImageMagick (magick/convert)

set -e

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SVG_SOURCE="${SCRIPT_DIR}/../linux/nexus.svg"
MACOS_DIR="${SCRIPT_DIR}"

# Check if SVG source exists
if [ ! -f "$SVG_SOURCE" ]; then
    echo "Error: SVG source not found at $SVG_SOURCE" >&2
    exit 1
fi

# Check for required tools
if ! command -v magick >/dev/null 2>&1 && ! command -v convert >/dev/null 2>&1; then
    echo "Error: ImageMagick not found (need 'magick' or 'convert' command)" >&2
    echo "Install with: brew install imagemagick (macOS)" >&2
    echo "            or apt install imagemagick (Linux)" >&2
    exit 1
fi

# Determine which ImageMagick command to use
if command -v magick >/dev/null 2>&1; then
    CONVERT_CMD="magick"
else
    CONVERT_CMD="convert"
fi

echo "Generating macOS assets from $SVG_SOURCE"
echo ""

# Generate macOS PNG (1024x1024) with transparency
echo "Generating PNG (1024x1024)..."
"$CONVERT_CMD" -background none "$SVG_SOURCE" -resize 1024x1024 "${MACOS_DIR}/nexus.png"
echo "✓ nexus.png"

echo ""
echo "✓ macOS asset generation complete!"
echo ""
echo "Note: cargo-bundle will automatically convert the PNG to ICNS format"