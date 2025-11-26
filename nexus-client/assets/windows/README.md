# Windows Assets

This directory contains Windows-specific application assets for Nexus BBS.

## Files

- `nexus.ico` - Windows application icon (multi-resolution)
- `generate_assets.sh` - Script to generate Windows assets from SVG source

## Generating Assets

```bash
./generate_assets.sh
```

Or on Windows PowerShell/CMD:

```powershell
sh generate_assets.sh
```

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
- Linux: `apt install imagemagick` or `dnf install imagemagick`
